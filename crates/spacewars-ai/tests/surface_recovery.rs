use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieAction, SurfaceSortieScenario,
        SurfaceSortieState,
        impact::{RecoveryDisruption, SurfaceImpactAction},
        recovery_sensors::RecoveryTaskObservationV1,
    },
};
use spacewars_ai::{
    BrainReset,
    flight_pilot::FlightIntent,
    recovery_pilot::RulePilotV3,
    recovery_task::{RecoverShipTask, RecoveryGoal, TaskStatus},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);

#[test]
fn tipped_pod_uses_shared_lift_and_releases_before_rearming() {
    use scenario_spacewars::surface_sortie::pod_righting::PodRightingObservation;
    let (mut task, mut o) = airborne_pod();
    o.flight.pilot.ship.angle = 1.7;
    o.flight.pilot.landing.altitude = 0.2;
    o.pod_righting = Some(PodRightingObservation {
        eligible: true,
        ..Default::default()
    });
    let intent = task.step(&o);
    assert!(intent.controls.brake_held && intent.controls.primary_held);
    assert!(!intent.controls.interact_held);
    let telemetry = task.telemetry().clone();
    assert_eq!(task.step(&o), intent);
    assert_eq!(task.telemetry(), &telemetry);
    let mut clone = task.clone();
    o.flight.pilot.tick += 1;
    o.pod_righting.as_mut().unwrap().needs_release = true;
    assert_eq!(task.step(&o), clone.step(&o));
    assert!(!task.step(&o).controls.primary_held);
    o.flight.pilot.tick += 1;
    o.pod_righting.as_mut().unwrap().remaining_seconds = 1.0;
    assert!(task.step(&o).controls.primary_held);
    assert_eq!(task.telemetry().started_tick, telemetry.started_tick);
}

// Observation fixtures isolate the task's progress/deadline contract. Physical
// collision and full recovery are exercised separately below and in the soak.
fn airborne_pod() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    SurfaceSortieScenario::step(&mut s, &[], DT);
    let mut o = s.recovery_task_observation(0, None);
    let p = &mut o.flight.pilot;
    p.controls_armed = true;
    p.ship_form = ShipForm::EscapePod;
    p.ship_available = true;
    p.location = PilotLocation::Aboard(p.vehicle);
    p.planet.motion.spin = 0.0;
    p.planet.motion.velocity = Vec2::ZERO;
    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 100.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.spin = 0.0;
    p.ship.angle = 0.0;
    p.gravity = Vec2::ZERO;
    p.landing.phase = LandingPhase::Flying;
    p.landing.altitude = 26.0;
    p.landing.assist_strength = 0.0;
    (
        RecoverShipTask::new(BrainReset {
            actor: p.owner,
            episode_seed: 42,
        }),
        o,
    )
}

fn rejected_pod_site() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (mut task, mut o) = airborne_pod();
    o.sites.truncate(1);
    assert_eq!(o.sites.len(), 1);
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert!(task.site_request().is_some());
    // A stationary approach exhausts the existing ten-second progress budget.
    o.flight.pilot.tick = 614;
    task.step(&o);
    assert_eq!(task.telemetry().landing_retries, 1);
    assert_eq!(task.telemetry().landing_rejections.len(), 1);
    (task, o)
}

#[test]
fn landing_retry_reuses_pod_righting_and_accepts_actual_hatch_access() {
    use scenario_spacewars::surface_sortie::{
        TransferResult, pod_righting::PodRightingObservation,
    };
    let (mut task, mut o) = rejected_pod_site();
    let started = task.telemetry().started_tick;
    o.flight.pilot.tick += 1;
    o.flight.pilot.ship.angle = 1.7;
    o.flight.pilot.landing.altitude = 1.0;
    o.pod_righting = Some(PodRightingObservation {
        eligible: true,
        ..Default::default()
    });
    let intent = task.step(&o);
    assert!(intent.controls.primary_held && intent.controls.brake_held);
    assert_eq!(task.telemetry().goal, RecoveryGoal::StabilizePod);
    assert_eq!(task.telemetry().stabilization.as_ref().unwrap().attempts, 2);
    let telemetry = task.telemetry().clone();
    assert_eq!(task.step(&o), intent);
    assert_eq!(task.telemetry(), &telemetry);
    let mut clone = task.clone();
    o.flight.pilot.tick += 1;
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::Ready;
    o.sites.clear();
    let intent = task.step(&o);
    assert_eq!(intent, clone.step(&o));
    assert!(intent.controls.interact_held && !intent.controls.primary_held);
    assert_eq!(task.telemetry().goal, RecoveryGoal::ExitPod);
    assert_eq!(task.telemetry().started_tick, started);
}

