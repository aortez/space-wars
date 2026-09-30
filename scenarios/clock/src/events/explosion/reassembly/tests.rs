use super::*;
use crate::ClockReading;
use engine_common::ClockTimeFormat;

fn display(hour: u8, minute: u8, format: ClockTimeFormat) -> DisplaySnapshot {
    digits::snapshot(ClockReading::new(hour, minute, 0).unwrap(), format)
}

fn debris(display: DisplaySnapshot, layout: Layout) -> Vec<Cell> {
    targets(display, layout)
        .into_iter()
        .enumerate()
        .map(|(index, target)| Cell {
            origin: target.position,
            position: Vec2::new(index as f32 * 2.0 - 100.0, layout.floor_y + 10.0),
            angle: index as f32 * 0.7 - 20.0,
            side: target.side,
            label: target.label,
            velocity: Vec2::ZERO,
            spin: 0.0,
        })
        .collect()
}

fn assert_assignments(assembly: &Reassembly, display: DisplaySnapshot, layout: Layout) {
    let targets = targets(display, layout);
    let assigned = assembly
        .flights
        .iter()
        .filter(|flight| flight.assigned)
        .map(|flight| flight.target)
        .collect::<Vec<_>>();
    assert_eq!(assigned.len(), targets.len());
    for target in targets {
        assert_eq!(assigned.iter().filter(|slot| **slot == target).count(), 1);
    }
}

fn assert_arrived(cells: &[Cell], display: DisplaySnapshot, layout: Layout) {
    let targets = targets(display, layout);
    let visible = cells
        .iter()
        .filter(|cell| cell.side > 0.0)
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), targets.len());
    for target in targets {
        assert_eq!(
            visible
                .iter()
                .filter(|cell| {
                    cell.position == target.position
                        && cell.side == target.side
                        && cell.label == target.label
                        && cell.angle == 0.0
                })
                .count(),
            1,
            "exactly one opaque block must arrive at {target:?}"
        );
    }
}

#[test]
fn unchanged_blocks_return_to_their_own_slots_with_shortest_rotation() {
    let layout = Layout::new(4.0 / 3.0);
    let display = display(23, 58, ClockTimeFormat::TwelveHour);
    let mut cells = debris(display, layout);
    let original = cells.clone();
    let assembly = Reassembly::new(&mut cells, display, layout);
    assembly.sample(&mut cells, 0);
    assert_eq!(
        cells, original,
        "reformation begins at the last physics pose"
    );
    assembly.sample(&mut cells, REFORMING_TICKS / 2);
    for (cell, before) in cells.iter().zip(&original) {
        let expected = before.position + (before.origin - before.position) * 0.5;
        assert_eq!(cell.position, expected);
        assert_eq!(cell.side, before.side);
        assert!((cell.angle - before.angle).abs() <= std::f32::consts::FRAC_PI_2 + 1e-5);
    }
    assembly.sample(&mut cells, REFORMING_TICKS);
    assert_arrived(&cells, display, layout);
    for (cell, before) in cells.iter().zip(original) {
        assert_eq!(cell.position, before.origin);
    }
}

#[test]
fn denser_sparser_midnight_and_format_changes_assign_every_target_once() {
    use ClockTimeFormat::{TwelveHour, TwentyFourHour};
    let readings = [
        display(11, 11, TwentyFourHour),
        display(20, 8, TwentyFourHour),
        display(23, 59, TwentyFourHour),
        display(0, 0, TwentyFourHour),
        display(11, 59, TwelveHour),
        display(12, 0, TwelveHour),
        display(23, 59, TwelveHour),
        display(0, 0, TwelveHour),
    ];
    for aspect in [0.25, 0.6, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        let layout = Layout::new(aspect);
        for source in readings {
            for target in readings {
                let mut cells = debris(source, layout);
                let original = cells.clone();
                let assembly = Reassembly::new(&mut cells, target, layout);
                assert_eq!(
                    cells.len(),
                    original.len().max(targets(target, layout).len())
                );
                assert!(cells.len() <= MAX_EXPLOSION_CELLS);
                assert_assignments(&assembly, target, layout);
                for (cell, before) in cells.iter().zip(&original) {
                    assert_eq!(
                        cell.position, before.position,
                        "no pose teleport on retarget"
                    );
                    assert_eq!(cell.side, before.side);
                }
                for cell in &cells[original.len()..] {
                    assert_eq!(cell.side, 0.0);
                    assert!(original.iter().any(|donor| donor.position == cell.position));
                }
                assembly.sample(&mut cells, REFORMING_TICKS);
                assert_arrived(&cells, target, layout);
            }
        }
    }
}

