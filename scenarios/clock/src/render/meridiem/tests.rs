use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, DRAINING_TICKS, FALLING_TICKS,
    MAX_MELTDOWN_CELLS, MAX_SPILL_PARCELS, MELTING_TICKS,
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockTimeFormat, Scenario};
use std::time::Duration;

const ASPECTS: [f32; 4] = [0.6, 1024.0 / 768.0, 800.0 / 480.0, 4.0];

fn ready(aspect_ratio: f32, hour: u8, time_format: ClockTimeFormat) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio,
            time_format,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(hour, 59, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn preview(state: &mut ClockState, kind: ClockEventKind) {
    ClockScenario::step(state, &[ClockAction::preview_event(kind)], Duration::ZERO);
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
    }
}

fn label_pixels(state: &ClockState) -> Vec<RenderPrimitive> {
    ClockScenario::render_frame(state)
        .layers
        .into_iter()
        .filter(|layer| layer.z == LABEL_LAYER)
        .flat_map(|layer| layer.primitives)
        .collect()
}

fn clean_frame(state: &ClockState) -> RenderFrame {
    let mut clean = ClockScenario::init(state.config, 42);
    ClockScenario::step(
        &mut clean,
        &[ClockAction::set_reading(state.reading().unwrap())],
        Duration::ZERO,
    );
    ClockScenario::render_frame(&clean)
}

fn assert_volume(state: &ClockState) {
    let m = state.meltdown_state().unwrap();
    let total = m.solid_microunits
        + m.pooled_microunits
        + m.spilling_microunits
        + m.drained_microunits
        + m.reclaimed_microunits;
    assert!(total.abs_diff(m.initial_microunits) <= 3, "{m:?}");
    assert!(m.initial_cells <= MAX_MELTDOWN_CELLS);
    assert!(m.spill_parcels <= MAX_SPILL_PARCELS);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}

#[test]
fn falling_meridiem_uses_two_compound_letters_and_releases_them_before_reforming() {
    for aspect in ASPECTS {
        for (hour, count) in [(0, 23), (12, 21)] {
            let mut state = ready(aspect, hour, ClockTimeFormat::TwelveHour);
            let original = label_pixels(&state);
            assert_eq!(original.len(), count);
            let segments = state
                .segments()
                .iter()
                .filter(|s| s.lit)
                .collect::<Vec<_>>();
            let bodies = segments.len() + 6;
            let colliders = segments
                .iter()
                .map(|s| digits::cells(s.id.kind).len())
                .sum::<usize>()
                + 4
                + count;
            preview(&mut state, ClockEventKind::Falling);
            assert_eq!(
                (state.body_count(), state.collider_count()),
                (bodies, colliders)
            );
            assert_eq!(
                label_pixels(&state),
                original,
                "no duplicate anchored label"
            );
            let mut rotated = [false; 2];
            for _ in 0..60 {
                ticks(&mut state, 1);
                let Some(ActiveEvent::Falling(event)) = &state.active_event else {
                    panic!()
                };
                for (slot, letter) in event.letters().unwrap().iter().enumerate() {
                    assert!(
                        letter.position.x.is_finite()
                            && letter.position.y.is_finite()
                            && letter.angle.is_finite()
                    );
                    rotated[slot] |= letter.angle.abs() > 0.01;
                }
            }
            assert_ne!(label_pixels(&state), original);
            assert_eq!(
                rotated, [true; 2],
                "both compound letters can tumble, even if they settle flat"
            );
            ticks(&mut state, FALLING_TICKS - 60);
            assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            ticks(&mut state, REFORMING_TICKS);
            assert!(state.active_event.is_none());
            assert_eq!(ClockScenario::render_frame(&state), clean_frame(&state));
        }
    }
}

#[test]
fn water_lab_keeps_the_live_meridiem_and_its_fixed_material_budget() {
    let mut state = ready(800.0 / 480.0, 11, ClockTimeFormat::TwelveHour);
    state.config.water_lab = crate::ClockWaterLab::Cascade;
    let original = label_pixels(&state);
    preview(&mut state, ClockEventKind::Meltdown);
    ticks(&mut state, 90);
    assert_eq!(label_pixels(&state), original);
    let m = state.meltdown_state().unwrap();
    assert_eq!(
        (m.initial_cells, m.initial_microunits, m.solid_microunits),
        (24, 24_000_000, 0)
    );
}

