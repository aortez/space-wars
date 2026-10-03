//! Opt-in drain splashes. A small amount of colliding outfall water
//! becomes independent ballistic drops. The upward fan is authored; this is
//! not a pressure solve. Its kinetic energy is bounded by dissipated motion.
use crate::{Parcel, WaterConfig, WaterWorld};
use engine_core::{
    Vec2,
    rng::{random_unit_f32, seeded_rng},
};

#[cfg(test)]
mod tests;

const DROPS: usize = 3;
// Optional spray must leave ordinary outlets room to keep draining.
const FLOW_RESERVE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplashVariation {
    /// Each accepted burst derives its own sequence from this seed and burst
    /// number. Rejected impacts do not consume randomness or change later fans.
    pub seed: u64,
    /// Sample the next cooldown between `SplashConfig::interval` and this value.
    pub max_interval: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplashConfig {
    /// Normal wall speed or RMS opposing horizontal speed below which no spray forms.
    pub min_speed: f64,
    /// Fraction of the impacted parcel redirected into spray, in (0, 0.25].
    pub volume_fraction: f64,
    /// Fraction of dissipated kinetic energy available to the upward fan, in (0, 1].
    pub energy_fraction: f64,
    /// Minimum simulated seconds between bursts across the whole water world.
    pub interval: f64,
    /// Optional irregular timing, angles and droplet shares. None retains the
    /// original fixed fan and interval for comparison.
    pub variation: Option<SplashVariation>,
}

impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            min_speed: 24.0,
            volume_fraction: 0.12,
            energy_fraction: 0.75,
            interval: 0.35,
            variation: None,
        }
    }
}

impl SplashConfig {
    pub(crate) fn valid(self) -> bool {
        self.min_speed.is_finite()
            && (0.0..=1.0e6).contains(&self.min_speed)
            && self.volume_fraction.is_finite()
            && self.volume_fraction > 0.0
            && self.volume_fraction <= 0.25
            && self.energy_fraction.is_finite()
            && self.energy_fraction > 0.0
            && self.energy_fraction <= 1.0
            && self.interval.is_finite()
            && (0.05..=60.0).contains(&self.interval)
            && self.variation.is_none_or(|v| {
                v.max_interval.is_finite() && (self.interval..=60.0).contains(&v.max_interval)
            })
    }
}

struct Pattern {
    directions: [Vec2; DROPS],
    weights: [f64; DROPS],
    speed_factors: [f32; DROPS],
    interval: f64,
}

impl Pattern {
    fn new(config: SplashConfig, normal: Vec2, burst: u64) -> Self {
        let wall = normal.x.abs() > 0.5;
        let mut pattern = Self {
            directions: std::array::from_fn(|i| {
                let x = if wall {
                    normal.x.signum() * [0.2, 0.65, 1.0][i]
                } else {
                    [-0.65, 0.0, 0.65][i]
                };
                Vec2::new(x, 1.0).normalized()
            }),
            weights: [0.28, 0.44, 0.28],
            speed_factors: [0.85, 1.0, 0.85],
            interval: config.interval,
        };
        let Some(variation) = config.variation else {
            return pattern;
        };
        let mut rng = seeded_rng(
            variation
                .seed
                .wrapping_add(burst.wrapping_mul(0x9e3779b97f4a7c15)),
        );
        pattern.interval +=
            (variation.max_interval - config.interval) * f64::from(random_unit_f32(&mut rng));
        let shares: [f64; DROPS] =
            std::array::from_fn(|_| 0.5 + f64::from(random_unit_f32(&mut rng)));
        let total: f64 = shares.iter().sum();
        pattern.weights = shares.map(|share| share / total);
        // Close the sum explicitly so randomized shares never invent water.
        pattern.weights[2] = 1.0 - pattern.weights[0] - pattern.weights[1];
        for i in 0..DROPS {
            let base = pattern.directions[i].x.atan2(pattern.directions[i].y);
            let jitter = (random_unit_f32(&mut rng) * 20.0 - 10.0).to_radians();
            let angle = if wall {
                // Mirror the same seeded pattern for the opposite wall. Keep
                // every ray on the free side, including tilted panel faces.
                let minimum = (-normal.y).atan2(normal.x.abs()).max(0.0) + 0.001;
                (base.abs() + jitter).clamp(minimum, 55.0_f32.to_radians()) * normal.x.signum()
            } else {
                base + jitter
            };
            let (x, y) = angle.sin_cos();
            pattern.directions[i] = Vec2::new(x, y).normalized();
            pattern.speed_factors[i] = 0.75 + 0.25 * random_unit_f32(&mut rng);
        }
        pattern
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Impact {
    pub position: Vec2,
    pub normal: Vec2,
    pub speed: f64,
}

#[derive(Default)]
pub(crate) struct State {
    cooldown: f64,
    pending: Option<[Parcel; DROPS]>,
    pub bursts: u64,
    pub volume: f64,
    pub suppressed: u64,
}

impl State {
    pub fn begin_step(&mut self, dt: f64) {
        debug_assert!(self.pending.is_none());
        self.cooldown = (self.cooldown - dt).max(0.0);
    }

    pub fn pending_count(&self) -> usize {
        if self.pending.is_some() { DROPS } else { 0 }
    }

    pub fn take(&mut self) -> Option<[Parcel; DROPS]> {
        self.pending.take()
    }

    /// Returns true only after volume has been moved into the fixed pending
    /// buffer. Capacity/threshold rejection leaves the source exactly intact.
    pub fn split(
        &mut self,
        source: &mut Parcel,
        impact: Impact,
        free: usize,
        config: WaterConfig,
    ) -> bool {
        let Some(splash) = config.splash else {
            return false;
        };
        if self.cooldown > 1.0e-9
            || self.pending.is_some()
            || impact.speed < splash.min_speed
            || impact.normal.y < -0.5
        {
            return false;
        }
        if free < DROPS + FLOW_RESERVE {
            self.suppressed += 1;
            return false;
        }
        let amount = source.volume * splash.volume_fraction;
        let speed = (impact.speed * (splash.energy_fraction / splash.volume_fraction).sqrt())
            .min(config.max_speed) as f32;
        let pattern = Pattern::new(splash, impact.normal, self.bursts);
        let drops = std::array::from_fn(|i| Parcel {
            position: impact.position,
            velocity: pattern.directions[i] * speed * pattern.speed_factors[i],
            volume: amount * pattern.weights[i],
            duration: 1.0 / 120.0,
            horizontal_bounds: source.horizontal_bounds,
        });
        source.volume -= amount;
        self.pending = Some(drops);
        self.cooldown = pattern.interval;
        self.bursts += 1;
        self.volume += amount;
        true
    }
}

impl WaterWorld {
    pub(crate) fn splash_wall(
        &mut self,
        index: usize,
        parcel: &mut Parcel,
        impact: Option<Impact>,
    ) -> bool {
        let Some(impact) = impact else { return false };
        if self.spills[index].is_none_or(|s| s.source.outlet_id().is_none()) {
            return false;
        }
        let free = (self.config.max_parcels - self.config.reserved_release_parcels)
            .saturating_sub(self.parcels.len());
        if !self.splash.split(parcel, impact, free, self.config) {
            return false;
        }
        // A split slice no longer has its original material faces.
        self.spills[index] = None;
        true
    }
}
