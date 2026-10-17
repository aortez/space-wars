use super::*;
use engine_common::{
    ClockDuckFlapHold, ClockDuckLedgePhase as Phase, ClockDuckLedgeReason as Reason,
};
use ledge::LedgeCommand;

fn entrance(aspect: f32, direction: f32) -> DuckEvent {
    let mut duck = DuckEvent::new_player(Layout::new(aspect), 42, None, 1, 1);
    duck.direction = direction;
    for _ in 0..OPENING_TICKS {
        duck.step();
    }
    duck
}

fn catch_flap(duck: &mut DuckEvent) {
    let mut trace = Vec::new();
    for _ in 0..65 {
        let jump = duck.jumps == 0 && duck.grounded();
        let steer = if duck.position().unwrap().x > duck.radius * 5.6 {
            (-duck.direction * 300.0) as i16
        } else {
            0
        };
        duck.set_player_input(steer, jump, false, false);
        duck.set_ledge_command(LedgeCommand::Catch);
        duck.step();
        if trace.len() < 30 {
            trace.push((
                duck.phase_tick,
                duck.position().unwrap().x / duck.radius,
                (duck.position().unwrap().y - duck.layout.floor_y) / duck.radius,
                duck.grounded(),
                duck.jumps,
                duck.door_openness().0,
                duck.diagnostics().ledge,
            ));
        }
        if duck.diagnostics().ledge.phase == Phase::Hanging {
            return;
        }
    }
    panic!(
        "ordinary-spawn catch failed: {:?}\n{trace:#?}",
        duck.diagnostics()
    );
}

fn climb(duck: &mut DuckEvent) {
    duck.set_ledge_command(LedgeCommand::Climb);
    for _ in 0..150 {
        duck.step();
        if duck.diagnostics().ledge.climbs == 1 {
            return;
        }
    }
    panic!("climb failed: {:?}", duck.diagnostics());
}

#[test]
fn normal_jump_catches_hangs_and_climbs_the_fully_open_flap() {
    for aspect in [5.0 / 3.0, 4.0 / 3.0, 0.6] {
        for direction in [-1.0, 1.0] {
            let mut duck = entrance(aspect, direction);
            let mass = duck.world.as_ref().unwrap().body_mass(DUCK_BODY);
            catch_flap(&mut duck);
            assert_eq!(duck.jumps, 1);
            assert!(!duck.grounded(), "hanging is not ground support");
            for _ in 0..600 {
                duck.step();
                assert_eq!(duck.diagnostics().ledge.phase, Phase::Hanging);
                assert_eq!(
                    duck.diagnostics().flaps[0].hold,
                    Some(ClockDuckFlapHold::Attachment)
                );
                assert_eq!(duck.door_openness().0, 1.0);
            }
            climb(&mut duck);
            assert!(duck.grounded());
            assert_eq!(duck.world.as_ref().unwrap().body_mass(DUCK_BODY), mass);
            assert_eq!(
                duck.world.as_ref().unwrap().body_kind(DUCK_BODY),
                Some(BodyKind::Dynamic)
            );
            duck.step();
            assert_eq!(
                duck.diagnostics().flaps[0].hold,
                Some(ClockDuckFlapHold::Standing)
            );
            duck.set_player_input((direction * 1000.0) as i16, true, false, false);
            duck.step();
            assert_eq!(duck.jumps, 2);
            for _ in 0..40 {
                duck.step();
            }
            assert_eq!(duck.door_openness().0, 0.0);
        }
    }
}

