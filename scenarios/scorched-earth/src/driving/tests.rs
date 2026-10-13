use super::*;

pub(crate) fn step(state: &mut ScorchedState, controls: Controls) {
    ScorchedScenario::step(
        state,
        &[ScorchedAction::Controls(controls).encode()],
        Duration::from_secs_f64(1.0 / 60.0),
    );
}

pub(crate) fn fixture(
    shape: GrainShape,
    strength: BankStrength,
    floor: impl Fn(f32) -> f32,
) -> ScorchedState {
    let config = ScorchedConfig {
        shape,
        cpu_driving: false,
        ..Default::default()
    }
    .with_destruction(strength);
    let mut state = ScorchedState::new(config, 42);
    state.physics.remove_entity(GROUND);
    let terrain = Terrain::generate(
        193,
        129,
        CELL_SIZE,
        vec![Material {
            id: DIRT,
            hardness: 100,
        }],
        |c| {
            let x = (c.x as f32 + 0.5 - 193.0 * 0.5) * CELL_SIZE;
            let y = (c.y as f32 + 0.5 - 129.0 * 0.5) * CELL_SIZE;
            if y <= floor(x) {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    state.initial_cells = terrain
        .cells()
        .iter()
        .filter(|c| c.material != MaterialId::VOID)
        .count() as u64;
    let geometry = TerrainGeometry::with_surface(&terrain, TerrainSurface::Interpolated);
    let assembly = TerrainAssembly::insert(
        &mut state.physics,
        GROUND,
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec {
            surface: TerrainSurface::Interpolated,
            ..Default::default()
        },
    )
    .unwrap();
    state.terrain = vec![TerrainFragment {
        id: GROUND,
        hash: terrain.hash(),
        terrain,
        geometry,
        assembly,
        edited_chunks: Vec::new(),
    }];
    for (i, x) in [-20.0, 20.0].into_iter().enumerate() {
        let position = Vec2::new(x, floor(x) + 1.5);
        state
            .physics
            .set_pose(state.tanks[i].body, position, 0.0, true);
        state
            .physics
            .set_velocity(state.tanks[i].body, Vec2::ZERO, 0.0, true);
        state.tanks[i].next_shot = u64::MAX;
        state.driving[i] = TankDrive::new(position);
    }
    for _ in 0..90 {
        step(&mut state, Controls::default());
    }
    state
}

#[test]
fn drive_stop_reverse_conserves_and_replays_both_shapes_and_strengths() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut state = fixture(shape, strength, |_| -5.0);
            let body = state.tanks[0].body;
            let initial = state.physics.motion(body).unwrap().position;
            let controls = Controls {
                drive: 1.0,
                ..Default::default()
            };
            for _ in 0..120 {
                step(&mut state, controls);
            }
            let mut replay = state.clone();
            for _ in 0..60 {
                step(&mut state, controls);
                step(&mut replay, controls);
                assert_eq!(state.observation_hash(), replay.observation_hash());
            }
            let moving = state.physics.motion(body).unwrap();
            assert!(
                moving.position.x > initial.x + 5.0,
                "{shape:?} {strength:?}: {moving:?}"
            );
            assert!(moving.position.y > -4.3, "hull must stay above the floor");
            for _ in 0..60 {
                step(&mut state, Controls::default());
            }
            let stopped = state.physics.motion(body).unwrap();
            assert!(stopped.linear_velocity.length() < 0.05, "{stopped:?}");
            assert!(stopped.position.x - moving.position.x < 1.5);
            for _ in 0..120 {
                step(
                    &mut state,
                    Controls {
                        drive: -1.0,
                        ..Default::default()
                    },
                );
            }
            assert!(state.physics.motion(body).unwrap().position.x < stopped.position.x - 3.0);
            state.audit().unwrap();
        }
    }
}

