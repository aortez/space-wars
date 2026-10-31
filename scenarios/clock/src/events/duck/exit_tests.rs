use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockDuckJumpProfile, ClockEventKind, ClockEventProfile, Scenario};
use std::time::Duration;

const STEP: Duration = Duration::from_nanos(16_666_667);

fn scene(aspect: f32, direction: f32, profile: ClockDuckJumpProfile) -> ClockState {
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
        &[
            ClockAction::set_reading(ClockReading::new(10, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Duck),
        ],
        Duration::ZERO,
    );
    let mut duck = TestDuck::new_platforms(Layout::new(aspect), 42);
    duck.direction = direction;
    duck.controller.profile = profile;
    duck.admit(&mut state);
    state
}

fn at_opportunity(duck: &DuckEvent, world: &PhysicsWorld, near_wall: bool) -> bool {
    if duck.controller.direction >= 0.0
        || !duck.grounded(world)
        || duck.controller.navigator.flight_tick.is_some()
    {
        return false;
    }
    let x = duck.position(world).unwrap().x;
    let velocity = world.motion(DUCK_BODY).unwrap().linear_velocity.x * duck.direction;
    velocity < 0.0
        && if near_wall {
            x <= duck.radius * 5.0 && x > duck.radius * 3.5
        } else {
            x > duck.width * 0.75 && x < duck.width - duck.radius * 4.0
        }
}

fn capture(state: &ClockState, width: u32, height: u32, name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name, "width": width, "height": height,
        "duck": state.duck_scene().unwrap().diagnostics(),
        "frame": ClockScenario::render_frame(state),
    })
}

#[test]
fn exit_turns_and_near_wall_tags_replay_on_all_target_layouts() {
    let mut frames = Vec::new();
    let mut clip = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for direction in [-1.0, 1.0] {
            for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
                for near_wall in [false, true] {
                    let name = format!(
                        "exit-{width}x{height}-{}-{profile:?}-{}",
                        if direction > 0.0 { "left" } else { "right" },
                        if near_wall { "tag" } else { "turn" }
                    );
                    let mut previous = None;
                    for replay in 0..2 {
                        let mut state = scene(width as f32 / height as f32, direction, profile);
                        for _ in 0..EXIT_DELAY_TICKS {
                            ClockScenario::step(&mut state, &[], STEP);
                            if at_opportunity(
                                state.duck_visit.as_ref().unwrap(),
                                state.arena.as_ref().unwrap(),
                                near_wall,
                            ) {
                                break;
                            }
                        }
                        let duck = state.duck_visit.as_mut().unwrap();
                        let world = state.arena.as_ref().unwrap();
                        assert!(
                            at_opportunity(duck, world, near_wall),
                            "{name}: {:?}",
                            duck.diagnostics(Some(world))
                        );
                        assert!(!duck.exit_visible());
                        let tags = duck.controller.wall_tags[0];
                        // Stage only the door-opening time. The duck reached this
                        // pose via ordinary spawn, calibration and course travel.
                        duck.tick = OPENING_TICKS + EXIT_DELAY_TICKS - 1;
                        let mut samples =
                            vec![capture(&state, width, height, &format!("{name}-opening"))];
                        let mut turned_at = None;
                        let mut terminal = None;
                        for elapsed in 0..DUCK_TICKS {
                            ClockScenario::step(&mut state, &[], STEP);
                            let duck = state.duck_scene().unwrap();
                            if elapsed == 0 {
                                assert_eq!(
                                    duck.controller.direction,
                                    if near_wall { -1.0 } else { 1.0 },
                                    "{name}"
                                );
                            }
                            if turned_at.is_none() && duck.controller.direction > 0.0 {
                                turned_at = Some(elapsed);
                                assert_eq!(
                                    duck.controller.wall_tags[0],
                                    tags + u32::from(near_wall),
                                    "{name}"
                                );
                                samples.push(capture(
                                    &state,
                                    width,
                                    height,
                                    &format!("{name}-turned"),
                                ));
                            }
                            if turned_at.is_some() {
                                assert!(duck.controller.direction > 0.0, "no renewed excursion");
                            }
                            if elapsed == 6 {
                                let before = ClockScenario::render_frame(&state);
                                let stats = state.duck_scene().unwrap().diagnostics();
                                for _ in 0..3 {
                                    ClockScenario::step(&mut state, &[], Duration::ZERO);
                                }
                                assert_eq!(ClockScenario::render_frame(&state), before);
                                assert_eq!(state.duck_scene().unwrap().diagnostics(), stats);
                                samples.push(capture(
                                    &state,
                                    width,
                                    height,
                                    &format!("{name}-departing"),
                                ));
                            }
                            if replay == 1
                                && width == 800
                                && direction > 0.0
                                && profile == ClockDuckJumpProfile::Careful
                                && elapsed % 6 == 0
                                && elapsed <= if near_wall { 420 } else { 120 }
                            {
                                clip.push(capture(
                                    &state,
                                    width,
                                    height,
                                    &format!("{name}-clip-{elapsed:04}"),
                                ));
                            }
                            if let Some(outcome) = state.duck_scene().unwrap().outcome {
                                assert_eq!(outcome, ClockDuckOutcome::Exited, "{name}");
                                terminal = Some(elapsed);
                                break;
                            }
                        }
                        assert!(
                            turned_at.is_some_and(|tick| tick <= 45),
                            "{name}: {turned_at:?}"
                        );
                        assert!(terminal.is_some(), "{name}");
                        assert_eq!(samples.len(), 3);
                        if replay == 0 {
                            previous = Some(samples);
                        } else {
                            assert_eq!(
                                previous.as_ref().unwrap(),
                                &samples,
                                "deterministic {name}"
                            );
                            frames.extend(samples);
                            eprintln!(
                                "{name}: turn={} exit={}",
                                turned_at.unwrap(),
                                terminal.unwrap()
                            );
                        }
                    }
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_EXIT_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&frames).unwrap()).unwrap();
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_EXIT_CLIP_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&clip).unwrap()).unwrap();
    }
}

