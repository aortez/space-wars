use super::*;

fn settled_task() -> FlightTask {
    FlightTask {
        goal_offset: Vec2::ZERO,
        ..Default::default()
    }
}

#[test]
fn arrival_needs_thirty_complete_intervals_and_terminal_state_cannot_advance() {
    let mut env = FlightEnvironment::new(settled_task()).unwrap();
    assert!(env.frame().center_of_mass.distance_to(env.frame().origin) > 1.0);
    for tick in 1..=30 {
        let step = env.step(FlightAction::default()).unwrap();
        assert_eq!(step.progress.elapsed_ticks, tick);
        assert_eq!(step.progress.dwell_ticks, tick);
        assert_eq!(
            step.progress.outcome,
            (tick == 30).then_some(FlightOutcome::Success)
        );
    }
    let before = env.state.world.physics.world.snapshot_bytes().unwrap();
    assert!(env.step(FlightAction::default()).is_err());
    assert_eq!(
        before,
        env.state.world.physics.world.snapshot_bytes().unwrap()
    );
    let observation = env.observation().unwrap();
    assert_eq!(observation.remaining_ticks, 1770);
    assert_eq!(observation.surroundings.geometry.cells.len(), 1472);
    assert_eq!(observation.surroundings.geometry.rays.len(), 128);
    assert_eq!(observation.surroundings.tick, observation.frame.world_tick);
}

#[test]
fn moving_goal_uses_analytic_world_velocity_and_com_arrival() {
    let mut task = settled_task();
    task.start.relative_velocity = Vec2::new(10.0, 0.0);
    task.goal_velocity = task.start.relative_velocity;
    let mut env = FlightEnvironment::new(task).unwrap();
    let initial = env.frame().target_position;
    for tick in 1..=30 {
        let step = env.step(FlightAction::default()).unwrap();
        let frame = step.frame.as_ref().unwrap();
        assert_eq!(
            frame.target_position,
            initial + Vec2::new(10.0, 0.0) * (tick as f32 / 60.0)
        );
        // World-space f32 integration accumulates a few thousandths over the
        // half-second dwell; the analytic target must not inherit that drift.
        assert!(
            frame.errors.velocity < 0.001 && frame.errors.position < 0.005,
            "tick {tick}: {:?}",
            frame.errors
        );
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
}

#[test]
fn flythrough_and_spinning_arrival_do_not_qualify() {
    let mut fast = FlightTask {
        goal_offset: Vec2::new(10.0, 0.0),
        deadline_ticks: 60,
        ..Default::default()
    };
    fast.start.relative_velocity = Vec2::new(70.0, 0.0);
    let mut env = FlightEnvironment::new(fast).unwrap();
    let mut entered_position = false;
    for _ in 0..60 {
        let step = env.step(FlightAction::default()).unwrap();
        entered_position |= step.frame.as_ref().unwrap().errors.position <= 3.0;
        assert_eq!(step.progress.dwell_ticks, 0);
    }
    assert!(entered_position);
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Deadline));

    let mut spinning = settled_task();
    spinning.start.spin = 1.8;
    spinning.deadline_ticks = 60;
    let mut env = FlightEnvironment::new(spinning).unwrap();
    assert!(
        (env.frame().spin - 1.775).abs() < 0.0001,
        "the setup tick applies ordinary neutral spin damping"
    );
    for _ in 0..60 {
        let step = env
            .step(FlightAction {
                steering: -1.0,
                ..Default::default()
            })
            .unwrap();
        let frame = step.frame.as_ref().unwrap();
        assert!(frame.errors.position < 0.01 && frame.errors.velocity < 0.01);
        assert!((frame.spin - 1.8).abs() < 0.0001);
        assert_eq!(step.progress.dwell_ticks, 0);
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Deadline));
}