#[test]
fn failed_footing_is_reconsidered_after_cooldown_or_local_geometry_change() {
    for changed in [false, true] {
        let (mut task, mut o) = rejected_pod_site();
        for tick in 615..=628 {
            o.flight.pilot.tick = tick;
            task.step(&o);
        }
        assert_eq!(task.telemetry().goal, RecoveryGoal::SurveyPod);
        assert!(task.site_request().is_none());
        assert_eq!(task.telemetry().site_search_since, Some(628));
        let retry_after = task.telemetry().landing_rejections[0].retry_after_tick;
        o.flight.pilot.tick = 629;
        o.flight.pilot.queries_ready = false;
        task.step(&o);
        assert_eq!(task.telemetry().status, TaskStatus::Running);
        o.flight.pilot.queries_ready = true;
        if changed {
            o.sites[0].local_position.x += 0.6;
            o.sites[0].revision += 1;
            o.flight.pilot.tick += 1;
        } else {
            o.flight.pilot.tick = retry_after;
        }
        task.step(&o);
        assert_eq!(task.site_request(), Some(o.sites[0].id));
        assert_eq!(task.telemetry().goal, RecoveryGoal::LandPod);
        assert_eq!(task.telemetry().landing_retries, 1);
        assert_eq!(task.telemetry().site_search_since, None);
        assert_eq!(task.telemetry().started_tick, Some(0));
    }
}

#[test]
fn empty_pod_survey_waits_for_landing_but_retains_a_finite_failure_budget() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = airborne_pod();
    o.sites.clear();
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().goal, RecoveryGoal::SurveyPod);
    let mut landed = task.clone();
    o.flight.pilot.tick = 13 + 6 * 60;
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(landed.step(&o).controls.interact_held);
    assert_eq!(landed.telemetry().goal, RecoveryGoal::ExitPod);
    o.flight.pilot.landing.phase = LandingPhase::Flying;
    o.flight.pilot.tick = 13 + 15 * 60 + 1;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("no suitable pod landing site")
    );
    let mut expired = landed.clone();
    o.flight.pilot.tick = 120 * 60 + 1;
    expired.step(&o);
    assert_eq!(
        expired.telemetry().reason,
        Some("recovery exceeded two-minute task budget")
    );
}

#[test]
fn exhausted_pod_retries_allow_a_ready_exit_but_cannot_restart_forever() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = airborne_pod();
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::ExitBlocked;
    for retry in 0..4 {
        o.flight.pilot.tick = retry * 122;
        task.step(&o);
        o.flight.pilot.tick += 121;
        task.step(&o);
    }
    assert_eq!(task.telemetry().landing_retries, 4);
    let mut ready = task.clone();
    o.flight.pilot.tick += 1;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("pod landing retries exhausted")
    );
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(ready.step(&o).controls.interact_held);
    for tick in 600..800 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().landing_retries, 4);
}

#[test]
fn rebuild_flight_keeps_its_destination_until_ownership_changes() {
    use scenario_spacewars::surface_sortie::jetpack::{
        CrossingAnchor, CrossingDirection, CrossingPlan, JetpackNavigationObservation,
    };
    use spacewars_ai::ground_task::GroundDestination;
    let (mut task, mut o) = rebuild_relocation_fixture();
    o.ground.as_mut().unwrap().edges.clear();
    let site = o.rebuild.as_ref().unwrap().site.unwrap();
    o.jetpack = Some(JetpackNavigationObservation {
        charge: 1.0,
        burning: false,
        burn_seconds: 0.0,
        gravity: -Vec2::Y * 18.0,
        surveyed: true,
        crossing: None,
        terrain_crossings: vec![CrossingPlan {
            planet: site.planet,
            revision: site.revision,
            direction: CrossingDirection::Left,
            start: Vec2::new(0.0, 60.0),
            destination: site.position,
            cruise_radius: 64.0,
            anchor: CrossingAnchor::GroundGap { from: 0, to: 2 },
        }],
    });
    task.step(&o);
    o.rebuild = None;
    for tick in 1..=4 {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        let mut replay = task.clone();
        assert_eq!(task.step(&o), replay.step(&o));
        let ground = task.telemetry().ground.as_ref().unwrap();
        assert!(ground.crossing.is_some());
        assert_eq!(
            ground.destination,
            GroundDestination::Rebuild {
                planet: site.planet,
                position: site.position
            }
        );
    }
    o.flight.pilot.tick = 5;
    o.flight.pilot.planet.claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2);
    task.step(&o);
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().destination,
        GroundDestination::Flag
    );
}

