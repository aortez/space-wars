//! One cheap resident visitor. No wall-clock sampling or separate physics world.
//! Perch feet use the same lit-cell geometry as rendering.
mod environment;
mod flight;
mod water_tolerance;
pub(crate) use engine_common::ClockCrowPhase as Phase;
use engine_common::{ClockCrowState, ClockCrowWaterTolerance, ClockEventKind};
use engine_core::Vec2;
pub(crate) use environment::Environment;
pub(crate) use water_tolerance::WaterTolerance;
#[cfg(test)]
mod behavior_tests;
use flight::Flight;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{SegmentRepresentation, SegmentState, layout::Layout};

pub const CROW_TICKS: u64 = 22 * 60;
const MAX_FLIGHT_TICKS: u64 = 8 * 60;
const EXIT_TICKS: u64 = 5 * 60;
const PECK_TICKS: u64 = 84;

#[derive(Debug, Clone, Copy)]
struct Perch {
    key: [u8; 3],
    feet: Vec2,
}

#[derive(Debug, Clone, Copy)]
enum Target {
    Digit(Perch),
    Ground(Vec2),
}

impl Target {
    fn feet(self) -> Vec2 {
        match self {
            Self::Digit(p) => p.feet,
            Self::Ground(p) => p,
        }
    }
    fn key(self) -> Option<[u8; 3]> {
        match self {
            Self::Digit(p) => Some(p.key),
            Self::Ground(_) => None,
        }
    }
}

/// Only the upper silhouette in each of the 24 digit columns is landable.
/// This excludes hidden lower bars and keeps flight descents clear of digits.
fn perches(
    layout: Layout,
    segments: &[SegmentState],
    event: Option<ClockEventKind>,
) -> [Option<Perch>; 24] {
    let mut result: [Option<Perch>; 24] = [None; 24];
    if matches!(
        event,
        Some(
            ClockEventKind::Falling
                | ClockEventKind::Meltdown
                | ClockEventKind::DigitSlide
                | ClockEventKind::Marquee
                | ClockEventKind::Explosion
        )
    ) {
        return result;
    }
    for segment in segments
        .iter()
        .filter(|s| s.lit && s.representation == SegmentRepresentation::Anchored)
    {
        for cell in segment.cells() {
            let index = usize::from(segment.id.digit_slot) * 6 + cell.x as usize;
            let feet = layout.cell_center(segment.id, cell) + Vec2::new(0.0, layout.pitch * 0.4);
            if result[index].is_none_or(|old| feet.y > old.feet.y) {
                result[index] = Some(Perch {
                    key: [segment.id.digit_slot, cell.x as u8, cell.y as u8],
                    feet,
                });
            }
        }
    }
    result
}

pub(crate) struct CrowVisit {
    layout: Layout,
    visit_id: u64,
    age: u64,
    pub phase: Phase,
    pub phase_tick: u64,
    pub position: Vec2,
    pub facing_right: bool,
    target: Option<Target>,
    from: Vec2,
    to: Vec2,
    duration: u64,
    rng: StdRng,
    hops: u32,
    escapes: u32,
    pub flight: Flight,
    ground_attempted: bool,
    ground_visits: u32,
    pecks: u32,
    wetness: f32,
    water_tolerance: WaterTolerance,
    wet_departures: u32,
}

impl CrowVisit {
    pub fn new(
        layout: Layout,
        seed: u64,
        visit_id: u64,
        water_tolerance: ClockCrowWaterTolerance,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let facing_right = rng.random_bool(0.5);
        let position = Vec2::new(
            if facing_right {
                layout.bounds_min.x - layout.pitch * 2.0
            } else {
                layout.bounds_max.x + layout.pitch * 2.0
            },
            Self::cruise_height(layout),
        );
        let mut visit = Self {
            layout,
            visit_id,
            age: 0,
            phase: Phase::Entering,
            phase_tick: 0,
            position,
            facing_right,
            target: None,
            from: position,
            to: position,
            duration: MAX_FLIGHT_TICKS,
            rng,
            hops: 0,
            escapes: 0,
            flight: Flight::default(),
            ground_attempted: false,
            ground_visits: 0,
            pecks: 0,
            wetness: 0.0,
            water_tolerance: WaterTolerance::new(water_tolerance, seed),
            wet_departures: 0,
        };
        let candidates = perches(layout, segments, event);
        let target = visit.choose(&candidates, false, Environment::default());
        if let Some(target) = target {
            visit.travel(Target::Digit(target), Phase::Entering);
        } else {
            visit.leave();
        }
        visit
    }

    fn cruise_height(layout: Layout) -> f32 {
        // Fly above the tallest possible digit, with room for raised wings.
        // Keeping this relative to the face avoids enormous portrait detours.
        (layout.pitch * 5.2).min(layout.canopy_y - layout.pitch * 1.6 - 8.0)
    }