#[test]
fn each_tolerance_resets_dwell_and_failure_precedes_arrival_on_deadline() {
    let task = FlightTask {
        deadline_ticks: 30,
        ..settled_task()
    };
    let good = ArrivalErrors {
        position: 3.0,
        velocity: 1.0,
        spin: 0.2,
    };
    for bad in [
        ArrivalErrors {
            position: 3.001,
            ..good
        },
        ArrivalErrors {
            velocity: 1.001,
            ..good
        },
        ArrivalErrors {
            spin: 0.201,
            ..good
        },
    ] {
        let mut progress = FlightProgress {
            elapsed_ticks: 10,
            dwell_ticks: 10,
            outcome: None,
        };
        progress.advance(good, bad, FlightEvidence::default(), &task);
        assert_eq!(progress.dwell_ticks, 0);
        progress.advance(bad, good, FlightEvidence::default(), &task);
        assert_eq!(
            progress.dwell_ticks, 0,
            "newly entering tolerance does not count a full interval"
        );
        progress.advance(good, good, FlightEvidence::default(), &task);
        assert_eq!(progress.dwell_ticks, 1);
    }
    for (evidence, expected) in [
        (FlightEvidence::default(), FlightOutcome::Success),
        (
            FlightEvidence {
                collision: true,
                damage: true,
                ..Default::default()
            },
            FlightOutcome::Collision,
        ),
        (
            FlightEvidence {
                damage: true,
                ..Default::default()
            },
            FlightOutcome::Damage,
        ),
        (
            FlightEvidence {
                ship_lost: true,
                collision: true,
                ..Default::default()
            },
            FlightOutcome::ShipLost,
        ),
    ] {
        let mut progress = FlightProgress {
            elapsed_ticks: 29,
            dwell_ticks: 29,
            outcome: None,
        };
        progress.advance(good, good, evidence, &task);
        assert_eq!(progress.outcome, Some(expected));
    }
    let mut progress = FlightProgress {
        elapsed_ticks: 29,
        dwell_ticks: 28,
        outcome: None,
    };
    progress.advance(good, good, FlightEvidence::default(), &task);
    assert_eq!(progress.outcome, Some(FlightOutcome::Deadline));
}

#[test]
fn reset_is_isolated_transactional_and_json_trace_replays_exactly() {
    let task = FlightTask::default();
    let mut env = FlightEnvironment::new(task.clone()).unwrap();
    for i in 0..90 {
        env.step(FlightAction {
            steering: if i < 30 { 0.7 } else { -0.2 },
            thrust: f32::from(i < 60),
            brake: f32::from(i >= 60),
        })
        .unwrap();
    }
    let encoded = serde_json::to_vec(env.trace()).unwrap();
    let trace: FlightTrace = serde_json::from_slice(&encoded).unwrap();
    let replay = FlightEnvironment::replay(&trace).unwrap();
    assert_eq!(replay.frame(), env.frame());
    let before = env.state.world.physics.world.snapshot_bytes().unwrap();
    let mut invalid = task.clone();
    invalid.start.altitude = f32::NAN;
    assert!(env.reset(invalid).is_err());
    assert_eq!(
        before,
        env.state.world.physics.world.snapshot_bytes().unwrap()
    );
    env.reset(task.clone()).unwrap();
    let fresh = FlightEnvironment::new(task).unwrap();
    assert_eq!(env.trace(), fresh.trace());
    assert_eq!(
        env.state.world.physics.world.snapshot_bytes().unwrap(),
        fresh.state.world.physics.world.snapshot_bytes().unwrap()
    );
    let before = env.state.world.physics.world.snapshot_bytes().unwrap();
    env.observation().unwrap();
    env.observation().unwrap();
    assert_eq!(
        before,
        env.state.world.physics.world.snapshot_bytes().unwrap()
    );
}

#[test]
fn invalid_controller_action_is_recorded_without_advancing_or_substitution() {
    for steering in [f32::NAN, f32::INFINITY, 1.01] {
        let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
        let initial = env.frame().clone();
        let step = env
            .step(FlightAction {
                steering,
                thrust: 1.0,
                brake: 1.0,
            })
            .unwrap();
        assert_eq!(step.progress.outcome, Some(FlightOutcome::ControllerError));
        assert_eq!(step.progress.elapsed_ticks, 0);
        assert_eq!(step.frame.as_ref(), Some(&initial));
        assert!(step.action.is_none());
        FlightEnvironment::replay(env.trace()).unwrap();
    }
}

