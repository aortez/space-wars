//! Clock-owned sunlight opportunity. The timed event slot never owns the hold.
use crate::{ClockState, layout::Layout};
use engine_core::lighting::{ApertureSource, LightId};
use std::time::Duration;

pub const OPENING_DURATION: Duration = Duration::from_secs(5);
pub const OPEN_DURATION: Duration = Duration::from_secs(300);
pub const CLOSING_DURATION: Duration = Duration::from_secs(60);
/// Owner half of the sunlight identity; the opening sequence completes it.
pub const SUNLIGHT_OWNER: u64 = 0x4345_494c_494e_4700;

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
    pub fn start(&mut self) -> bool {
        if self.phase != Phase::Closed {
            return false;
        }
        self.sequence += 1;
        self.phase = Phase::Opening;
        self.elapsed = Duration::ZERO;
        self.pending = true;
        true
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
    pub(crate) fn region(layout: Layout) -> (f32, f32) {
        // Keep the baseline opportunity beside every 6×9 font, rather than
        // beneath whichever segments happen to form the current hour.
        // Narrow layouts admit fewer seeds into the smaller real corridor;
        // wide layouts keep material and beam work bounded to a small patch.
        let left = layout.bounds_min.x + layout.frame_width + 2.0;
        let right = (layout.face_origin.x - 2.0).min(left + 48.0);
        (left, right)
    }
    pub(crate) fn source(&self, layout: Layout) -> Option<ApertureSource> {
        let fraction = self.fraction();
        if fraction <= 0.0 {
            return None;
        }
        // The admission corridor and source share one layout-derived footprint.
        // Closure removes light from both edges.
        let (left, right) = Self::region(layout);
        let center = (left + right) * 0.5;
        let half = (right - left) * 0.5 * fraction;
        Some(ApertureSource {
            id: LightId(SUNLIGHT_OWNER, self.sequence),
            left: center - half,
            right: center + half,
            ceiling_y: layout.canopy_y,
            strength: 950.0,
        })
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
        let (left, right) = Ceiling::region(layout);
        let (surfaces, _) = self.soil_surfaces();
        let supports: Vec<_> = surfaces
            .iter()
            .map(|s| {
                if self.config.favorable_grass_fixture {
                    (s.left, s.right)
                } else {
                    (s.left.max(left), s.right.min(right))
                }
            })
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
        self.plants.aperture = None;
    }
}

#[cfg(test)]
mod ceiling_light_contract;
