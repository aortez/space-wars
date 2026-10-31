//! Retained height-column aquarium. The fluid does not model pressure cavities
//! behind glyphs: glyph masks are fish obstacles, not scarce water displacers.
//! One water step precedes the optional, leased native duck-world step.
mod fish;
mod navigation;
mod solids;
#[cfg(test)]
mod tests;

use super::{
    EventContext, EventPhase,
    duck::{self, DuckEvent},
};
use crate::{
    DisplaySnapshot, SegmentState,
    arena::ClockArena,
    digits,
    floor::responsive::{FloorShape, ResponsiveFloor},
    layout::Layout,
};
use engine_common::{ClockAquariumState, ClockEventKind};
use engine_core::Vec2;
use engine_rapier::world::PhysicsId;
use engine_water::{WaterConfig, WaterWorld};
use fish::School;
pub(crate) use fish::{Fish, Species};
use navigation::Obstacle;
use rand::{Rng, SeedableRng, rngs::StdRng};

pub const FILL_TICKS: u64 = 6 * 60;
pub const SWIM_TICKS: u64 = 40 * 60;
pub const EXIT_TICKS: u64 = 5 * 60;
pub const DRAIN_TICKS: u64 = 12 * 60;
pub const CLEAR_TICKS: u64 = 3 * 60;
pub const AQUARIUM_TICKS: u64 = FILL_TICKS + SWIM_TICKS + EXIT_TICKS + DRAIN_TICKS + CLEAR_TICKS;
const DRAIN_AT: u64 = FILL_TICKS + SWIM_TICKS + EXIT_TICKS;
const DT: f32 = 1.0 / 60.0;
const LINER_ID: u64 = crate::arena::ids::canonical::AQUARIUM_LINER.start;
/// The colon's two cells, in face rows.
const COLON_ROWS: [f32; 2] = [2.75, 6.25];
/// The most cells Aquarium's solid digits hold: every digit cell, the
/// colon's and the meridiem's.
pub(crate) const MAX_DIGIT_SOLIDS: usize =
    crate::fonts::MAX_DIGIT_CELLS + COLON_ROWS.len() + crate::meridiem::MAX_CELLS;

struct SharedAquarium {
    digits: solids::DigitSolids,
    /// Existing courses/panels are preserved; these visits get a lower liner.
    extra_liner: bool,
}

pub(crate) struct AquariumEvent {
    pub layout: Layout,
    pub water: WaterWorld,
    pub floor: ResponsiveFloor,
    pub port: Vec2,
    pub target_y: f32,
    pub tick: u64,
    seed: u64,
    school: School,
    supports: Vec<Obstacle>,
    glyphs: Vec<Obstacle>,
    shared: Option<SharedAquarium>,
    native_steps: u64,
    peak_water_parcels: usize,
}

pub(crate) fn fish_radius(layout: Layout) -> f32 {
    layout.pitch * 0.48
}