#[test]
fn deliberate_drop_keeps_momentum_and_requires_rearming() {
    let mut duck = entrance(5.0 / 3.0, 1.0);
    catch_flap(&mut duck);
    let before = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
    duck.set_ledge_command(LedgeCommand::Release);
    duck.set_player_input(0, true, false, true);
    duck.step();
    let after = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
    assert!(
        (after.linear_velocity.y - before.linear_velocity.y + duck.movement.gravity * DT).abs()
            < 0.001
    );
    assert_eq!(duck.diagnostics().ledge.reason, Some(Reason::Released));
    assert_eq!(duck.jumps, 1);
    for _ in 0..30 {
        duck.set_ledge_command(LedgeCommand::Catch);
        duck.step();
        assert_eq!(duck.diagnostics().ledge.catches, 1);
        assert!(duck.diagnostics().ledge.rearm_required);
    }
    duck.step();
    assert!(!duck.diagnostics().ledge.rearm_required);
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
fn blocked_pull_up_waits_in_a_real_hang_and_can_retry_when_clear() {
    let mut duck = entrance(5.0 / 3.0, 1.0);
    catch_flap(&mut duck);
    let anchor = duck.ledge_anchor().unwrap();
    let r = duck.radius;
    let blocker = block(
        &mut duck,
        anchor + Vec2::new(-r * 1.5, r * 1.5),
        Vec2::splat(r * 0.5),
    );
    duck.step();
    for _ in 0..60 {
        duck.set_ledge_command(LedgeCommand::Climb);
        duck.step();
        assert_eq!(duck.ledge_state().phase, Phase::Hanging);
        assert_eq!(duck.ledge_state().reason, Some(Reason::ClimbBlocked));
        assert_eq!(duck.ledge_state().climbs, 0);
    }
    duck.world.as_mut().unwrap().remove_entity(blocker);
    duck.step();
    climb(&mut duck);
}

#[test]
fn an_obstacle_added_during_the_pull_up_releases_without_passing_through_it() {
    let mut duck = entrance(5.0 / 3.0, 1.0);
    catch_flap(&mut duck);
    duck.set_ledge_command(LedgeCommand::Climb);
    duck.step();
    let anchor = duck.ledge_anchor().unwrap();
    let r = duck.radius;
    let blocker = block(
        &mut duck,
        anchor + Vec2::new(-r * 1.5, r * 1.5),
        Vec2::splat(r * 0.5),
    );
    for _ in 0..100 {
        duck.step();
        if duck.ledge_state().releases > 0 {
            break;
        }
    }
    assert_eq!(duck.ledge_state().climbs, 0);
    assert_eq!(duck.ledge_state().releases, 1);
    assert!(matches!(
        duck.ledge_state().reason,
        Some(Reason::Obstructed | Reason::Disrupted)
    ));
    let (min, max) = duck
        .world
        .as_ref()
        .unwrap()
        .body_solid_bounds(BodyId::new(blocker, BodyRole::PRIMARY))
        .unwrap();
    let p = duck
        .world
        .as_ref()
        .unwrap()
        .motion(DUCK_BODY)
        .unwrap()
        .position;
    assert!(p.x < min.x || p.x > max.x || p.y < min.y || p.y > max.y);
}

#[test]
fn support_loss_and_external_impulses_release_the_same_dynamic_body() {
    for remove in [false, true] {
        let mut duck = entrance(5.0 / 3.0, 1.0);
        catch_flap(&mut duck);
        let mass = duck.world.as_ref().unwrap().body_mass(DUCK_BODY);
        if remove {
            duck.remove_flaps();
            // A retired flap that reopens has a new generation, never the old anchor.
        } else {
            let r = duck.radius;
            duck.world.as_mut().unwrap().apply_velocity_delta(
                DUCK_BODY,
                Vec2::new(r * 40.0, 0.0),
                true,
            );
        }
        duck.step();
        assert_eq!(duck.ledge_state().releases, 1);
        assert_eq!(
            duck.ledge_state().reason,
            Some(if remove {
                Reason::SupportLost
            } else {
                Reason::Disrupted
            })
        );
        assert_eq!(duck.world.as_ref().unwrap().body_mass(DUCK_BODY), mass);
        assert!(duck.ledge_anchor().is_none());
    }
}

#[test]
fn water_does_not_erase_a_valid_hang_or_apply_a_swim_stroke() {
    use engine_water::{Boundary, PoolSpec, WaterConfig};
    let mut duck = entrance(5.0 / 3.0, 1.0);
    catch_flap(&mut duck);
    let half = duck.width * 0.5;
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
                f64::from(duck.radius * 8.0 * duck.width) / 128.0,
            )
            .unwrap();
    }
    for _ in 0..120 {
        duck.set_player_input(0, true, false, true);
        duck.step_with_water(Some(&water));
        assert_eq!(duck.ledge_state().phase, Phase::Hanging);
        assert_eq!(duck.player.as_ref().unwrap().swim_strokes, 0);
        assert_eq!(duck.jumps, 1);
    }
    assert!(duck.water_report.submerged_fraction > 0.9);
    duck.set_ledge_command(LedgeCommand::Release);
    duck.step_with_water(Some(&water));
    assert_eq!(
        duck.player.as_ref().unwrap().swim_strokes,
        0,
        "release is the only movement action this tick"
    );
    duck.step_with_water(Some(&water));
    assert_eq!(duck.player.as_ref().unwrap().swim_strokes, 1);
}