#[test]
fn melting_meridiem_uses_small_pixels_conserves_volume_and_reforms_the_same_label() {
    for aspect in ASPECTS {
        for (hour, count) in [(0, 23), (12, 21)] {
            let mut state = ready(aspect, hour, ClockTimeFormat::TwelveHour);
            let original = label_pixels(&state);
            preview(&mut state, ClockEventKind::Meltdown);
            assert_eq!(
                label_pixels(&state),
                original,
                "only the event's original pixels are drawn"
            );
            let m = state.meltdown_state().unwrap();
            assert_eq!(
                m.initial_microunits,
                (m.initial_cells - count) as u64 * 1_000_000 + count as u64 * 32_400
            );
            let mut moved = false;
            for tick in 0..MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS {
                assert_volume(&state);
                let Some(ActiveEvent::Meltdown(event)) = &state.active_event else {
                    panic!()
                };
                let remaining = event.cells.iter().filter(|cell| cell.meridiem).count();
                if tick < MELTING_TICKS + DRAINING_TICKS {
                    assert_eq!(label_pixels(&state).len(), remaining);
                    moved |= remaining > 0 && label_pixels(&state) != original;
                }
                if tick == MELTING_TICKS {
                    assert_eq!(
                        remaining, 0,
                        "all letter pixels should have reached the floor"
                    );
                }
                if tick == MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS - 1 {
                    assert_eq!(
                        label_pixels(&state),
                        original,
                        "last reform frame restores the label exactly"
                    );
                }
                ticks(&mut state, 1);
            }
            assert!(moved);
            assert!(state.meltdown_state().is_none());
            assert_eq!(ClockScenario::render_frame(&state), clean_frame(&state));
        }
    }
}

#[test]
fn noon_midnight_and_format_changes_recover_the_latest_meridiem_without_advancing_paused_events() {
    use ClockTimeFormat::{TwelveHour, TwentyFourHour};
    for (kind, recovery) in [
        (ClockEventKind::Falling, FALLING_TICKS),
        (ClockEventKind::Meltdown, MELTING_TICKS + DRAINING_TICKS),
    ] {
        for (from_hour, from_format, to_hour, to_format) in [
            (11, TwelveHour, 12, TwelveHour),
            (23, TwelveHour, 0, TwelveHour),
            (8, TwelveHour, 8, TwentyFourHour),
            (8, TwentyFourHour, 8, TwelveHour),
        ] {
            for change_tick in [60, recovery + 30] {
                let mut state = ready(800.0 / 480.0, from_hour, from_format);
                preview(&mut state, kind);
                ticks(&mut state, change_tick);
                let before = ClockScenario::render_frame(&state);
                let simulation_tick = state.simulation_tick();
                let mut settings = state.settings();
                settings.time_format = to_format;
                ClockScenario::step(
                    &mut state,
                    &[
                        ClockAction::configure(settings),
                        ClockAction::set_reading(ClockReading::new(to_hour, 0, 0).unwrap()),
                    ],
                    Duration::ZERO,
                );
                assert_eq!(state.simulation_tick(), simulation_tick);
                let corrected = ClockScenario::render_frame(&state);
                if change_tick < recovery {
                    assert_eq!(
                        corrected, before,
                        "released geometry stays latched until recovery"
                    );
                }
                ClockScenario::step(&mut state, &[], Duration::ZERO);
                assert_eq!(ClockScenario::render_frame(&state), corrected);
                for _ in change_tick..recovery + REFORMING_TICKS {
                    if kind == ClockEventKind::Meltdown {
                        assert_volume(&state);
                    }
                    ticks(&mut state, 1);
                }
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                assert!(state.active_event.is_none());
                assert_eq!(ClockScenario::render_frame(&state), clean_frame(&state));
                assert_eq!(label_pixels(&state).is_empty(), to_format == TwentyFourHour);
            }
        }
    }
}

#[test]
fn meridiem_events_replay_replace_resize_and_restart_without_leftover_geometry() {
    for (kind, recovery) in [
        (ClockEventKind::Falling, FALLING_TICKS),
        (ClockEventKind::Meltdown, MELTING_TICKS + DRAINING_TICKS),
    ] {
        for elapsed in [60, recovery + 30] {
            let make = || {
                let mut state = ready(1024.0 / 768.0, 11, ClockTimeFormat::TwelveHour);
                preview(&mut state, kind);
                ticks(&mut state, elapsed);
                state
            };
            let mut a = make();
            let b = make();
            assert_eq!(
                ClockScenario::render_frame(&a),
                ClockScenario::render_frame(&b)
            );
            assert_eq!(a.meltdown_state(), b.meltdown_state());
            ClockScenario::step(
                &mut a,
                &[ClockAction::set_reading(
                    ClockReading::new(12, 0, 0).unwrap(),
                )],
                Duration::ZERO,
            );
            a.set_aspect_ratio(0.6);
            assert!(a.active_event.is_none());
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            assert_eq!(ClockScenario::render_frame(&a), clean_frame(&a));
            for replacement in [
                ClockEventKind::Falling,
                ClockEventKind::Meltdown,
                ClockEventKind::ColorCycle,
            ] {
                preview(&mut a, replacement);
                ticks(&mut a, 60);
                assert!(a.body_count() <= 34 && a.collider_count() <= 123);
                assert_eq!(
                    a.meltdown_state().is_some(),
                    replacement == ClockEventKind::Meltdown
                );
            }
            assert_eq!(label_pixels(&a).len(), 21);
            // Host restart discards the entire event-owning ClockState.
            a = ClockScenario::init(a.config, 42);
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            assert!(a.meltdown_state().is_none());
            assert!(label_pixels(&a).is_empty());
            ClockScenario::step(
                &mut a,
                &[ClockAction::set_reading(
                    ClockReading::new(12, 0, 0).unwrap(),
                )],
                Duration::ZERO,
            );
            assert_eq!(ClockScenario::render_frame(&a), clean_frame(&a));
        }
    }
}