fn rebuild_relocation_fixture() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::{
        SurfaceRecoveryStatus,
        ground_navigation::{GroundEdge, GroundEdgeKind, GroundMap, GroundNode},
        pilot::PilotMotion,
        rebuild_placement::{RebuildRelocationSurvey, RebuildStandingSite},
    };
    let (task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.location = PilotLocation::OnFoot;
    p.ship_available = false;
    p.balanced = true;
    p.supported_planet = Some(p.planet.index);
    p.actor_up = Vec2::Y;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.angle = 0.0;
    p.actor = Some(PilotMotion {
        position: Vec2::new(0.0, 60.9),
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
    });
    p.planet.claim.as_mut().unwrap().owner = Some(p.owner);
    p.recovery.as_mut().unwrap().status = SurfaceRecoveryStatus::HatchBlocked;
    o.ground = Some(GroundMap {
        version: 1,
        actor: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: 0,
        nodes: (0..3)
            .map(|id| GroundNode {
                id,
                position: Vec2::new(f32::from(id) * 2.0, 60.0),
                normal: Vec2::Y,
            })
            .collect(),
        edges: (0..2)
            .map(|from| GroundEdge {
                from,
                to: from + 1,
                kind: GroundEdgeKind::Walk,
                length: 2.0,
            })
            .collect(),
        rejected: Vec::new(),
    });
    o.rebuild = Some(RebuildRelocationSurvey {
        tick: 0,
        checked: 2,
        attempts: Vec::new(),
        site: Some(RebuildStandingSite {
            planet: p.planet.index,
            revision: p.planet.revision,
            position: Vec2::new(4.0, 60.0),
            walk_length: 4.0,
            flight_length: 0.0,
            jetpack_flights: 0,
            hatch_walk_length: 0.0,
        }),
    });
    (task, o)
}

#[test]
fn rebuild_relocation_uses_measured_walk_controls_and_invalidates_with_terrain() {
    let (mut task, mut o) = rebuild_relocation_fixture();
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().relocations, 1);
    let before = task.telemetry().clone();
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry(), &before);
    o.rebuild = None;
    for tick in 1..=2 {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        let action = task.step(&o);
        if tick == 2 {
            assert!(action.controls.horizontal > 0.0);
            assert!(!action.controls.interact_held);
        }
    }
    o.flight.pilot.tick = 3;
    o.flight.pilot.planet.revision += 1;
    o.ground = None;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert!(task.telemetry().relocation_site.is_none());
    assert_eq!(task.telemetry().invalidations, 1);
    o.flight.pilot.tick = 304;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
    o.flight.pilot.tick = 305;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
}

#[test]
fn progressing_high_spin_pod_can_take_longer_than_fifteen_seconds() {
    for turn_acceleration in [3.0, 6.0] {
        let (mut task, mut o) = airborne_pod();
        o.flight.flight.limits.turn_acceleration = turn_acceleration;
        // Both actuators are making observable progress, at their real limits.
        for tick in 0..=40 * 60 {
            let p = &mut o.flight.pilot;
            p.tick = tick;
            p.ship.spin = (114.0 - turn_acceleration * tick as f32 / 60.0).max(0.0);
            p.ship.velocity = Vec2::X * (600.0 - 40.0 * tick as f32 / 60.0).max(0.0);
            let intent = task.step(&o);
            assert!(intent.controls.brake_held);
            assert!(!intent.controls.interact_held);
            assert_eq!(task.telemetry().status, TaskStatus::Running);
            let telemetry = task.telemetry().clone();
            assert_eq!(task.step(&o), intent);
            assert_eq!(task.telemetry(), &telemetry);
            let s = telemetry.stabilization.as_ref().unwrap();
            if let Some(settled) = s.settled_tick {
                assert!(settled > 15 * 60);
                assert!(s.relative_spin.abs() < 0.5);
                assert!(s.relative_speed < 6.0);
                break;
            }
        }
        assert!(
            task.telemetry()
                .stabilization
                .as_ref()
                .unwrap()
                .settled_tick
                .is_some()
        );
    }
}