#[test]
fn near_wall_allowance_has_a_body_scaled_boundary_in_both_controller_paths() {
    for platforms in [false, true] {
        for direction in [-1.0, 1.0] {
            for radii in [4.8, 5.2] {
                let mut duck = if platforms {
                    TestDuck::new_platforms(Layout::new(800.0 / 480.0), 42)
                } else {
                    TestDuck::new(Layout::new(800.0 / 480.0), 42)
                };
                duck.direction = direction;
                duck.skip_flap_detour_for_fixture();
                for _ in 0..EXIT_DELAY_TICKS {
                    duck.step();
                    if at_opportunity(&duck, duck.world.as_ref().unwrap(), true) {
                        break;
                    }
                }
                assert!(at_opportunity(&duck, duck.world.as_ref().unwrap(), true));
                // Put this boundary fixture just inside/outside the threshold
                // on the same flat physical bank, after real calibration.
                let position = duck
                    .physics_position(Vec2::new(duck.radius * radii, duck.position().unwrap().y));
                let velocity = Vec2::new(-direction * duck.radius * 2.0, 0.0);
                let world = duck.world.as_mut().unwrap();
                world.set_pose(DUCK_BODY, position, 0.0, true);
                world.set_velocity(DUCK_BODY, velocity, 0.0, true);
                duck.tick = OPENING_TICKS + EXIT_DELAY_TICKS - 1;
                duck.step();
                assert_eq!(
                    duck.controller.direction,
                    if radii < 5.0 { -1.0 } else { 1.0 },
                    "platforms={platforms} direction={direction} radii={radii}"
                );
            }
        }
    }
}

#[test]
fn opening_mid_jump_finishes_the_physical_landing_before_replanning() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        for direction in [-1.0, 1.0] {
            for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
                let mut duck = TestDuck::new_platforms(Layout::new(aspect), 42);
                duck.direction = direction;
                duck.controller.profile = profile;
                for _ in 0..EXIT_DELAY_TICKS {
                    duck.step();
                    if duck.controller.direction < 0.0
                        && duck
                            .controller
                            .navigator
                            .flight_tick
                            .is_some_and(|t| t >= 5)
                    {
                        break;
                    }
                }
                assert!(duck.controller.navigator.flight_tick.is_some());
                assert_eq!(duck.controller.direction, -1.0);
                let plan = duck.controller.navigator.plan.unwrap();
                let confirmed = duck.controller.navigator.confirmed;
                let jumps = duck.jumps;
                duck.tick = OPENING_TICKS + EXIT_DELAY_TICKS - 1;
                for _ in 0..120 {
                    duck.step();
                    assert_eq!(duck.controller.direction, -1.0);
                    assert_eq!(duck.jumps, jumps);
                    if duck.controller.navigator.confirmed > confirmed {
                        break;
                    }
                }
                assert_eq!(duck.controller.navigator.confirmed, confirmed + 1);
                assert_eq!(duck.supported_surface(), Some(plan.target));
                duck.step();
                assert_eq!(duck.controller.direction, 1.0);
                while duck.outcome.is_none() && duck.tick < DUCK_TICKS {
                    duck.step();
                }
                assert_eq!(
                    duck.outcome,
                    Some(ClockDuckOutcome::Exited),
                    "{aspect} {direction} {profile:?}"
                );
            }
        }
    }
}
