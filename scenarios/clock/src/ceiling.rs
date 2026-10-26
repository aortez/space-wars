//! Clock-owned sunlight opportunity. The timed event slot never owns the hold.
use crate::{ClockState, layout::Layout};
use engine_core::lighting::{ApertureSource, LightId, MAX_APERTURES};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::time::Duration;

pub const OPENING_DURATION: Duration = Duration::from_secs(5);
pub const OPEN_DURATION: Duration = Duration::from_secs(300);
pub const CLOSING_DURATION: Duration = Duration::from_secs(60);
/// Owner half of the sunlight identity; the opening sequence completes it.
pub const SUNLIGHT_OWNER: u64 = 0x4345_494c_494e_4700;
/// One opening is the quiet case; several give separate lit patches. Bounded
/// below the organism limit so every patch can hold its own resident.
pub const MAX_OPENINGS: usize = 3;
/// Narrowest useful opening: a patch needs a few columns of lit soil.
const MIN_OPENING_CELLS: f32 = 3.0;
/// Lit floor runs beside the clock face, one corridor on each side.
pub(crate) const CORRIDORS: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    Opening,
    Open,
    Closing,
    #[default]
    Closed,
}

#[derive(Default)]
pub struct Ceiling {
    pub phase: Phase,
    pub elapsed: Duration,
    pub sequence: u64,
    pub deliveries: u64,
    pub cancellations: u64,
    pending: bool,
    pub(crate) material: Vec<crate::grass::SoilSite>,
    pub cancelled_material: u64,
    pub planned_material: u64,
    pub committed_material: u64,
    /// Disjoint openings as (corridor, seat, low, high), with low and high the
    /// fractional span inside that seat. Seats partition their corridor, so
    /// openings cannot overlap, and fractions survive a resize as the same
    /// holes rather than new ones.
    openings: Vec<(usize, usize, f32, f32)>,
}
/// Explicit compatibility for every current event category. Water replacement
/// retires the opportunity; other effects keep one independent source owner.
pub(crate) fn compatible_event(kind: engine_common::ClockEventKind) -> bool {
    use engine_common::ClockEventKind::*;
    match kind {
        Meltdown | Aquarium => false,
        Falling | ColorCycle | Explosion | Duck | Marquee | DigitSlide | Rain | Crow | CellSwap
        | Grass => true,
    }
}
impl Ceiling {
    pub fn start(&mut self, seed: u64) -> bool {
        if self.phase != Phase::Closed {
            return false;
        }
        self.sequence += 1;
        self.phase = Phase::Opening;
        self.elapsed = Duration::ZERO;
        self.pending = true;
        self.openings = Self::plan_openings(seed);
        true
    }

    /// One opening per equal slot of its corridor, jittered inside that slot.
    /// Partitioning keeps every opening disjoint, which the lighting
    /// publication requires, without rejecting a drawn layout. Sides alternate,
    /// so a single opening takes the left corridor and a pair takes one each.
    fn plan_openings(seed: u64) -> Vec<(usize, usize, f32, f32)> {
        let mut rng = StdRng::seed_from_u64(seed ^ 0x4345_494c_5f48_4f4c);
        let count = rng.random_range(1..=MAX_OPENINGS);
        let mut openings = Vec::with_capacity(count);
        for side in 0..CORRIDORS {
            let share = count / CORRIDORS + usize::from(side < count % CORRIDORS);
            for seat in 0..share {
                let width = rng.random_range(0.62..0.98);
                let low = (1.0 - width) * rng.random_range(0.0..1.0);
                openings.push((side, seat, low, low + width));
            }
        }
        openings
    }
    pub fn cancel(&mut self) {
        if self.phase != Phase::Closed {
            self.cancellations += 1;
            self.phase = Phase::Closed;
            self.elapsed = Duration::ZERO;
        }
        self.pending = false;
        self.cancelled_material += self.material.len() as u64;
        self.material.clear();
    }
    pub fn disable_delivery(&mut self) {
        self.cancelled_material += self.material.len() as u64;
        self.material.clear();
        self.pending = false;
    }
    /// Normal simulation time only. Consume overshoot without shortening the hold.
    /// Reports whether the pending introduction was delivered at the open boundary.
    pub fn advance(&mut self, mut dt: Duration) -> bool {
        let mut delivered = false;
        while !dt.is_zero() && self.phase != Phase::Closed {
            let (duration, next) = match self.phase {
                Phase::Opening => (OPENING_DURATION, Phase::Open),
                Phase::Open => (OPEN_DURATION, Phase::Closing),
                Phase::Closing => (CLOSING_DURATION, Phase::Closed),
                Phase::Closed => unreachable!(),
            };
            let consumed = dt.min(duration - self.elapsed);
            self.elapsed += consumed;
            dt -= consumed;
            if self.elapsed == duration {
                self.phase = next;
                self.elapsed = Duration::ZERO;
                if next == Phase::Open && std::mem::take(&mut self.pending) {
                    delivered = true;
                    self.deliveries += 1;
                }
            }
        }
        delivered
    }
    pub fn fraction(&self) -> f32 {
        match self.phase {
            Phase::Closed => 0.0,
            Phase::Opening => self.elapsed.as_secs_f32() / OPENING_DURATION.as_secs_f32(),
            Phase::Open => 1.0,
            Phase::Closing => 1.0 - self.elapsed.as_secs_f32() / CLOSING_DURATION.as_secs_f32(),
        }
    }
    /// Floor an opening may be cut above. Material covers the whole floor, but
    /// sunlight only reaches the corridors beside the 6×9 font: the face spans
    /// nearly the whole arena, and a vertical beam through whichever segments
    /// happen to form the current hour does not sustain a patch. Measured over
    /// 180 s with openings under the face, every organism died.
    pub(crate) fn corridors(layout: Layout) -> [(f32, f32); CORRIDORS] {
        [
            (
                layout.bounds_min.x + layout.frame_width,
                layout.face_origin.x - 2.0,
            ),
            (
                -layout.face_origin.x + 2.0,
                layout.bounds_max.x - layout.frame_width,
            ),
        ]
    }