#[test]
fn stalled_pod_blocks_and_a_brief_alignment_does_not_count_as_settled() {
    let (mut task, mut o) = airborne_pod();
    for tick in 0..=902 {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship.spin = 30.0;
        task.step(&o);
    }
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("pod stabilization stopped making progress")
    );
    assert!(
        task.telemetry()
            .stabilization
            .as_ref()
            .unwrap()
            .settled_tick
            .is_none()
    );

    let (mut task, mut o) = airborne_pod();
    for tick in 0..=24 {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship.spin = if tick == 10 { 1.0 } else { 0.0 };
        task.step(&o);
        let settled = task
            .telemetry()
            .stabilization
            .as_ref()
            .unwrap()
            .settled_tick;
        if tick < 23 {
            assert!(settled.is_none());
        } else {
            assert_eq!(settled, Some(23));
        }
    }
}

#[test]
fn another_strike_restarts_stabilization_without_restarting_task_budget() {
    let (mut task, mut o) = airborne_pod();
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().goal, RecoveryGoal::LandPod);
    assert!(task.site_request().is_some());
    let started = task.telemetry().started_tick;
    o.flight.pilot.tick = 14;
    o.flight.pilot.ship.spin = -50.0;
    o.flight.pilot.ship.velocity = Vec2::X * 120.0;
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::StabilizePod);
    assert!(task.site_request().is_none());
    assert_eq!(task.telemetry().stabilization.as_ref().unwrap().attempts, 2);
    assert_eq!(task.telemetry().started_tick, started);
    let mut copy = task.clone();
    o.flight.pilot.tick = 120 * 60 + 1;
    assert_eq!(task.step(&o), copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert_eq!(
        task.telemetry().reason,
        Some("recovery exceeded two-minute task budget")
    );
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    });
    assert!(task.telemetry().stabilization.is_none());
}

#[test]
fn real_strike_recovers_and_departs_in_both_seats_and_angles() {
    for seat in 0..2 {
        for oblique in [false, true] {
            let owner = PlayerId::from_index(seat).unwrap();
            let mut s = SurfaceSortieScenario::init_material(42, 2);
            let mut brain = RulePilotV3::new(BrainReset {
                actor: owner,
                episode_seed: 42,
            });
            let mut fired = false;
            for _ in 0..180 * 60 {
                let o = s.recovery_task_observation(seat, brain.site_request());
                let intent = brain.intent(&o);
                assert!(
                    intent.controls.horizontal.is_finite()
                        && intent.controls.horizontal.abs() <= 1.0
                );
                assert!(
                    !(intent.controls.primary_held
                        && intent.controls.brake_held
                        && intent.controls.interact_held)
                );
                if !o.flight.pilot.controls_armed {
                    assert_eq!(intent, FlightIntent::default());
                }
                let strike = !fired && brain.telemetry().flight.completed_tick.is_some();
                fired |= strike;
                let mut actions = intent.encode(owner).to_vec();
                actions.push(
                    SurfaceImpactAction {
                        held: strike,
                        oblique,
                        ..Default::default()
                    }
                    .encode(owner),
                );
                SurfaceSortieScenario::step(&mut s, &actions, DT);
                if brain.telemetry().departed_tick.is_some() {
                    break;
                }
            }
            assert!(
                brain.telemetry().departed_tick.is_some(),
                "seat={seat} oblique={oblique}: {:?}",
                brain.telemetry()
            );
            assert_eq!(brain.telemetry().completed_recoveries, 1);
            let recovery = s.observation(seat).recovery.unwrap();
            assert_eq!(
                (
                    recovery.ships_lost,
                    recovery.pod_ejections,
                    recovery.rebuilds
                ),
                (1, 1, 1)
            );
            assert_eq!(s.damage_observation(seat).last_source, Some("asteroid"));
            assert!(s.terrain_diagnostics().issues.is_empty());
        }
    }
}