#[test]
fn cpu_drives_both_tanks_and_stops_to_fire() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = fixture(shape, BankStrength::Off, |_| -5.0);
        state.config.demo = true;
        state.config.cpu_driving = true;
        for tank in &mut state.tanks {
            tank.next_shot = state.tick + 240;
        }
        let before = state
            .tanks
            .map(|t| state.physics.motion(t.body).unwrap().position.x);
        let mut firing_speeds = Vec::new();
        for _ in 0..250 {
            let shots = state.tanks.map(|t| t.shots);
            step(&mut state, Controls::default());
            for (i, previous) in shots.into_iter().enumerate() {
                if state.tanks[i].shots > previous {
                    firing_speeds.push(
                        state
                            .physics
                            .motion(state.tanks[i].body)
                            .unwrap()
                            .linear_velocity
                            .length(),
                    );
                }
            }
        }
        let after = state
            .tanks
            .map(|t| state.physics.motion(t.body).unwrap().position.x);
        assert!(
            after[0] > before[0] + 2.0 && after[1] < before[1] - 2.0,
            "{before:?} -> {after:?}"
        );
        assert_eq!(firing_speeds.len(), 2, "both CPUs must actually shoot");
        assert!(firing_speeds.iter().all(|v| *v < 0.7), "{firing_speeds:?}");
        assert!(
            state
                .driving
                .iter()
                .all(|d| d.diagnostics.powered_ticks > 60)
        );
        state.audit().unwrap();
    }
}

#[test]
fn cpu_refuses_a_cliff_and_rechecks_changed_ground() {
    let mut state = fixture(GrainShape::Hexagon, BankStrength::Off, |x| {
        if x < 0.0 { -5.0 } else { -20.0 }
    });
    let body = state.tanks[0].body;
    state
        .physics
        .set_pose(body, Vec2::new(-2.5, -4.0), 0.0, true);
    state.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
    for _ in 0..90 {
        step(&mut state, Controls::default());
    }
    assert!(state.has_track_support(0));
    assert!(!state.safe_drive_direction(0, 1.0));
    state.config.demo = true;
    state.config.cpu_driving = true;
    // The far enemy and blocked launch encourage approach, not range holding.
    state
        .physics
        .set_pose(state.tanks[1].body, Vec2::new(35.0, -19.0), 0.0, true);
    let before = state.physics.motion(body).unwrap().position.x;
    for _ in 0..180 {
        step(&mut state, Controls::default());
    }
    assert!((state.physics.motion(body).unwrap().position.x - before).abs() < 0.15);
    assert_eq!(state.driving[0].diagnostics.mode, "blocked ground");

    let mut flat = fixture(GrainShape::Round, BankStrength::Off, |_| -5.0);
    let position = flat.physics.motion(flat.tanks[0].body).unwrap().position;
    assert!(flat.safe_drive_direction(0, 1.0));
    flat.explode(position + Vec2::new(3.0, -2.0));
    assert!(
        !flat.safe_drive_direction(0, 1.0),
        "the old floor must not remain a route after a crater"
    );
    flat.audit().unwrap();
}

#[test]
fn cpu_backs_off_after_repeated_no_progress() {
    // A deliberately underpowered assembly stalls on these steps even with
    // valid support. Repeated attempts must still back off.
    let mut state = ScorchedState::new(
        ScorchedConfig {
            suspension: true,
            wheel_tuning: WheelTuning::REFERENCE,
            course: Some(DrivingCourse::Slope),
            demo: true,
            ..Default::default()
        },
        42,
    );
    let mut cooldown_ticks = 0;
    let mut stopped_in_cooldown = false;
    for _ in 0..1200 {
        step(&mut state, Controls::default());
        let drive = state.driving[0].diagnostics;
        if drive.mode == "cooldown" {
            cooldown_ticks += 1;
            stopped_in_cooldown |= drive.track_speed == 0.0;
        }
    }
    assert!(cooldown_ticks >= 120, "{cooldown_ticks}");
    assert!(stopped_in_cooldown);
    assert!(state.driving[0].diagnostics.planning_checks < 40);
    assert!(
        state
            .physics
            .motion(state.tanks[0].body)
            .unwrap()
            .position
            .x
            < -16.0
    );
}

#[test]
fn height_profiles_admit_a_cell_step_and_refuse_tall_steps_drops_and_steep_slopes() {
    for direction in [-1.0, 1.0] {
        let player = usize::from(direction < 0.0);
        for height in [0.5, 1.5, -2.0] {
            let state = fixture(GrainShape::Round, BankStrength::Off, |x| {
                if x * direction >= -18.0 {
                    -5.0 + height
                } else {
                    -5.0
                }
            });
            let check = state.check_drive_direction(player, direction);
            assert_eq!(
                check.reason.is_none(),
                height == 0.5,
                "{direction} {height}: {check:?}"
            );
        }
        let mut state = fixture(GrainShape::Round, BankStrength::Off, |x| {
            -5.0 + (x * direction + 22.0).clamp(0.0, 12.0)
        });
        state.physics.set_pose(
            state.tanks[player].body,
            Vec2::new(-20.0 * direction, -1.5),
            0.0,
            true,
        );
        let check = state.check_drive_direction(player, direction);
        assert_eq!(
            check.reason,
            Some("steep profile"),
            "{direction}: {check:?}"
        );
    }
}

