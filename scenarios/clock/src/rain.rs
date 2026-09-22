//! Rain's physical arena, lit collecting digits and one passive floating duck.
mod physics;
pub(crate) mod source;
mod surfaces;

use engine_common::{ClockRainAmount, ClockRainDuckPhase, ClockRainState};
use engine_core::Vec2;
use engine_water::{Parcel, WaterWorld};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::events::duck::arena::CourseGeometry;
use crate::{
    DisplaySnapshot, SegmentState,
    events::EventPhase,
    floor::responsive::{FloorShape, ResponsiveFloor},
    layout::Layout,
};
use physics::{DT, FloatWorld};
#[cfg(test)]
use surfaces::FLOOR_POOLS;
use surfaces::{DigitSurfaces, PARCELS, RELEASE_SLOTS};

pub const RAINING_TICKS: u64 = 20 * 60;
pub const DRAIN_TICKS: u64 = 20 * 60;
pub const CLEAR_TICKS: u64 = 120;
pub const RAIN_TICKS: u64 = RAINING_TICKS + DRAIN_TICKS + CLEAR_TICKS;
// Floor pools step first. Leave their two new outfalls room even when a wet
// digit change temporarily crowds the ordinary source/outlet parcel budget.
const SOURCE_LIMIT: usize = PARCELS - RELEASE_SLOTS - 2;
const OPEN_TICKS: u64 = 36;
const CLOSE_TICKS: u64 = 24;
const DEPTH_TICKS: u64 = 30;

pub(crate) enum RainArena {
    Responsive(ResponsiveFloor),
    /// No second Rapier world, responsive panels or passive duck. Water uses
    /// the same immutable slab geometry as the player's mechanics world.
    Course(CourseGeometry),
}

pub(crate) struct RainEvent {
    pub layout: Layout,
    pub water: WaterWorld,
    pub tick: u64,
    pub entry_x: f32,
    pub facing: f32,
    arena: RainArena,
    // A joined shower never spawns another passive actor, even after a visit
    // ends. The current live player hull is supplied before the one water step.
    player_joined: bool,
    player_hull: Option<(Vec2, f64)>,
    seed: u64,
    source: source::RainSource,
    amount: ClockRainAmount,
    budget: f64,
    scheduled: f64,
    source_limited: u64,
    depth_ticks: u64,
    door_started: Option<u64>,
    door_floor: f32,
    phase: ClockRainDuckPhase,
    spawns: u32,
    floats: Option<FloatWorld>,
    reclaimed_pose: Option<(Vec2, f32)>,
    surfaces: DigitSurfaces,
}

impl RainEvent {
    pub fn new(
        layout: Layout,
        seed: u64,
        amount: ClockRainAmount,
        display: DisplaySnapshot,
    ) -> Self {
        Self::in_arena(
            layout,
            seed,
            amount,
            display,
            RainArena::Responsive(ResponsiveFloor::new(FloorShape::clock(layout), 0.0)),
            false,
        )
    }

    pub fn on_course(
        geometry: CourseGeometry,
        seed: u64,
        amount: ClockRainAmount,
        display: DisplaySnapshot,
    ) -> Self {
        Self::in_arena(
            geometry.layout,
            seed,
            amount,
            display,
            RainArena::Course(geometry),
            true,
        )
    }

    pub fn on_responsive_floor(
        layout: Layout,
        seed: u64,
        amount: ClockRainAmount,
        display: DisplaySnapshot,
        floor: ResponsiveFloor,
    ) -> Self {
        Self::in_arena(
            layout,
            seed,
            amount,
            display,
            RainArena::Responsive(floor),
            true,
        )
    }

    fn in_arena(
        layout: Layout,
        seed: u64,
        amount: ClockRainAmount,
        display: DisplaySnapshot,
        arena: RainArena,
        player_joined: bool,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let amount = if amount == ClockRainAmount::Varied {
            ClockRainAmount::ALL[rng.random_range(1..4)]
        } else {
            amount
        };
        let rate = match amount {
            ClockRainAmount::Light => 0.015,
            ClockRainAmount::Medium => 0.16,
            ClockRainAmount::Heavy => 0.30,
            ClockRainAmount::Varied => unreachable!(),
        };
        let facing = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let (surfaces, water) = match &arena {
            RainArena::Course(geometry) => {
                DigitSurfaces::with_floor(layout, display, geometry.water_pools())
            }
            RainArena::Responsive(floor) if floor.opening != 0.0 => {
                DigitSurfaces::on_responsive_floor(layout, display, floor)
            }
            RainArena::Responsive(_) => DigitSurfaces::new(layout, display),
        };
        let phase = if player_joined {
            ClockRainDuckPhase::NotSpawned
        } else {
            ClockRainDuckPhase::Waiting
        };
        Self {
            layout,
            water,
            tick: 0,
            entry_x: -facing * layout.bounds_max.x * 0.8,
            facing,
            arena,
            player_joined,
            player_hull: None,
            seed,
            source: source::RainSource::new(rng),
            amount,
            budget: f64::from(layout.bounds_max.x * 2.0 * layout.pitch)
                * rate
                * RAINING_TICKS as f64
                * DT,
            scheduled: 0.0,
            source_limited: 0,
            depth_ticks: 0,
            door_started: None,
            door_floor: layout.floor_y,
            phase,
            spawns: 0,
            floats: None,
            reclaimed_pose: None,
            surfaces,
        }
    }