#[test]
fn learned_mode_removes_near_surface_assist_and_preserves_far_actuators() {
    let mut task = FlightTask::default();
    task.start.altitude = 15.0;
    task.start.relative_velocity = Vec2::new(5.0, -2.0);
    task.start.planet_spin = 0.015;
    let mut env = FlightEnvironment::new(task).unwrap();
    assert!(env.frame().potential_landing_assist > 0.0);
    let mut standard = env.state.clone();
    standard.set_flight_control_mode(0, FlightControlMode::Standard);
    let velocity = env.frame().velocity;
    env.step(FlightAction::default()).unwrap();
    SurfaceSortieScenario::step(&mut standard, &[], FLIGHT_DT);
    assert!((env.frame().velocity - velocity).length() < 0.0001);
    assert!(env.frame().spin.abs() < 0.0001);
    let body = standard.world.physics.ship_body(0);
    let motion = standard.world.physics.world.motion(body).unwrap();
    assert!((motion.linear_velocity - velocity).length() > 0.001);
    assert!(motion.angular_velocity > 0.0);

    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let mut standard = env.state.clone();
    standard.set_flight_control_mode(0, FlightControlMode::Standard);
    for i in 0..60 {
        let action = FlightAction {
            steering: 0.5,
            thrust: f32::from(i < 30),
            brake: f32::from(i >= 20),
        };
        env.step(action).unwrap();
        SurfaceSortieScenario::step(
            &mut standard,
            &[SurfaceSortieAction {
                horizontal: action.steering,
                primary_held: action.thrust == 1.0,
                brake_held: action.brake == 1.0,
                interact_held: false,
            }
            .encode(PlayerId::PLAYER_1)],
            FLIGHT_DT,
        );
        assert_eq!(
            env.state.free_flight_observation(0),
            standard.free_flight_observation(0)
        );
    }
    let ordinary = SurfaceSortieScenario::init_material_flight(42, 1, &[]);
    assert_eq!(
        ordinary.flight_control_mode(0),
        Some(FlightControlMode::Standard)
    );
}

#[test]
fn physical_wall_impact_ends_attempt_and_separated_near_contact_does_not() {
    let mut task = FlightTask::default();
    task.start.relative_velocity = Vec2::new(70.0, 0.0);
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(25.0, 0.0),
        half_extents: Vec2::new(1.0, 20.0),
        angle: 0.0,
    });
    let mut env = FlightEnvironment::new(task).unwrap();
    while env.progress().outcome.is_none() {
        env.step(FlightAction::default()).unwrap();
    }
    let last = env.trace().steps.last().unwrap();
    assert_eq!(last.progress.outcome, Some(FlightOutcome::Collision));
    assert!(last.evidence.collision && last.evidence.maximum_impulse > 0.0);
    assert!(last.progress.elapsed_ticks < 30);
    FlightEnvironment::replay(env.trace()).unwrap();

    let mut task = settled_task();
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(6.52, 0.0),
        half_extents: Vec2::new(1.0, 10.0),
        angle: 0.0,
    });
    let mut env = FlightEnvironment::new(task).unwrap();
    assert!(
        env.frame().near_contact,
        "exercise the legacy 0.04 separation margin"
    );
    for _ in 0..30 {
        let step = env.step(FlightAction::default()).unwrap();
        assert!(!step.evidence.collision);
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
}

#[test]
fn overlap_is_rejected_and_live_terrain_edits_are_fresh_and_replayable() {
    let mut overlap = FlightTask::default();
    overlap.obstacles.push(FlightObstacle {
        offset: Vec2::ZERO,
        half_extents: Vec2::new(10.0, 10.0),
        angle: 0.0,
    });
    assert!(FlightEnvironment::new(overlap).is_err());
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let center = env.state.world.terrain.planets[&0]
        .field
        .local_to_cell(Vec2::new(0.0, 55.0))
        .unwrap();
    let edit = TerrainEdit {
        brush: engine_terrain::Brush::Circle { center, radius: 12 },
        mode: engine_terrain::EditMode::Remove,
    };
    let before = env.observation().unwrap().surroundings.geometry;
    env.queue_terrain_edit(edit).unwrap();
    assert_eq!(env.observation().unwrap().surroundings.geometry, before);
    env.step(FlightAction::default()).unwrap();
    let after = env.observation().unwrap().surroundings.geometry;
    assert_ne!(before.cells, after.cells);
    let replay = FlightEnvironment::replay(env.trace()).unwrap();
    assert_eq!(replay.observation().unwrap().surroundings.geometry, after);
}

