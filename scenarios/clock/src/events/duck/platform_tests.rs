use super::planner::{Capabilities, Course, Rejection, Surface};
use super::*;

const ASPECTS: [f32; 7] = [
    0.25,
    0.6,
    0.75,
    1024.0 / 768.0,
    800.0 / 480.0,
    1280.0 / 720.0,
    4.0,
];

#[derive(Debug, Default)]
struct RunStats {
    landings: u32,
    exit_tick: u64,
    first_crossing: u64,
    stopped_before_first_wall: u64,
    running: u32,
    moving: u32,
    fallbacks: u32,
    lookahead: u32,
}

fn run(event: DuckEvent, label: &str) -> (u32, u64) {
    let stats = run_stats(event, label);
    (stats.landings, stats.exit_tick)
}

fn run_stats(mut event: DuckEvent, label: &str) -> RunStats {
    let mut stats = RunStats::default();
    let mut landings = 0;
    let mut exit_tick = None;
    for tick in 1..=DUCK_TICKS {
        let before = event.diagnostics();
        event.step();
        let after = event.diagnostics();
        let navigation = after.navigation.unwrap();
        let planning = navigation.planning.unwrap();
        if stats.first_crossing == 0 {
            if navigation.wall_tags.iter().sum::<u32>() > 0 {
                stats.first_crossing = tick;
            } else if after.grounded
                && navigation.speed_samples == 9
                && event
                    .world
                    .as_ref()
                    .unwrap()
                    .motion(DUCK_BODY)
                    .unwrap()
                    .linear_velocity
                    .x
                    .abs()
                    < navigation.run_speed_milli.unwrap() as f32 / 1000.0 * 0.05
            {
                stats.stopped_before_first_wall += 1;
            }
        }
        assert!(event.physics_counts().0 <= planner::MAX_SURFACES + 1);
        if after.jumps > before.jumps {
            assert!(before.grounded, "{label}: airborne jump");
            if let Some(plan) = planning.plan
                && plan.running_takeoff
            {
                let speed = event
                    .world
                    .as_ref()
                    .unwrap()
                    .motion(DUCK_BODY)
                    .unwrap()
                    .linear_velocity
                    .x
                    .abs();
                assert!(speed >= navigation.run_speed_milli.unwrap() as f32 / 1000.0 * 0.14);
                stats.lookahead += u32::from(plan.next_target.is_some());
            }
        }
        if planning.confirmed_landings > landings {
            let previous_plan = before.navigation.unwrap().planning.unwrap().plan.unwrap();
            assert_eq!(planning.support, Some(previous_plan.target));
            assert!(before.grounded, "landing must be solver-supported");
            landings = planning.confirmed_landings;
        }
        if let Some(outcome) = after.outcome {
            assert_eq!(
                outcome,
                ClockDuckOutcome::Exited,
                "{label} tick={tick} {after:?} course={:?}",
                event.course
            );
            exit_tick.get_or_insert(tick);
        }
        assert_eq!(
            (
                planning.undershoots,
                planning.overshoots,
                planning.wrong_surface_landings
            ),
            (0, 0, 0),
            "{label} tick={tick} {after:?}"
        );
    }
    let final_state = event.diagnostics();
    let navigation = final_state.navigation.unwrap();
    assert!(
        navigation.wall_tags.iter().all(|count| *count > 0),
        "{label}: {final_state:?}"
    );
    assert!(landings >= (event.course.as_ref().unwrap().surfaces.len() - 1) as u32 * 3);
    assert_eq!(event.physics_counts(), (0, 0));
    let planning = navigation.planning.unwrap();
    stats.landings = landings;
    stats.exit_tick = exit_tick.expect("must exit");
    stats.running = planning.running_jumps;
    stats.moving = planning.moving_landings;
    stats.fallbacks = planning.flowing_fallbacks;
    stats
}

#[test]
fn jumping_profiles_compare_on_identical_generated_courses() {
    compare_profiles(0..8);
}

