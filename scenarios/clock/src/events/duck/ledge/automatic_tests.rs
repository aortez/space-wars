use super::*;
use automatic::{ATTEMPT_TICKS, COOLDOWN_TICKS, HANG_TICKS, MAX_ATTEMPTS};
use engine_common::{ClockDuckAutoLedgeOutcome as Outcome, ClockDuckJumpProfile};

const ASPECTS: [f32; 3] = [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0];
const PROFILES: [ClockDuckJumpProfile; 2] =
    [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing];

/// Let the real controller calibrate and launch, then stage a repeatable
/// disrupted flight beside its selected target. This is a near-miss fixture,
/// not evidence that an unperturbed seeded run normally misses this jump.
fn near_miss(aspect: f32, direction: f32, profile: ClockDuckJumpProfile) -> (DuckEvent, Edge) {
    let mut duck = DuckEvent::new_platforms(Layout::new(aspect), 42);
    duck.direction = direction;
    duck.controller.profile = profile;
    for _ in 0..900 {
        duck.step();
        if duck
            .controller
            .navigator
            .flight_tick
            .is_some_and(|t| t >= 5)
        {
            let target = duck.controller.navigator.plan.unwrap().target;
            let edge = edges(&duck)
                .into_iter()
                .flatten()
                .find(|edge| {
                    edge.body.entity.value() == 1000 + target as u64
                        && edge.outward * direction < 0.0
                })
                .unwrap();
            stage_miss(&mut duck, edge);
            return (duck, edge);
        }
    }
    panic!("no ordinary takeoff: {:?}", duck.diagnostics());
}

fn stage_miss(duck: &mut DuckEvent, edge: Edge) {
    let r = duck.radius;
    let world = duck.world.as_mut().unwrap();
    let anchor = edge.anchor(world).unwrap();
    world.set_pose(
        DUCK_BODY,
        anchor + Vec2::new(edge.outward * r * 1.4, -r * 0.6),
        0.0,
        true,
    );
    world.set_velocity(DUCK_BODY, Vec2::new(-edge.outward * r * 3.0, -r), 0.0, true);
}

fn wait_for_hang(duck: &mut DuckEvent) {
    for _ in 0..40 {
        duck.step();
        if duck.ledge_state().phase == Phase::Hanging {
            return;
        }
    }
    panic!("no automatic hang: {:?}", duck.diagnostics());
}

fn block(duck: &mut DuckEvent, position: Vec2, half: Vec2) -> PhysicsId {
    let id = PhysicsId::new(9000);
    assert!(duck.world.as_mut().unwrap().insert_body(
        BodyId::new(id, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(id, ColliderRole::PRIMARY, 0),
            half.x,
            half.y
        )],
    ));
    id
}

