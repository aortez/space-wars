//! Clock adaptation of the bounded pool/spill model. One melted square supplies
//! its world-space area; digit animation and event cleanup remain scenario-owned.
use engine_common::ClockMeltdownState;
use engine_core::Vec2;
use engine_water::{Boundary, Parcel, PoolSpec, WaterConfig, WaterWorld};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{EventContext, EventPhase, REFORMING_TICKS};
use crate::{ClockWaterLab, SegmentRepresentation, digits, layout::Layout};

pub const MAX_MELTDOWN_CELLS: usize = 96;
pub const WATER_COLUMNS: usize = 128;
pub const MAX_SPILL_PARCELS: usize = 128;
pub const MELTING_TICKS: u64 = 180;
pub const DRAINING_TICKS: u64 = 240;
const SIDE_COLUMNS: usize = WATER_COLUMNS / 2;
const DT: f32 = 1.0 / 60.0;

#[cfg(test)]
mod tests;
mod water_lab;

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
    pub water: WaterWorld,
    pub lab: bool,
    pub floats: Option<water_lab::WaterLab>,
    initial_cells: usize,
    cell_area: f64,
    bypassed: f64,
    reclaimed_cells: f64,
}

impl MeltdownEvent {
    pub fn new(context: EventContext<'_>, seed: u64, mode: ClockWaterLab) -> Self {
        let lab = mode != ClockWaterLab::Off;
        let mut rng = StdRng::seed_from_u64(seed);
        let mut cells = Vec::with_capacity(MAX_MELTDOWN_CELLS);
        if !lab {
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
        }
        let cell_area = (context.layout.pitch as f64 * 0.8).powi(2);
        let mut water = Self::water_world(context.layout, mode);
        let initial_cells = if lab { 24 } else { cells.len() };
        if lab {
            let spec = water.pools()[0].spec().clone();
            for i in 0..spec.bed.len() {
                water
                    .add_to_pool(
                        0,
                        spec.left + (i as f64 + 0.5) * spec.column_width,
                        initial_cells as f64 * cell_area / spec.bed.len() as f64,
                    )
                    .unwrap();
            }
        }
        let floats = lab.then(|| water_lab::WaterLab::new(&water, context.layout, mode));
        Self {
            tick: 0,
            initial_cells,
            cells,
            water,
            lab,
            floats,
            cell_area,
            bypassed: 0.0,
            reclaimed_cells: 0.0,
        }
    }

    fn water_world(layout: Layout, mode: ClockWaterLab) -> WaterWorld {
        let half = layout.bounds_max.x as f64;
        let lip = layout.drain_half_width() as f64;
        let floor = layout.floor_y as f64;
        let specs = if mode.is_tank() {
            let width = layout.pitch as f64 * 9.6;
            vec![PoolSpec {
                left: -width * 0.5,
                column_width: width / WATER_COLUMNS as f64,
                bed: vec![-220.0; WATER_COLUMNS],
                boundaries: [Boundary::Closed; 2],
            }]
        } else if mode == ClockWaterLab::Cascade {
            vec![
                PoolSpec {
                    left: -half * 0.8,
                    column_width: half * 0.6 / SIDE_COLUMNS as f64,
                    bed: (0..SIDE_COLUMNS)
                        .map(|i| {
                            floor + layout.pitch as f64 * 0.2 * (2 - (i * 3 / SIDE_COLUMNS)) as f64
                        })
                        .collect(),
                    boundaries: [Boundary::Closed, Boundary::Spill { lip: floor }],
                },
                PoolSpec {
                    left: -half * 0.36,
                    column_width: half * 1.06 / SIDE_COLUMNS as f64,
                    bed: vec![-226.0; SIDE_COLUMNS],
                    boundaries: [Boundary::Closed; 2],
                },
            ]
        } else {
            vec![
                PoolSpec {
                    left: -half,
                    column_width: (half - lip) / SIDE_COLUMNS as f64,
                    bed: vec![floor; SIDE_COLUMNS],
                    boundaries: [Boundary::Closed, Boundary::Spill { lip: floor }],
                },
                PoolSpec {
                    left: lip,
                    column_width: (half - lip) / SIDE_COLUMNS as f64,
                    bed: vec![floor; SIDE_COLUMNS],
                    boundaries: [Boundary::Spill { lip: floor }, Boundary::Closed],
                },
            ]
        };
        WaterWorld::new(
            WaterConfig {
                exit_y: layout.bounds_min.y as f64,
                max_parcels: MAX_SPILL_PARCELS,
                spill_channel: (mode == ClockWaterLab::Off).then_some([-lip, lip]),
                ..WaterConfig::default()
            },
            specs,
        )
        .expect("bounded Clock basin geometry")
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
        if let Some(floats) = &mut self.floats {
            floats.prepare_displacement(&mut self.water, self.tick);
        }
        if self.tick < MELTING_TICKS + DRAINING_TICKS {
            self.step_material(context.layout);
        } else {
            if self.tick == MELTING_TICKS + DRAINING_TICKS {
                self.reclaimed_cells = self.cells.len() as f64;
                self.cells = Vec::new();
                if !self.lab {
                    for segment in context.segments.iter_mut() {
                        segment.representation = SegmentRepresentation::Reforming {
                            position: context.layout.segment_center(segment.id),
                            angle: 0.0,
                            was_lit: false,
                        };
                    }
                }
                digits::apply_snapshot(context.segments, context.display);
            }
            let remaining =
                (MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS).saturating_sub(self.tick);
            if remaining > 0 {
                self.water.step(1.0 / 60.0).expect("fixed water step");
                self.water
                    .reclaim_fraction(1.0 / remaining as f64)
                    .expect("bounded reform fraction");
            }
        }
        if let Some(floats) = &mut self.floats {
            floats.step(&self.water);
        }
        self.tick >= MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        self.floats
            .as_ref()
            .map_or((0, 0), |f| (f.world.body_count(), f.world.collider_count()))
    }