#[test]
#[ignore = "extended deterministic two-profile comparison"]
fn jumping_profile_stress() {
    compare_profiles(8..64);
}

fn compare_profiles(seeds: std::ops::Range<u64>) {
    use engine_common::ClockDuckJumpProfile;
    let mut totals = [RunStats::default(), RunStats::default()];
    let cases = (seeds.end - seeds.start) as usize * ASPECTS.len();
    for aspect in ASPECTS {
        for seed in seeds.clone() {
            let careful = DuckEvent::new_platforms(Layout::new(aspect), seed);
            let mut flowing = DuckEvent::new_platforms(Layout::new(aspect), seed);
            flowing.select_jump_profile(Some(ClockDuckJumpProfile::Flowing));
            assert_eq!(
                careful.course.as_ref().unwrap().surfaces,
                flowing.course.as_ref().unwrap().surfaces
            );
            for (index, event) in [careful, flowing].into_iter().enumerate() {
                let result = run_stats(
                    event,
                    &format!("profile={index} aspect={aspect} seed={seed}"),
                );
                totals[index].first_crossing += result.first_crossing;
                totals[index].stopped_before_first_wall += result.stopped_before_first_wall;
                totals[index].landings += result.landings;
                totals[index].running += result.running;
                totals[index].moving += result.moving;
                totals[index].fallbacks += result.fallbacks;
                totals[index].lookahead += result.lookahead;
            }
        }
    }
    eprintln!(
        "jump profiles ({cases} paired courses): careful={:?}; flowing={:?}",
        totals[0], totals[1]
    );
    assert_eq!(totals[0].running, 0);
    assert_eq!(totals[0].fallbacks, 0);
    assert!(totals[1].running > cases as u32);
    assert!(totals[1].lookahead > 0);
    assert!(totals[1].moving >= totals[1].running * 9 / 10);
    assert!(totals[1].first_crossing < totals[0].first_crossing);
    assert!(totals[1].stopped_before_first_wall < totals[0].stopped_before_first_wall);
}

#[test]
fn fixed_platform_step_and_gap_courses_land_in_both_directions() {
    for aspect in ASPECTS {
        for fixture in 0..3 {
            let mut event = DuckEvent::new_platforms(Layout::new(aspect), 42);
            event.course = Some(Course::fixed(event.width, event.radius, fixture));
            run(event, &format!("aspect={aspect} fixture={fixture}"));
        }
    }
}

#[test]
fn generated_platform_courses_have_confirmed_landings_and_bounded_exits() {
    generated_matrix(0..32);
}

#[test]
#[ignore = "extended deterministic platform-course seed sweep"]
fn platform_seed_stress() {
    generated_matrix(32..256);
}

fn generated_matrix(seeds: std::ops::Range<u64>) {
    let cases = ASPECTS.len() as u64 * (seeds.end - seeds.start);
    let mut landings = 0;
    let mut exits = [u64::MAX, 0];
    let mut fallbacks = 0;
    for aspect in ASPECTS {
        for seed in seeds.clone() {
            let event = DuckEvent::new_platforms(Layout::new(aspect), seed);
            fallbacks += usize::from(event.course.as_ref().unwrap().fallback);
            let (count, exit) = run(event, &format!("aspect={aspect} seed={seed}"));
            landings += count;
            exits[0] = exits[0].min(exit);
            exits[1] = exits[1].max(exit);
        }
    }
    eprintln!(
        "platform matrix: {cases} courses, {landings} confirmed landings, exit ticks {exits:?}, {fallbacks} fallbacks"
    );
}

