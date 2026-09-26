//! One cheap resident visitor. No wall-clock sampling or separate physics world.
//! Perch feet use the same lit-cell geometry as rendering; flight is kinematic.
pub(crate) use engine_common::ClockCrowPhase as Phase;
use engine_common::{ClockCrowState, ClockEventKind};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{SegmentRepresentation, SegmentState, digits, layout::Layout};

pub const CROW_TICKS: u64 = 22 * 60;
const FLIGHT_TICKS: u64 = 100;
const EXIT_TICKS: u64 = 100;

#[derive(Debug, Clone, Copy)]
struct Perch {
    key: [u8; 3],
    feet: Vec2,
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
        for cell in digits::cells(segment.id.kind) {
            let index = usize::from(segment.id.digit_slot) * 6 + cell.x as usize;
            let feet = layout.cell_center(segment.id, *cell) + Vec2::new(0.0, layout.pitch * 0.4);
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
    target: Option<Perch>,
    from: Vec2,
    to: Vec2,
    duration: u64,
    rng: StdRng,
    hops: u32,
    escapes: u32,
}

impl CrowVisit {
    pub fn new(
        layout: Layout,
        seed: u64,
        visit_id: u64,
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
            duration: FLIGHT_TICKS,
            rng,
            hops: 0,
            escapes: 0,
        };
        let candidates = perches(layout, segments, event);
        let target = visit.choose(&candidates, false);
        if let Some(target) = target {
            visit.travel(target, Phase::Entering);
        } else {
            visit.leave();
        }
        visit
    }

    fn cruise_height(layout: Layout) -> f32 {
        layout.canopy_y - layout.pitch * 1.4 - 4.0
    }

    fn choose(&mut self, candidates: &[Option<Perch>; 24], nearby: bool) -> Option<Perch> {
        let mut count = 0;
        let mut selected = None;
        for candidate in candidates.iter().flatten().copied() {
            if self.target.is_some_and(|old| old.key == candidate.key) {
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

    fn travel(&mut self, target: Perch, phase: Phase) {
        self.target = Some(target);
        self.from = self.position;
        self.to = target.feet;
        self.facing_right = self.to.x >= self.from.x;
        self.phase = phase;
        self.phase_tick = 0;
        self.duration = if phase == Phase::Hopping {
            28
        } else {
            FLIGHT_TICKS
        };
    }

    fn leave(&mut self) {
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
    }

    /// React to support changes even in a zero-dt control/read synchronization.
    /// Position and age remain frozen until an actual simulation tick.
    pub fn synchronize(&mut self, segments: &[SegmentState], event: Option<ClockEventKind>) {
        if self.phase == Phase::Leaving {
            return;
        }
        let candidates = perches(self.layout, segments, event);
        if self
            .target
            .is_some_and(|target| candidates.iter().flatten().any(|p| p.key == target.key))
        {
            return;
        }
        self.escapes += 1;
        if let Some(target) = self.choose(&candidates, false) {
            self.travel(target, Phase::Flying);
        } else {
            self.leave();
        }
    }

    pub fn step(&mut self, segments: &[SegmentState], event: Option<ClockEventKind>) -> bool {
        self.synchronize(segments, event);
        self.age += 1;
        if self.age >= CROW_TICKS {
            return true;
        }
        if self.age >= CROW_TICKS - EXIT_TICKS && self.phase != Phase::Leaving {
            self.leave();
        }
        self.phase_tick += 1;
        if self.phase == Phase::Perched {
            if self.phase_tick >= self.duration {
                let candidates = perches(self.layout, segments, event);
                if let Some(target) = self.choose(&candidates, true) {
                    self.hops += 1;
                    self.travel(target, Phase::Hopping);
                } else if let Some(target) = self.choose(&candidates, false) {
                    self.travel(target, Phase::Flying);
                } else {
                    self.leave();
                }
            }
            return false;
        }
        let t = (self.phase_tick as f32 / self.duration as f32).min(1.0);
        if self.phase == Phase::Hopping {
            self.position = self.from * (1.0 - t)
                + self.to * t
                + Vec2::new(0.0, self.layout.pitch * 0.9 * 4.0 * t * (1.0 - t));
        } else {
            // Lift, cross above the face, then descend onto exposed support.
            // Each eased leg is continuous and avoids sweeping through digits.
            let cruise = Self::cruise_height(self.layout)
                .max(self.from.y)
                .max(self.to.y);
            let (a, b, p) = if t < 0.25 {
                (self.from, Vec2::new(self.from.x, cruise), t * 4.0)
            } else if t < 0.8 {
                (
                    Vec2::new(self.from.x, cruise),
                    Vec2::new(self.to.x, cruise),
                    (t - 0.25) / 0.55,
                )
            } else {
                (Vec2::new(self.to.x, cruise), self.to, (t - 0.8) * 5.0)
            };
            let eased = p * p * (3.0 - 2.0 * p);
            self.position = a * (1.0 - eased) + b * eased;
        }
        if self.phase_tick >= self.duration {
            if self.phase == Phase::Leaving {
                return true;
            }
            self.position = self.to;
            self.phase = Phase::Perched;
            self.phase_tick = 0;
            self.duration = self.rng.random_range(50..=120);
        }
        false
    }

    pub fn diagnostics(&self) -> ClockCrowState {
        ClockCrowState {
            visit_id: self.visit_id,
            phase: self.phase,
            phase_tick: self.phase_tick,
            age_ticks: self.age,
            position_milli: [self.position.x, self.position.y].map(|v| (v * 1000.0).round() as i32),
            facing_right: self.facing_right,
            target: self.target.map(|p| p.key),
            hops: self.hops,
            escapes: self.escapes,
        }
    }
}
