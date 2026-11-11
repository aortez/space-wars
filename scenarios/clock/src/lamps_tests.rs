//! The corner lamps and their moods: when they come, how they look, how
//! they give way, and that they never move the event schedule.

use crate::lamps::{Lamps, Look, SPOTLIGHT_TICKS, Scene, YIELD_TICKS};
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockLampMood, ClockLampSide, RenderFrame, RenderPrimitive,
    Scenario,
};
use std::time::Duration;

const TICK: Duration = Duration::from_nanos(16_666_667);
const LIT_LAYER: i32 = 7;
const LAMP_LAYER: i32 = 11;

fn ready(profile: ClockEventProfile, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 39, 50).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn quiet(profile: ClockEventProfile) -> Scene {
    Scene {
        profile,
        quiet: true,
        next_event_in: None,
    }
}

/// Steps the lamps alone until a mood starts, or gives up after `limit`.
fn until_mood(lamps: &mut Lamps, scene: Scene, limit: u32) -> Option<u32> {
    (0..limit).find(|_| {
        lamps.step(scene);
        lamps.mood().is_some()
    })
}

fn layer(frame: &RenderFrame, z: i32) -> Vec<RenderPrimitive> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| layer.primitives.iter().cloned())
        .collect()
}

/// How much light one side's lamp spreads around its corner: the summed
/// alpha of its translucent bloom and halo, not its opaque head.
fn glow(frame: &RenderFrame, left: bool) -> f32 {
    layer(frame, LAMP_LAYER)
        .iter()
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) if (circle.center.x < 0.0) == left => {
                circle.fill.as_ref().map(|fill| fill.color.a)
            }
            _ => None,
        })
        .filter(|alpha| *alpha < 1.0)
        .sum()
}

#[test]
fn profile_off_keeps_the_lamps_steady() {
    let mut lamps = Lamps::new(3);
    assert_eq!(
        until_mood(&mut lamps, quiet(ClockEventProfile::Off), 60 * 60 * 20),
        None
    );
    let state = lamps.state(ClockEventProfile::Off);
    assert_eq!((state.flickers, state.outages, state.spotlights), (0, 0, 0));
    assert_eq!(state.due_in_quiet_ticks, None);
    assert_eq!(lamps.look(0), Look::STEADY);
    assert_eq!(lamps.look(1), Look::STEADY);
}

#[test]
fn a_mood_waits_for_enough_quiet_and_for_a_gap_that_holds_it() {
    for (profile, low, high) in [
        (ClockEventProfile::Calm, 45, 120),
        (ClockEventProfile::Demo, 12, 24),
    ] {
        for seed in 0..12 {
            let mut lamps = Lamps::new(seed);
            let due = lamps.state(profile).due_in_quiet_ticks.unwrap();
            assert!(
                (low * 60..=high * 60).contains(&due),
                "{profile:?} {seed}: {due}"
            );
            // Busy ticks do not count towards the wait.
            let busy = Scene {
                quiet: false,
                ..quiet(profile)
            };
            for _ in 0..due * 2 {
                lamps.step(busy);
            }
            assert!(lamps.mood().is_none());
            assert_eq!(lamps.state(profile).due_in_quiet_ticks, Some(due));
            // Due, but the next event comes too soon for any mood.
            let crowded = Scene {
                next_event_in: Some(20),
                ..quiet(profile)
            };
            for _ in 0..due + 600 {
                lamps.step(crowded);
            }
            assert!(lamps.mood().is_none(), "{profile:?} {seed}: no gap");
            // A long enough gap takes it at once.
            lamps.step(Scene {
                next_event_in: Some(u64::from(SPOTLIGHT_TICKS) + 30),
                ..quiet(profile)
            });
            let mood = lamps.mood().expect("the mood that came due");
            assert!(mood.ticks <= SPOTLIGHT_TICKS, "{mood:?}");
        }
    }
}

#[test]
fn moods_are_bounded_seeded_and_return_to_steady() {
    let mut kinds = std::collections::BTreeSet::new();
    for seed in 0..40 {
        let mut a = Lamps::new(seed);
        let mut b = Lamps::new(seed);
        let scene = quiet(ClockEventProfile::Demo);
        for _ in 0..3 {
            let started = until_mood(&mut a, scene, 60 * 60).expect("a mood within a minute");
            assert_eq!(until_mood(&mut b, scene, 60 * 60), Some(started));
            let mood = a.mood().unwrap().clone();
            assert_eq!(b.mood(), Some(&mood), "the same seed, the same mood");
            kinds.insert(mood.kind.as_str());
            let other = 1 - mood.lamp;
            for tick in 0..mood.ticks {
                let look = a.look(mood.lamp);
                assert!(look.brightness.is_finite() && (0.0..=1.15).contains(&look.brightness));
                assert!((0.0..=1.0).contains(&look.narrow));
                if mood.kind != ClockLampMood::Spotlight {
                    assert_eq!(look.narrow, 0.0);
                }
                assert_eq!(a.look(other), Look::STEADY, "one lamp at a time");
                assert_eq!(a.mood().unwrap().tick, tick);
                a.step(scene);
                b.step(scene);
            }
            assert!(a.mood().is_none(), "{mood:?} outlived its ticks");
            assert_eq!(a.look(mood.lamp), Look::STEADY);
        }
    }
    assert_eq!(kinds.len(), 3, "{kinds:?}");
}