#[test]
fn exhausted_generation_budget_uses_a_playable_fallback_and_seeds_vary_geometry() {
    for aspect in ASPECTS {
        let mut event = DuckEvent::new_platforms(Layout::new(aspect), 42);
        let course = Course::generate_with_budget(event.width, event.radius, 42, 0);
        assert!(course.fallback);
        event.course = Some(course);
        run(event, "forced fallback");
    }
    let a = Course::generated(800.0, 8.0, 42);
    assert_eq!(a.surfaces, Course::generated(800.0, 8.0, 42).surfaces);
    let variants = (0..32)
        .filter(|seed| Course::generated(800.0, 8.0, *seed).surfaces != a.surfaces)
        .count();
    assert!(variants >= 30, "course generation should visibly vary");
}

#[test]
fn forced_bad_landings_are_classified_using_real_supports_not_proximity() {
    for kind in 0..3 {
        let mut event = DuckEvent::new_platforms(Layout::new(800.0 / 480.0), 42);
        while event.jumps < 3 || event.grounded() {
            assert!(event.tick < 500);
            event.step();
        }
        let plan = event.controller.navigator.plan.unwrap();
        let floor = event.layout.floor_y;
        let radius = event.radius;
        let x = match kind {
            0 => event.width * 0.1,
            1 => event.width * 0.9,
            _ => plan.landing.x,
        };
        let mut y = floor + radius * 1.2;
        let world = event.world.as_mut().unwrap();
        if kind == 2 {
            // A foreign collider at the intended X coordinate: landing nearby
            // must not count as landing on the intended platform.
            let entity = PhysicsId::new(999);
            let top = floor + plan.landing.y + radius * 3.0;
            world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: Vec2::new(x, top - radius),
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    radius * 3.0,
                    radius,
                )],
            );
            y = top + radius * 1.2;
        }
        let motion = world.motion(DUCK_BODY).unwrap();
        world.set_pose(DUCK_BODY, Vec2::new(x, y), 0.0, true);
        world.apply_velocity_delta(DUCK_BODY, -motion.linear_velocity, true);
        for _ in 0..30 {
            event.step();
            let navigator = &event.controller.navigator;
            if navigator.undershoots + navigator.overshoots + navigator.wrong_surface > 0 {
                break;
            }
        }
        let navigator = &event.controller.navigator;
        assert_eq!(navigator.confirmed, 0);
        assert_eq!(
            [
                navigator.undershoots,
                navigator.overshoots,
                navigator.wrong_surface
            ],
            match kind {
                0 => [1, 0, 0],
                1 => [0, 1, 0],
                _ => [0, 0, 1],
            }
        );
    }
}

#[test]
fn unreachable_platform_is_refused_and_event_still_cleans_up() {
    let mut event = DuckEvent::new_platforms(Layout::new(800.0 / 480.0), 42);
    event.course.as_mut().unwrap().surfaces[1].height = event.radius * 20.0;
    for _ in 0..DUCK_TICKS {
        event.step();
    }
    let stats = event.diagnostics();
    let planning = stats.navigation.unwrap().planning.unwrap();
    assert_eq!(stats.outcome, Some(ClockDuckOutcome::TimedOut));
    assert_eq!(
        stats.jumps, 2,
        "do not blindly jump at an unreachable target"
    );
    assert_eq!(
        planning.rejection,
        Some(engine_common::ClockDuckRejection::TooHigh)
    );
    assert!(planning.rejected_plans > 0 && planning.rejected_plans < 100);
    assert_eq!(event.physics_counts(), (0, 0));
}

