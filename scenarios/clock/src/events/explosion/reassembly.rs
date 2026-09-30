//! Visual return paths after the physics batch is retired. Keep unchanged cell
//! destinations, then deterministically reuse spare blocks for the latest face.
//! This is bounded choreography, not another physics world or a path planner.
use super::{Cell, MAX_EXPLOSION_CELLS, REFORMING_TICKS};
use crate::{
    digits::{self, DisplaySnapshot},
    layout::Layout,
    meridiem::{Glyph, PIXEL_SIZE},
};
use engine_core::Vec2;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Target {
    position: Vec2,
    side: f32,
    label: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Flight {
    from: Cell,
    target: Target,
    /// Surplus blocks join another block and shrink, rather than fading out.
    assigned: bool,
    started: u64,
}

pub(super) struct Reassembly {
    display: DisplaySnapshot,
    flights: Vec<Flight>,
}

fn targets(display: DisplaySnapshot, layout: Layout) -> Vec<Target> {
    let mut targets = Vec::with_capacity(MAX_EXPLOSION_CELLS);
    let mut segments = digits::create_segments();
    digits::apply_snapshot(&mut segments, display);
    for segment in segments.iter().filter(|segment| segment.lit) {
        for cell in segment.cells() {
            targets.push(Target {
                position: layout.cell_center(segment.id, cell),
                side: layout.pitch * 0.8,
                label: false,
            });
        }
    }
    for glyph in display.meridiem.into_iter().flat_map(Glyph::for_label) {
        for cell in glyph.cells() {
            targets.push(Target {
                position: glyph.cell_center(layout, cell),
                side: layout.pitch * PIXEL_SIZE,
                label: true,
            });
        }
    }
    assert!(targets.len() <= MAX_EXPLOSION_CELLS);
    targets
}

impl Reassembly {
    pub(super) fn new(cells: &mut Vec<Cell>, display: DisplaySnapshot, layout: Layout) -> Self {
        let mut result = Self {
            display,
            flights: cells
                .iter()
                .map(|cell| Flight {
                    from: *cell,
                    target: Target {
                        position: cell.origin,
                        side: cell.side,
                        label: cell.label,
                    },
                    assigned: true,
                    started: 0,
                })
                .collect(),
        };
        result.retarget(cells, display, layout, 0);
        result
    }

    pub(super) fn synchronize(
        &mut self,
        cells: &mut Vec<Cell>,
        display: DisplaySnapshot,
        layout: Layout,
        tick: u64,
    ) {
        // The colon blinks independently; seconds must not restart flight paths.
        if self.display.font != display.font
            || self.display.digits != display.digits
            || self.display.meridiem != display.meridiem
        {
            self.retarget(cells, display, layout, tick);
        }
    }

    fn retarget(
        &mut self,
        cells: &mut Vec<Cell>,
        display: DisplaySnapshot,
        layout: Layout,
        tick: u64,
    ) {
        let targets = targets(display, layout);
        let mut claimed = vec![false; targets.len()];
        let mut used = vec![false; cells.len()];
        // Preserve both identity and trajectory for every still-needed slot.
        for (index, flight) in self.flights.iter().enumerate() {
            if flight.assigned
                && let Some(target) = targets.iter().position(|target| *target == flight.target)
            {
                claimed[target] = true;
                used[index] = true;
            }
        }
        for (target_index, target) in targets.iter().copied().enumerate() {
            if claimed[target_index] {
                continue;
            }
            // Stable iteration order breaks ties. Prefer the same visual kind,
            // then the nearest spare block; no RNG or per-frame matching work.
            let index = cells
                .iter()
                .enumerate()
                .filter(|(index, _)| !used[*index])
                .min_by(|(_, a), (_, b)| {
                    (a.label != target.label)
                        .cmp(&(b.label != target.label))
                        .then_with(|| {
                            (a.position - target.position)
                                .length_squared()
                                .total_cmp(&(b.position - target.position).length_squared())
                        })
                })
                .map(|(index, _)| index);
            let index = index.unwrap_or_else(|| {
                // A denser reading needs more blocks: split them from existing
                // debris, growing from zero size, never appearing at the goal.
                let donor = cells.iter().min_by(|a, b| {
                    (a.position - target.position)
                        .length_squared()
                        .total_cmp(&(b.position - target.position).length_squared())
                });
                let cell = Cell {
                    origin: target.position,
                    position: donor.map_or(Vec2::new(0.0, layout.floor_y), |cell| cell.position),
                    angle: donor.map_or(0.0, |cell| cell.angle),
                    side: 0.0,
                    label: target.label,
                    velocity: Vec2::ZERO,
                    spin: 0.0,
                };
                cells.push(cell);
                self.flights.push(Flight {
                    from: cell,
                    target,
                    assigned: true,
                    started: tick,
                });
                used.push(false);
                cells.len() - 1
            });
            self.flights[index] = Flight {
                from: cells[index],
                target,
                assigned: true,
                started: tick,
            };
            cells[index].label = target.label;
            used[index] = true;
        }
        // Fewer lit cells: fold spare blocks into a nearby destination. Reuse
        // these slots if another reading needs them before the event ends.
        for (index, cell) in cells.iter().copied().enumerate() {
            if !used[index] {
                let position = targets
                    .iter()
                    .min_by(|a, b| {
                        (a.position - cell.position)
                            .length_squared()
                            .total_cmp(&(b.position - cell.position).length_squared())
                    })
                    .map_or(cell.origin, |target| target.position);
                self.flights[index] = Flight {
                    from: cell,
                    target: Target {
                        position,
                        side: 0.0,
                        label: cell.label,
                    },
                    assigned: false,
                    started: tick,
                };
            }
        }
        assert!(cells.len() <= MAX_EXPLOSION_CELLS);
        self.display = display;
    }

    pub(super) fn sample(&self, cells: &mut [Cell], tick: u64) {
        for (cell, flight) in cells.iter_mut().zip(&self.flights) {
            let t = (tick.saturating_sub(flight.started) as f32
                / (REFORMING_TICKS - flight.started).max(1) as f32)
                .clamp(0.0, 1.0);
            let progress = t * t * (3.0 - 2.0 * t);
            if t == 1.0 {
                cell.position = flight.target.position;
                cell.side = flight.target.side;
                cell.angle = 0.0;
                continue;
            }
            cell.position =
                flight.from.position + (flight.target.position - flight.from.position) * progress;
            // Settle by the shortest rotation, rather than unwinding every spin
            // accumulated during the burst. Full turns are visually equivalent.
            let turn = (flight.from.angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            cell.angle = flight.from.angle - turn * progress;
            cell.side = flight.from.side + (flight.target.side - flight.from.side) * progress;
        }
    }
}