#[test]
fn damage_event_is_evidence_even_when_health_is_unchanged() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let health = env.state.world.ships[0].life;
    // Model an event followed by same-tick healing: net health cannot erase it.
    env.state.world.body_impacts.push(BodyImpact {
        ship: 0,
        body: BodyId::Planet(0),
        speed: 10.0,
        damage: 1.0,
    });
    assert_eq!(env.state.world.ships[0].life, health);
    assert!(read_evidence(&env.state).damage);
}

#[test]
fn learned_mode_does_not_teleport_an_escaped_ship_back_inside() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let radius = env.state.world.config.universe_radius as f32;
    let outside = Vec2::splat(radius) + Vec2::new(radius + 100.0, 0.0);
    env.state.world.ships[0].position = outside - SHIP_PIVOT;
    let mut standard = env.state.clone();
    standard.set_flight_control_mode(0, FlightControlMode::Standard);
    let step = env.step(FlightAction::default()).unwrap();
    assert_eq!(step.progress.outcome, Some(FlightOutcome::BoundaryEscape));
    assert!(step.evidence.boundary_escape);
    assert!(
        step.frame
            .as_ref()
            .unwrap()
            .center_of_mass
            .distance_to(Vec2::splat(radius))
            > radius
    );
    SurfaceSortieScenario::step(&mut standard, &[], FLIGHT_DT);
    let body = standard.world.physics.ship_body(0);
    assert!(
        standard
            .world
            .physics
            .world
            .center_of_mass(body)
            .unwrap()
            .distance_to(Vec2::splat(radius))
            < radius
    );
}

#[test]
fn fractional_thrust_and_braking_scale_real_velocity_changes() {
    for braking in [false, true] {
        let mut changes = Vec::new();
        for amount in [0.0, 0.25, 0.5, 1.0] {
            let mut task = FlightTask::default();
            if braking {
                task.start.relative_velocity = Vec2::new(10.0, 0.0);
            }
            let mut env = FlightEnvironment::new(task).unwrap();
            let before = env.frame().velocity;
            env.step(FlightAction {
                thrust: if braking { 0.0 } else { amount },
                brake: if braking { amount } else { 0.0 },
                ..Default::default()
            })
            .unwrap();
            changes.push(env.frame().velocity - before);
            assert_eq!(
                env.observation().unwrap().previous_action.thrust,
                if braking { 0.0 } else { amount }
            );
        }
        assert!(changes[3].length() > 0.5);
        for (i, amount) in [0.0, 0.25, 0.5, 1.0].into_iter().enumerate() {
            assert!(
                (changes[i] - changes[3] * amount).length() < 0.0001,
                "braking={braking}, amount={amount}, changes={changes:?}"
            );
        }
    }
}

#[test]
fn partial_brake_interpolates_neutral_spin_damping_and_preserves_steering_authority() {
    for steering in [0.0, 1.0] {
        let mut changes = Vec::new();
        for brake in [0.0, 0.5, 1.0] {
            let mut task = FlightTask::default();
            task.start.spin = 1.5;
            let mut env = FlightEnvironment::new(task).unwrap();
            let before = env.frame().spin;
            env.step(FlightAction {
                steering,
                thrust: 0.0,
                brake,
            })
            .unwrap();
            changes.push(before - env.frame().spin);
        }
        assert!((changes[1] - (changes[0] + changes[2]) * 0.5).abs() < 0.00001);
        if steering == 0.0 {
            assert!(changes[0] < changes[1] && changes[1] < changes[2]);
        } else {
            assert_eq!(changes[0], changes[2]);
        }
    }
}

