//! Visual return paths after the physics batch is retired. Keep unchanged cell
//! destinations, then deterministically reuse spare blocks for the latest face.
//! This is bounded choreography, not another physics world or a path planner.
//! Explosion and Coins bring the whole face home; Tanks brings home one
//! damaged digit at a time.
use super::{Cell, Home, MAX_EXPLOSION_CELLS, REFORMING_TICKS};
use crate::{
    DIGIT_SLOT_COUNT,
    digit_surfaces::{Retirement, SurfaceId, SurfaceLedger, SurfaceSource},
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
    /// The digit cell it is, which an AM/PM pixel or a surplus block's
    /// vanishing point is not.
    home: Option<Home>,
}

/// The part of the face a return brings home: some of its digits, and its
/// AM/PM pixels or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    slots: [bool; DIGIT_SLOT_COUNT],
    meridiem: bool,
}

impl Scope {
    /// Every digit and the AM/PM pixels.
    pub(crate) const FACE: Self = Self {
        slots: [true; DIGIT_SLOT_COUNT],
        meridiem: true,
    };

    /// One digit alone.
    pub(crate) const fn slot(slot: usize) -> Self {
        let mut slots = [false; DIGIT_SLOT_COUNT];
        slots[slot] = true;
        Self {
            slots,
            meridiem: false,
        }
    }

    /// Whether the part of `display` this scope covers differs from `last`'s.
    fn changed(self, last: DisplaySnapshot, display: DisplaySnapshot) -> bool {
        last.font != display.font
            || (0..DIGIT_SLOT_COUNT)
                .any(|slot| self.slots[slot] && last.digits[slot] != display.digits[slot])
            || (self.meridiem && last.meridiem != display.meridiem)
    }
}

/// The surface a digit block carries.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece {
    pub id: SurfaceId,
    pub departing: bool,
}

/// Where the return is taking one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Cell(Home),
    /// An AM/PM pixel.
    Label,
    /// A surplus block shrinking into a neighbour.
    Merging,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Flight {
    from: Cell,
    target: Target,
    /// Surplus blocks join another block and shrink, rather than fading out.
    assigned: bool,
    started: u64,
}

pub(crate) struct Reassembly {
    scope: Scope,
    display: DisplaySnapshot,
    flights: Vec<Flight>,
}

fn targets(scope: Scope, display: DisplaySnapshot, layout: Layout) -> Vec<Target> {
    let mut targets = Vec::with_capacity(MAX_EXPLOSION_CELLS);
    let mut segments = digits::create_segments();
    digits::apply_snapshot(&mut segments, display);
    for segment in segments
        .iter()
        .filter(|segment| segment.lit && scope.slots[usize::from(segment.id.digit_slot)])
    {
        for cell in segment.cells() {
            targets.push(Target {
                position: layout.cell_center(segment.id, cell),
                side: layout.pitch * 0.8,
                label: false,
                home: Some((segment.id.digit_slot, cell)),
            });
        }
    }
    for glyph in display
        .meridiem
        .filter(|_| scope.meridiem)
        .into_iter()
        .flat_map(Glyph::for_label)
    {
        for cell in glyph.cells() {
            targets.push(Target {
                position: glyph.cell_center(layout, cell),
                side: layout.pitch * PIXEL_SIZE,
                label: true,
                home: None,
            });
        }
    }
    assert!(targets.len() <= MAX_EXPLOSION_CELLS);
    targets
}

impl Reassembly {
    /// The whole face's return.
    pub(crate) fn new(cells: &mut Vec<Cell>, display: DisplaySnapshot, layout: Layout) -> Self {
        Self::within(Scope::FACE, cells, display, layout)
    }

    /// The return of the part of the face `scope` covers.
    pub(crate) fn within(
        scope: Scope,
        cells: &mut Vec<Cell>,
        display: DisplaySnapshot,
        layout: Layout,
    ) -> Self {
        let mut result = Self {
            scope,
            display,
            flights: cells
                .iter()
                .map(|cell| Flight {
                    from: *cell,
                    target: Target {
                        position: cell.origin,
                        side: cell.side,
                        label: cell.label,
                        home: cell.home,
                    },
                    assigned: true,
                    started: 0,
                })
                .collect(),
        };
        result.retarget(cells, display, layout, 0);
        result
    }

    pub(crate) fn synchronize(
        &mut self,
        cells: &mut Vec<Cell>,
        display: DisplaySnapshot,
        layout: Layout,
        tick: u64,
    ) {
        // The colon blinks independently; seconds must not restart flight
        // paths, and nor may a reading outside the return's scope.
        if self.scope.changed(self.display, display) {
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
        let targets = targets(self.scope, display, layout);
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
                    taken: false,
                    home: target.home,
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
                        home: None,
                    },
                    assigned: false,
                    started: tick,
                };
            }
        }
        assert!(cells.len() <= MAX_EXPLOSION_CELLS);
        self.display = display;
    }

    /// Where block `index` is going, in the order of the event's blocks.
    pub(crate) fn destination(&self, index: usize) -> Destination {
        let flight = &self.flights[index];
        match (flight.assigned, flight.target.home) {
            (false, _) => Destination::Merging,
            (true, Some(home)) => Destination::Cell(home),
            (true, None) => Destination::Label,
        }
    }

    /// Brings every block's surface, in `pieces`, up to where the return is
    /// taking it. A block coming home to a digit cell keeps the surface it
    /// has, whichever cell that is; one with none, or with one already
    /// departing, gets a new surface for that cell. A block that turns into an
    /// AM/PM pixel gives its surface up, and a surplus block's surface departs
    /// as it shrinks away.
    pub(crate) fn assign(&self, pieces: &mut Vec<Option<Piece>>, surfaces: &mut SurfaceLedger) {
        pieces.resize(self.flights.len(), None);
        for (index, piece) in pieces.iter_mut().enumerate() {
            let surviving = piece.is_some_and(|piece| !piece.departing);
            match self.destination(index) {
                Destination::Cell(_) if surviving => (),
                Destination::Cell((slot, cell)) => {
                    if let Some(old) = piece.take() {
                        surfaces.retire(old.id, Retirement::Replaced);
                    }
                    *piece = Some(Piece {
                        id: surfaces.admit(slot, cell, SurfaceSource::Return),
                        departing: false,
                    });
                }
                Destination::Label => {
                    if let Some(old) = piece.take() {
                        surfaces.retire(old.id, Retirement::Replaced);
                    }
                }
                Destination::Merging => {
                    if let Some(piece) = piece {
                        piece.departing = true;
                    }
                }
            }
        }
    }

    pub(crate) fn sample(&self, cells: &mut [Cell], tick: u64) {
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