    fn choose(
        &mut self,
        candidates: &[Option<Perch>; 24],
        nearby: bool,
        environment: Environment<'_>,
    ) -> Option<Perch> {
        let mut count = 0;
        let mut selected = None;
        for candidate in candidates.iter().flatten().copied() {
            if self.target.and_then(Target::key) == Some(candidate.key)
                || environment.wet_feet(candidate.feet, self.layout, self.water_tolerance)
            {
                continue;
            }
            let delta = candidate.feet - self.position;
            // Same-level adjacent blocks make readable, short two-footed hops.
            if nearby && (delta.y.abs() > 0.01 || delta.x.abs() > self.layout.pitch * 2.1) {
                continue;
            }
            count += 1;
            if self.rng.random_range(0..count) == 0 {
                selected = Some(candidate);
            }
        }
        selected
    }

    fn travel(&mut self, target: Target, phase: Phase) {
        let grounded = matches!(self.phase, Phase::Perched | Phase::Pecking);
        self.target = Some(target);
        self.from = self.position;
        self.to = target.feet();
        self.facing_right = self.to.x >= self.from.x;
        self.phase = phase;
        self.phase_tick = 0;
        self.duration = if phase == Phase::Hopping {
            28
        } else {
            MAX_FLIGHT_TICKS
        };
        if phase != Phase::Hopping {
            self.flight.retarget(self.position, grounded, self.layout);
        }
    }

    fn leave(&mut self) {
        let grounded = matches!(self.phase, Phase::Perched | Phase::Pecking);
        self.target = None;
        self.from = self.position;
        self.to = Vec2::new(
            if self.facing_right {
                self.layout.bounds_max.x + self.layout.pitch * 2.0
            } else {
                self.layout.bounds_min.x - self.layout.pitch * 2.0
            },
            Self::cruise_height(self.layout),
        );
        self.phase = Phase::Leaving;
        self.phase_tick = 0;
        self.duration = EXIT_TICKS;
        self.flight.retarget(self.position, grounded, self.layout);
    }

    /// React to support changes even in a zero-dt control/read synchronization.
    /// Position and age remain frozen until an actual simulation tick.
    pub fn synchronize(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
    ) {
        if self.phase == Phase::Leaving {
            return;
        }
        let candidates = perches(self.layout, segments, event);
        let valid = self.target.is_some_and(|target| match target {
            Target::Digit(p) => {
                candidates.iter().flatten().any(|c| c.key == p.key)
                    && (self.phase == Phase::Perched
                        || !environment.wet_feet(p.feet, self.layout, self.water_tolerance))
            }
            Target::Ground(p) => {
                candidates.iter().any(Option::is_some)
                    && environment.ground_available
                    && (self.phase == Phase::Pecking
                        || (!environment.wet_feet(p, self.layout, self.water_tolerance)
                            && (!self.water_tolerance.avoids_spray
                                || environment.spray(p, self.layout) == 0.0)))
            }
        });
        if valid {
            return;
        }
        self.escapes += 1;
        self.return_to_perch(&candidates, environment);
    }