#[test]
fn analog_action_protocol_and_digital_release_are_unambiguous() {
    let analog = SurfaceFlightAction {
        thrust: 0.3,
        brake: 0.6,
    };
    assert_eq!(
        SurfaceFlightAction::decode(&analog.encode(PlayerId::PLAYER_1)),
        Some((PlayerId::PLAYER_1, analog))
    );
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        for action in [
            SurfaceFlightAction {
                thrust: value,
                brake: 0.0,
            },
            SurfaceFlightAction {
                thrust: 0.0,
                brake: value,
            },
        ] {
            assert!(SurfaceFlightAction::decode(&action.encode(PlayerId::PLAYER_1)).is_none());
        }
    }
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    env.step(FlightAction {
        steering: 0.0,
        thrust: 0.3,
        brake: 0.6,
    })
    .unwrap();
    assert_eq!(env.state.world.ships[0].thrust, 0.3);
    assert_eq!(env.state.world.ships[0].brake, 0.6);
    SurfaceSortieScenario::step(
        &mut env.state,
        &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
        FLIGHT_DT,
    );
    assert_eq!(env.state.world.ships[0].thrust, 0.0);
    assert_eq!(env.state.world.ships[0].brake, 0.0);
    // Re-arming requires release of analog controls too; a held throttle must
    // not resume immediately after a transfer or recovery.
    env.state.pilots[0].controls_armed = false;
    SurfaceSortieScenario::step(
        &mut env.state,
        &[analog.encode(PlayerId::PLAYER_1)],
        FLIGHT_DT,
    );
    assert!(!env.state.pilots[0].controls_armed);
    assert_eq!(env.state.world.ships[0].thrust, 0.0);
    assert_eq!(env.state.world.ships[0].brake, 0.0);
    SurfaceSortieScenario::step(
        &mut env.state,
        &[SurfaceFlightAction::default().encode(PlayerId::PLAYER_1)],
        FLIGHT_DT,
    );
    assert!(env.state.pilots[0].controls_armed);
}

#[test]
fn fractional_trace_replays_exactly_and_old_or_invalid_actions_are_rejected() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    for i in 0..24 {
        env.step(FlightAction {
            steering: 0.15,
            thrust: i as f32 / 24.0,
            brake: 0.37,
        })
        .unwrap();
    }
    let json = serde_json::to_string(env.trace()).unwrap();
    let trace: FlightTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(
        FlightEnvironment::replay(&trace).unwrap().trace(),
        env.trace()
    );
    let mut old = trace.clone();
    old.version = 1;
    assert!(FlightEnvironment::replay(&old).is_err());
    assert!(
        serde_json::from_str::<FlightAction>(r#"{"steering":0,"thrust":true,"brake":false}"#)
            .is_err()
    );
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        for action in [
            FlightAction {
                thrust: value,
                ..Default::default()
            },
            FlightAction {
                brake: value,
                ..Default::default()
            },
        ] {
            let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
            let initial = env.frame().clone();
            assert_eq!(
                env.step(action).unwrap().progress.outcome,
                Some(FlightOutcome::ControllerError)
            );
            assert_eq!(env.frame(), &initial);
        }
    }
}

#[test]
fn early_hold_failure_has_distinct_terminal_and_version_without_changing_motion() {
    let normal = settled_task();
    let mut early = normal.clone();
    early.hold_failure = Some(HoldFailure::FirstToleranceExitV1);
    let mut baseline = FlightEnvironment::new(normal).unwrap();
    let mut holding = FlightEnvironment::new(early).unwrap();
    assert_eq!(holding.frame(), baseline.frame());
    assert_eq!(holding.trace().version, FLIGHT_HOLD_ENVIRONMENT_VERSION);
    assert_eq!(baseline.trace().version, FLIGHT_ENVIRONMENT_VERSION);
    for _ in 0..30 {
        let command = FlightAction {
            steering: 0.0,
            thrust: 0.5,
            brake: 0.5,
        };
        let a = baseline.step(command).unwrap();
        let b = holding.step(command).unwrap();
        assert_eq!(a.frame, b.frame);
        assert_eq!(a.evidence, b.evidence);
        if b.progress.outcome.is_some() {
            assert_eq!(b.progress.outcome, Some(FlightOutcome::HoldBroken));
            assert_eq!(a.progress.outcome, None);
            assert!(b.frame.as_ref().unwrap().errors.velocity > 1.0);
            break;
        }
    }
    assert_eq!(holding.progress().outcome, Some(FlightOutcome::HoldBroken));
    assert!(holding.step(FlightAction::default()).is_err());
    assert_eq!(
        FlightEnvironment::replay(holding.trace())
            .unwrap()
            .progress(),
        holding.progress()
    );
    assert!(FlightEnvironment::replay(baseline.trace()).is_ok());
    let mut corrupt = holding.trace().clone();
    corrupt.version = FLIGHT_ENVIRONMENT_VERSION;
    assert!(FlightEnvironment::replay(&corrupt).is_err());
    corrupt.version = FLIGHT_HOLD_ENVIRONMENT_VERSION;
    corrupt.task.hold_failure = None;
    assert!(FlightEnvironment::replay(&corrupt).is_err());
}