#[test]
fn a_mood_gives_way_within_half_a_second_when_the_scene_gets_busy() {
    for kind in ClockLampMood::ALL {
        for at in [0, 5, 40, 100, 200, 300] {
            let mut lamps = Lamps::new(9);
            lamps.preview(kind, ClockLampSide::Right);
            for _ in 0..at {
                lamps.step(quiet(ClockEventProfile::Off));
            }
            if lamps.mood().is_none() {
                continue;
            }
            let before = lamps.look(1);
            let busy = Scene {
                quiet: false,
                ..quiet(ClockEventProfile::Calm)
            };
            lamps.step(busy);
            let mood = lamps.mood().expect("still returning");
            assert!(mood.yielding(), "{kind:?} at {at}");
            // The return starts where the mood was, without a jump.
            let after = lamps.look(1);
            assert!(
                (after.narrow - before.narrow).abs() < 0.05,
                "{kind:?} at {at}"
            );
            for _ in 1..YIELD_TICKS {
                lamps.step(busy);
            }
            assert!(lamps.mood().is_none(), "{kind:?} at {at}: still going");
            assert_eq!(lamps.look(1), Look::STEADY);
        }
    }
}

#[test]
fn a_preview_runs_with_the_profile_off_but_a_drawn_mood_does_not() {
    let mut lamps = Lamps::new(4);
    lamps.preview(ClockLampMood::Outage, ClockLampSide::Left);
    let ticks = lamps.mood().unwrap().ticks;
    for _ in 0..ticks {
        assert!(!lamps.mood().unwrap().yielding());
        lamps.step(quiet(ClockEventProfile::Off));
    }
    assert!(lamps.mood().is_none());
    assert_eq!(lamps.state(ClockEventProfile::Off).outages, 1);
    // A drawn mood stops when the profile turns Off.
    let mut lamps = Lamps::new(4);
    until_mood(&mut lamps, quiet(ClockEventProfile::Demo), 60 * 60).unwrap();
    lamps.step(quiet(ClockEventProfile::Off));
    assert!(lamps.mood().unwrap().yielding());
}

#[test]
fn moods_come_only_in_quiet_gaps_and_never_move_the_event_schedule() {
    let starts = |previews: bool| {
        let mut state = ready(ClockEventProfile::Demo, 42);
        let mut events = Vec::new();
        let mut moods = 0;
        let mut previous = None;
        for tick in 0..6 * 60 * 60 {
            if previews && tick % 1500 == 700 {
                state.preview_lamp_mood(ClockLampMood::Spotlight, ClockLampSide::Left);
            }
            ClockScenario::step(&mut state, &[], TICK);
            let id = state.event_id();
            if previous != Some(id) {
                events.push((state.simulation_tick(), id, state.event_kind()));
                previous = Some(id);
            }
            if let Some(mood) = state.lamps.mood() {
                moods += u32::from(mood.tick == 0);
                if !mood.yielding() {
                    assert!(state.active_event.is_none(), "tick {tick}: {mood:?}");
                    assert!(state.duck_visit.is_none() && state.crow_visit.is_none());
                }
            }
        }
        (events, moods, state.lamps_state())
    };
    let (events, moods, lamps) = starts(false);
    assert!(events.len() > 10, "{events:?}");
    assert!(
        moods > 0 && lamps.flickers + lamps.outages + lamps.spotlights > 0,
        "{lamps:?}"
    );
    let (with_previews, _, previewed) = starts(true);
    assert!(previewed.spotlights > lamps.spotlights, "{previewed:?}");
    assert_eq!(events, with_previews, "the lamps moved an event");
}