    fn return_to_perch(&mut self, candidates: &[Option<Perch>; 24], environment: Environment<'_>) {
        let nearby = matches!(self.target, Some(Target::Ground(_)))
            .then(|| {
                candidates
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|p| !environment.wet_feet(p.feet, self.layout, self.water_tolerance))
                    .min_by(|a, b| {
                        let cost = |p: Perch| {
                            (p.feet.x - self.position.x).abs()
                                + (Self::cruise_height(self.layout) - p.feet.y).abs() * 2.0
                        };
                        cost(*a).total_cmp(&cost(*b))
                    })
            })
            .flatten();
        if let Some(target) = nearby.or_else(|| self.choose(candidates, false, environment)) {
            self.travel(Target::Digit(target), Phase::Flying);
        } else {
            self.leave();
        }
    }

    fn try_ground(&mut self, environment: Environment<'_>) -> bool {
        if self.ground_attempted {
            return false;
        }
        self.ground_attempted = true;
        // Reserve a climb back to the digits and the ordinary offscreen exit.
        // Very tall/narrow layouts can simply keep the existing perch visit.
        let round_trip = 2.0 * (Self::cruise_height(self.layout) - self.layout.floor_y)
            / (self.layout.pitch * 5.0)
            + 8.0;
        let available = (CROW_TICKS - EXIT_TICKS).saturating_sub(self.age) as f32 / 60.0;
        if round_trip > available || !self.rng.random_bool(0.65) {
            return false;
        }
        let spots = environment.ground_spots(self.layout, self.water_tolerance);
        if let Some(target) = spots.into_iter().flatten().min_by(|a, b| {
            (a.x - self.position.x)
                .abs()
                .total_cmp(&(b.x - self.position.x).abs())
        }) {
            self.travel(Target::Ground(target), Phase::Flying);
            return true;
        }
        false
    }

    pub fn step(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
    ) -> bool {
        self.synchronize(segments, event, environment);
        self.age += 1;
        if self.age >= CROW_TICKS {
            return true;
        }
        if self.age >= CROW_TICKS - EXIT_TICKS && self.phase != Phase::Leaving {
            self.leave();
        }
        let spray = environment.spray(self.position, self.layout);
        self.wetness = (self.wetness + spray / self.water_tolerance.soak_ticks
            - if spray == 0.0 { 1.0 / 180.0 } else { 0.0 })
        .clamp(0.0, 1.0);
        if self.phase != Phase::Leaving
            && (self.wetness >= 1.0
                || (matches!(self.phase, Phase::Perched | Phase::Pecking)
                    && environment.wet_feet(self.position, self.layout, self.water_tolerance)))
        {
            self.wet_departures += 1;
            self.leave();
        }
        self.phase_tick += 1;
        if self.phase == Phase::Pecking {
            if self.phase_tick % 28 == 16 {
                self.pecks += 1;
            }
            if self.phase_tick >= self.duration {
                self.return_to_perch(&perches(self.layout, segments, event), environment);
            }
            return false;
        }
        if self.phase == Phase::Perched {
            // Consider the one ground excursion after a short look around,
            // before spending the visit's time budget on additional hops.
            if self.phase_tick >= 30 && self.try_ground(environment) {
                return false;
            }
            if self.phase_tick >= self.duration {
                let candidates = perches(self.layout, segments, event);
                if let Some(target) = self.choose(&candidates, true, environment) {
                    self.hops += 1;
                    self.travel(Target::Digit(target), Phase::Hopping);
                } else if let Some(target) = self.choose(&candidates, false, environment) {
                    self.travel(Target::Digit(target), Phase::Flying);
                } else {
                    self.leave();
                }
            }
            return false;
        }
        if self.phase == Phase::Hopping {
            let t = (self.phase_tick as f32 / self.duration as f32).min(1.0);
            self.position = self.from * (1.0 - t)
                + self.to * t
                + Vec2::new(0.0, self.layout.pitch * 0.9 * 4.0 * t * (1.0 - t));
        } else {
            let leaving = self.phase == Phase::Leaving;
            let previous = self.position;
            let mut landed = self.flight.step(
                &mut self.position,
                self.to,
                Self::cruise_height(self.layout),
                self.layout,
                leaving,
            );
            // Retargeting or departure can interrupt a descending approach.
            // Its old support still catches the feet if that digit survives.
            let contact = perches(self.layout, segments, event)
                .into_iter()
                .flatten()
                .filter(|p| previous.y >= p.feet.y && self.position.y < p.feet.y)
                .filter(|p| {
                    let fraction = (previous.y - p.feet.y) / (previous.y - self.position.y);
                    let x = previous.x + (self.position.x - previous.x) * fraction;
                    (x - p.feet.x).abs() <= self.layout.pitch * 0.4
                })
                .max_by(|a, b| a.feet.y.total_cmp(&b.feet.y));
            if let Some(contact) = contact {
                self.position.y = contact.feet.y;
                self.flight.retarget(self.position, true, self.layout);
                landed = false;
            }
            // A wet destination can be abandoned on the last descent tick.
            // Its solid floor still catches the feet while the wings reverse
            // the downward momentum; aborting a plan does not remove support.
            if let Some(floor) = environment.floor_support(self.position.x, self.layout)
                && self.position.y < floor
                && previous.y >= floor
            {
                self.position.y = floor;
                self.flight.retarget(self.position, true, self.layout);
                landed = false;
            }
            if self.flight.velocity.x.abs() > self.layout.pitch * 0.2 {
                self.facing_right = self.flight.velocity.x > 0.0;
            }
            if leaving {
                return self.position.x < self.layout.bounds_min.x - self.layout.pitch * 1.5
                    || self.position.x > self.layout.bounds_max.x + self.layout.pitch * 1.5
                    || self.phase_tick >= self.duration;
            }
            if !landed {
                if self.phase_tick >= self.duration {
                    self.leave();
                }
                return false;
            }
        }
        if self.phase != Phase::Hopping || self.phase_tick >= self.duration {
            self.position = self.to;
            self.flight = Flight::default();
            self.phase_tick = 0;
            if matches!(self.target, Some(Target::Ground(_))) {
                self.phase = Phase::Pecking;
                self.ground_visits += 1;
                self.duration = PECK_TICKS;
            } else {
                self.phase = Phase::Perched;
                self.duration = self.rng.random_range(50..=120);
            }
        }
        false
    }

    pub fn diagnostics(&self) -> ClockCrowState {
        ClockCrowState {
            visit_id: self.visit_id,
            water_tolerance: self.water_tolerance.kind,
            phase: self.phase,
            phase_tick: self.phase_tick,
            age_ticks: self.age,
            position_milli: [self.position.x, self.position.y].map(|v| (v * 1000.0).round() as i32),
            facing_right: self.facing_right,
            target: self.target.and_then(Target::key),
            ground_target_milli: self.target.and_then(|t| match t {
                Target::Ground(p) => Some([p.x, p.y].map(|v| (v * 1000.0).round() as i32)),
                Target::Digit(_) => None,
            }),
            ground_visits: self.ground_visits,
            pecks: self.pecks,
            wetness_milli: (self.wetness * 1000.0).round() as u16,
            wet_departures: self.wet_departures,
            hops: self.hops,
            escapes: self.escapes,
        }
    }
}