    fn step_material(&mut self, layout: Layout) {
        self.water.step(1.0 / 60.0).expect("fixed water step");
        let half = layout.pitch * 0.4;
        self.cells.retain_mut(|cell| {
            if self.tick <= cell.release_tick {
                return true;
            }
            cell.velocity.y -= 400.0 * DT;
            cell.position += cell.velocity * DT;
            cell.angle += cell.spin * DT;
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
                let footprint =
                    [-0.6, -0.2, 0.2, 0.6].map(|offset| cell.position.x + offset * half);
                let spills = footprint
                    .iter()
                    .filter(|x| x.abs() < layout.drain_half_width())
                    .count();
                if self.water.parcels().len() + spills > MAX_SPILL_PARCELS {
                    // Source backpressure too: keep the original cell's mass
                    // while waiting for spill capacity; do not partially inject.
                    cell.position.y = layout.floor_y + extent;
                    cell.velocity.y = 0.0;
                    return true;
                }
                for x in footprint {
                    let volume = self.cell_area * 0.25;
                    if x.abs() < layout.drain_half_width() {
                        self.water
                            .add_falling(Parcel {
                                position: Vec2::new(x, layout.floor_y),
                                velocity: Vec2::new(cell.velocity.x, cell.velocity.y.max(-800.0)),
                                volume,
                                duration: 1.0 / 60.0,
                            })
                            .expect("reserved spill capacity");
                    } else {
                        let pool = usize::from(x > 0.0);
                        self.water
                            .add_to_pool(pool, x as f64, volume)
                            .expect("impact inside floor");
                    }
                }
                false
            } else if cell.position.y + extent < layout.bounds_min.y {
                self.bypassed += 1.0;
                false
            } else {
                true
            }
        });
    }

    pub fn diagnostics(&self) -> ClockMeltdownState {
        let waiting = self
            .cells
            .iter()
            .filter(|cell| self.tick <= cell.release_tick)
            .count();
        let water = self.water.stats();
        let micro = |area: f64| (area / self.cell_area * 1_000_000.0).round() as u64;
        ClockMeltdownState {
            initial_cells: self.initial_cells,
            waiting_cells: waiting,
            airborne_cells: self.cells.len() - waiting,
            water_columns: water.wet_columns,
            pooled_microunits: micro(water.pooled),
            spilling_microunits: micro(water.in_flight),
            displaced_microunits: micro(water.displaced),
            spill_parcels: water.parcels,
            capacity_limited_ticks: water.capacity_limited_ticks,
            drained_microunits: micro(water.drained) + (self.bypassed * 1_000_000.0).round() as u64,
            reclaimed_microunits: micro(water.reclaimed)
                + (self.reclaimed_cells * 1_000_000.0).round() as u64,
        }
    }
}