#[test]
fn both_profiles_recover_near_misses_then_resume_and_exit_in_all_target_layouts() {
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            for profile in PROFILES {
                let (mut duck, edge) = near_miss(aspect, direction, profile);
                let caps = duck.controller.capabilities().unwrap();
                let samples = (duck.controller.heights.count, duck.controller.speeds.count);
                let confirmed = duck.controller.navigator.confirmed;
                let mass = duck.world.as_ref().unwrap().body_mass(DUCK_BODY);
                let count = duck.physics_counts();
                let jumps = duck.jumps;
                wait_for_hang(&mut duck);
                assert!(duck.controller.navigator.plan.is_none());
                assert!(duck.controller.navigator.flight_tick.is_none());
                assert_eq!(duck.controller.navigator.confirmed, confirmed);
                let mut hanging = 0;
                for _ in 0..ATTEMPT_TICKS {
                    if duck.ledge_state().phase == Phase::Hanging {
                        hanging += 1;
                    }
                    duck.step();
                    assert!(duck.ledge.automatic.stats.candidates_checked <= 16);
                    assert!(duck.ledge_state().clearance_queries <= 64);
                    if duck.ledge.automatic.stats.completed == 1 {
                        break;
                    }
                }
                assert_eq!(hanging, HANG_TICKS);
                assert_eq!(
                    duck.ledge.automatic.stats.outcome,
                    Some(Outcome::Climbed),
                    "{:?}",
                    duck.diagnostics()
                );
                assert!(duck.grounded());
                assert_eq!(
                    duck.supported_surface().map(|i| i as u64 + 1000),
                    Some(edge.body.entity.value())
                );
                assert_eq!(duck.world.as_ref().unwrap().body_mass(DUCK_BODY), mass);
                assert_eq!(duck.physics_counts(), count);
                assert_eq!(duck.jumps, jumps, "no airborne rescue jump");
                assert_eq!(
                    (duck.controller.heights.count, duck.controller.speeds.count),
                    samples
                );
                let after = duck.controller.capabilities().unwrap();
                assert_eq!(
                    (after.height, after.flight, after.speed, after.acceleration),
                    (caps.height, caps.flight, caps.speed, caps.acceleration)
                );
                assert_eq!(
                    duck.controller.navigator.confirmed, confirmed,
                    "climbs are not ordinary landings"
                );
                while duck.phase != EventPhase::Resetting && duck.tick < DUCK_TICKS {
                    duck.step();
                }
                assert_eq!(
                    duck.outcome,
                    Some(ClockDuckOutcome::Exited),
                    "{aspect} {direction} {profile:?}: {:?}",
                    duck.diagnostics()
                );
            }
        }
    }
}

#[test]
fn ordinary_clearable_jumps_calibration_and_wrong_way_edges_do_not_request_catches() {
    for profile in PROFILES {
        let (mut duck, _) = near_miss(ASPECTS[0], 1.0, profile);
        let r = duck.radius;
        duck.world.as_mut().unwrap().set_velocity(
            DUCK_BODY,
            Vec2::new(r * 3.0, r * 20.0),
            0.0,
            true,
        );
        duck.decide_automatic_ledge();
        assert_eq!(
            duck.ledge.automatic.stats.attempts, 0,
            "leave a clearable jump alone"
        );

        let (mut duck, _) = near_miss(ASPECTS[0], 1.0, profile);
        duck.controller = Controller::new();
        duck.decide_automatic_ledge();
        assert_eq!(
            duck.ledge.automatic.stats.attempts, 0,
            "never catch during calibration"
        );

        let (mut duck, _) = near_miss(ASPECTS[0], 1.0, profile);
        duck.controller.direction = -1.0;
        duck.decide_automatic_ledge();
        assert_eq!(
            duck.ledge.automatic.stats.attempts, 0,
            "do not catch behind the intended route"
        );
        duck.controller.direction = 1.0;
        duck.controller.navigator.plan.as_mut().unwrap().target = 0;
        duck.decide_automatic_ledge();
        assert_eq!(
            duck.ledge.automatic.stats.attempts, 0,
            "do not substitute another target"
        );
    }
}

#[test]
fn blocked_climbs_release_once_and_do_not_wait_forever() {
    for profile in PROFILES {
        for direction in [-1.0, 1.0] {
            let (mut duck, edge) = near_miss(ASPECTS[0], direction, profile);
            wait_for_hang(&mut duck);
            let r = duck.radius;
            let anchor = duck.ledge_anchor().unwrap();
            block(
                &mut duck,
                anchor + Vec2::new(-edge.outward * r * 1.5, r * 1.5),
                Vec2::splat(r * 0.5),
            );
            for _ in 0..HANG_TICKS + 2 {
                duck.step();
            }
            assert_eq!(duck.ledge.automatic.stats.outcome, Some(Outcome::Blocked));
            assert_eq!(duck.ledge.automatic.stats.failed, 1);
            assert!(!duck.ledge_attached());
            assert_eq!(duck.ledge_state().climbs, 0);
            assert_eq!(duck.ledge_state().releases, 1);
            assert!(duck.ledge.automatic.stats.cooldown_ticks > 0);
        }
    }
}

