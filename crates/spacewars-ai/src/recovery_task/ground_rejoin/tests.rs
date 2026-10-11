use super::*;
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieScenario,
    ground_navigation::GroundNode,
    ground_posture::{CrawlStep, SpacelingGetUpResult},
    pilot::PilotMotion,
};
use std::time::Duration;

fn fixture() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let mut state = SurfaceSortieScenario::init_material(42, 1);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.recovery_task_observation(0, None);
    o.jetpack = None;
    o.rebuild = None;
    let p = &mut o.flight.pilot;
    p.tick = 100;
    p.location = PilotLocation::OnFoot;
    p.controls_armed = true;
    p.queries_ready = true;
    p.ship_available = false;
    p.ship_form = ShipForm::Ship;
    p.actor_up = Vec2::Y;
    p.supported_planet = Some(p.planet.index);
    p.balanced = true;
    p.planet.motion = PilotMotion {
        position: Vec2::ZERO,
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
    };
    p.planet.claim.as_mut().unwrap().owner = Some(p.owner);
    p.actor = Some(PilotMotion {
        position: Vec2::new(0.0, 60.0 + HALF_HEIGHT),
        ..p.planet.motion
    });
    p.recovery.as_mut().unwrap().status = SurfaceRecoveryStatus::HatchBlocked;
    o.ground = Some(GroundMap {
        version: 1,
        actor: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: p.tick,
        nodes: vec![GroundNode {
            id: 0,
            position: Vec2::new(5.0, 60.0),
            normal: Vec2::Y,
        }],
        edges: vec![],
        rejected: vec![],
    });
    o.posture = Some(GroundPostureObservation {
        version: 1,
        owner: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: p.tick,
        balance: SpacelingBalance::Balanced,
        stable: true,
        standing_clear: false,
        get_up_result: SpacelingGetUpResult::NotRequested,
        get_up_attempts: 0,
        crawl_clearance: [None, Some(0.64)],
        crawl_floor: [None, Some(true)],
        crawl: [
            None,
            Some(CrawlStep {
                direction: 1.0,
                position: Vec2::new(0.6, 60.0 + HALF_HEIGHT),
            }),
        ],
    });
    let mut task = RecoverShipTask::new(BrainReset {
        actor: p.owner,
        episode_seed: 42,
    });
    task.telemetry.started_tick = Some(0);
    task.telemetry.relocations = 2;
    task.telemetry.scuttle_attempts = 1;
    task.relocation_missing_since = Some(90);
    (task, o)
}

fn advance(o: &mut RecoveryTaskObservationV1, tick: u64) {
    o.flight.pilot.tick = tick;
    if let Some(map) = &mut o.ground {
        map.tick = tick;
    }
    if let Some(posture) = &mut o.posture {
        posture.tick = tick;
    }
}

#[test]
fn explicit_selection_preserves_history_duplicate_ticks_clone_and_reset() {
    let (mut task, o) = fixture();
    let mut disabled = task.clone();
    assert_eq!(disabled.step(&o).controls, SurfaceSortieAction::default());
    assert!(disabled.telemetry.rebuild_ground_rejoin.is_none());
    task.set_rebuild_ground_rejoin(true);
    let mut copy = task.clone();
    let action = task.step(&o);
    assert_eq!(copy.step(&o), action);
    assert_eq!(action.controls.horizontal, 1.0);
    assert!(!action.controls.primary_held && !action.controls.interact_held);
    assert_eq!(
        task.telemetry.ground.as_ref().unwrap().goal,
        GroundGoal::Rejoin
    );
    let before = task.telemetry.clone();
    assert_eq!(task.step(&o), action);
    assert_eq!(task.telemetry, before);
    assert_eq!(task.telemetry.started_tick, Some(0));
    assert_eq!(task.telemetry.relocations, 2);
    assert_eq!(task.telemetry.scuttle_attempts, 1);
    copy.reset(task.context);
    assert!(copy.rebuild_ground_rejoin_enabled);
    assert!(copy.telemetry.rebuild_ground_rejoin.is_none());
}

