use super::*;
use engine_common::ClockCellSwapPattern;
use events::cell_swap::CellSwapEvent;
use std::collections::BTreeSet;

fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second).unwrap()
}

fn ready(from: ClockReading, format: ClockTimeFormat, font: ClockFont) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            time_format: format,
            event_profile: ClockEventProfile::Off,
            fonts: ClockFontSettings {
                selected: font,
                ..Default::default()
            },
            ..Default::default()
        },
        42,
    );
    state.apply_reading(from, false);
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
    }
}

fn event(state: &ClockState) -> &CellSwapEvent {
    let Some(ActiveEvent::CellSwap(event)) = &state.active_event else {
        panic!("no cell swap")
    };
    event
}

#[test]
fn mappings_are_seeded_permutations_of_only_occupied_cells_for_every_font() {
    for font in ClockFont::ALL {
        let mut patterns = Vec::new();
        for from in [
            reading(8, 8, 0),
            reading(1, 12, 0),
            reading(23, 59, 0),
            reading(11, 11, 0),
        ] {
            let state = ready(from, ClockTimeFormat::TwelveHour, font);
            for seed in 0..24 {
                let swap = CellSwapEvent::new(state.display(), seed);
                let replay = CellSwapEvent::new(state.display(), seed);
                assert_eq!(swap.exchanges, replay.exchanges);
                assert_eq!(swap.diagnostics(), replay.diagnostics());
                patterns.push(swap.pattern);
                let sources: BTreeSet<_> = swap.exchanges.iter().map(|c| c.from.index()).collect();
                let destinations: BTreeSet<_> =
                    swap.exchanges.iter().map(|c| c.to.index()).collect();
                assert_eq!(sources, destinations);
                assert_eq!(sources.len(), swap.exchanges.len());
                assert!(sources.len() <= fonts::MAX_DIGIT_CELLS);
                assert!(swap.exchanges.iter().any(|cell| cell.from != cell.to));
                assert!(
                    std::mem::size_of::<CellSwapEvent>()
                        + swap.exchanges.capacity()
                            * std::mem::size_of::<events::cell_swap::Exchange>()
                        <= 4096
                );
                for exchange in &swap.exchanges {
                    assert_eq!(exchange.position(0.0), exchange.from.point());
                    assert_eq!(exchange.position(1.0), exchange.to.point());
                    if exchange.from != exchange.to {
                        let a = exchange.from.point();
                        let b = exchange.to.point();
                        match swap.pattern {
                            ClockCellSwapPattern::MirrorHorizontal => {
                                assert_eq!(a.x, b.x);
                                assert_eq!(a.y + b.y, 9.0);
                            }
                            ClockCellSwapPattern::MirrorVertical => {
                                assert_eq!(a.x + b.x, 30.0);
                                assert_eq!(a.y, b.y);
                            }
                            ClockCellSwapPattern::Shuffle => (),
                        }
                    }
                    for tick in 0..=60 {
                        let position = exchange.position(tick as f32 / 60.0);
                        assert!((0.49999..=29.50001).contains(&position.x));
                        assert!((0.49999..=8.50001).contains(&position.y));
                    }
                }
            }
        }
        assert!(
            ClockCellSwapPattern::ALL
                .into_iter()
                .all(|p| patterns.contains(&p))
        );
    }
}

#[test]
fn unchanged_swaps_finish_at_the_destination_in_one_second_without_reassembly() {
    for font in ClockFont::ALL {
        let mut state = ready(reading(12, 34, 0), ClockTimeFormat::TwentyFourHour, font);
        let before = ClockScenario::render_frame(&state);
        state.preview_event(ClockEventKind::CellSwap);
        ticks(&mut state, 30);
        assert_ne!(ClockScenario::render_frame(&state), before);
        assert_eq!(state.event_phase(), Some(EventPhase::Swapping));
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        ticks(&mut state, 30);
        assert!(state.cell_swap_state().is_none());
        assert_eq!(ClockScenario::render_frame(&state), before);
        assert_eq!(
            state.event_ready_at_tick(ClockEventKind::CellSwap),
            60 + 20 * 60
        );
        assert!(state.next_event_tick().is_none());
    }
}

#[test]
fn rollovers_keep_the_original_paths_then_reconcile_increasing_and_decreasing_masks() {
    for font in ClockFont::ALL {
        for (from, to, format) in [
            (
                reading(12, 34, 59),
                reading(12, 35, 0),
                ClockTimeFormat::TwentyFourHour,
            ),
            (
                reading(12, 59, 59),
                reading(13, 0, 0),
                ClockTimeFormat::TwelveHour,
            ),
            (
                reading(9, 59, 59),
                reading(10, 0, 0),
                ClockTimeFormat::TwelveHour,
            ),
            (
                reading(23, 59, 59),
                reading(0, 0, 0),
                ClockTimeFormat::TwentyFourHour,
            ),
        ] {
            for change_at in [0, 1, 30, 59] {
                let mut state = ready(from, format, font);
                state.preview_event(ClockEventKind::CellSwap);
                ticks(&mut state, change_at);
                let mapping = event(&state).exchanges.clone();
                let opacity = event(&state).opacity;
                state.apply_reading(to, true);
                assert_eq!(event(&state).exchanges, mapping);
                assert_eq!(event(&state).tick, change_at);
                assert_eq!(event(&state).opacity, opacity);
                assert_eq!(state.reading(), Some(to));
                assert!(state.digit_slide_state().is_none());
                ticks(&mut state, CELL_SWAP_TICKS - change_at);
                assert_eq!(state.event_phase(), Some(EventPhase::Reconciling));
                ticks(&mut state, CELL_SWAP_FADE_TICKS / 2);
                assert!(event(&state).opacity.iter().any(|v| *v > 0.0 && *v < 1.0));
                ticks(&mut state, CELL_SWAP_FADE_TICKS - CELL_SWAP_FADE_TICKS / 2);
                assert!(state.cell_swap_state().is_none());
                assert_eq!(state.event_id(), 1);
                assert_eq!(
                    ClockScenario::render_frame(&state),
                    ClockScenario::render_frame(&ready(to, format, font))
                );
            }
        }
    }
}

