//! Cell Swap carries each of the face's surfaces along its exchange to the
//! cell it lands on, and a reading that moves on fades surfaces out and new
//! ones in rather than handing one over.

use super::*;
use crate::events::cell_swap::{CELL_SWAP_TICKS, CellPosition, CellSwapEvent};

fn event(state: &ClockState) -> &CellSwapEvent {
    match &state.active_event {
        Some(crate::events::ActiveEvent::CellSwap(event)) => event,
        _ => panic!("a cell swap is running"),
    }
}

fn sorted(surfaces: &[DigitSurface]) -> Vec<SurfaceId> {
    let mut ids = ids(surfaces);
    ids.sort_unstable();
    ids
}

/// Checks that no cell holds more than one surface coming home and one fading
/// out of it.
fn assert_at_most_two_per_cell(surfaces: &[DigitSurface], context: &str) {
    for surface in surfaces {
        let sharing = surfaces
            .iter()
            .filter(|other| (other.origin - surface.origin).length() < 1e-3)
            .count();
        assert!(sharing <= 2, "{context}: {sharing} surfaces on one cell");
    }
}

#[test]
fn surface_contract_a_cell_swap_carries_each_surface_to_the_cell_it_lands_on() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let mut state = clock(
                aspect,
                font,
                ClockTimeFormat::TwentyFourHour,
                reading(10, 28, 0),
            );
            let layout = Layout::new(aspect);
            let local = Vec2::new(0.25, -0.2);
            let mut markers = Markers::attach(&state, |_| Some(local));
            let face = state.digit_surfaces();
            let accounting = state.surface_accounting();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::CellSwap)],
            );
            // Which surface each exchange carries: the face's own on its cell.
            let carried: Vec<(SurfaceId, CellPosition)> = event(&state)
                .exchanges
                .iter()
                .map(|exchange| {
                    let surface = face
                        .iter()
                        .find(|s| s.slot == exchange.from.slot && s.cell == exchange.from.cell)
                        .expect("every exchange starts on a surface");
                    (surface.id, exchange.to)
                })
                .collect();
            assert_eq!(carried.len(), face.len(), "{context}");
            assert!(
                event(&state)
                    .exchanges
                    .iter()
                    .any(|exchange| exchange.from != exchange.to),
                "{context}"
            );
            let mut steps = [face.clone(), face.clone()];
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::CellSwap) {
                let context = format!("{context}, tick {ticks}");
                let surfaces = markers.follow(&state);
                assert_eq!(sorted(&surfaces), sorted(&face), "{context}");
                assert!(
                    surfaces.iter().all(|s| s.source == SurfaceSource::Swap
                        && s.eligible()
                        && s.opacity == 1.0),
                    "{context}"
                );
                assert_no_jump([&steps[0], &steps[1], &surfaces], layout.pitch, &context);
                if ticks % 3 == 0 || ticks + 2 > CELL_SWAP_TICKS {
                    assert_face_agreement(&state, &context);
                }
                steps = [std::mem::take(&mut steps[1]), surfaces];
                tick(&mut state, &[]);
                ticks += 1;
            }
            assert_eq!(ticks, CELL_SWAP_TICKS, "{context}: nothing to fade");
            let after = markers.follow(&state);
            assert_no_jump([&steps[0], &steps[1], &after], layout.pitch, &context);
            assert_face_agreement(&state, &context);
            for (id, to) in &carried {
                let surface = after
                    .iter()
                    .find(|surface| surface.id == *id)
                    .expect("every carried surface lands");
                assert_eq!(surface.source, SurfaceSource::Anchored, "{context}");
                assert_eq!(
                    surface.origin,
                    layout.slot_cell_center(usize::from(to.slot), to.cell),
                    "{context}: surface {} lands where its exchange ends",
                    id.get()
                );
                let marker = markers
                    .markers
                    .iter()
                    .find(|marker| marker.surface == *id)
                    .expect("every surface carries a marker");
                assert_eq!(marker.lost, None, "{context}");
                assert!(
                    (marker.world - surface.world(local)).length() < 1e-3,
                    "{context}"
                );
            }
            let now = state.surface_accounting();
            assert_eq!(now.admitted, accounting.admitted, "{context}");
            assert_eq!(now.total_retired(), 0, "{context}");
            for marker in &markers.markers {
                let handoffs: Vec<_> = markers
                    .handoffs
                    .iter()
                    .filter(|(id, ..)| *id == marker.surface)
                    .map(|(_, from, to)| (*from, *to))
                    .collect();
                assert_eq!(
                    handoffs,
                    [
                        (SurfaceSource::Anchored, SurfaceSource::Swap),
                        (SurfaceSource::Swap, SurfaceSource::Anchored)
                    ],
                    "{context}"
                );
            }
        }
    }
}