fn stranded_on_foot() -> SurfaceSortieState {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut s, &[], DT);
    }
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        DT,
    );
    for _ in 0..240 {
        SurfaceSortieScenario::step(
            &mut s,
            &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
    }
    assert_eq!(s.location(0), PilotLocation::OnFoot);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction {
            held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        DT,
    );
    for _ in 0..120 {
        SurfaceSortieScenario::step(
            &mut s,
            &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
        if s.observation(0).recovery.unwrap().ships_lost > 0 {
            return s;
        }
    }
    panic!("asteroid did not destroy the parked ship");
}

#[test]
fn standalone_task_rebuilds_after_real_loss_and_flag_or_support_removal() {
    for disruption in [
        RecoveryDisruption::FlagFooting,
        RecoveryDisruption::SpacelingSupport,
    ] {
        let mut s = stranded_on_foot();
        let mut task = RecoverShipTask::new(BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        });
        let mut edited = false;
        let mut last_survey = None;
        for _ in 0..120 * 60 {
            let o = s.recovery_task_observation(0, task.site_request());
            if o.rebuild.is_some() {
                last_survey = o.rebuild.clone();
            }
            let intent = task.step(&o);
            if !edited && o.flight.pilot.recovery.as_ref().unwrap().rebuild_progress > 0.3 {
                assert!(s.queue_recovery_disruption(0, disruption));
                edited = true;
            }
            SurfaceSortieScenario::step(&mut s, &intent.encode(PlayerId::PLAYER_1), DT);
            if task.telemetry().status == TaskStatus::Succeeded {
                break;
            }
        }
        assert!(edited);
        assert_eq!(
            task.telemetry().status,
            TaskStatus::Succeeded,
            "{disruption:?}: {:?}\nsurvey: {}\nstate: {:?}",
            task.telemetry(),
            serde_json::to_string_pretty(&last_survey).unwrap(),
            s.observation(0)
        );
        let r = s.observation(0).recovery.unwrap();
        assert!(r.rebuild_interruptions > 0);
        assert_eq!((r.ships_lost, r.pod_ejections, r.rebuilds), (1, 0, 1));
        assert!(matches!(s.location(0), PilotLocation::Aboard(_)));
    }
}