#[test]
fn cpu_finishes_small_step_courses_and_stays_before_the_trench_with_both_gears() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for suspension in [false, true] {
            for course in [
                DrivingCourse::Flat,
                DrivingCourse::Step,
                DrivingCourse::Trench,
            ] {
                let mut state = ScorchedState::new(
                    ScorchedConfig {
                        shape,
                        suspension,
                        course: Some(course),
                        demo: true,
                        ..Default::default()
                    },
                    42,
                );
                for _ in 0..900 {
                    step(&mut state, Controls::default());
                }
                let drive = crate::driving_lab::snapshot(&state, 0, 1.0);
                if course == DrivingCourse::Trench {
                    assert!(
                        drive.position[0] < -20.0,
                        "{shape:?} {suspension}: {drive:?}"
                    );
                    assert!(drive.pitch_degrees.abs() < 10.0 && drive.supported);
                    assert_eq!(drive.driving.mode, "blocked ground");
                } else {
                    assert!(
                        drive.position[0] >= course.finish_x(),
                        "{shape:?} {suspension}: {drive:?}"
                    );
                }
                state.audit().unwrap();
            }
        }
    }
}

#[test]
fn frozen_two_probe_failures_continue_with_the_candidate_without_resetting() {
    use crate::driving_lab::{physics_snapshot, snapshot, step_reference_support_trial};
    // Source identities and original hashes are retained in the driving-course
    // receipt. These assertions prevent an improved initialization masquerading
    // as a successful continuation of the old blocked state.
    for (shape, suspension, tick, expected) in [
        (GrainShape::Round, false, 110, 0xbda8ac8a19b60dde),
        (GrainShape::Hexagon, false, 110, 0x2cdeab9d8283fa5e),
        (GrainShape::Round, true, 124, 0x1f184cbfc30b6952),
        (GrainShape::Hexagon, true, 124, 0x051dc2ddda702a82),
    ] {
        let mut original = ScorchedState::new(
            ScorchedConfig {
                shape,
                suspension,
                wheel_tuning: WheelTuning::REFERENCE,
                demo: true,
                ..Default::default()
            },
            42,
        );
        for _ in 0..tick {
            step_reference_support_trial(&mut original, 0, None, false);
        }
        assert_eq!(
            original.observation_hash(),
            expected,
            "{shape:?} {suspension}"
        );
        let mut candidate = original.clone();
        let native = physics_snapshot(&original);
        assert_eq!(physics_snapshot(&candidate), native);
        let x = snapshot(&original, 0, 1.0).position[0];
        for _ in 0..720 {
            step_reference_support_trial(&mut original, 0, None, false);
            step_reference_support_trial(&mut candidate, 0, None, true);
        }
        let moved = snapshot(&candidate, 0, 1.0);
        assert!(
            moved.position[0] > x + 5.0,
            "{shape:?} {suspension}: {moved:?}"
        );
        assert!(moved.position[0] > snapshot(&original, 0, 1.0).position[0] + 4.0);
        assert!(moved.health > 0.0 && moved.pitch_degrees.abs() < 60.0);
        original.audit().unwrap();
        candidate.audit().unwrap();
    }
}

#[test]
fn drive_chord_does_not_select_and_transfer_requires_neutral() {
    let mut state = fixture(GrainShape::Round, BankStrength::Off, |_| -5.0);
    let body = state.tanks[0].body;
    let start = state.physics.motion(body).unwrap().position.x;
    step(
        &mut state,
        Controls {
            select: true,
            ..Default::default()
        },
    );
    for _ in 0..60 {
        step(
            &mut state,
            Controls {
                select: true,
                drive: 1.0,
                ..Default::default()
            },
        );
    }
    step(&mut state, Controls::default());
    assert_eq!(state.selected, 0);
    assert!(state.physics.motion(body).unwrap().position.x > start + 1.0);
    state.command(Command::Select);
    for _ in 0..60 {
        step(
            &mut state,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
    }
    assert_eq!(state.driving[1].diagnostics.powered_ticks, 0);
    step(&mut state, Controls::default());
    step(
        &mut state,
        Controls {
            drive: 1.0,
            ..Default::default()
        },
    );
    assert!(state.driving[1].diagnostics.track_speed > 0.0);
    ScorchedScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.driving[1].diagnostics.track_speed, 0.0);
    step(
        &mut state,
        Controls {
            drive: 1.0,
            ..Default::default()
        },
    );
    assert_eq!(state.driving[1].diagnostics.track_speed, 0.0);
}