    pub fn synchronize(&mut self, display: DisplaySnapshot, segments: &mut [SegmentState]) {
        self.surfaces
            .synchronize(&mut self.water, display, segments);
    }

    pub fn required_depth(&self) -> f32 {
        self.layout.pitch * 0.65
    }

    pub fn course(&self) -> Option<&CourseGeometry> {
        match &self.arena {
            RainArena::Course(course) => Some(course),
            _ => None,
        }
    }

    pub fn responsive_floor(&self) -> Option<&ResponsiveFloor> {
        match &self.arena {
            RainArena::Responsive(floor) => Some(floor),
            _ => None,
        }
    }

    /// Transfer the passive actor's translation to a player, without advancing
    /// water, the event clock or the panel actuator. Drop the old world first.
    pub fn join_player(&mut self) -> Option<(Vec2, Vec2)> {
        assert!(self.responsive_floor().is_some());
        let motion = self.floats.take().and_then(|floats| {
            floats
                .duck
                .as_ref()
                .and_then(|hull| floats.world.motion(hull.body()))
        });
        if motion.is_some() {
            self.phase = ClockRainDuckPhase::HandedOff;
        } else if matches!(
            self.phase,
            ClockRainDuckPhase::Waiting | ClockRainDuckPhase::Opening
        ) {
            self.phase = ClockRainDuckPhase::NotSpawned;
        }
        // Preserve a historical exit/reclamation/handoff across later rejoins.
        self.player_joined = true;
        self.door_started = None;
        self.reclaimed_pose = None;
        motion.map(|m| (m.position, m.linear_velocity))
    }

    pub fn set_player_hull(&mut self, hull: Option<(Vec2, f64)>) {
        self.player_hull = hull;
    }

    pub fn entry_depth(&self) -> f32 {
        let half = physics::half_extents(self.layout).x;
        self.water
            .pools()
            .iter()
            .take(self.surfaces.floor_pools)
            .flat_map(|p| {
                p.columns_in_range(
                    f64::from(self.entry_x - half),
                    f64::from(self.entry_x + half),
                )
            })
            .map(|c| {
                // Use the shallowest point actually under the hull, not the
                // minimum bed height of an inclined column.
                let left = c.left.max(f64::from(self.entry_x - half));
                let right = (c.left + c.width).min(f64::from(self.entry_x + half));
                (c.surface - c.bed_at(left).max(c.bed_at(right))).max(0.0) as f32
            })
            .reduce(f32::min)
            .unwrap_or(0.0)
    }

    fn entry_surface(&self) -> f32 {
        self.water.pools()[..self.surfaces.floor_pools]
            .iter()
            .flat_map(|p| {
                p.columns_in_range(
                    f64::from(self.entry_x) - 1e-4,
                    f64::from(self.entry_x) + 1e-4,
                )
            })
            .map(|c| c.surface as f32)
            .next()
            .unwrap_or(self.layout.floor_y)
    }