#[test]
fn platform_preview_pause_resize_and_debug_overlay_do_not_change_physics() {
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use std::time::Duration;
    for profile in [
        engine_common::ClockDuckJumpProfile::Careful,
        engine_common::ClockDuckJumpProfile::Flowing,
    ] {
        let init = |debug| {
            let mut state = ClockScenario::init(
                ClockConfig {
                    event_profile: ClockEventProfile::Off,
                    duck_debug_overlay: debug,
                    duck_jump_profile: Some(profile),
                    ..ClockConfig::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                    ClockAction::preview_event(ClockEventKind::Duck),
                ],
                Duration::ZERO,
            );
            state
        };
        let mut plain = init(false);
        let mut debug = init(true);
        assert_eq!(
            debug.duck_state().unwrap().navigation.unwrap().jump_profile,
            profile
        );
        let mut saw_overlay = false;
        for _ in 0..600 {
            assert_eq!(plain.duck_state(), debug.duck_state());
            let stats = plain.duck_state().unwrap();
            if stats.navigation.unwrap().planning.unwrap().plan.is_some() {
                assert_ne!(
                    ClockScenario::render_frame(&plain),
                    ClockScenario::render_frame(&debug)
                );
                saw_overlay = true;
            }
            ClockScenario::step(&mut plain, &[], Duration::from_nanos(16_666_667));
            ClockScenario::step(&mut debug, &[], Duration::from_nanos(16_666_667));
        }
        assert!(saw_overlay);
        let before = debug.duck_state();
        ClockScenario::step(
            &mut debug,
            &[ClockAction::set_reading(
                ClockReading::new(12, 1, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        assert_eq!(debug.duck_state(), before);
        ClockScenario::step(
            &mut debug,
            &[ClockAction::preview_event(ClockEventKind::Duck)],
            Duration::ZERO,
        );
        assert_eq!(debug.body_count(), 0);
        assert_eq!(
            debug
                .duck_state()
                .unwrap()
                .navigation
                .unwrap()
                .planning
                .unwrap()
                .confirmed_landings,
            0
        );
        debug.set_aspect_ratio(0.6);
        assert_eq!(debug.duck_state(), None);
        assert_eq!(debug.body_count(), 0);
    }
}

#[test]
fn careful_fixture_is_preserved_and_profiles_share_physics_and_calibration() {
    let layout = Layout::new(800.0 / 480.0);
    let mut default = DuckEvent::new_platforms(layout, 42);
    let mut explicit = DuckEvent::new_platforms(layout, 42);
    explicit.select_jump_profile(Some(engine_common::ClockDuckJumpProfile::Careful));
    let mut flowing = DuckEvent::new_platforms(layout, 42);
    flowing.select_jump_profile(Some(engine_common::ClockDuckJumpProfile::Flowing));
    for _ in 0..DUCK_TICKS {
        assert_eq!(default.diagnostics(), explicit.diagnostics());
        if default.controller.speeds.count < 9 {
            assert_eq!(default.position(), flowing.position());
            assert_eq!(default.jumps, flowing.jumps);
        }
        default.step();
        explicit.step();
        flowing.step();
    }
}

#[test]
fn mixed_spawns_replay_both_personalities_without_changing_courses_or_schedules() {
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
    use engine_common::{ClockDuckJumpProfile, ClockEventKind, ClockEventProfile, Scenario};
    use std::time::Duration;

    let init = |profile| {
        let mut state = ClockScenario::init(
            ClockConfig {
                duck_jump_profile: profile,
                event_profile: ClockEventProfile::Demo,
                ..ClockConfig::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(8, 8, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        state
    };
    let geometry = |state: &ClockState| {
        let Some(crate::events::ActiveEvent::Duck(event)) = &state.active_event else {
            panic!("expected duck")
        };
        event.course.as_ref().unwrap().surfaces.clone()
    };
    assert_eq!(ClockConfig::default().duck_jump_profile, None);
    let mut mixed = init(None);
    let mut replay = init(None);
    let mut forced = init(Some(ClockDuckJumpProfile::Careful));
    let mut counts = [0; 2];
    for _ in 0..32 {
        for state in [&mut mixed, &mut replay, &mut forced] {
            ClockScenario::step(
                state,
                &[ClockAction::preview_event(ClockEventKind::Duck)],
                Duration::ZERO,
            );
        }
        let profile = mixed.duck_state().unwrap().navigation.unwrap().jump_profile;
        counts[usize::from(profile == ClockDuckJumpProfile::Flowing)] += 1;
        assert_eq!(geometry(&mixed), geometry(&replay));
        assert_eq!(geometry(&mixed), geometry(&forced));
        assert_eq!(
            mixed.duck_state().unwrap().left_to_right,
            forced.duck_state().unwrap().left_to_right
        );
        for _ in 0..DUCK_TICKS + 130 {
            assert_eq!(mixed.duck_state(), replay.duck_state());
            if let Some(duck) = mixed.duck_state() {
                assert_eq!(duck.navigation.unwrap().jump_profile, profile);
            }
            if let Some(duck) = forced.duck_state() {
                assert_eq!(
                    duck.navigation.unwrap().jump_profile,
                    ClockDuckJumpProfile::Careful
                );
            }
            assert_eq!(mixed.next_event_tick(), forced.next_event_tick());
            assert_eq!(mixed.event_id(), forced.event_id());
            for state in [&mut mixed, &mut replay, &mut forced] {
                ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
            }
        }
        assert!(mixed.duck_state().is_none());
        assert_eq!(mixed.body_count(), 0);
    }
    assert!(
        counts.iter().all(|count| *count > 0),
        "both personalities should appear: {counts:?}"
    );
    eprintln!(
        "mixed spawn replay: 32 visits, Careful={}, Flowing={}",
        counts[0], counts[1]
    );
}

#[test]
fn failed_running_takeoff_brakes_and_finishes_with_careful_fallback() {
    let mut event = DuckEvent::new_platforms(Layout::new(800.0 / 480.0), 42);
    event.select_jump_profile(Some(engine_common::ClockDuckJumpProfile::Flowing));
    // Find an approach with enough lead to inject a lost-speed disturbance.
    loop {
        assert!(event.tick < 600);
        event.step();
        if event
            .controller
            .navigator
            .plan
            .is_some_and(|plan| plan.running)
            && event.controller.navigator.flight_tick.is_none()
            && event.grounded()
        {
            break;
        }
    }
    let plan = event.controller.navigator.plan.unwrap();
    let before = event.controller.navigator.flowing_fallbacks;
    let world = event.world.as_mut().unwrap();
    let motion = world.motion(DUCK_BODY).unwrap();
    world.set_pose(
        DUCK_BODY,
        Vec2::new(plan.takeoff.x, motion.position.y),
        0.0,
        true,
    );
    world.apply_velocity_delta(DUCK_BODY, -motion.linear_velocity, true);
    world.step(DT);
    event.step();
    assert_eq!(event.controller.navigator.flowing_fallbacks, before + 1);
    assert!(!event.controller.navigator.plan.unwrap().running);
    run_stats(event, "lost-speed takeoff fallback");
}

#[test]
fn planner_rejects_unreachable_narrow_and_obstructed_landings() {
    let capabilities = Capabilities {
        height: 36.0,
        flight: 0.86,
        speed: 160.0,
        acceleration: 960.0,
    };
    let base = Surface {
        start: 0.0,
        end: 100.0,
        height: 0.0,
    };
    for (target, expected) in [
        (
            Surface {
                start: 120.0,
                end: 180.0,
                height: 40.0,
            },
            Rejection::High,
        ),
        (
            Surface {
                start: 120.0,
                end: 130.0,
                height: 0.0,
            },
            Rejection::Narrow,
        ),
        (
            Surface {
                start: 600.0,
                end: 680.0,
                height: 0.0,
            },
            Rejection::Range,
        ),
    ] {
        let course = Course {
            surfaces: vec![base, target],
            attempts: 0,
            fallback: false,
        };
        assert_eq!(
            planner::plan(&course, 0, 1, 8.0, capabilities).unwrap_err(),
            expected
        );
    }
    let course = Course {
        surfaces: vec![
            base,
            Surface {
                start: 105.0,
                end: 110.0,
                height: 100.0,
            },
            Surface {
                start: 120.0,
                end: 180.0,
                height: 0.0,
            },
        ],
        attempts: 0,
        fallback: false,
    };
    assert_eq!(
        planner::plan(&course, 0, 2, 8.0, capabilities).unwrap_err(),
        Rejection::Obstructed
    );
}