#[test]
fn takeover_and_retirement_preserve_then_release_attachment_ownership() {
    for climbing in [false, true] {
        let mut duck = entrance(5.0 / 3.0, 1.0);
        catch_flap(&mut duck);
        if climbing {
            duck.set_ledge_command(LedgeCommand::Climb);
            duck.step();
        }
        // The shared fixture's source is now the automatic side. No body/state
        // mutation occurs when the real takeover operation changes ownership.
        duck.player = None;
        let motion = duck.world.as_ref().unwrap().motion(DUCK_BODY);
        let stats = duck.ledge_state();
        let anchor = duck.ledge_anchor();
        duck.set_ledge_command(LedgeCommand::Release);
        assert!(duck.take_control(8, 2));
        assert_eq!(duck.ledge_state(), stats);
        assert_eq!(duck.ledge_anchor(), anchor);
        assert_eq!(duck.world.as_ref().unwrap().motion(DUCK_BODY), motion);
        duck.step();
        assert_eq!(
            duck.ledge_state().releases,
            0,
            "old source's uncommitted drop is discarded"
        );
        let mut arena = VisitArena::claim(&mut duck);
        arena.retain(Box::new(duck));
        let vacant = arena.vacant().unwrap();
        assert!(vacant.ledge_anchor().is_none());
        assert_eq!(vacant.ledge_state(), Default::default());
        assert!(vacant.door_panels().iter().all(Option::is_none));
        let rejoined = arena.rejoin(9, 1).unwrap();
        assert_eq!(rejoined.ledge_state(), Default::default());
    }
}

#[test]
fn no_intent_or_unreachable_edges_never_attach() {
    for with_catch in [false, true] {
        let mut duck = entrance(5.0 / 3.0, 1.0);
        for _ in 0..40 {
            let jump = duck.jumps == 0 && duck.grounded();
            // Walk away in the catch case, so the flap leaves the reach envelope.
            duck.set_player_input(if with_catch { 1000 } else { 0 }, jump, false, false);
            if with_catch {
                duck.set_ledge_command(LedgeCommand::Catch);
            }
            duck.step();
            assert_eq!(duck.ledge_state().catches, 0);
            assert!(duck.ledge_state().candidates_checked <= 16);
            assert!(duck.ledge_state().clearance_queries <= 64);
        }
    }
}