#[test]
fn catch_refusals_have_a_cooldown_and_a_visit_attempt_limit() {
    let (mut duck, edge) = near_miss(ASPECTS[0], 1.0, PROFILES[0]);
    let r = duck.radius;
    let anchor = edge.anchor(duck.world.as_ref().unwrap()).unwrap();
    block(
        &mut duck,
        anchor + Vec2::new(-edge.outward * r * 1.5, r * 1.5),
        Vec2::splat(r * 0.5),
    );
    // Hold only the fixture's physical observation constant while exercising
    // repeated controller decisions. Actual movement/blocked climbs are above.
    duck.world.as_mut().unwrap().step(DT);
    for tick in 0..COOLDOWN_TICKS * (MAX_ATTEMPTS + 2) {
        duck.decide_automatic_ledge();
        assert!(!duck.drive_ledge());
        duck.observe_automatic_ledge();
        assert!(duck.ledge.automatic.stats.attempts <= 1 + tick / COOLDOWN_TICKS);
        assert!(duck.ledge.automatic.stats.attempts <= MAX_ATTEMPTS);
    }
    assert_eq!(duck.ledge.automatic.stats.attempts, MAX_ATTEMPTS);
    assert_eq!(duck.ledge.automatic.stats.failed, MAX_ATTEMPTS);
    assert_eq!(duck.ledge.automatic.stats.outcome, Some(Outcome::Missed));
}

#[test]
fn takeover_keeps_a_hang_or_committed_climb_and_stops_automatic_decisions() {
    for climbing in [false, true] {
        let (mut duck, _) = near_miss(ASPECTS[0], -1.0, PROFILES[1]);
        wait_for_hang(&mut duck);
        if climbing {
            for _ in 0..HANG_TICKS {
                duck.step();
            }
            assert_eq!(duck.ledge_state().phase, Phase::Climbing);
        }
        let anchor = duck.ledge_anchor();
        let motion = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert!(duck.take_control(99, 2));
        let taken = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert_eq!(taken.position, motion.position);
        assert_eq!(taken.linear_velocity, motion.linear_velocity);
        assert_eq!(duck.ledge_anchor(), anchor);
        assert_eq!(duck.ledge.automatic.stats.outcome, Some(Outcome::TakenOver));
        for _ in 0..150 {
            duck.step();
        }
        assert!(!duck.ledge.automatic.stats.active);
        if climbing {
            assert_eq!(duck.ledge_state().climbs, 1);
            assert!(duck.grounded());
        } else {
            assert_eq!(duck.ledge_state().phase, Phase::Hanging);
            assert_eq!(duck.ledge_state().climbs, 0);
        }
    }
}

#[test]
fn water_timeout_support_loss_and_visit_deadline_cancel_automatic_intent() {
    for interruption in 0..5 {
        let (mut duck, edge) = near_miss(ASPECTS[0], 1.0, PROFILES[0]);
        wait_for_hang(&mut duck);
        let mass = duck.world.as_ref().unwrap().body_mass(DUCK_BODY);
        let expected = match interruption {
            0 => {
                duck.water_report.submerged_fraction = 0.5;
                Outcome::Wet
            }
            1 => {
                duck.ledge.automatic.stats.attempt_ticks = ATTEMPT_TICKS - 1;
                Outcome::TimedOut
            }
            2 => {
                duck.world.as_mut().unwrap().remove_entity(edge.body.entity);
                Outcome::Interrupted
            }
            3 => {
                let r = duck.radius;
                duck.world.as_mut().unwrap().apply_velocity_delta(
                    DUCK_BODY,
                    Vec2::new(r * 40.0, 0.0),
                    true,
                );
                Outcome::Interrupted
            }
            _ => {
                duck.tick = DUCK_TICKS - RESET_TICKS - 1;
                Outcome::Deadline
            }
        };
        duck.decide_automatic_ledge();
        assert!(duck.drive_ledge());
        duck.observe_automatic_ledge();
        assert!(!duck.ledge_attached());
        assert_eq!(duck.ledge.automatic.stats.outcome, Some(expected));
        assert_eq!(duck.ledge.automatic.stats.failed, 1);
        assert_eq!(duck.world.as_ref().unwrap().body_mass(DUCK_BODY), mass);
    }
}