#[test]
fn task_replay_reset_identity_and_bounded_failure_contract() {
    let mut s = stranded_on_foot();
    let context = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    let mut task = RecoverShipTask::new(context);
    for _ in 0..120 {
        let o = s.recovery_task_observation(0, task.site_request());
        let intent = task.step(&o);
        let telemetry = task.telemetry().clone();
        assert_eq!(task.step(&o), intent);
        assert_eq!(task.telemetry(), &telemetry);
        SurfaceSortieScenario::step(&mut s, &intent.encode(context.actor), DT);
    }
    let mut copy = task.clone();
    let mut other = s.clone();
    for _ in 0..120 {
        let a = s.recovery_task_observation(0, task.site_request());
        let b = other.recovery_task_observation(0, copy.site_request());
        assert_eq!(a, b);
        let intent = task.step(&a);
        assert_eq!(intent, copy.step(&b));
        SurfaceSortieScenario::step(&mut s, &intent.encode(context.actor), DT);
        SurfaceSortieScenario::step(&mut other, &intent.encode(context.actor), DT);
    }
    let mut wrong = s.recovery_task_observation(0, task.site_request());
    task.step(&wrong);
    wrong.flight.pilot.owner = PlayerId::PLAYER_2;
    assert_eq!(task.step(&wrong), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    task.reset(context);
    assert_eq!(task.telemetry(), RecoverShipTask::new(context).telemetry());
    let mut blocked = s.recovery_task_observation(0, None);
    blocked.flight.pilot.ship_available = false;
    blocked.flight.pilot.ship_form = ShipForm::EscapePod;
    blocked.flight.pilot.location = PilotLocation::Aboard(blocked.flight.pilot.vehicle);
    blocked.flight.pilot.controls_armed = true;
    task.step(&blocked);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    let reason = task.telemetry().reason;
    blocked.flight.pilot.tick += 1000;
    blocked.flight.pilot.ship_available = true;
    task.step(&blocked);
    assert_eq!(
        task.telemetry().reason,
        reason,
        "terminal failure requires caller reset"
    );
}

#[test]
fn dirty_queries_do_not_reject_a_pod_site_but_completed_edits_do() {
    let owner = PlayerId::PLAYER_1;
    let mut s = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            owner,
            scenario_spacewars::surface_sortie::pilot::MaterialFlightStart {
                bearing: 0.0,
                altitude: 100.0,
                radial_speed: 0.0,
                lateral_speed: 0.0,
                heading_offset: 0.0,
            },
        )],
    );
    SurfaceSortieScenario::step(&mut s, &[], DT);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction {
            held: true,
            ..Default::default()
        }
        .encode(owner)],
        DT,
    );
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut s, &[SurfaceImpactAction::default().encode(owner)], DT);
        if s.observation(0).recovery.unwrap().ships_lost > 0 {
            break;
        }
    }
    assert_eq!(
        s.recovery_task_observation(0, None).flight.pilot.ship_form,
        ShipForm::EscapePod
    );
    let mut task = RecoverShipTask::new(BrainReset {
        actor: owner,
        episode_seed: 42,
    });
    for _ in 0..60 * 30 {
        let o = s.recovery_task_observation(0, task.site_request());
        let intent = task.step(&o);
        let mut actions = intent.encode(owner).to_vec();
        actions.push(SurfaceImpactAction::default().encode(owner));
        SurfaceSortieScenario::step(&mut s, &actions, DT);
        if task.site_request().is_some() {
            break;
        }
    }
    let site = task
        .site_request()
        .expect("pod must select a surveyed site");
    let mut dirty = s.recovery_task_observation(0, Some(site));
    dirty.flight.pilot.queries_ready = false;
    dirty.sites.clear();
    task.step(&dirty);
    assert_eq!(task.site_request(), Some(site));
    let before = task.telemetry().invalidations;
    assert!(s.queue_recovery_disruption(0, RecoveryDisruption::LandingSite(site)));
    SurfaceSortieScenario::step(&mut s, &[], DT);
    let changed = s.recovery_task_observation(0, Some(site));
    task.step(&changed);
    assert_eq!(task.site_request(), None);
    assert_eq!(task.telemetry().invalidations, before + 1);
}

#[test]
fn hostile_ground_gets_one_fixed_extension_without_restarting_the_recovery_clock() {
    use scenario_spacewars::surface_sortie::PlanetFlagObservation;
    let (mut task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.location = PilotLocation::OnFoot;
    p.ship_available = false;
    p.balanced = true;
    p.supported_planet = Some(p.planet.index);
    p.actor = Some(p.ship);
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.actor.unwrap().position + Vec2::X * 20.0,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    task.step(&o);
    assert_eq!(task.telemetry().ground_budget_ticks, 90 * 60);
    assert_eq!(task.telemetry().started_tick, Some(0));
    o.flight.pilot.tick = 30;
    o.flight.pilot.planet.revision += 1;
    o.flight
        .pilot
        .planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position
        .x += 1.0;
    task.step(&o);
    assert_eq!(task.telemetry().ground_budget_ticks, 90 * 60);
    assert_eq!(task.telemetry().started_tick, Some(0));
    o.flight.pilot.tick = 210 * 60 + 1;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("recovery exceeded combined flight and ground budget")
    );
}

fn ship_on_other_planet() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.location = PilotLocation::OnFoot;
    p.ship_form = ShipForm::Ship;
    p.balanced = true;
    p.relative_speed = 0.0;
    p.supported_planet = Some(p.planet.index);
    p.actor = Some(p.ship);
    p.landing.phase = LandingPhase::Landed;
    p.landing.supported_feet = 2;
    p.landing.planet = Some(p.planet.index + 1);
    p.hatch = Some(p.ship.position);
    p.transfer = scenario_spacewars::surface_sortie::TransferResult::TooFar;
    (task, o)
}

