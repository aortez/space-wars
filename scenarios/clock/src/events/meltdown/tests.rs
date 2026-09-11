use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, ClockTimeFormat, Scenario};
use std::time::Duration;

fn ready(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Meltdown),
        ],
        Duration::ZERO,
    );
    state
}

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

fn assert_volume(state: &ClockState) {
    let stats = state.meltdown_state().unwrap();
    let accounted = ((stats.waiting_cells + stats.airborne_cells) as u64) * 1_000_000
        + stats.pooled_microunits
        + stats.drained_microunits
        + stats.reclaimed_microunits;
    assert!(
        accounted.abs_diff(stats.initial_cells as u64 * 1_000_000) <= 1,
        "{stats:?}"
    );
    assert!(stats.initial_cells <= MAX_MELTDOWN_CELLS);
    assert!(stats.water_columns <= WATER_COLUMNS);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event else {
        panic!()
    };
    assert!(event.water.iter().all(|v| v.is_finite() && *v >= 0.0));
    assert!(
        event
            .cells
            .iter()
            .all(|c| c.position.x.is_finite() && c.position.y.is_finite())
    );
}

#[test]
fn cells_pool_drain_and_reform_with_conserved_bounded_material_at_every_aspect() {
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        for seed in [0, 42, 4242] {
            let mut state = ready(aspect, seed);
            let initial = state.meltdown_state().unwrap().initial_cells;
            assert!(initial > 60);
            let mut pooled = false;
            let mut falling = false;
            for _ in 0..MELTING_TICKS + DRAINING_TICKS {
                assert_volume(&state);
                let s = state.meltdown_state().unwrap();
                pooled |= s.pooled_microunits > 1_000_000;
                falling |= s.airborne_cells > 0;
                tick(&mut state);
            }
            assert!(pooled && falling);
            assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
            assert_volume(&state);
            let stats = state.meltdown_state().unwrap();
            assert_eq!(
                (
                    stats.waiting_cells,
                    stats.airborne_cells,
                    stats.water_columns
                ),
                (0, 0, 0)
            );
            assert!(
                stats.drained_microunits > initial as u64 * 990_000,
                "{aspect}: {stats:?}"
            );
            for _ in 0..REFORMING_TICKS {
                tick(&mut state);
            }
            assert_eq!(state.meltdown_state(), None);
            assert!(
                state
                    .segments()
                    .iter()
                    .all(|s| s.representation == SegmentRepresentation::Anchored)
            );
        }
    }
}

#[test]
fn pool_passes_are_symmetric_nonnegative_and_conservative() {
    let mut state = ready(800.0 / 480.0, 0);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    event.cells.clear();
    event.water[10] = 48.0;
    event.water[WATER_COLUMNS - 11] = 48.0;
    event.initial_cells = 96;
    for _ in 0..300 {
        event.step_material(Layout::new(800.0 / 480.0));
        assert!(event.water.iter().all(|v| *v >= 0.0));
        for i in 0..SIDE_COLUMNS {
            assert!((event.water[i] - event.water[WATER_COLUMNS - 1 - i]).abs() < 1e-10);
        }
        assert!((event.water.iter().sum::<f64>() + event.drained - 96.0).abs() < 1e-9);
    }
    assert!(event.drained > 95.9);
}

#[test]
fn pause_settings_time_corrections_and_resize_preserve_or_clean_the_right_state() {
    let mut state = ready(800.0 / 480.0, 4);
    for _ in 0..100 {
        tick(&mut state);
    }
    let material = state.meltdown_state();
    let frame = ClockScenario::render_frame(&state);
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(ClockScenario::render_frame(&state), frame);
    let settings = ClockSettings {
        time_format: ClockTimeFormat::TwelveHour,
        event_profile: ClockEventProfile::Off,
        ..state.settings()
    };
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::configure(settings),
            ClockAction::set_reading(ClockReading::new(0, 1, 0).unwrap()),
        ],
        Duration::ZERO,
    );
    assert_eq!(state.meltdown_state(), material);
    assert_eq!(state.phase_tick(), 100);
    assert_eq!(state.display().digits, [Some(1), Some(2), Some(0), Some(1)]);
    for _ in 100..MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS {
        tick(&mut state);
    }
    let mut reference = ClockScenario::init(
        ClockConfig {
            time_format: ClockTimeFormat::TwelveHour,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        4,
    );
    ClockScenario::step(
        &mut reference,
        &[ClockAction::set_reading(
            ClockReading::new(0, 1, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(
        ClockScenario::render_frame(&state),
        ClockScenario::render_frame(&reference)
    );
    for elapsed in [20, 210, 450] {
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
            Duration::ZERO,
        );
        for _ in 0..elapsed {
            tick(&mut state);
        }
        state.set_aspect_ratio(if state.aspect_ratio() == 1.0 {
            0.75
        } else {
            1.0
        });
        assert_eq!(state.meltdown_state(), None);
        assert_eq!(state.event_kind(), None);
    }
}

#[test]
fn repeated_previews_and_seeded_motion_are_identical_and_release_material() {
    let mut a = ready(800.0 / 480.0, 42);
    let mut b = ready(800.0 / 480.0, 42);
    for cycle in 0..12 {
        for kind in ClockEventKind::ALL {
            for state in [&mut a, &mut b] {
                ClockScenario::step(state, &[ClockAction::preview_event(kind)], Duration::ZERO);
            }
            for n in 0..(100 + cycle * 31) {
                tick(&mut a);
                tick(&mut b);
                assert_eq!(a.meltdown_state(), b.meltdown_state());
                if a.meltdown_state().is_some() {
                    assert_volume(&a);
                }
                if n % 30 == 0 {
                    assert_eq!(
                        ClockScenario::render_frame(&a),
                        ClockScenario::render_frame(&b)
                    );
                }
            }
            if kind != ClockEventKind::Meltdown {
                assert_eq!(a.meltdown_state(), None);
            }
        }
    }
}