#[test]
fn clean_seeded_visits_keep_their_normal_jumps_without_ledge_attempts() {
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            for profile in PROFILES {
                let mut duck = DuckEvent::new_platforms(Layout::new(aspect), 42);
                duck.direction = direction;
                duck.controller.profile = profile;
                for _ in 0..DUCK_TICKS {
                    duck.step();
                    assert_eq!(
                        duck.ledge.automatic.stats.attempts,
                        0,
                        "{:?}",
                        duck.diagnostics()
                    );
                }
                assert_eq!(duck.outcome, Some(ClockDuckOutcome::Exited));
                assert!(duck.controller.navigator.confirmed > 0);
            }
        }
    }
}

#[test]
fn real_water_releases_a_hang_then_owns_recovery_without_recalibration() {
    use engine_water::{Boundary, PoolSpec, WaterConfig};
    let (mut duck, _) = near_miss(ASPECTS[0], 1.0, PROFILES[1]);
    wait_for_hang(&mut duck);
    let samples = (duck.controller.heights.count, duck.controller.speeds.count);
    let jumps = duck.jumps;
    let half = duck.width * 0.5;
    let top = duck.position().unwrap().y + duck.radius * 5.0;
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: f64::from(-half),
            column_width: f64::from(duck.width) / 128.0,
            bed: vec![f64::from(duck.layout.floor_y); 128],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for i in 0..128 {
        water
            .add_to_pool(
                0,
                f64::from(-half) + (i as f64 + 0.5) * f64::from(duck.width) / 128.0,
                f64::from((top - duck.layout.floor_y) * duck.width) / 128.0,
            )
            .unwrap();
    }
    duck.step_with_water(Some(&water));
    assert_eq!(duck.ledge.automatic.stats.outcome, Some(Outcome::Wet));
    assert!(!duck.ledge_attached());
    for _ in 0..60 {
        duck.step_with_water(Some(&water));
    }
    assert!(duck.controller.water_active());
    assert_eq!(duck.controller.water.stats.interruptions, 1);
    assert_eq!(duck.ledge.automatic.stats.attempts, 1);
    assert_eq!(duck.jumps, jumps);
    assert_eq!(
        (duck.controller.heights.count, duck.controller.speeds.count),
        samples
    );
}