#[test]
fn corrections_retarget_current_brightness_without_jumps_or_unbounded_extension() {
    let mut state = ready(
        reading(12, 34, 59),
        ClockTimeFormat::TwentyFourHour,
        ClockFont::Classic,
    );
    state.preview_event(ClockEventKind::CellSwap);
    state.apply_reading(reading(12, 35, 0), true);
    ticks(&mut state, 60);
    for tick in 60..90 {
        let before = event(&state).opacity;
        let latest = if tick % 2 == 0 {
            reading(1, 11, 0)
        } else {
            reading(8, 8, 0)
        };
        state.apply_reading(latest, true);
        assert_eq!(event(&state).opacity, before);
        assert_eq!(event(&state).tick, tick);
        ticks(&mut state, 1);
    }
    assert!(state.cell_swap_state().is_none());
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.display().digits, [Some(0), Some(8), Some(0), Some(8)]);
    assert_eq!(
        ClockScenario::render_frame(&state),
        ClockScenario::render_frame(&ready(
            reading(8, 8, 0),
            state.time_format(),
            ClockFont::Classic
        ))
    );
}

#[test]
fn pause_seconds_format_and_font_changes_preserve_progress_and_use_the_latest_target() {
    let mut state = ready(
        reading(13, 34, 0),
        ClockTimeFormat::TwentyFourHour,
        ClockFont::Classic,
    );
    state.preview_event(ClockEventKind::CellSwap);
    ticks(&mut state, 30);
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..20 {
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(13, 34, 2))],
            Duration::ZERO,
        );
        assert_eq!(ClockScenario::render_frame(&state), frame);
        assert_eq!(state.cell_swap_state().unwrap().elapsed_ticks, 30);
    }
    let mut settings = state.settings();
    settings.time_format = ClockTimeFormat::TwelveHour;
    settings.fonts.selected = ClockFont::Serif;
    settings.events.cell_swap = false;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert_eq!(state.active_font(), ClockFont::Classic);
    assert_eq!(state.display().digits[0], None);
    assert_eq!(state.cell_swap_state().unwrap().elapsed_ticks, 30);
    ticks(&mut state, 45);
    assert!(state.cell_swap_state().is_none());
    assert_eq!(state.active_font(), ClockFont::Serif);
    assert_eq!(
        ClockScenario::render_frame(&state),
        ClockScenario::render_frame(&ready(
            reading(13, 34, 2),
            ClockTimeFormat::TwelveHour,
            ClockFont::Serif
        ))
    );
}

#[test]
fn replacements_resize_and_duck_ownership_restore_current_digits_without_owned_physics() {
    let mut state = ready(
        reading(12, 34, 0),
        ClockTimeFormat::TwentyFourHour,
        ClockFont::Classic,
    );
    state.preview_event(ClockEventKind::Duck);
    ticks(&mut state, 30);
    let counts = (state.body_count(), state.collider_count());
    let floor = state.floor_mode();
    for _ in 0..3 {
        state.preview_event(ClockEventKind::CellSwap);
        assert!(state.has_duck_visit());
        assert_eq!(state.floor_mode(), floor);
        assert_eq!((state.body_count(), state.collider_count()), counts);
        state.apply_reading(reading(8, 8, 0), false);
        state.preview_event(ClockEventKind::ColorCycle);
        assert!(state.cell_swap_state().is_none());
        assert!(state.has_duck_visit());
        assert_eq!((state.body_count(), state.collider_count()), counts);
    }
    state.preview_event(ClockEventKind::CellSwap);
    state.set_aspect_ratio(0.6);
    assert!(state.cell_swap_state().is_none());
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    assert_eq!(state.display().digits, [Some(0), Some(8), Some(0), Some(8)]);
}

#[test]
fn rendered_paths_and_glow_stay_inside_all_target_frames_with_bounded_primitives() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0, 0.25, 4.0] {
        for font in ClockFont::ALL {
            let mut state = ready(reading(8, 8, 0), ClockTimeFormat::TwentyFourHour, font);
            state.set_aspect_ratio(aspect);
            state.preview_event(ClockEventKind::CellSwap);
            for _ in 0..CELL_SWAP_TICKS {
                let frame = ClockScenario::render_frame(&state);
                let bounds = frame.camera.world_bounds(aspect);
                assert!(
                    frame
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                        < 1600
                );
                for primitive in frame.layers.iter().flat_map(|l| &l.primitives) {
                    if let engine_common::RenderPrimitive::Polygon(p) = primitive {
                        assert!(p.points.iter().all(|p| p.x.is_finite()
                            && p.y.is_finite()
                            && p.x >= bounds.min.x
                            && p.x <= bounds.max.x
                            && p.y >= bounds.min.y
                            && p.y <= bounds.max.y));
                    }
                }
                ticks(&mut state, 1);
            }
        }
    }
}
