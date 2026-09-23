//! Clock adaptation of the bounded pool/spill model. One melted square supplies
//! its world-space area; digit animation and event cleanup remain scenario-owned.
use engine_common::ClockMeltdownState;
use engine_core::Vec2;
use engine_water::{Boundary, Parcel, PoolSpec, WaterConfig, WaterWorld};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{EventContext, EventPhase, REFORMING_TICKS, duck::DuckEvent};
use crate::{
    ClockWaterLab, SegmentRepresentation, digits,
    floor::responsive::{FloorShape, ResponsiveFloor},
    layout::Layout,
};

pub const MAX_MELTDOWN_CELLS: usize = 96 + crate::meridiem::MAX_CELLS;
pub const WATER_COLUMNS: usize = 128;
// Moving banks release uncovered strips as well as ordinary overflow. Keep a
// fixed ceiling with room for both; stationary development labs stay at 128.
pub const MAX_SPILL_PARCELS: usize = 192;
const LAB_SPILL_PARCELS: usize = 128;
pub const MELTING_TICKS: u64 = 180;
pub const DRAINING_TICKS: u64 = 240;
const SIDE_COLUMNS: usize = WATER_COLUMNS / 2;
const LAB_INITIAL_CELLS: usize = 24;
const DT: f32 = 1.0 / 60.0;

mod material;
mod shared;
#[cfg(test)]
mod tests;
mod water_lab;

pub(crate) use material::{MeltCell, soften};

pub(crate) struct MeltdownEvent {
    pub tick: u64,
    pub cells: Vec<MeltCell>,
    pub water: WaterWorld,
    pub lab: bool,
    pub floats: Option<water_lab::WaterLab>,
    pub floor: Option<ResponsiveFloor>,
    shared: Option<shared::SharedMeltdown>,
    initial_cells: usize,
    initial_area: f64,
    cell_area: f64,
    reclaimed_area: f64,
    exited_solid_area: f64,
}

impl MeltdownEvent {
    pub fn new(context: EventContext<'_>, seed: u64, mode: ClockWaterLab) -> Self {
        Self::in_arena(context, seed, mode, None)
    }

    pub fn with_player(context: EventContext<'_>, seed: u64, player: &mut DuckEvent) -> Self {
        Self::in_arena(context, seed, ClockWaterLab::Off, Some(player))
    }