#[test]
fn fresh_supported_cramped_footing_and_a_measured_opening_are_required() {
    for fault in 0..11 {
        let (mut task, mut o) = fixture();
        task.set_rebuild_ground_rejoin(true);
        match fault {
            0 => o.flight.pilot.queries_ready = false,
            1 => o.flight.pilot.supported_planet = None,
            2 => o.ground.as_mut().unwrap().tick -= 1,
            3 => o.ground.as_mut().unwrap().revision += 1,
            4 => o.posture.as_mut().unwrap().tick -= 1,
            5 => o.posture.as_mut().unwrap().stable = false,
            6 => o.posture.as_mut().unwrap().standing_clear = true,
            7 => o.posture.as_mut().unwrap().crawl = [None, None],
            8 => o.ground.as_mut().unwrap().nodes[0].position.x = 2.99,
            9 => o.ground.as_mut().unwrap().nodes[0].position.x = 8.01,
            _ => o.ground.as_mut().unwrap().actor = scenario_spacewars::PlayerId::Player2,
        }
        task.step(&o);
        assert!(
            task.telemetry.rebuild_ground_rejoin.is_none(),
            "fault {fault}"
        );
    }
}

#[test]
fn deadline_and_distance_are_enforced_even_with_dirty_queries_or_disarmed_controls() {
    for far in [false, true] {
        let (mut task, mut o) = fixture();
        task.set_rebuild_ground_rejoin(true);
        task.step(&o);
        advance(&mut o, if far { 101 } else { 581 });
        o.flight.pilot.queries_ready = false;
        o.flight.pilot.controls_armed = false;
        if far {
            o.flight.pilot.actor.as_mut().unwrap().position.x = 6.01;
        }
        assert_eq!(task.step(&o).controls, SurfaceSortieAction::default());
        assert_eq!(task.telemetry.status, TaskStatus::Blocked);
        let attempt = task.telemetry.rebuild_ground_rejoin.clone().unwrap();
        assert_eq!(attempt.deadline_tick, 580);
        assert_eq!(attempt.finished_tick, Some(o.flight.pilot.tick));
        advance(&mut o, 600);
        o.flight.pilot.queries_ready = true;
        o.flight.pilot.controls_armed = true;
        task.step(&o);
        assert_eq!(task.telemetry.rebuild_ground_rejoin, Some(attempt));
    }
}

#[test]
fn actual_standing_and_a_fresh_valid_map_resume_only_the_remaining_search_time() {
    let (mut task, mut o) = fixture();
    task.set_rebuild_ground_rejoin(true);
    task.step(&o);
    let mut map = o.ground.take().unwrap();
    advance(&mut o, 110);
    o.flight.pilot.actor.as_mut().unwrap().position.x = 2.5;
    o.posture.as_mut().unwrap().standing_clear = true;
    o.posture.as_mut().unwrap().crawl = [None, None];
    task.step(&o);
    assert!(
        task.telemetry
            .rebuild_ground_rejoin
            .as_ref()
            .unwrap()
            .rejoined_tick
            .is_none()
    );
    advance(&mut o, 115);
    map.tick = 115;
    o.ground = Some(map);
    assert_eq!(task.step(&o).controls, SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry
            .rebuild_ground_rejoin
            .as_ref()
            .unwrap()
            .rejoined_tick,
        Some(115)
    );
    assert_eq!(task.relocation_missing_since, Some(105));
    assert_eq!(task.telemetry.started_tick, Some(0));
    assert_eq!(task.telemetry.relocations, 2);
    assert_eq!(task.telemetry.scuttle_attempts, 1);
    advance(&mut o, 406);
    task.step(&o);
    assert_eq!(
        task.telemetry.reason,
        Some("no reachable standing site with hatch access")
    );
}

#[test]
fn invalid_new_map_cannot_claim_rejoin_success() {
    let (mut task, mut o) = fixture();
    task.set_rebuild_ground_rejoin(true);
    task.step(&o);
    advance(&mut o, 115);
    o.flight.pilot.actor.as_mut().unwrap().position.x = 2.5;
    o.posture.as_mut().unwrap().standing_clear = true;
    o.ground.as_mut().unwrap().nodes[0].id = u16::MAX;
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Blocked);
    assert!(
        task.telemetry
            .rebuild_ground_rejoin
            .as_ref()
            .unwrap()
            .rejoined_tick
            .is_none()
    );
}

#[test]
fn a_native_ship_or_changed_ownership_cancels_the_attempt() {
    for ship in [false, true] {
        let (mut task, mut o) = fixture();
        task.set_rebuild_ground_rejoin(true);
        task.step(&o);
        advance(&mut o, 101);
        if ship {
            o.flight.pilot.ship_available = true;
            o.flight.pilot.transfer = TransferResult::Ready;
        } else {
            o.flight.pilot.planet.claim.as_mut().unwrap().owner = None;
        }
        task.step(&o);
        let attempt = task.telemetry.rebuild_ground_rejoin.as_ref().unwrap();
        assert_eq!(attempt.finished_tick, Some(101));
        assert_eq!(
            attempt.reason,
            Some(if ship {
                "ship available"
            } else {
                "ownership changed"
            })
        );
    }
}