#[test]
fn mid_return_changes_preserve_unchanged_paths_and_do_not_move_existing_blocks() {
    let layout = Layout::new(4.0 / 3.0);
    let source = display(23, 58, ClockTimeFormat::TwentyFourHour);
    let target = display(23, 59, ClockTimeFormat::TwentyFourHour);
    let mut cells = debris(source, layout);
    let mut assembly = Reassembly::new(&mut cells, source, layout);
    assembly.sample(&mut cells, 30);
    let previous = cells.clone();
    let flights = assembly.flights.clone();
    assembly.synchronize(&mut cells, target, layout, 30);
    assert_assignments(&assembly, target, layout);
    for (cell, before) in cells.iter().zip(&previous) {
        assert_eq!(cell.position, before.position);
        assert_eq!(cell.angle, before.angle);
        assert_eq!(cell.side, before.side);
    }
    for (flight, before) in assembly.flights.iter().zip(flights) {
        if targets(target, layout).contains(&before.target) {
            assert_eq!(
                *flight, before,
                "unchanged slots retain their whole trajectory"
            );
        }
    }
    assembly.sample(&mut cells, 30);
    for (cell, before) in cells.iter().zip(previous) {
        assert_eq!(cell.position, before.position);
    }
    let flights = assembly.flights.clone();
    let blinking = DisplaySnapshot {
        colon_lit: false,
        ..target
    };
    assembly.synchronize(&mut cells, blinking, layout, 31);
    assert_eq!(
        assembly.flights, flights,
        "seconds must not restart the return"
    );
    assembly.sample(&mut cells, REFORMING_TICKS);
    assert_arrived(&cells, target, layout);
}

#[test]
fn repeated_corrections_reuse_surplus_slots_and_still_finish_at_the_deadline() {
    let layout = Layout::new(0.6);
    let sparse = display(11, 11, ClockTimeFormat::TwentyFourHour);
    let full = DisplaySnapshot {
        digits: [Some(8); 4],
        colon_lit: true,
        meridiem: Some("AM"),
    };
    let mut cells = debris(sparse, layout);
    let mut assembly = Reassembly::new(&mut cells, sparse, layout);
    for tick in 0..REFORMING_TICKS {
        let next = if tick % 2 == 0 { full } else { sparse };
        assembly.synchronize(&mut cells, next, layout, tick);
        assert_assignments(&assembly, next, layout);
        assembly.sample(&mut cells, tick + 1);
        assert_eq!(cells.len(), MAX_EXPLOSION_CELLS, "reuse, never accumulate");
        assert!(cells.iter().all(|cell| cell.position.x.is_finite()
            && cell.position.y.is_finite()
            && cell.angle.is_finite()
            && cell.side >= 0.0));
    }
    assert_arrived(&cells, sparse, layout);
}

#[test]
fn initial_unsynchronized_face_can_reform_a_reading_without_any_debris() {
    let layout = Layout::new(5.0 / 3.0);
    let target = display(12, 34, ClockTimeFormat::TwentyFourHour);
    let mut cells = Vec::new();
    let assembly = Reassembly::new(&mut cells, target, layout);
    assert!(
        cells
            .iter()
            .all(|cell| cell.side == 0.0 && cell.position.y == layout.floor_y)
    );
    assembly.sample(&mut cells, REFORMING_TICKS);
    assert_arrived(&cells, target, layout);
}
