//! Rain's physical arena, independent of digit events and duck navigation.
mod physics;

use engine_common::{ClockRainAmount, ClockRainDuckPhase, ClockRainState};
use engine_core::Vec2;
use engine_water::{Parcel, WaterWorld};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{events::EventPhase, layout::Layout};
use physics::{DT, FloatWorld};

pub const RAINING_TICKS: u64 = 20 * 60;
pub const DRAIN_TICKS: u64 = 20 * 60;
pub const CLEAR_TICKS: u64 = 120;
pub const RAIN_TICKS: u64 = RAINING_TICKS + DRAIN_TICKS + CLEAR_TICKS;
// Existing outlet parcels already consume slots. Reserve two *new* slots for
// this tick's two outlets, rather than starving the source behind those parcels.
const SOURCE_LIMIT: usize = physics::PARCELS - 2;
const OPEN_TICKS: u64 = 36;
const CLOSE_TICKS: u64 = 24;
const DEPTH_TICKS: u64 = 30;

pub(crate) struct RainEvent {
    pub layout: Layout,
    pub water: WaterWorld,
    pub tick: u64,
    pub entry_x: f32,
    pub facing: f32,
    seed: u64,
    rng: StdRng,
    amount: ClockRainAmount,
    budget: f64,
    scheduled: f64,
    source_limited: u64,
    drops: u64,
    depth_ticks: u64,
    door_started: Option<u64>,
    door_floor: f32,
    phase: ClockRainDuckPhase,
    spawns: u32,
    floats: Option<FloatWorld>,
    reclaimed_pose: Option<(Vec2, f32)>,
}

impl RainEvent {
    pub fn new(layout: Layout, seed: u64, amount: ClockRainAmount) -> Self {
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
        Self {
            layout,
            water: physics::water(layout),
            tick: 0,
            entry_x: -facing * layout.bounds_max.x * 0.8,
            facing,
            seed,
            rng,
            amount,
            budget: f64::from(layout.bounds_max.x * 2.0 * layout.pitch)
                * rate
                * RAINING_TICKS as f64
                * DT,
            scheduled: 0.0,
            source_limited: 0,
            drops: 0,
            depth_ticks: 0,
            door_started: None,
            door_floor: layout.floor_y,
            phase: ClockRainDuckPhase::Waiting,
            spawns: 0,
            floats: None,
            reclaimed_pose: None,
        }
    }

    pub fn required_depth(&self) -> f32 {
        self.layout.pitch * 0.65
    }

    pub fn entry_depth(&self) -> f32 {
        let half = physics::half_extents(self.layout).x;
        self.water
            .pools()
            .iter()
            .flat_map(|p| {
                p.columns_in_range(
                    f64::from(self.entry_x - half),
                    f64::from(self.entry_x + half),
                )
            })
            .map(|c| (c.surface - c.bed) as f32)
            .reduce(f32::min)
            .unwrap_or(0.0)
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
        if self.tick > RAINING_TICKS || self.tick % 2 != 0 {
            return;
        }
        let pending = (self.scheduled - self.water.stats().injected).max(0.0);
        if pending <= 1e-9 {
            return;
        }
        if self.water.parcels().len() + 2 > SOURCE_LIMIT {
            self.source_limited += 1;
            return;
        }
        for _ in 0..2 {
            let half = self.layout.bounds_max.x;
            // Stratified showers avoid accidentally concentrating a whole
            // storm in one column. Jitter keeps the visible rain irregular.
            let fraction =
                (((self.drops * 13) % 32) as f32 + self.rng.random_range(0.15..0.85)) / 32.0;
            self.drops += 1;
            self.water
                .add_falling(Parcel {
                    position: Vec2::new(
                        (fraction * 2.0 - 1.0) * half * 0.99,
                        self.layout.bounds_max.y - 1.0,
                    ),
                    velocity: Vec2::new(0.0, -220.0),
                    volume: pending * 0.5,
                    duration: 0.06,
                    horizontal_bounds: Some([f64::from(-half), f64::from(half)]),
                })
                .expect("rain fits reserved source budget");
        }
    }

    fn update_duck(&mut self) {
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
                self.door_floor = self.layout.floor_y + depth;
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
                let mut floats = FloatWorld::new(self.layout);
                // Start just above the water, not intersecting a floor or pinned
                // to its moving surface. Buoyancy takes over during integration.
                floats.spawn(
                    Vec2::new(
                        self.entry_x,
                        self.layout.floor_y + depth + floats.half_extents.y * 1.4,
                    ),
                    0.0,
                );
                self.floats = Some(floats);
                self.spawns += 1;
                self.phase = ClockRainDuckPhase::Floating;
            }
        }
        if let Some(floats) = &mut self.floats {
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