#[test]
fn a_fully_open_exit_flap_is_local_recovery_support_and_retires_with_its_arena() {
    for direction in [-1.0, 1.0] {
        for profile in PROFILES {
            let (mut duck, _) = near_miss(ASPECTS[0], direction, profile);
            // Stage the exit opportunity after real calibration. The flap is
            // opened through the usual actuator before staging the near miss.
            duck.controller.interrupt_for_ledge();
            duck.tick = OPENING_TICKS + EXIT_DELAY_TICKS;
            duck.enter(EventPhase::Exiting);
            for tick in 1..=25 {
                duck.phase_tick = tick;
                duck.advance_flaps();
                duck.world.as_mut().unwrap().step(DT);
            }
            let edge = edges(&duck)
                .into_iter()
                .flatten()
                .find(|e| e.flap.is_some_and(|(side, _)| side == 1))
                .unwrap();
            stage_miss(&mut duck, edge);
            wait_for_hang(&mut duck);
            assert_eq!(
                duck.ledge_state().support_body,
                Some(flaps::body(1).entity.value())
            );
            assert_eq!(duck.flap_diagnostics()[1].open_milli, 1000);
            for _ in 0..ATTEMPT_TICKS {
                duck.step();
                if duck.ledge.automatic.stats.completed == 1 {
                    break;
                }
            }
            assert_eq!(
                duck.ledge.automatic.stats.completed,
                1,
                "{:?}",
                duck.diagnostics()
            );
            assert!(duck.grounded());
            if profile == ClockDuckJumpProfile::Careful {
                while duck.phase != EventPhase::Resetting && duck.tick < DUCK_TICKS {
                    duck.step();
                }
                assert_eq!(
                    duck.outcome,
                    Some(ClockDuckOutcome::Exited),
                    "{:?}",
                    duck.diagnostics()
                );
                continue;
            }
            let mut arena = VisitArena::claim(&mut duck);
            arena.retain(Box::new(duck));
            let vacant = arena.vacant().unwrap();
            assert!(!vacant.ledge_attached());
            assert!(!vacant.ledge.automatic.stats.active);
            assert!(vacant.door_panels().iter().all(Option::is_none));
            let rejoined = arena.rejoin(10, 1).unwrap();
            assert_eq!(rejoined.ledge.automatic.stats, Default::default());
        }
    }
}

#[test]
fn automatic_ledge_scene_fixtures_replay_pause_and_render_at_target_sizes() {
    use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use engine_common::{ClockEventProfile, Scenario};
    use std::time::Duration;
    let mut captures = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for direction in [-1.0, 1.0] {
            for profile in PROFILES {
                let mut previous = None;
                for replay in 0..2 {
                    let aspect = width as f32 / height as f32;
                    let (duck, _) = near_miss(aspect, direction, profile);
                    let mut state = ClockScenario::init(
                        ClockConfig {
                            aspect_ratio: aspect,
                            event_profile: ClockEventProfile::Off,
                            ..Default::default()
                        },
                        42,
                    );
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::set_reading(
                            ClockReading::new(10, 8, 0)
                                .unwrap()
                                .with_date(ClockDate::new(2026, 10, 17).unwrap()),
                        )],
                        Duration::ZERO,
                    );
                    state.duck_visit = Some(Box::new(duck));
                    let mut frames = Vec::new();
                    for _ in 0..ATTEMPT_TICKS {
                        ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                        let duck = state.duck_scene().unwrap().diagnostics();
                        let stage = match frames.len() {
                            0 if duck.ledge.phase == Phase::Hanging => "hanging",
                            1 if duck.ledge.progress_milli == 600 => "pull-up",
                            2 if duck.navigation.unwrap().ledge.completed == 1 => "landed",
                            _ => continue,
                        };
                        let frame = ClockScenario::render_frame(&state);
                        for _ in 0..5 {
                            ClockScenario::step(&mut state, &[], Duration::ZERO);
                        }
                        assert_eq!(state.duck_scene().unwrap().diagnostics(), duck);
                        assert_eq!(ClockScenario::render_frame(&state), frame);
                        frames.push(serde_json::json!({
                            "name":format!("auto-ledge-{width}x{height}-{}-{profile:?}-{stage}", if direction > 0.0 {"left"} else {"right"}),
                            "width":width,"height":height,"stage":stage,"seed":42,
                            "duck":duck,"bodies":state.body_count(),"colliders":state.collider_count(),"frame":frame
                        }));
                        if frames.len() == 3 {
                            break;
                        }
                    }
                    assert_eq!(frames.len(), 3);
                    state.set_aspect_ratio(0.75);
                    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                    assert!(state.duck_state().is_none());
                    if replay == 0 {
                        previous = Some(frames);
                    } else {
                        assert_eq!(previous.as_ref().unwrap(), &frames);
                        captures.extend(frames);
                    }
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_AUTO_LEDGE_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&captures).unwrap()).unwrap();
    }
}