#[test]
fn drive_packets_retain_legacy_controls_and_reject_nonfinite_axes() {
    let controls = Controls {
        drive: -1.0,
        aim: 0.5,
        fire: true,
        ..Default::default()
    };
    let encoded = ScorchedAction::Controls(controls).encode();
    assert_eq!(
        ScorchedAction::decode(&encoded),
        Some(ScorchedAction::Controls(controls))
    );
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(
            ScorchedAction::decode(
                &ScorchedAction::Controls(Controls {
                    drive: value,
                    ..controls
                })
                .encode()
            )
            .is_none()
        );
    }
    let Action::Scenario { payload, .. } = encoded else {
        panic!()
    };
    for length in [12, 14] {
        let Some(ScorchedAction::Controls(old)) =
            ScorchedAction::decode(&Action::scenario(1, payload[..length].to_vec()))
        else {
            panic!()
        };
        assert_eq!(old.drive, 0.0);
        assert_eq!(old.aim, 0.5);
        assert!(old.fire);
    }
}

#[test]
fn tracks_climb_a_gentle_slope_and_do_not_drive_on_a_steep_one() {
    let mut state = fixture(GrainShape::Round, BankStrength::Off, |x| {
        -5.0 + x * 25.0_f32.to_radians().tan()
    });
    let body = state.tanks[0].body;
    let before = state.physics.motion(body).unwrap().position;
    for _ in 0..180 {
        step(
            &mut state,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
    }
    let after = state.physics.motion(body).unwrap().position;
    assert!(
        after.x > before.x + 3.0 && after.y > before.y + 1.0,
        "{before:?} -> {after:?}"
    );
    state.audit().unwrap();

    let mut steep = fixture(GrainShape::Round, BankStrength::Off, |x| {
        x * 50.0_f32.to_radians().tan()
    });
    assert!(!steep.has_track_support(0));
    let mut control = steep.clone();
    for _ in 0..60 {
        step(
            &mut steep,
            Controls {
                drive: 1.0,
                ..Default::default()
            },
        );
        step(&mut control, Controls::default());
        assert_eq!(
            steep.physics.snapshot_bytes().unwrap(),
            control.physics.snapshot_bytes().unwrap()
        );
    }
}

#[test]
fn driving_through_crater_debris_preserves_material_and_replay() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut state = fixture(shape, strength, |_| -5.0);
            let position = state.physics.motion(state.tanks[0].body).unwrap().position;
            state.explode(position + Vec2::new(5.0, -2.0));
            assert!(state.loose_cells() > 0);
            let mut replay = state.clone();
            for tick in 0..480 {
                let controls = Controls {
                    drive: if tick < 240 { 1.0 } else { 0.0 },
                    ..Default::default()
                };
                step(&mut state, controls);
                step(&mut replay, controls);
                if tick % 60 == 0 {
                    state.audit().unwrap();
                    assert_eq!(state.observation_hash(), replay.observation_hash());
                }
            }
            assert!(state.driving[0].diagnostics.powered_ticks > 30);
        }
    }
}

#[test]
fn pointer_driving_releases_on_drag_cancel_and_pause() {
    use engine_common::PointerAction;
    let mut state = fixture(GrainShape::Hexagon, BankStrength::Off, |_| -5.0);
    let pointer = |phase, x| {
        Action::Pointer(PointerAction {
            phase,
            position: RenderPoint::new(x, -30.0),
        })
    };
    ScorchedScenario::step(
        &mut state,
        &[pointer(PointerPhase::Press, -20.0)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
    for _ in 0..40 {
        step(&mut state, Controls::default());
    }
    assert!(state.driving[0].diagnostics.track_speed > 2.0);
    ScorchedScenario::step(
        &mut state,
        &[pointer(PointerPhase::Drag, 40.0)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
    assert_eq!(state.pointer_drive, 0.0);
    for phase in [PointerPhase::Cancel, PointerPhase::Release] {
        ScorchedScenario::step(
            &mut state,
            &[pointer(PointerPhase::Press, -20.0), pointer(phase, -20.0)],
            Duration::from_secs_f64(1.0 / 60.0),
        );
        assert_eq!(state.pointer_drive, 0.0);
    }
    ScorchedScenario::step(
        &mut state,
        &[pointer(PointerPhase::Press, -20.0)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
    ScorchedScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.pointer_drive, 0.0);
    assert_eq!(state.driving[0].diagnostics.track_speed, 0.0);
}
