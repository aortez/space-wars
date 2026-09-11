//! Bounded, scenario-local melting: ballistic squares feed a conservative
//! one-dimensional pool. This is a stylized drain current, not a fluid engine.
//! No pairwise particle collisions, growing particle lists or extra Rapier solve.
use engine_common::ClockMeltdownState;
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{EventContext, EventPhase, REFORMING_TICKS};
use crate::{SegmentRepresentation, digits, layout::Layout};

pub const MAX_MELTDOWN_CELLS: usize = 96;
pub const WATER_COLUMNS: usize = 128;
pub const MELTING_TICKS: u64 = 180;
pub const DRAINING_TICKS: u64 = 240;
const SIDE_COLUMNS: usize = WATER_COLUMNS / 2;
const DT: f32 = 1.0 / 60.0;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeltCell {
    pub position: Vec2,
    velocity: Vec2,
    pub angle: f32,
    spin: f32,
    release_tick: u64,
}

pub(crate) struct MeltdownEvent {
    pub tick: u64,
    pub cells: Vec<MeltCell>,
    /// Each side has 64 columns ending exactly at the existing drain lip.
    pub water: [f64; WATER_COLUMNS],
    pub stream: f32,
    initial_cells: usize,
    drained: f64,
    reclaimed: f64,
}