#[test]
fn nearby_foreign_solids_are_not_grab_targets_and_blocked_hangs_are_refused() {
    for dynamic in [false, true] {
        let mut duck = entrance(5.0 / 3.0, 1.0);
        for _ in 0..60 {
            duck.step();
        }
        let r = duck.radius;
        let p = duck
            .world
            .as_ref()
            .unwrap()
            .motion(DUCK_BODY)
            .unwrap()
            .position;
        let id = block(
            &mut duck,
            p + Vec2::new(-r * 4.5, r * 4.0),
            Vec2::new(r * 3.0, r * 0.5),
        );
        if dynamic {
            let world = duck.world.as_mut().unwrap();
            world.set_body_kind(BodyId::new(id, BodyRole::PRIMARY), BodyKind::Dynamic, true);
            world.set_gravity_scale(BodyId::new(id, BodyRole::PRIMARY), 0.0, true);
        }
        for tick in 0..50 {
            duck.set_player_input(0, tick == 0, false, false);
            duck.set_ledge_command(LedgeCommand::Catch);
            duck.step();
            assert_eq!(
                duck.ledge_state().catches,
                0,
                "ordinary foreign solids are outside the first surface scope"
            );
        }
        assert_eq!(duck.jumps, 1);
    }
    let mut duck = entrance(5.0 / 3.0, 1.0);
    let panel = duck.door_panels()[0].unwrap();
    let r = duck.radius;
    let tip = panel.point(Vec2::new(panel.hinge.x + panel.width, panel.bottom));
    block(
        &mut duck,
        tip + Vec2::new(r * 0.5, -r),
        Vec2::new(r * 0.1, r * 0.5),
    );
    for _ in 0..50 {
        let jump = duck.jumps == 0 && duck.grounded();
        duck.set_player_input(0, jump, false, false);
        duck.set_ledge_command(LedgeCommand::Catch);
        duck.step();
        assert_eq!(
            duck.ledge_state().catches,
            0,
            "no attachment with an obstructed hanging hull"
        );
    }
}

#[test]
fn a_course_near_miss_uses_the_same_shared_climb_in_both_directions() {
    for direction in [-1.0, 1.0] {
        let mut duck = DuckEvent::new_player(Layout::new(5.0 / 3.0), 42, None, 1, 1);
        duck.direction = direction;
        let r = duck.radius;
        duck.course.as_mut().unwrap().surfaces = vec![
            planner::Surface {
                start: 0.0,
                end: r * 12.0,
                height: 0.0,
            },
            planner::Surface {
                start: r * 15.0,
                end: duck.width,
                height: r * 6.0,
            },
        ];
        for _ in 0..OPENING_TICKS {
            duck.step();
        }
        for _ in 0..100 {
            let jump = duck.jumps == 0 && duck.grounded() && duck.position().unwrap().x > r * 9.5;
            duck.set_player_input((direction * 700.0) as i16, jump, false, false);
            duck.set_ledge_command(LedgeCommand::Catch);
            duck.step();
            if duck.ledge_state().phase == Phase::Hanging {
                break;
            }
        }
        assert_eq!(
            duck.ledge_state().phase,
            Phase::Hanging,
            "{:?}",
            duck.diagnostics()
        );
        assert_eq!(duck.ledge_state().support_body, Some(1001));
        duck.set_player_input(0, false, false, false);
        climb(&mut duck);
        assert!(duck.grounded());
    }
}