impl AquariumEvent {
    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        Self::build(context, seed, false)
    }

    fn build(context: EventContext<'_>, seed: u64, lower_liner: bool) -> Self {
        let layout = context.layout;
        let mut shape = FloorShape::aquarium(layout);
        if lower_liner {
            // Retain water below existing course gaps without moving the course
            // or adding a second native world. This liner is visible and owned.
            shape.floor_y = f64::from(layout.bounds_min.y + layout.frame_width);
        }
        let water = WaterWorld::new(
            WaterConfig {
                exit_y: f64::from(layout.bounds_min.y - layout.pitch * 2.0) - shape.max_drop,
                max_parcels: 128,
                retain_spill_shapes: true,
                ..WaterConfig::default()
            },
            shape.pools().to_vec(),
        )
        .expect("bounded aquarium pools");
        let glyph_top = layout.face_origin.y + layout.pitch * 9.0;
        let target_y = (layout.floor_y + (layout.canopy_y - layout.floor_y) * 0.85)
            .max(glyph_top + layout.pitch * 2.0)
            .min(layout.canopy_y - layout.pitch * 0.55);
        let radius = fish_radius(layout);
        let port = Vec2::new(
            layout.bounds_min.x + layout.frame_width + radius * 2.3,
            (target_y + glyph_top) * 0.5,
        );
        let mut result = Self {
            layout,
            water,
            floor: ResponsiveFloor::new(shape, 0.0),
            port,
            target_y,
            tick: 0,
            seed,
            school: School::new(layout, seed, radius),
            supports: Vec::new(),
            glyphs: Vec::new(),
            shared: None,
            native_steps: 0,
            peak_water_parcels: 0,
        };
        shape.configure(&mut result.water);
        result.synchronize(context.display, context.segments);
        result
    }

    pub fn with_visit(
        context: EventContext<'_>,
        seed: u64,
        duck: &mut DuckEvent,
        arena: &mut ClockArena,
    ) -> Self {
        let mut result = Self::build(context, seed, true);
        if let Some(floor) = duck.terrain.responsive_floor() {
            for side in 0..2 {
                let points = floor.shape.panel_points(side, floor.opening);
                result.supports.push(Obstacle {
                    min: Vec2::new(
                        points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
                        points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
                    ),
                    max: Vec2::new(
                        points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
                        points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
                    ),
                });
            }
        } else {
            let course = super::duck::arena::CourseGeometry::from_terrain(&duck.terrain);
            for surface in &course.course.surfaces {
                let a = course.screen_position(Vec2::new(surface.start, 0.0)).x;
                let b = course.screen_position(Vec2::new(surface.end, 0.0)).x;
                result.supports.push(Obstacle {
                    min: Vec2::new(a.min(b), result.layout.bounds_min.y),
                    max: Vec2::new(a.max(b), result.layout.floor_y + surface.height),
                });
            }
        }
        duck.claim_arena(arena, ClockEventKind::Aquarium);
        let world = arena.world_mut().expect("claimed arena");
        result.floor.insert_panels_at(world, LINER_ID);
        result.shared = Some(SharedAquarium {
            digits: solids::DigitSolids::new(&result.glyphs, Some(duck), world),
            extra_liner: true,
        });
        result
    }

    pub fn synchronize(&mut self, display: DisplaySnapshot, segments: &mut [SegmentState]) {
        digits::apply_snapshot(segments, display);
        let mut obstacles = self.supports.clone();
        let half = Vec2::new(self.layout.pitch * 0.4, self.layout.pitch * 0.4);
        for segment in segments.iter().filter(|s| s.lit) {
            for cell in segment.cells() {
                let center = self.layout.cell_center(segment.id, cell);
                obstacles.push(Obstacle {
                    min: center - half,
                    max: center + half,
                });
            }
        }
        // The blinking colon still has visible dim cells between flashes.
        for y in COLON_ROWS {
            let center = Vec2::new(
                self.layout.face_origin.x + 15.0 * self.layout.pitch,
                self.layout.face_origin.y + y * self.layout.pitch,
            );
            obstacles.push(Obstacle {
                min: center - half,
                max: center + half,
            });
        }
        if let Some(label) = display.meridiem {
            for glyph in crate::meridiem::Glyph::for_label(label) {
                let half = Vec2::splat(self.layout.pitch * crate::meridiem::PIXEL_SIZE * 0.5);
                for cell in glyph.cells() {
                    let center = glyph.cell_center(self.layout, cell);
                    obstacles.push(Obstacle {
                        min: center - half,
                        max: center + half,
                    });
                }
            }
        }
        self.glyphs.clear();
        self.glyphs
            .extend_from_slice(&obstacles[self.supports.len()..]);
        self.school.set_obstacles(obstacles, &self.water, self.port);
    }

    /// The event's tick up to the arena's step. A standalone Aquarium finishes
    /// its tick here; a shared one reads the arena back in `observe`. Returns
    /// true when the event has finished.
    pub fn step(
        &mut self,
        context: EventContext<'_>,
        mut player: Option<&mut DuckEvent>,
        arena: &mut ClockArena,
    ) -> bool {
        self.synchronize(context.display, context.segments);
        self.tick += 1;
        const INLET_TICKS: u64 = FILL_TICKS - 30;
        if self.tick <= INLET_TICKS {
            let depth = f64::from(self.target_y) - self.floor.shape.floor_y;
            let volume = depth * self.floor.shape.half_width
                / (INLET_TICKS * self.floor.shape.columns as u64) as f64;
            // A distributed inlet fills a column solver without an artificial
            // waterfall's unbounded source parcels. Its volume is fully ledgered.
            for side in 0..2 {
                for i in 0..self.floor.shape.columns {
                    let spec = self.water.pools()[side].spec();
                    let x = spec.left + (i as f64 + 0.5) * spec.column_width;
                    self.water
                        .add_to_pool(side, x, volume)
                        .expect("aquarium inlet");
                }
            }
        }
        if self.tick == DRAIN_AT {
            self.school.finish_departure();
        }
        if self.tick >= DRAIN_AT {
            // Only a character in the leased arena keeps the floor clear.
            let hull = self
                .shared
                .as_ref()
                .and(player.as_deref())
                .and_then(|duck| duck.clearance_hull(arena.world().expect("leased arena")));
            self.floor.step_retained(
                &mut self.water,
                f64::from(DT),
                f64::from(self.target_y) - self.floor.shape.floor_y,
                f64::from(self.layout.pitch) * 0.05,
                hull,
            );
        }
        self.water
            .step(f64::from(DT))
            .expect("fixed aquarium water step");
        self.peak_water_parcels = self.peak_water_parcels.max(self.water.parcels().len());
        if self.tick > DRAIN_AT + DRAIN_TICKS {
            // Residual recovery is explicit in the ledger, separate from drain.
            self.water
                .reclaim_fraction(1.0 / (AQUARIUM_TICKS - self.tick + 1) as f64)
                .expect("bounded aquarium recovery");
        }
        if let Some(mut shared) = self.shared.take() {
            let leased = arena.world_mut().expect("leased arena");
            shared
                .digits
                .synchronize(&self.glyphs, player.as_deref(), leased);
            if shared.extra_liner {
                self.floor.move_panels_at(leased, LINER_ID);
            } else {
                duck::leased_terrain(player.as_deref_mut(), &mut arena.vacant)
                    .sync_responsive_floor(&self.floor);
            }
            self.shared = Some(shared);
            return false;
        }
        if self.tick < DRAIN_AT {
            self.school
                .step(&self.water, self.layout, self.port, self.tick, None);
        }
        self.tick >= AQUARIUM_TICKS
    }

    /// A shared Aquarium after the arena's step: the school keeps clear of
    /// where the character now is. Returns true when the event has finished.
    pub fn observe(&mut self, player: Option<&DuckEvent>, arena: &ClockArena) -> bool {
        self.native_steps += 1;
        if self.tick < DRAIN_AT {
            self.school.step(
                &self.water,
                self.layout,
                self.port,
                self.tick,
                player
                    .zip(arena.world())
                    .and_then(|(duck, world)| duck.clearance_hull(world)),
            );
        }
        self.tick >= AQUARIUM_TICKS
    }

    pub fn phase(&self) -> EventPhase {
        match self.tick {
            t if t < FILL_TICKS => EventPhase::Filling,
            t if t < FILL_TICKS + SWIM_TICKS => EventPhase::Swimming,
            t if t < DRAIN_AT => EventPhase::Exiting,
            t if t < DRAIN_AT + DRAIN_TICKS => EventPhase::Draining,
            _ => EventPhase::Clearing,
        }
    }

    pub fn phase_tick(&self) -> u64 {
        let start = match self.phase() {
            EventPhase::Filling => 0,
            EventPhase::Swimming => FILL_TICKS,
            EventPhase::Exiting => FILL_TICKS + SWIM_TICKS,
            EventPhase::Draining => DRAIN_AT,
            _ => DRAIN_AT + DRAIN_TICKS,
        };
        self.tick - start
    }

    pub fn fish(&self) -> &[Fish] {
        &self.school.fish
    }
    pub fn shares_visit_arena(&self) -> bool {
        self.shared.is_some()
    }

    pub fn join_player(
        &mut self,
        arena: &mut ClockArena,
        seed: u64,
        session: u64,
        seat: u8,
    ) -> Option<Box<DuckEvent>> {
        if self.shared.is_some() {
            return None;
        }
        let mut duck = Box::new(DuckEvent::new_responsive_player(
            self.layout,
            seed,
            session,
            seat,
            1.0,
            self.floor.clone(),
        ));
        duck.claim_arena(arena, ClockEventKind::Aquarium);
        let world = arena.world_mut().expect("claimed arena");
        self.shared = Some(SharedAquarium {
            digits: solids::DigitSolids::new(&self.glyphs, Some(&duck), world),
            extra_liner: false,
        });
        Some(duck)
    }

    pub fn release(&mut self, player: Option<&mut DuckEvent>, arena: &mut ClockArena) {
        if let Some(shared) = &mut self.shared {
            let leased = arena.world_mut().expect("leased arena");
            shared.digits.remove(leased);
            if shared.extra_liner {
                for id in LINER_ID..LINER_ID + 2 {
                    leased.remove_entity(PhysicsId::new(id));
                }
            }
            duck::end_lease(player, arena, ClockEventKind::Aquarium);
        }
    }

    pub fn diagnostics(&self) -> ClockAquariumState {
        let s = self.water.stats();
        let units =
            |v: f64| (v / f64::from(self.layout.pitch).powi(2) * 1_000_000.0).round() as u64;
        let mut live = [0; 3];
        for fish in self.fish().iter().filter(|f| f.visible()) {
            live[fish.species.index()] += 1;
        }
        ClockAquariumState {
            seed: self.seed,
            admitted: self.school.admitted,
            live_by_species: live,
            exited: self.school.exited,
            failed_exits: self.school.failed,
            scatter_events: self.school.scatters,
            regroup_events: self.school.regroups,
            geometry_revisions: self.school.revisions,
            clearance_corrections: self.school.corrections,
            duck_clearance_corrections: self.shared.as_ref().map_or(0, |s| s.digits.corrections),
            navigation_searches: self.school.nav.searches,
            shared_arena: self.shared.is_some(),
            water_steps: self.tick,
            native_steps: self.native_steps,
            water_parcels: s.parcels,
            peak_water_parcels: self.peak_water_parcels,
            floor_open_milli: (self.floor.opening * 1000.0).round() as u32,
            floor_tilt_millidegrees: (self.floor.shape.angle_degrees(self.floor.opening) * 1000.0)
                .round() as u32,
            injected_microunits: units(s.injected),
            pooled_microunits: units(s.pooled),
            in_flight_microunits: units(s.in_flight),
            drained_microunits: units(s.drained),
            reclaimed_microunits: units(s.reclaimed),
            water_limited_ticks: s.capacity_limited_ticks,
        }
    }
}