#[test]
fn blocked_tasks_and_overall_deadlines_never_receive_a_new_attempt() {
    for blocked in [false, true] {
        let (mut task, mut o) = fixture();
        if blocked {
            task.block("retained failure", 99);
        } else {
            advance(&mut o, 7201);
        }
        task.set_rebuild_ground_rejoin(true);
        assert_eq!(task.step(&o).controls, SurfaceSortieAction::default());
        assert_eq!(task.telemetry.status, TaskStatus::Blocked);
        assert!(task.telemetry.rebuild_ground_rejoin.is_none());
    }
}

#[test]
fn continued_corridors_are_independent_and_do_not_outlive_the_attempt() {
    let (mut task, mut o) = fixture();
    task.set_rebuild_rejoin_corridors(true);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    task.set_rebuild_ground_rejoin(true);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    task.step(&o);
    assert!(task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    let before = format!("{task:?}");
    assert!(
        task.clone()
            .rebuild_rejoin_corridors_requested(&o.flight.pilot)
    );
    assert_eq!(format!("{task:?}"), before);
    task.set_rebuild_rejoin_corridors(false);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    task.set_rebuild_rejoin_corridors(true);
    advance(&mut o, 580);
    assert!(task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    advance(&mut o, 581);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    task.step(&o);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
    task.reset(task.context);
    assert!(task.rebuild_rejoin_corridors);
    assert!(!task.rebuild_rejoin_corridors_requested(&o.flight.pilot));
}

#[test]
fn corridor_request_checks_current_limits_support_ownership_and_completion() {
    for fault in 0..15 {
        let (mut task, mut o) = fixture();
        task.set_rebuild_ground_rejoin(true);
        task.set_rebuild_rejoin_corridors(true);
        task.step(&o);
        advance(&mut o, 101);
        match fault {
            0 => o.flight.pilot.queries_ready = false,
            1 => o.flight.pilot.controls_armed = false,
            2 => o.flight.pilot.supported_planet = None,
            3 => o.flight.pilot.balanced = false,
            4 => o.flight.pilot.actor = None,
            5 => o.flight.pilot.actor.as_mut().unwrap().position.x = 6.01,
            6 => o.flight.pilot.planet.claim.as_mut().unwrap().owner = None,
            7 => o.flight.pilot.ship_available = true,
            8 => o.flight.pilot.planet.index += 1,
            9 => o.flight.pilot.tick = 99,
            10 => o.flight.pilot.tick = 7201,
            11 => task.set_rebuild_ground_rejoin(false),
            12 => task.block("test cancellation", 101),
            13 => o.flight.pilot.owner = scenario_spacewars::PlayerId::Player2,
            _ => {
                o.flight.pilot.actor.as_mut().unwrap().position.x = 2.5;
                o.posture.as_mut().unwrap().standing_clear = true;
                task.step(&o);
                assert_eq!(
                    task.telemetry
                        .rebuild_ground_rejoin
                        .as_ref()
                        .unwrap()
                        .rejoined_tick,
                    Some(101)
                );
            }
        }
        assert!(
            !task.rebuild_rejoin_corridors_requested(&o.flight.pilot),
            "fault {fault}"
        );
    }
}

fn left_corridor(o: &mut RecoveryTaskObservationV1) {
    o.posture.as_mut().unwrap().crawl[0] = Some(CrawlStep {
        direction: -1.0,
        position: Vec2::new(-0.6, 60.0 + HALF_HEIGHT),
    });
    o.posture.as_mut().unwrap().crawl_clearance[0] = Some(0.64);
    o.posture.as_mut().unwrap().crawl_floor[0] = Some(true);
}

#[test]
fn target_selection_changes_only_the_next_uncached_direction_and_preserves_history() {
    let (mut control, mut o) = fixture();
    control.set_rebuild_ground_rejoin(true);
    left_corridor(&mut o);
    let right = o.posture.as_mut().unwrap().crawl[1].take();
    assert_eq!(control.step(&o).controls.horizontal, -1.0);
    let mut candidate = control.clone();
    let before = candidate.telemetry.clone();
    candidate.set_rebuild_rejoin_target_directed(true);
    o.posture.as_mut().unwrap().crawl[1] = right;
    assert_eq!(candidate.step(&o).controls.horizontal, -1.0);
    assert_eq!(candidate.telemetry, before);
    advance(&mut o, 101);
    assert_eq!(control.step(&o).controls.horizontal, -1.0);
    let mut copy = candidate.clone();
    let action = candidate.step(&o);
    assert_eq!(action.controls.horizontal, 1.0);
    assert_eq!(copy.step(&o), action);
    assert!(!action.controls.primary_held && !action.controls.interact_held);
    assert_eq!(
        candidate.telemetry.rebuild_ground_rejoin,
        before.rebuild_ground_rejoin
    );
    assert_eq!(candidate.telemetry.started_tick, Some(0));
    assert_eq!(candidate.telemetry.relocations, 2);
    assert_eq!(candidate.telemetry.scuttle_attempts, 1);
    copy.reset(copy.context);
    assert!(copy.rebuild_rejoin_target_directed);
    assert!(copy.telemetry.rebuild_ground_rejoin.is_none());
}

#[test]
fn target_selection_keeps_ties_and_single_corridors_and_bounds_retained_steps() {
    let (mut task, mut o) = fixture();
    task.set_rebuild_ground_rejoin(true);
    task.set_rebuild_rejoin_target_directed(true);
    o.ground.as_mut().unwrap().nodes[0].position = Vec2::new(0.0, 65.0);
    left_corridor(&mut o);
    let right = o.posture.as_mut().unwrap().crawl[1].take();
    assert_eq!(task.step(&o).controls.horizontal, -1.0);
    advance(&mut o, 101);
    o.posture.as_mut().unwrap().crawl[1] = right;
    assert_eq!(
        task.step(&o).controls.horizontal,
        -1.0,
        "equal distances retain direction"
    );
    advance(&mut o, 102);
    o.posture.as_mut().unwrap().crawl[0] = None;
    assert_eq!(
        task.step(&o).controls.horizontal,
        1.0,
        "the sole valid corridor wins"
    );
    o.posture.as_mut().unwrap().crawl[1] = None;
    advance(&mut o, 116);
    assert_eq!(task.step(&o).controls.horizontal, 1.0);
    advance(&mut o, 117);
    assert_eq!(
        task.step(&o).controls.horizontal,
        0.0,
        "retention expires after 15 ticks"
    );
}

#[test]
fn target_selection_does_not_enable_an_attempt_or_change_other_ground_navigation() {
    let (mut task, mut o) = fixture();
    task.set_rebuild_rejoin_target_directed(true);
    assert_eq!(task.step(&o).controls, SurfaceSortieAction::default());
    assert!(task.telemetry.rebuild_ground_rejoin.is_none());
    let destination = GroundDestination::Rebuild {
        planet: o.flight.pilot.planet.index,
        position: Vec2::new(5.0, 60.0),
    };
    advance(&mut o, 101);
    left_corridor(&mut o);
    let right = o.posture.as_mut().unwrap().crawl[1].take();
    assert_eq!(task.traverse(&o, destination).horizontal, -1.0);
    advance(&mut o, 102);
    o.posture.as_mut().unwrap().crawl[1] = right;
    assert_eq!(task.traverse(&o, destination).horizontal, -1.0);
}

#[test]
fn target_selection_preserves_support_observation_and_attempt_limits() {
    for fault in 0..9 {
        let (mut task, mut o) = fixture();
        task.set_rebuild_ground_rejoin(true);
        task.set_rebuild_rejoin_target_directed(true);
        task.step(&o);
        advance(&mut o, 101);
        match fault {
            0 => o.flight.pilot.queries_ready = false,
            1 => o.flight.pilot.controls_armed = false,
            2 => o.flight.pilot.supported_planet = None,
            3 => o.posture.as_mut().unwrap().stable = false,
            4 => o.posture.as_mut().unwrap().tick -= 1,
            5 => {
                o.posture.as_mut().unwrap().crawl[1]
                    .as_mut()
                    .unwrap()
                    .position
                    .x = f32::NAN
            }
            6 => advance(&mut o, 581),
            7 => o.flight.pilot.actor.as_mut().unwrap().position.x = 6.01,
            _ => {
                task.set_rebuild_ground_rejoin(false);
                advance(&mut o, 406);
            }
        }
        assert_eq!(
            task.step(&o).controls,
            SurfaceSortieAction::default(),
            "fault {fault}"
        );
        assert_eq!(
            task.telemetry
                .rebuild_ground_rejoin
                .as_ref()
                .unwrap()
                .deadline_tick,
            580
        );
    }
}