#[test]
fn ledge_scene_fixtures_replay_and_render_with_pause_and_teardown() {
    use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use engine_common::{ClockEventProfile, Scenario};
    use std::time::Duration;
    let mut captures = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for direction in [-1.0, 1.0] {
            let aspect = width as f32 / height as f32;
            let mut previous = None;
            for replay in 0..2 {
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
                state.duck_visit = Some(Box::new(entrance(aspect, direction)));
                let step = |state: &mut crate::ClockState| {
                    ClockScenario::step(state, &[], Duration::from_nanos(16_666_667))
                };
                let mut frames = Vec::new();
                let capture = |state: &mut crate::ClockState,
                               frames: &mut Vec<serde_json::Value>,
                               stage| {
                    let duck = state.duck_scene().unwrap().diagnostics();
                    let frame = ClockScenario::render_frame(state);
                    for _ in 0..5 {
                        ClockScenario::step(state, &[], Duration::ZERO);
                    }
                    assert_eq!(state.duck_scene().unwrap().diagnostics(), duck);
                    assert_eq!(ClockScenario::render_frame(state), frame);
                    assert!(
                        duck.ledge.candidates_checked <= 16 && duck.ledge.clearance_queries <= 64
                    );
                    frames.push(serde_json::json!({
                        "name":format!("ledge-{width}x{height}-{}-{stage}", if direction > 0.0 {"left"} else {"right"}),
                        "width":width,"height":height,"stage":stage,"seed":42,
                        "duck":duck,"bodies":state.body_count(),"colliders":state.collider_count(),"frame":frame
                    }));
                };
                for _ in 0..65 {
                    let duck = state.duck_visit.as_mut().unwrap();
                    let jump = duck.jumps == 0 && duck.grounded();
                    let steer = if duck.position().unwrap().x > duck.radius * 5.6 {
                        (-direction * 300.0) as i16
                    } else {
                        0
                    };
                    duck.set_player_input(steer, jump, false, false);
                    duck.set_ledge_command(LedgeCommand::Catch);
                    step(&mut state);
                    let ledge = state.duck_scene().unwrap().ledge_state();
                    if frames.is_empty()
                        && ledge.phase == Phase::Reaching
                        && state.duck_scene().unwrap().position().unwrap().y
                            > state.duck_scene().unwrap().layout.floor_y
                                + state.duck_scene().unwrap().radius * 2.8
                    {
                        capture(&mut state, &mut frames, "reaching");
                    }
                    if ledge.phase == Phase::Hanging {
                        break;
                    }
                }
                assert_eq!(
                    state.duck_scene().unwrap().ledge_state().phase,
                    Phase::Hanging
                );
                for _ in 0..60 {
                    step(&mut state);
                }
                capture(&mut state, &mut frames, "hanging");
                state
                    .duck_visit
                    .as_mut()
                    .unwrap()
                    .set_ledge_command(LedgeCommand::Climb);
                for _ in 0..150 {
                    step(&mut state);
                    let ledge = state.duck_scene().unwrap().ledge_state();
                    if frames.len() == 2 && ledge.progress_milli == 600 {
                        capture(&mut state, &mut frames, "pull-up");
                    }
                    if ledge.climbs == 1 {
                        break;
                    }
                }
                assert!(state.duck_scene().unwrap().grounded());
                // An ordinary turn toward the course makes the perch readable
                // before the next Jump; no body pose or support is edited.
                state.duck_visit.as_mut().unwrap().set_player_input(
                    (direction * 1000.0) as i16,
                    false,
                    false,
                    false,
                );
                step(&mut state);
                assert!(state.duck_scene().unwrap().grounded());
                capture(&mut state, &mut frames, "standing");
                state.duck_visit.as_mut().unwrap().set_player_input(
                    (direction * 1000.0) as i16,
                    true,
                    false,
                    false,
                );
                for _ in 0..6 {
                    step(&mut state);
                }
                capture(&mut state, &mut frames, "jump-away");
                // A separate ordinary-spawn catch demonstrates deliberate release.
                let mut duck = entrance(aspect, direction);
                catch_flap(&mut duck);
                duck.set_ledge_command(LedgeCommand::Release);
                state.duck_visit = Some(Box::new(duck));
                for _ in 0..3 {
                    step(&mut state);
                }
                capture(&mut state, &mut frames, "dropping");
                assert_eq!(frames.len(), 6);
                state.set_aspect_ratio(0.75);
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                assert!(state.duck_state().is_none());
                if replay == 0 {
                    previous = Some(frames);
                } else {
                    assert_eq!(previous.as_ref().unwrap(), &frames, "same-build replay");
                    captures.extend(frames);
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_LEDGE_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&captures).unwrap()).unwrap();
    }
}