#[test]
fn unreachable_ship_uses_shared_scuttle_then_releases_and_retains_recovery_budget() {
    let (mut task, mut o) = ship_on_other_planet();
    let first = task.step(&o);
    assert!(
        first.controls.primary_held && first.controls.interact_held && first.controls.brake_held
    );
    assert_eq!(task.telemetry().goal, RecoveryGoal::Scuttle);
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    let initial = task.telemetry().started_tick;
    assert_eq!(task.step(&o), first);
    let mut copy = task.clone();
    o.flight.pilot.tick += 1;
    o.flight.pilot.queries_ready = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().started_tick, initial);
    o.flight.pilot.tick += 1;
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.ship_available = false;
    o.flight.pilot.controls_armed = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(copy.step(&o), FlightIntent::default());
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().scuttled_tick, Some(o.flight.pilot.tick));
    assert_eq!(task.telemetry().started_tick, initial);
    o.flight.pilot.tick += 1;
    o.flight.pilot.planet.claim.as_mut().unwrap().owner = Some(o.flight.pilot.owner);
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::Rebuild);
}

#[test]
fn returning_ship_cancels_scuttle_and_support_loss_never_holds_the_chord() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = ship_on_other_planet();
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.supported_planet = None;
    assert_eq!(task.step(&o), FlightIntent::default());
    o.flight.pilot.tick += 1;
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    o.flight.pilot.landing.planet = Some(o.flight.pilot.planet.index);
    o.flight.pilot.transfer = TransferResult::Ready;
    assert_eq!(
        task.step(&o),
        FlightIntent::default(),
        "release the cancelled chord"
    );
    o.flight.pilot.tick += 1;
    let board = task.step(&o).controls;
    assert!(board.interact_held && !board.primary_held && !board.brake_held);
    assert!(task.telemetry().scuttled_tick.is_none());
}

#[test]
fn an_unsettled_nearby_ship_waits_before_replacement_but_moving_ships_do_not_qualify() {
    for moving in [false, true] {
        let (mut task, mut o) = ship_on_other_planet();
        let p = &mut o.flight.pilot;
        p.landing.planet = Some(p.planet.index);
        p.landing.phase = LandingPhase::Assisted;
        p.landing.supported_feet = 0;
        p.landing.lateral_speed = if moving { 5.0 } else { 0.0 };
        p.hatch = None;
        task.step(&o);
        assert_eq!(task.telemetry().scuttle_attempts, 0);
        o.flight.pilot.tick += 901;
        let action = task.step(&o).controls;
        assert_eq!(
            action.interact_held && action.brake_held && action.primary_held,
            !moving
        );
        if moving {
            o.flight.pilot.tick += 901;
            task.step(&o);
            assert_eq!(task.telemetry().status, TaskStatus::Blocked);
            assert_eq!(task.telemetry().scuttle_attempts, 0);
        }
    }
}

#[test]
fn a_replacement_cannot_trigger_another_scuttle_in_the_same_task() {
    let (mut task, mut o) = ship_on_other_planet();
    task.step(&o);
    o.flight.pilot.tick += 181;
    o.flight.pilot.ship_available = false;
    task.step(&o);
    assert!(task.telemetry().scuttled_tick.is_some());
    o.flight.pilot.tick += 1;
    o.flight.pilot.ship_available = true;
    let action = task.step(&o);
    assert_eq!(action, FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    assert_eq!(
        task.telemetry().reason,
        Some("replacement ship also has no accessible return")
    );
}

#[test]
fn both_host_policies_keep_the_active_recovery_clock_across_a_deliberate_loss() {
    use engine_common::CombatBreakSettings;
    use spacewars_ai::mission_pilot::MaterialMissionPilot;
    let (_, mut o) = ship_on_other_planet();
    let context = BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    };
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut mission_o = state.mission_observation(0, None);
    let mut mission = MaterialMissionPilot::new(context, CombatBreakSettings::default());
    let mut sortie = RulePilotV3::new(context);
    // A prior loss starts the standalone host's recovery. Both hosts then
    // encounter the same unavailable return and ordinary scuttle observation.
    o.flight.pilot.recovery.as_mut().unwrap().ships_lost = 1;
    mission_o.local.combat.recovery = o.clone();
    mission.intent(&mission_o);
    sortie.intent(&o);
    let started = o.flight.pilot.tick;
    for tick in [started + 181, started + 182] {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship_available = false;
        o.flight.pilot.recovery.as_mut().unwrap().ships_lost = 2;
        mission_o.local.combat.recovery = o.clone();
        mission.intent(&mission_o);
        sortie.intent(&o);
        for telemetry in [
            mission.telemetry().recovery.as_ref().unwrap(),
            sortie.telemetry().recovery.as_ref().unwrap(),
        ] {
            assert_eq!(telemetry.started_tick, Some(started));
            assert_eq!(telemetry.scuttle_attempts, 1);
            assert_eq!(telemetry.scuttled_tick, Some(started + 181));
        }
    }
}