#[test]
fn surface_contract_a_reading_that_keeps_changing_fades_surfaces_in_and_out() {
    for (index, font) in FONTS.into_iter().enumerate() {
        let aspect = ASPECTS[index % ASPECTS.len()];
        let context = format!("{font:?} at {aspect}");
        let mut state = clock(
            aspect,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 59),
        );
        let layout = Layout::new(aspect);
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::CellSwap)],
        );
        let face = state.digit_surfaces();
        let mut ink = shared_ink(&face);
        let mut steps = [face.clone(), face.clone()];
        let mut ticks = 0;
        while state.event_kind() == Some(ClockEventKind::CellSwap) {
            // Once on the way, then every tick of the first half of the fade,
            // as a host correcting its clock might.
            let actions = match ticks {
                20 => vec![ClockAction::set_reading(reading(12, 35, 0))],
                62..=75 if ticks % 2 == 0 => vec![ClockAction::set_reading(reading(1, 11, 0))],
                62..=75 => vec![ClockAction::set_reading(reading(8, 8, 0))],
                _ => Vec::new(),
            };
            tick(&mut state, &actions);
            ticks += 1;
            let context = format!("{context}, tick {ticks}");
            let surfaces = markers.follow(&state);
            assert_agreement(&state, &context, |_, _| false);
            assert_no_jump([&steps[0], &steps[1], &surfaces], layout.pitch, &context);
            if ticks >= CELL_SWAP_TICKS {
                // A fade's steepest step is a tenth of a cell's ink.
                ink = assert_ink_continues(&ink, &surfaces, 0.11, &context);
                assert_one_per_cell(&surfaces, &context);
                assert_at_most_two_per_cell(&surfaces, &context);
            } else {
                ink = shared_ink(&surfaces);
            }
            steps = [std::mem::take(&mut steps[1]), surfaces];
        }
        let after = markers.follow(&state);
        assert_face_agreement(&state, &context);
        // 8:08 was the last reading, set on tick 75.
        assert_eq!(state.display().digits, [Some(0), Some(8), Some(0), Some(8)]);
        assert_eq!(after.len(), lit_cells(&state).iter().sum::<usize>());
        let accounting = state.surface_accounting();
        assert!(
            accounting.peak_live <= crate::digit_surfaces::MAX_SURFACES,
            "{context}"
        );
        assert_eq!(accounting.retired(Retirement::Interrupted), 0, "{context}");
        for marker in &markers.markers {
            match marker.lost {
                // Still on a lit cell of every reading since the fade began.
                None => assert!(after.iter().any(|s| s.id == marker.surface)),
                Some(lost) => {
                    assert_eq!(lost, Loss::Retired(Retirement::Replaced), "{context}");
                }
            }
        }
        assert!(markers.attached().next().is_some(), "{context}");
        assert!(
            markers.markers.iter().any(|marker| marker.lost.is_some()),
            "{context}"
        );
    }
}

#[test]
fn surface_contract_a_cell_swap_cut_short_retires_what_it_left_on_the_way() {
    for stop in [0, 30, 70] {
        let context = format!("stopped at {stop}");
        let mut state = clock(
            4.0 / 3.0,
            ClockFont::Sans,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 59),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let face = state.digit_surfaces();
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::CellSwap)],
        );
        let exchanges = event(&state).exchanges.clone();
        let changed = stop > CELL_SWAP_TICKS;
        for tick_index in 0..stop {
            let actions = if changed && tick_index == 10 {
                vec![ClockAction::set_reading(reading(12, 35, 0))]
            } else {
                Vec::new()
            };
            tick(&mut state, &actions);
            markers.follow(&state);
        }
        // Paused, the swap neither moves nor records anything.
        let held = state.digit_surfaces();
        let sequence = state.surface_accounting().sequence;
        for _ in 0..3 {
            sync(&mut state, &[]);
            assert_eq!(state.digit_surfaces(), held, "{context}");
        }
        assert_eq!(state.surface_accounting().sequence, sequence, "{context}");
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        );
        let after = markers.follow(&state);
        assert_face_agreement(&state, &context);
        for surface in &held {
            let kept = after.iter().any(|other| other.id == surface.id);
            let still = |id| {
                let start = face.iter().find(|s| s.id == id);
                start.is_some_and(|start| {
                    exchanges.iter().any(|exchange| {
                        exchange.from.slot == start.slot
                            && exchange.from.cell == start.cell
                            && exchange.from == exchange.to
                    })
                })
            };
            // A piece that has landed, or never left, is home already. One
            // on its way is retired there, and one fading out either way.
            let expected = match stop {
                0 => true,
                _ if stop < CELL_SWAP_TICKS => still(surface.id),
                _ => !surface.departing,
            };
            assert_eq!(kept, expected, "{context}: surface {}", surface.id.get());
            if !kept && let Some(marker) = markers.markers.iter().find(|m| m.surface == surface.id)
            {
                let reason = if surface.departing {
                    Retirement::Replaced
                } else {
                    Retirement::Interrupted
                };
                assert_eq!(marker.lost, Some(Loss::Retired(reason)), "{context}");
            }
        }
        if stop == 0 {
            assert_eq!(sorted(&after), sorted(&face), "{context}");
        }
        if changed {
            assert!(held.iter().any(|s| s.departing), "{context}");
        }
    }
}