    fn in_arena(
        context: EventContext<'_>,
        seed: u64,
        mode: ClockWaterLab,
        player: Option<&mut DuckEvent>,
    ) -> Self {
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
                            meridiem: false,
                            velocity: Vec2::new(rng.random_range(-0.25..0.25), 0.0)
                                * context.layout.pitch,
                            angle: 0.0,
                            spin: rng.random_range(-0.4..0.4),
                            // Release solid blocks bottom-up with small seeded
                            // variation. Only floor contact changes the material.
                            release_tick: 24 + cell.y as u64 * 4 + rng.random_range(0..24),
                        });
                    }
                }
                segment.representation = SegmentRepresentation::Disintegrated;
            }
            if let Some(label) = context.display.meridiem {
                for glyph in crate::meridiem::Glyph::for_label(label) {
                    for cell in glyph.cells() {
                        assert!(cells.len() < MAX_MELTDOWN_CELLS);
                        cells.push(MeltCell {
                            position: glyph.cell_center(context.layout, cell),
                            meridiem: true,
                            velocity: Vec2::new(rng.random_range(-0.25..0.25), 0.0)
                                * context.layout.pitch,
                            angle: 0.0,
                            spin: rng.random_range(-0.4..0.4),
                            release_tick: 24 + cell.y as u64 * 4 + rng.random_range(0..24),
                        });
                    }
                }
            }
        }
        let cell_area = (Self::water_pitch(context.layout, mode) * 0.8).powi(2);
        let floor = match player.as_deref() {
            Some(duck) => duck.responsive_floor().cloned(),
            None => (!lab).then(|| ResponsiveFloor::new(FloorShape::clock(context.layout), 0.0)),
        };
        let mut water = if lab {
            Self::lab_water_world(context.layout, mode)
        } else {
            let pools = if let Some(floor) = &floor {
                floor.shape.pools_at(floor.opening).into()
            } else {
                super::duck::arena::CourseGeometry::from_duck(player.as_deref().unwrap())
                    .water_pools()
            };
            let mut water = WaterWorld::new(
                WaterConfig {
                    max_parcels: MAX_SPILL_PARCELS,
                    exit_y: f64::from(context.layout.bounds_min.y),
                    ..WaterConfig::default()
                },
                pools,
            )
            .expect("bounded Meltdown floor");
            if let Some(floor) = &floor {
                floor.shape.configure_at(&mut water, floor.opening);
            }
            water
        };
        let initial_cells = if lab { LAB_INITIAL_CELLS } else { cells.len() };
        let initial_area = cell_area
            * if lab {
                initial_cells as f64
            } else {
                cells.iter().map(MeltCell::area_scale).sum()
            };
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
        let shared = player.map(|duck| shared::SharedMeltdown::new(duck, cells.len()));
        Self {
            tick: 0,
            initial_cells,
            initial_area,
            cells,
            water,
            lab,
            floats,
            floor,
            shared,
            cell_area,
            reclaimed_area: 0.0,
            exited_solid_area: 0.0,
        }
    }

    fn water_pitch(layout: Layout, mode: ClockWaterLab) -> f64 {
        // Fit a two-level fixture below the readable face, even on wide screens.
        // These are lab cell-equivalent units; normal digit melting is unchanged.
        f64::from(if mode.is_spilling_tank() {
            layout.pitch.min(20.0)
        } else {
            layout.pitch
        })
    }

    fn lab_water_world(layout: Layout, mode: ClockWaterLab) -> WaterWorld {
        let half = layout.bounds_max.x as f64;
        let floor = layout.floor_y as f64;
        let specs = if mode.is_spilling_tank() {
            let pitch = Self::water_pitch(layout, mode);
            let width = pitch * 9.6;
            let depth = LAB_INITIAL_CELLS as f64 * (pitch * 0.8).powi(2) / width;
            vec![
                PoolSpec {
                    left: -width * 0.5,
                    column_width: width / SIDE_COLUMNS as f64,
                    bed: vec![-184.0; SIDE_COLUMNS],
                    boundaries: [
                        Boundary::Closed,
                        Boundary::Spill {
                            lip: -184.0 + depth,
                        },
                    ],
                },
                PoolSpec {
                    left: -width * 0.5,
                    column_width: width * 1.6 / SIDE_COLUMNS as f64,
                    bed: vec![-230.0; SIDE_COLUMNS],
                    boundaries: [Boundary::Closed; 2],
                },
            ]
        } else if mode.is_tank() {
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
            unreachable!("normal Meltdown uses the responsive floor")
        };
        WaterWorld::new(
            WaterConfig {
                exit_y: layout.bounds_min.y as f64,
                max_parcels: LAB_SPILL_PARCELS,
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
        self.step_with_player(context, None)
    }

    pub fn step_with_player(
        &mut self,
        context: EventContext<'_>,
        player: Option<&mut DuckEvent>,
    ) -> bool {
        if self.shared.is_some() {
            return self.step_shared(context, player);
        }
        self.tick += 1;
        if let Some(floats) = &mut self.floats {
            floats.prepare_displacement(&mut self.water, self.tick);
        }
        if self.tick < MELTING_TICKS + DRAINING_TICKS {
            self.step_material(context.layout);
        } else {
            self.step_floor();
            self.reform(context);
        }
        if let Some(floats) = &mut self.floats {
            floats.step(&mut self.water);
        }
        self.tick >= MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS
    }

    fn reform(&mut self, context: EventContext<'_>) {
        if self.tick == MELTING_TICKS + DRAINING_TICKS {
            self.reclaimed_area = self
                .cells
                .iter()
                .map(|cell| self.cell_area * cell.area_scale())
                .sum();
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
                .reclaim_fraction(if self.lab {
                    1.0 / remaining as f64
                } else {
                    // Ease the visible recovery, but keep it explicit
                    // reclamation rather than pretending this drained.
                    let before = self.phase_tick() as f64 / REFORMING_TICKS as f64;
                    let after = (self.phase_tick() + 1) as f64 / REFORMING_TICKS as f64;
                    let ease = |t: f64| t * t * (3.0 - 2.0 * t);
                    ((ease(after) - ease(before)) / (1.0 - ease(before))).clamp(0.0, 1.0)
                })
                .expect("bounded reform fraction");
        }
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        if let Some(shared) = &self.shared {
            // ClockState counts the occupied world with its player, once.
            return shared
                .arena
                .vacant()
                .map_or((0, 0), DuckEvent::physics_counts);
        }
        self.floats
            .as_ref()
            .map_or((0, 0), |f| (f.world.body_count(), f.world.collider_count()))
    }

    /// Recovery blends the event-owned floor into the ordinary closed arena,
    /// just as Rain does. The underlying wet bed continues its slow physical
    /// motion; it is not snapped shut through residual water.
    pub fn floor_opacity(&self) -> f32 {
        if self.phase() == EventPhase::Reforming {
            1.0 - soften(self.phase_tick() as f32 / (REFORMING_TICKS - 1) as f32)
        } else {
            1.0
        }
    }

    fn step_floor(&mut self) {
        if let Some(floor) = &mut self.floor {
            floor.step(&mut self.water, f64::from(DT), None);
        }
    }

    fn step_material(&mut self, layout: Layout) {
        self.step_floor();
        self.water.step(1.0 / 60.0).expect("fixed water step");
        let water = &mut self.water;
        let cell_area = self.cell_area;
        self.cells.retain_mut(|cell| {
            if self.tick <= cell.release_tick {
                return true;
            }
            cell.velocity.y -= 400.0 * DT;
            cell.position += cell.velocity * DT;
            cell.angle += cell.spin * DT;
            let extent = cell.extent(layout.pitch);
            let x = cell.position.x.clamp(
                layout.bounds_min.x + extent.x,
                layout.bounds_max.x - extent.x,
            );
            if x != cell.position.x {
                cell.position.x = x;
                cell.velocity.x *= -0.35;
            }
            let area = cell_area * cell.area_scale();
            if material::merge(
                cell,
                water,
                area,
                layout,
                self.floor
                    .as_ref()
                    .expect("normal cells own a responsive floor"),
            ) {
                return false;
            }
            if cell.position.y + extent.y < layout.bounds_min.y {
                // A block fitting through the gap remains solid until it exits.
                // Never count this as injected water or cleanup reclamation.
                self.exited_solid_area += area;
                return false;
            }
            true
        });
    }

    pub fn diagnostics(&self) -> ClockMeltdownState {
        let (waiting, solid_area) = self.cells.iter().fold((0, 0.0), |(waiting, area), cell| {
            (
                waiting + usize::from(self.tick <= cell.release_tick),
                area + self.cell_area * cell.area_scale(),
            )
        });
        let water = self.water.stats();
        let micro = |area: f64| (area / self.cell_area * 1_000_000.0).round() as u64;
        ClockMeltdownState {
            initial_cells: self.initial_cells,
            initial_microunits: micro(self.initial_area),
            solid_microunits: micro(solid_area),
            waiting_cells: waiting,
            airborne_cells: self.cells.len() - waiting,
            water_columns: water.wet_columns,
            pooled_microunits: micro(water.pooled),
            spilling_microunits: micro(water.in_flight),
            displaced_microunits: micro(water.displaced),
            spill_parcels: water.parcels,
            capacity_limited_ticks: water.capacity_limited_ticks,
            drained_microunits: micro(water.drained + self.exited_solid_area),
            exited_solid_microunits: micro(self.exited_solid_area),
            reclaimed_microunits: micro(water.reclaimed + self.reclaimed_area),
            floor_open_milli: self
                .floor
                .as_ref()
                .map_or(0, |f| (f.opening * 1000.0).round() as u32),
            floor_load_milli: self
                .floor
                .as_ref()
                .map_or(0, |f| (f.load * 1000.0).round() as u32),
            floor_motion_deferrals: self.floor.as_ref().map_or(0, |f| f.deferrals),
        }
    }
}