    /// Outer bounds of the lit corridors, for callers that need one span.
    pub(crate) fn span(layout: Layout) -> (f32, f32) {
        let corridors = Self::corridors(layout);
        (corridors[0].0, corridors[CORRIDORS - 1].1)
    }

    /// Every live opening, left to right. Closure removes light from both edges
    /// of each one, so a closing ceiling dims its patches in place.
    pub(crate) fn sources(&self, layout: Layout) -> Vec<ApertureSource> {
        let fraction = self.fraction();
        if fraction <= 0.0 {
            return Vec::new();
        }
        let corridors = Self::corridors(layout);
        let minimum = MIN_OPENING_CELLS * crate::grass::SOIL_CELL_SIZE;
        let mut sources = Vec::with_capacity(self.openings.len());
        for side in 0..CORRIDORS {
            let (start, end) = corridors[side];
            let corridor = (end - start).max(0.0);
            let seated: Vec<_> = self
                .openings
                .iter()
                .enumerate()
                .filter(|(_, o)| o.0 == side)
                .collect();
            if seated.is_empty() {
                continue;
            }
            let slot = corridor / seated.len() as f32;
            // A corridor too narrow to seat every planned opening at a useful
            // width seats one across the whole corridor instead. Overlapping
            // openings would be refused by the lighting publication outright.
            let seats = if slot < minimum { 1 } else { seated.len() };
            for &(index, &(_, seat, low, high)) in seated.iter().take(seats) {
                let (slot_start, slot_width) = if seats == 1 {
                    (start, corridor)
                } else {
                    (start + slot * seat as f32, slot)
                };
                let width = (slot_width * (high - low)).clamp(minimum.min(slot_width), slot_width);
                let left = (slot_start + slot_width * low).min(slot_start + slot_width - width);
                let center = left + width * 0.5;
                let half = width * 0.5 * fraction;
                sources.push(ApertureSource {
                    // One identity per opening per sequence, so a later ceiling
                    // can never reuse a live source's lifetime.
                    id: LightId(
                        SUNLIGHT_OWNER,
                        self.sequence * MAX_APERTURES as u64 + index as u64,
                    ),
                    left: center - half,
                    right: center + half,
                    ceiling_y: layout.canopy_y,
                    strength: 950.0,
                });
            }
        }
        sources
    }
}
impl ClockState {
    pub(crate) fn plan_ceiling_material(&mut self) {
        use crate::grass;
        use engine_rapier::world::{CollisionGroups, PhysicsId};
        if !self.plants.plants.observe().tissues.is_empty() {
            return;
        }
        let layout = Layout::new(self.aspect_ratio());
        // An opening delivers an even layer over every actual support. Its
        // horizontal extent decides where light lands, not where dirt rests.
        let (surfaces, _) = self.soil_surfaces();
        let supports: Vec<_> = surfaces
            .iter()
            .map(|s| (s.left, s.right))
            .filter(|(a, b)| a < b)
            .collect();
        let temporary = grass::SoilHabitat::new(layout);
        let soil = self.soil.as_ref().unwrap_or(&temporary);
        let world = grass::surfaces::world(&self.active_event, self.duck_visit.as_deref());
        self.ceiling.material = soil
            .plan_admission(&supports, |site| {
                let Some(surface) = surfaces.iter().find(|s| s.covers(site.min.x, site.max.x))
                else {
                    return true;
                };
                if site.inside_support(layout.floor_y) {
                    return false;
                }
                world.is_some_and(|w| {
                    !w.current_cuboid_is_clear(
                        surface.map((site.min + site.max) * 0.5),
                        surface.angle,
                        (site.max - site.min) * 0.5 - engine_core::Vec2::new(0.001, 0.001),
                        CollisionGroups::ALL,
                        &[PhysicsId::new(grass::SOIL_PHYSICS_ID)],
                    )
                })
            })
            .sites;
        self.ceiling.planned_material += self.ceiling.material.len() as u64;
    }
    pub fn cancel_ceiling(&mut self) {
        self.ceiling.cancel();
        self.plants.apertures.clear();
    }
}

#[cfg(test)]
mod ceiling_light_contract;