#[test]
fn early_hold_keeps_full_dwell_and_physical_failure_precedence() {
    let mut task = settled_task();
    task.hold_failure = Some(HoldFailure::FirstToleranceExitV1);
    let mut env = FlightEnvironment::new(task.clone()).unwrap();
    for tick in 1..=30 {
        let step = env.step(FlightAction::default()).unwrap();
        assert_eq!(step.progress.dwell_ticks, tick);
        assert_eq!(
            step.progress.outcome,
            (tick == 30).then_some(FlightOutcome::Success)
        );
    }
    let good = ArrivalErrors {
        position: 0.0,
        velocity: 0.0,
        spin: 0.0,
    };
    for bad in [
        ArrivalErrors {
            position: 3.1,
            ..good
        },
        ArrivalErrors {
            velocity: 1.1,
            ..good
        },
        ArrivalErrors { spin: 0.21, ..good },
    ] {
        let mut progress = FlightProgress::default();
        progress.advance(good, bad, FlightEvidence::default(), &task);
        assert_eq!(progress.outcome, Some(FlightOutcome::HoldBroken));
        let mut progress = FlightProgress::default();
        progress.advance(
            good,
            bad,
            FlightEvidence {
                collision: true,
                ..Default::default()
            },
            &task,
        );
        assert_eq!(progress.outcome, Some(FlightOutcome::Collision));
    }
}

#[test]
fn early_hold_contract_is_explicit_and_requires_zero_error_start() {
    let normal = settled_task();
    let value = serde_json::to_value(&normal).unwrap();
    assert!(value.get("hold_failure").is_none());
    assert_eq!(serde_json::from_value::<FlightTask>(value).unwrap(), normal);
    let mut early = normal;
    early.hold_failure = Some(HoldFailure::FirstToleranceExitV1);
    assert_eq!(
        serde_json::to_value(&early).unwrap()["hold_failure"],
        "hold-first-exit-v1"
    );
    early.goal_offset.x = 0.01;
    assert!(FlightEnvironment::new(early).is_err());
}

#[test]
fn spinning_practice_goal_rests_in_the_surface_frame_and_has_its_own_version() {
    let mut task = FlightTask::default();
    task.start.planet_spin = 0.015;
    task.goal_offset = Vec2::new(40.0, 10.0);
    task.goal_velocity = Vec2::new(2.0, 0.0);
    let mut env = FlightEnvironment::new(task.clone()).unwrap();
    assert_eq!(env.trace().version, FLIGHT_SPINNING_ENVIRONMENT_VERSION);
    let initial = env.frame().clone();
    let planet = motion::SurfaceFrame::read(&env.state.world.physics, 0);
    assert!((planet.angular_velocity - 0.015).abs() < 1e-4);
    let offset = initial.target_position - planet.position;
    let carried = Vec2::new(-offset.y, offset.x) * planet.angular_velocity;
    let expected = planet.linear_velocity + task.goal_velocity + carried;
    assert!((initial.target_velocity - expected).length() < 1e-4);
    for _ in 0..60 {
        env.step(FlightAction::default()).unwrap();
    }
    let later = env.frame();
    let planet = motion::SurfaceFrame::read(&env.state.world.physics, 0);
    let turned = (offset + task.goal_velocity).rotate_radians(planet.angular_velocity);
    assert!((later.target_position - planet.position - turned).length() < 0.05);
    assert!(
        (later.target_velocity - initial.target_velocity).length() > 0.01,
        "the goal's velocity turns with the surface"
    );
    // A still planet keeps the plain version and the world-fixed goal.
    let mut still = task;
    still.start.planet_spin = 0.0;
    let env = FlightEnvironment::new(still).unwrap();
    assert_eq!(env.trace().version, FLIGHT_ENVIRONMENT_VERSION);
}