    pub fn phase(&self) -> EventPhase {
        if self.tick < RAINING_TICKS {
            EventPhase::Raining
        } else if self.tick < RAINING_TICKS + DRAIN_TICKS {
            EventPhase::Draining
        } else {
            EventPhase::Clearing
        }
    }
    pub fn phase_tick(&self) -> u64 {
        self.tick
            - match self.phase() {
                EventPhase::Raining => 0,
                EventPhase::Draining => RAINING_TICKS,
                _ => RAINING_TICKS + DRAIN_TICKS,
            }
    }
    pub fn opacity(&self) -> f32 {
        if self.phase() == EventPhase::Clearing {
            1.0 - self.phase_tick() as f32 / CLEAR_TICKS as f32
        } else {
            1.0
        }
    }
    pub fn door(&self) -> Option<(f32, f32)> {
        let elapsed = self.tick.checked_sub(self.door_started?)?;
        if elapsed >= OPEN_TICKS + 12 + CLOSE_TICKS {
            return None;
        }
        let open = if elapsed < OPEN_TICKS {
            elapsed as f32 / OPEN_TICKS as f32
        } else {
            1.0 - elapsed.saturating_sub(OPEN_TICKS + 12) as f32 / CLOSE_TICKS as f32
        };
        Some((self.door_floor, open.clamp(0.0, 1.0)))
    }
    pub fn duck_pose(&self) -> Option<(Vec2, f32)> {
        self.floats
            .as_ref()
            .and_then(|f| {
                f.duck
                    .as_ref()
                    .and_then(|d| f.world.motion(d.body()).map(|m| (m.position, m.angle)))
            })
            .or(self.reclaimed_pose)
    }
    pub fn physics_counts(&self) -> (usize, usize) {
        self.floats
            .as_ref()
            .map_or((0, 0), |f| (f.world.body_count(), f.world.collider_count()))
    }

    fn emit_rain(&mut self) {
        let t = self.tick.min(RAINING_TICKS) as f64 / RAINING_TICKS as f64;
        self.scheduled = self.budget * (t * t * (3.0 - 2.0 * t));
        if self.tick > RAINING_TICKS {
            return;
        }
        let count = self.source.emission_count(self.tick);
        // Attempt a final batch even if the next random shower is later.
        // Any undelivered budget remains explicit, not counted as liquid.
        let count = if self.tick == RAINING_TICKS {
            count.max(1)
        } else {
            count
        };
        if count == 0 {
            return;
        }
        let pending = (self.scheduled - self.water.stats().injected).max(0.0);
        if pending <= 1e-9 {
            return;
        }
        // Course slabs may have more than the ordinary floor's two outlets.
        // Reserve their first slices too, without enlarging the global budget.
        let source_limit = if self.course().is_some() {
            PARCELS - RELEASE_SLOTS - self.surfaces.floor_pools * 2
        } else {
            SOURCE_LIMIT
        };
        if self.water.parcels().len() + count > source_limit {
            self.source_limited += 1;
            return;
        }
        // Retiring wet digits can briefly occupy the reserved release slots.
        // Catch up the delayed shower over several ordinary batches, not one
        // screen-sized blob. Pending budget is not yet liquid; status/benchmarks
        // continue to show it until injected. One drop is at most one cell area.
        let volume = (pending / count as f64).min(f64::from(self.layout.pitch * 0.8).powi(2));
        for _ in 0..count {
            let half = self.layout.bounds_max.x;
            let fraction = self.source.next_fraction();
            self.water
                .add_falling(Parcel {
                    position: Vec2::new(
                        (fraction * 2.0 - 1.0) * half * 0.99,
                        self.layout.bounds_max.y - 1.0,
                    ),
                    velocity: Vec2::new(0.0, -220.0),
                    volume,
                    duration: 0.06,
                    horizontal_bounds: Some([f64::from(-half), f64::from(half)]),
                })
                .expect("rain fits reserved source budget");
        }
    }

    fn update_duck(&mut self) {
        if self.player_joined {
            return;
        }
        let RainArena::Responsive(floor) = &self.arena else {
            return;
        };
        let depth = self.entry_depth();
        if self.phase == ClockRainDuckPhase::Waiting {
            self.depth_ticks = if depth >= self.required_depth() {
                self.depth_ticks + 1
            } else {
                0
            };
            if self.depth_ticks >= DEPTH_TICKS && self.tick + OPEN_TICKS < RAINING_TICKS {
                self.phase = ClockRainDuckPhase::Opening;
                self.door_started = Some(self.tick);
                self.door_floor = self.entry_surface();
            } else if self.tick >= RAINING_TICKS {
                self.phase = ClockRainDuckPhase::NotSpawned;
            }
        }
        if self.phase == ClockRainDuckPhase::Opening
            && self.tick - self.door_started.unwrap() >= OPEN_TICKS
        {
            if depth < self.required_depth() {
                self.phase = ClockRainDuckPhase::Waiting;
                self.depth_ticks = 0;
                self.door_started = None;
            } else {
                let mut floats = FloatWorld::responsive(self.layout, floor);
                // Start just above the water, not intersecting a floor or pinned
                // to its moving surface. Buoyancy takes over during integration.
                floats.spawn(
                    Vec2::new(
                        self.entry_x,
                        self.entry_surface() + floats.half_extents.y * 1.4,
                    ),
                    0.0,
                );
                self.floats = Some(floats);
                self.spawns += 1;
                self.phase = ClockRainDuckPhase::Floating;
            }
        }
        if let Some(floats) = &mut self.floats {
            floats.move_floor(floor);
            floats.step(&self.water, 5.0);
            if floats.position().unwrap().y + floats.half_extents.length()
                < self.layout.bounds_min.y
            {
                floats.remove();
                self.floats = None;
                self.phase = ClockRainDuckPhase::Exited;
            }
        }
    }