impl MeltdownEvent {
    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut cells = Vec::with_capacity(MAX_MELTDOWN_CELLS);
        for segment in context.segments.iter_mut() {
            if segment.lit {
                for cell in digits::cells(segment.id.kind) {
                    assert!(cells.len() < MAX_MELTDOWN_CELLS);
                    cells.push(MeltCell {
                        position: context.layout.cell_center(segment.id, *cell),
                        velocity: Vec2::new(
                            rng.random_range(-0.8..0.8),
                            rng.random_range(0.0..1.3),
                        ) * context.layout.pitch,
                        angle: 0.0,
                        spin: rng.random_range(-2.5..2.5),
                        release_tick: rng.random_range(0..60) + (8 - cell.y) as u64 * 3,
                    });
                }
            }
            segment.representation = SegmentRepresentation::Disintegrated;
        }
        Self {
            tick: 0,
            initial_cells: cells.len(),
            cells,
            water: [0.0; WATER_COLUMNS],
            stream: 0.0,
            drained: 0.0,
            reclaimed: 0.0,
        }
    }

    pub fn phase(&self) -> EventPhase {
        if self.tick < MELTING_TICKS {
            EventPhase::Melting
        } else if self.tick < MELTING_TICKS + DRAINING_TICKS {
            EventPhase::Draining
        } else {
            EventPhase::Reforming
        }
    }

    pub fn phase_tick(&self) -> u64 {
        self.tick
            - match self.phase() {
                EventPhase::Melting => 0,
                EventPhase::Draining => MELTING_TICKS,
                _ => MELTING_TICKS + DRAINING_TICKS,
            }
    }

    pub fn step(&mut self, context: EventContext<'_>) -> bool {
        self.tick += 1;
        if self.tick < MELTING_TICKS + DRAINING_TICKS {
            self.step_material(context.layout);
        } else if self.tick == MELTING_TICKS + DRAINING_TICKS {
            // Explicitly account for residual material instead of claiming it
            // drained. Free the cell allocation before reforming the latest face.
            self.reclaimed = self.water.iter().sum::<f64>() + self.cells.len() as f64;
            self.water.fill(0.0);
            self.cells = Vec::new();
            self.stream = 0.0;
            for segment in context.segments.iter_mut() {
                segment.representation = SegmentRepresentation::Reforming {
                    position: context.layout.segment_center(segment.id),
                    angle: 0.0,
                    was_lit: false,
                };
            }
            digits::apply_snapshot(context.segments, context.display);
        }
        self.tick >= MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS
    }

    fn step_material(&mut self, layout: Layout) {
        let mut deposited = [0.0; WATER_COLUMNS];
        let mut newly_drained = 0.0;
        let half = layout.pitch * 0.4;
        self.cells.retain_mut(|cell| {
            if self.tick <= cell.release_tick {
                return true;
            }
            cell.velocity.y -= 400.0 * DT;
            cell.position += cell.velocity * DT;
            cell.angle += cell.spin * DT;
            // Use the square's conservative rotating extent for side walls.
            let extent = half * (cell.angle.sin().abs() + cell.angle.cos().abs());
            let x = cell
                .position
                .x
                .clamp(layout.bounds_min.x + extent, layout.bounds_max.x - extent);
            if x != cell.position.x {
                cell.position.x = x;
                cell.velocity.x *= -0.35;
            }
            if cell.position.y - extent <= layout.floor_y
                && cell.position.x.abs() + extent >= layout.drain_half_width()
            {
                // Spread one cell's volume over its footprint at first impact.
                for offset in [-0.6, -0.2, 0.2, 0.6] {
                    let x = cell.position.x + offset * half;
                    if x.abs() < layout.drain_half_width() {
                        newly_drained += 0.25;
                    } else {
                        deposited[Self::column_at(layout, x)] += 0.25;
                    }
                }
                false
            } else if cell.position.y + extent < layout.bounds_min.y {
                newly_drained += 1.0;
                false
            } else {
                true
            }
        });
        for (water, added) in self.water.iter_mut().zip(deposited) {
            *water += added;
        }
        // Two fixed conservative passes: diffuse toward lower neighboring
        // columns, then advect 22% toward the drain. Outflows never exceed the
        // donor's volume, even with a concentrated impact. Cost is O(128).
        for _ in 0..2 {
            let mut next = self.water;
            for i in 0..WATER_COLUMNS - 1 {
                if i == SIDE_COLUMNS - 1 {
                    continue;
                }
                let flow = (self.water[i] - self.water[i + 1]) * 0.16;
                next[i] -= flow;
                next[i + 1] += flow;
            }
            self.water = next;
            for i in 0..WATER_COLUMNS {
                let flow = self.water[i] * 0.22;
                next[i] -= flow;
                if i == SIDE_COLUMNS - 1 || i == SIDE_COLUMNS {
                    newly_drained += flow;
                } else {
                    next[if i < SIDE_COLUMNS { i + 1 } else { i - 1 }] += flow;
                }
            }
            self.water = next;
        }
        self.drained += newly_drained;
        self.stream = (self.stream * 0.84 + newly_drained as f32 * 0.8).min(1.0);
    }

    pub fn column_width(layout: Layout) -> f32 {
        (layout.bounds_max.x - layout.drain_half_width()) / SIDE_COLUMNS as f32
    }

    pub fn column_left(layout: Layout, index: usize) -> f32 {
        let width = Self::column_width(layout);
        if index < SIDE_COLUMNS {
            layout.bounds_min.x + index as f32 * width
        } else {
            layout.drain_half_width() + (index - SIDE_COLUMNS) as f32 * width
        }
    }

    fn column_at(layout: Layout, x: f32) -> usize {
        let origin = if x < 0.0 {
            layout.bounds_min.x
        } else {
            layout.drain_half_width()
        };
        let local = ((x - origin) / Self::column_width(layout))
            .clamp(0.0, (SIDE_COLUMNS - 1) as f32) as usize;
        local + if x < 0.0 { 0 } else { SIDE_COLUMNS }
    }

    pub fn diagnostics(&self) -> ClockMeltdownState {
        let waiting = self
            .cells
            .iter()
            .filter(|cell| self.tick <= cell.release_tick)
            .count();
        ClockMeltdownState {
            initial_cells: self.initial_cells,
            waiting_cells: waiting,
            airborne_cells: self.cells.len() - waiting,
            water_columns: self
                .water
                .iter()
                .filter(|volume| **volume > 0.000_001)
                .count(),
            pooled_microunits: (self.water.iter().sum::<f64>() * 1_000_000.0).round() as u64,
            drained_microunits: (self.drained * 1_000_000.0).round() as u64,
            reclaimed_microunits: (self.reclaimed * 1_000_000.0).round() as u64,
        }
    }
}