#[test]
fn steady_lamps_draw_the_same_frame_and_a_mood_changes_only_its_lamp() {
    let mut state = ready(ClockEventProfile::Off, 7);
    let steady = ClockScenario::render_frame(&state);
    for _ in 0..600 {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(ClockScenario::render_frame(&state), steady);
    assert!(layer(&steady, LIT_LAYER).is_empty());
    // Out: the right lamp's corner goes dark and the left lamp is untouched.
    state.preview_lamp_mood(ClockLampMood::Outage, ClockLampSide::Right);
    let mut dark = None;
    while let Some(mood) = state.lamps_state().mood {
        if mood.brightness_milli == 0 && mood.tick > 60 {
            dark = Some(ClockScenario::render_frame(&state));
            break;
        }
        ClockScenario::step(&mut state, &[], TICK);
    }
    let dark = dark.expect("an outage goes dark");
    assert_eq!(glow(&dark, true), glow(&steady, true));
    assert!(
        glow(&dark, false) < glow(&steady, false) * 0.2,
        "{} of {}",
        glow(&dark, false),
        glow(&steady, false)
    );
}

#[test]
fn a_spotlight_lights_the_cells_in_its_pool_and_stays_in_view() {
    for aspect in [800.0 / 480.0, 4.0 / 3.0, 480.0 / 800.0] {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: aspect,
                event_profile: ClockEventProfile::Off,
                ..ClockConfig::default()
            },
            5,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 58, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let steady = ClockScenario::render_frame(&state);
        let count = |frame: &RenderFrame| {
            frame
                .layers
                .iter()
                .map(|l| l.primitives.len())
                .sum::<usize>()
        };
        state.preview_lamp_mood(ClockLampMood::Spotlight, ClockLampSide::Left);
        let mut pool_x = Vec::new();
        for tick in 0..SPOTLIGHT_TICKS {
            let frame = ClockScenario::render_frame(&state);
            let bounds = frame.camera.world_bounds(aspect);
            for primitive in frame.layers.iter().flat_map(|layer| &layer.primitives) {
                if let RenderPrimitive::Polygon(polygon) = primitive {
                    for point in &polygon.points {
                        assert!(point.x >= bounds.min.x - 1e-3 && point.x <= bounds.max.x + 1e-3);
                        assert!(point.y >= bounds.min.y - 1e-3 && point.y <= bounds.max.y + 1e-3);
                    }
                }
            }
            // The light on the cells copies the cells it lands on.
            let cells: Vec<_> = [3, 4, 5]
                .into_iter()
                .flat_map(|z| layer(&frame, z))
                .filter_map(|primitive| match primitive {
                    RenderPrimitive::Polygon(polygon) => Some(polygon.points),
                    _ => None,
                })
                .collect();
            let lit = layer(&frame, LIT_LAYER);
            for primitive in &lit {
                let RenderPrimitive::Polygon(polygon) = primitive else {
                    panic!("the light on the cells is cell-shaped");
                };
                assert!(cells.contains(&polygon.points));
            }
            assert!(
                count(&frame) < count(&steady) + 160,
                "{aspect}: {}",
                count(&frame)
            );
            let look = state.lamps.look(0);
            if look.narrow == 1.0 {
                assert!(
                    !lit.is_empty(),
                    "{aspect} tick {tick}: the pool lights nothing"
                );
                pool_x.push(look.target.x);
            }
            ClockScenario::step(&mut state, &[], TICK);
        }
        // It settles on the digit nearest its lamp and sweeps away from it.
        assert!(pool_x.first() < pool_x.last(), "{pool_x:?}");
        assert!(pool_x.windows(2).all(|pair| pair[0] <= pair[1]), "{aspect}");
        assert!(state.lamps_state().mood.is_none());
        assert_eq!(ClockScenario::render_frame(&state), steady);
    }
}

#[test]
fn an_event_starting_under_a_spotlight_takes_the_scene_back_within_half_a_second() {
    let mut state = ready(ClockEventProfile::Off, 11);
    state.preview_lamp_mood(ClockLampMood::Spotlight, ClockLampSide::Right);
    for _ in 0..150 {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(state.lamps.look(1).narrow, 1.0);
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        TICK,
    );
    assert!(state.lamps_state().mood.unwrap().yielding);
    for _ in 0..YIELD_TICKS {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(state.lamps_state().mood, None);
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
}

#[test]
fn moods_fit_between_the_events_and_minute_changes_of_a_running_clock() {
    for (profile, minutes, seed) in [
        (ClockEventProfile::Demo, 6, 42),
        (ClockEventProfile::Demo, 6, 7),
        (ClockEventProfile::Calm, 15, 3),
    ] {
        let mut state = ready(profile, seed);
        let mut second = 12 * 3600 + 39 * 60 + 50;
        let mut started = 0;
        for tick in 1..minutes * 60 * 60 {
            // The host's reading moves on once a second, and a Digit Slide
            // comes with every minute.
            let mut actions = Vec::new();
            if tick % 60 == 0 {
                second += 1;
                let [hour, minute, second] = [second / 3600 % 24, second / 60 % 60, second % 60];
                actions.push(ClockAction::set_reading(
                    ClockReading::new(hour as u8, minute as u8, second as u8).unwrap(),
                ));
            }
            ClockScenario::step(&mut state, &actions, TICK);
            if let Some(mood) = state.lamps.mood() {
                started += u32::from(mood.tick == 0);
                assert!(
                    !mood.yielding(),
                    "{profile:?} seed {seed} tick {tick}: {mood:?} cut short by {:?}",
                    state.event_kind()
                );
            }
        }
        assert!(started >= 2, "{profile:?} seed {seed}: {started} moods");
    }
}