    pub fn step(&mut self) -> bool {
        if self.tick >= RAIN_TICKS {
            return true;
        }
        self.tick += 1;
        self.emit_rain();
        let hull = if self.player_joined {
            self.player_hull
        } else if self.phase() == EventPhase::Clearing {
            None
        } else {
            self.duck_pose()
                .map(|(p, _)| (p, f64::from(physics::half_extents(self.layout).length())))
        };
        if let RainArena::Responsive(floor) = &mut self.arena {
            floor.step(&mut self.water, DT, hull);
        }
        self.water.step(DT).expect("fixed rain water step");
        if self.phase() == EventPhase::Clearing {
            if self.floats.is_some() {
                self.reclaimed_pose = self.duck_pose();
                self.floats = None;
                self.phase = ClockRainDuckPhase::Reclaimed;
            }
            self.water
                .reclaim_fraction(1.0 / (RAIN_TICKS - self.tick + 1) as f64)
                .expect("bounded rain cleanup");
        } else {
            self.update_duck();
        }
        self.tick >= RAIN_TICKS
    }

    pub fn diagnostics(&self) -> ClockRainState {
        let s = self.water.stats();
        let unit = f64::from(self.layout.pitch * 0.8).powi(2);
        let micro = |volume: f64| (volume / unit * 1_000_000.0).round() as u64;
        let motion = self
            .floats
            .as_ref()
            .and_then(|f| f.duck.as_ref().and_then(|d| f.world.motion(d.body())));
        let vector = |v: Vec2| [(v.x * 1000.0).round() as i32, (v.y * 1000.0).round() as i32];
        ClockRainState {
            player_course: self.course().is_some(),
            player_joined: self.player_joined,
            seed: self.seed,
            amount: self.amount,
            requested_microunits: micro(self.budget),
            scheduled_microunits: micro(self.scheduled),
            injected_microunits: micro(s.injected),
            pooled_microunits: micro(s.pooled),
            in_flight_microunits: micro(s.in_flight),
            drained_microunits: micro(s.drained),
            reclaimed_microunits: micro(s.reclaimed),
            parcels: s.parcels,
            source_limited_ticks: self.source_limited,
            water_limited_ticks: s.capacity_limited_ticks,
            surface_digits: self.surfaces.digits,
            surface_water_microunits: micro(
                self.water.pools()[self.surfaces.floor_pools..]
                    .iter()
                    .flat_map(|p| p.columns())
                    .map(|c| c.volume)
                    .sum(),
            ),
            drip_parcels_emitted: s.drip_parcels_emitted,
            surface_impacts: s.impact_transfers,
            surface_change_pending: self.surfaces.pending,
            surface_change_deferrals: self.surfaces.deferrals,
            floor_open_milli: self
                .responsive_floor()
                .map_or(0, |f| (f.opening * 1000.0).round() as u32),
            floor_load_milli: (self.course().map_or_else(
                || self.responsive_floor().unwrap().load,
                |geometry| {
                    self.water.pools()[..self.surfaces.floor_pools]
                        .iter()
                        .flat_map(|p| p.columns())
                        .map(|c| c.volume)
                        .sum::<f64>()
                        / f64::from(geometry.width)
                },
            ) * 1000.0)
                .round() as u32,
            floor_motion_deferrals: self.responsive_floor().map_or(0, |f| f.deferrals),
            floor_clearance_holds: self.responsive_floor().map_or(0, |f| f.clearance_holds),
            entry_depth_milli: (self.entry_depth() * 1000.0).round() as u32,
            required_depth_milli: (self.required_depth() * 1000.0).round() as u32,
            duck_phase: self.phase,
            duck_spawns: self.spawns,
            duck_position_milli: motion.map(|m| vector(m.position)),
            duck_velocity_milli: motion.map(|m| vector(m.linear_velocity)),
            duck_angle_milli: motion.map(|m| (m.angle * 1000.0).round() as i32),
            submerged_milli: self
                .floats
                .as_ref()
                .map_or(0, |f| (f.report.submerged_fraction * 1000.0).round() as u32),
            door_open_milli: self
                .door()
                .map_or(0, |(_, open)| (open * 1000.0).round() as u32),
        }
    }
}

#[cfg(test)]
mod tests;