#[test]
fn physical_return_boards_and_departs_with_replacement_only_when_needed() {
    use scenario_spacewars::surface_sortie::return_trial::ReturnTrial;
    for (seat, mirror, bearing, trial) in [
        (0, false, 0.0, ReturnTrial::Reachable),
        (
            1,
            true,
            std::f32::consts::FRAC_PI_2,
            ReturnTrial::TippedShip,
        ),
        (0, true, 0.0, ReturnTrial::OtherPlanet),
    ] {
        let owner = PlayerId::from_index(seat).unwrap();
        let mut state =
            SurfaceSortieScenario::init_material_return_trial(42, seat, mirror, bearing, trial);
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut task = RecoverShipTask::new(BrainReset {
            actor: owner,
            episode_seed: 42,
        });
        let mut claimed = false;
        let mut departed = false;
        for _ in 0..90 * 60 {
            let o = state.recovery_task_observation(seat, task.site_request());
            let p = &o.flight.pilot;
            claimed |= p
                .planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(owner));
            let mut intent = if claimed {
                task.step(&o)
            } else {
                FlightIntent::default()
            };
            if task.telemetry().status == TaskStatus::Succeeded {
                assert!(matches!(p.location, PilotLocation::Aboard(_)));
                if p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 70.0 {
                    departed = true;
                    break;
                }
                intent.controls.primary_held = true;
            }
            SurfaceSortieScenario::step(&mut state, &intent.encode(owner), DT);
        }
        assert!(departed, "{trial:?}: {:?}", task.telemetry());
        let r = state.observation(seat).recovery.unwrap();
        let expected = u64::from(trial != ReturnTrial::Reachable);
        assert_eq!(
            (r.ships_lost, r.pod_ejections, r.rebuilds),
            (expected, 0, expected),
            "{trial:?}"
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn a_present_hatch_cannot_cancel_replacement_until_the_ship_actually_settles() {
    use scenario_spacewars::surface_sortie::TransferResult;
    for moving in [false, true] {
        let (mut task, mut o) = ship_on_other_planet();
        let p = &mut o.flight.pilot;
        p.landing.planet = Some(p.planet.index);
        p.landing.phase = LandingPhase::Settling;
        p.landing.supported_feet = 1;
        p.landing.lateral_speed = if moving { 5.0 } else { 0.0 };
        p.transfer = TransferResult::ShipNotSettled;
        task.step(&o);
        assert_eq!(task.telemetry().scuttle_attempts, 0);
        o.flight.pilot.tick += 901;
        let action = task.step(&o).controls;
        assert_eq!(
            action.primary_held && action.interact_held && action.brake_held,
            !moving
        );
        if moving {
            o.flight.pilot.tick += 901;
            task.step(&o);
            assert_eq!(task.telemetry().status, TaskStatus::Blocked);
            assert_eq!(task.telemetry().scuttle_attempts, 0);
        } else {
            let mut copy = task.clone();
            o.flight.pilot.tick += 1;
            assert_eq!(task.step(&o), copy.step(&o));
            assert!(
                task.step(&o).controls.brake_held,
                "visible but unsettled hatch must not cancel"
            );
            o.flight.pilot.tick += 1;
            o.flight.pilot.supported_planet = None;
            assert_eq!(task.step(&o), FlightIntent::default());
            o.flight.pilot.tick += 1;
            o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
            o.flight.pilot.landing.phase = LandingPhase::Landed;
            o.flight.pilot.landing.supported_feet = 2;
            o.flight.pilot.transfer = TransferResult::Ready;
            assert_eq!(
                task.step(&o),
                FlightIntent::default(),
                "release cancelled chord"
            );
            o.flight.pilot.tick += 1;
            let action = task.step(&o).controls;
            assert!(action.interact_held && !action.primary_held && !action.brake_held);
            assert!(task.telemetry().scuttled_tick.is_none());
        }
    }
}
