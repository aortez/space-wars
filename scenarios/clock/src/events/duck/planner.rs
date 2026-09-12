//! Bounded, course-local planning. Geometry is shared with rendering/physics;
//! reachability uses measured movement capabilities, never a second simulator.

use engine_common::ClockDuckCoursePattern;
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::DT;

pub const MAX_SURFACES: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    pub start: f32,
    pub end: f32,
    /// Height above the event floor.
    pub height: f32,
}

impl Surface {
    pub fn inside(self, radius: f32) -> Option<(f32, f32)> {
        let margin = radius * 1.5;
        (self.end - self.start > margin * 2.0).then_some((self.start + margin, self.end - margin))
    }
}

#[derive(Debug, Clone)]
pub struct Course {
    pub surfaces: Vec<Surface>,
    pub pattern: ClockDuckCoursePattern,
    pub attempts: u32,
    pub fallback: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub height: f32,
    pub flight: f32,
    pub speed: f32,
    pub acceleration: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Narrow,
    High,
    Range,
    Obstructed,
}

#[derive(Debug, Clone, Copy)]
pub struct Plan {
    pub source: usize,
    pub target: usize,
    pub takeoff: Vec2,
    pub landing: Vec2,
    pub flight: f32,
    pub cruise: f32,
    pub running: bool,
    pub next_target: Option<usize>,
}

impl Plan {
    pub fn sample(self, capabilities: Capabilities, fraction: f32) -> Vec2 {
        let time = self.flight * fraction.clamp(0.0, 1.0);
        let gravity = 8.0 * capabilities.height / capabilities.flight.powi(2);
        let launch = 4.0 * capabilities.height / capabilities.flight;
        let direction = (self.landing.x - self.takeoff.x).signum();
        Vec2::new(
            self.takeoff.x
                + direction
                    * if self.running {
                        self.cruise * time
                    } else {
                        travel(
                            time,
                            self.flight,
                            self.cruise,
                            capabilities.acceleration * 0.85,
                        )
                    },
            self.takeoff.y + launch * time - 0.5 * gravity * time * time,
        )
    }
}

/// Distance travelled while accelerating from rest and braking back to rest.
fn travel(time: f32, duration: f32, cruise: f32, acceleration: f32) -> f32 {
    let ramp = cruise / acceleration;
    let t = time.clamp(0.0, duration);
    if t < ramp {
        0.5 * acceleration * t * t
    } else if t <= duration - ramp {
        cruise * (t - ramp * 0.5)
    } else {
        let remaining = duration - t;
        cruise * (duration - ramp) - 0.5 * acceleration * remaining * remaining
    }
}

pub fn plan(
    course: &Course,
    source: usize,
    target: usize,
    radius: f32,
    capabilities: Capabilities,
) -> Result<Plan, Rejection> {
    if source >= course.surfaces.len()
        || target >= course.surfaces.len()
        || source == target
        || ![
            radius,
            capabilities.height,
            capabilities.flight,
            capabilities.speed,
            capabilities.acceleration,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
    {
        return Err(Rejection::Range);
    }
    let from = course.surfaces[source];
    let to = course.surfaces[target];
    let direction = if target > source { 1.0 } else { -1.0 };
    let (from_left, from_right) = from.inside(radius).ok_or(Rejection::Narrow)?;
    let (left, right) = to.inside(radius).ok_or(Rejection::Narrow)?;
    let takeoff = Vec2::new(
        if direction > 0.0 {
            from_right
        } else {
            from_left
        },
        from.height,
    );
    let rise = to.height - from.height;
    if rise >= capabilities.height * 0.85 {
        return Err(Rejection::High);
    }
    // A parabola reconstructed from measured peak height and same-height
    // airtime. Higher/lower targets use the descending root of that parabola.
    let gravity = 8.0 * capabilities.height / capabilities.flight.powi(2);
    let launch = 4.0 * capabilities.height / capabilities.flight;
    let flight = (launch + (launch * launch - 2.0 * gravity * rise).sqrt()) / gravity;
    if !flight.is_finite() || flight > 2.0 {
        return Err(Rejection::Range);
    }
    let acceleration = capabilities.acceleration * 0.85;
    let near = if direction > 0.0 { left } else { right };
    let middle = (left + right) * 0.5;
    let mut rejection = Rejection::Range;
    for landing_x in [middle, (middle + near) * 0.5, near] {
        let distance = (landing_x - takeoff.x) * direction;
        let discriminant = flight * flight - 4.0 * distance / acceleration;
        if distance <= 0.0 || discriminant < 0.0 {
            continue;
        }
        let cruise = acceleration * 0.5 * (flight - discriminant.sqrt());
        if cruise > capabilities.speed * 0.9 {
            continue;
        }
        let result = Plan {
            source,
            target,
            takeoff,
            landing: Vec2::new(landing_x, to.height),
            flight,
            cruise,
            running: false,
            next_target: None,
        };
        // Conservative expanded body clearance along the arc, including any
        // intervening obstacles. Work is capped by the small course and flight.
        let steps = (flight / DT).ceil() as u32;
        let clear = (1..=steps).all(|step| {
            let time = (step as f32 * DT).min(flight);
            let x = takeoff.x + direction * travel(time, flight, cruise, acceleration);
            let feet = takeoff.y + launch * time - 0.5 * gravity * time * time;
            course.surfaces.iter().enumerate().all(|(index, surface)| {
                let overlaps = x + radius * 1.2 > surface.start && x - radius * 1.2 < surface.end;
                !overlaps
                    || feet >= surface.height + radius * 0.15
                    || (index == target
                        && x >= left - radius * 0.02
                        && x <= right + radius * 0.02
                        && feet >= surface.height - radius * 0.02)
            })
        });
        if clear {
            return Ok(result);
        }
        rejection = Rejection::Obstructed;
    }
    Err(rejection)
}

impl Course {
    #[cfg(test)]
    pub fn generated(width: f32, radius: f32, seed: u64) -> Self {
        Self::generate_with_budget(width, radius, seed, 16)
    }

    #[cfg(test)]
    pub fn generate_with_budget(width: f32, radius: f32, seed: u64, budget: u32) -> Self {
        Self::pattern_with_budget(
            width,
            radius,
            seed,
            ClockDuckCoursePattern::Platforms,
            budget,
        )
    }

    pub fn varied(
        width: f32,
        radius: f32,
        seed: u64,
        pattern: Option<ClockDuckCoursePattern>,
    ) -> Self {
        let mut selector = StdRng::seed_from_u64(seed ^ 0x434f_5552_5345_5459);
        let pattern = pattern.unwrap_or_else(|| {
            [
                ClockDuckCoursePattern::Platforms,
                ClockDuckCoursePattern::Terraces,
                ClockDuckCoursePattern::TwoJump,
                ClockDuckCoursePattern::Shortcut,
            ][selector.random_range(0..4)]
        });
        Self::pattern_with_budget(width, radius, seed, pattern, 16)
    }

    fn pattern_with_budget(
        width: f32,
        radius: f32,
        seed: u64,
        pattern: ClockDuckCoursePattern,
        budget: u32,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed ^ 0x504c_4154_464f_524d);
        // Generate against a smaller capability envelope than the tuned duck.
        // The runtime planner still has to measure its own actual capabilities.
        let conservative = Capabilities {
            height: radius * 4.0,
            flight: 0.80,
            speed: width / 5.0,
            acceleration: width / 5.0 * 6.0,
        };
        for attempt in 1..=budget.min(16) {
            let surfaces = if pattern == ClockDuckCoursePattern::Platforms {
                let count = rng.random_range(2..=3);
                let mut surfaces = Vec::with_capacity(MAX_SURFACES);
                surfaces.push(Surface {
                    start: -radius * 8.0,
                    end: width * 0.25,
                    height: 0.0,
                });
                let slot = width * 0.5 / count as f32;
                for i in 0..count {
                    let center = width * 0.25
                        + slot * (i as f32 + 0.5)
                        + rng.random_range(-0.04..0.04) * slot;
                    let half_width = slot * rng.random_range(0.29..0.37);
                    surfaces.push(Surface {
                        start: center - half_width,
                        end: center + half_width,
                        height: radius * rng.random_range(0.7..2.6),
                    });
                }
                surfaces.push(Surface {
                    start: width * 0.75,
                    end: width + radius * 8.0,
                    height: 0.0,
                });
                surfaces
            } else {
                let mut surfaces = Self::authored(width, radius, pattern).surfaces;
                // Keep recognizable routes; vary raised surfaces without changing
                // the authored runway/landing widths. Every result is revalidated.
                for surface in &mut surfaces {
                    surface.height *= rng.random_range(0.9..1.1);
                }
                surfaces
            };
            let course = Self {
                surfaces,
                pattern,
                attempts: attempt,
                fallback: false,
            };
            if course.valid_routes(radius, conservative) {
                return course;
            }
        }
        let mut course = Self::fixed(width, radius, 0);
        course.attempts = budget.min(16);
        course.fallback = true;
        course
    }

    pub fn authored(width: f32, radius: f32, pattern: ClockDuckCoursePattern) -> Self {
        let spans: &[(f32, f32, f32)] = match pattern {
            ClockDuckCoursePattern::TwoJump => {
                &[(0.0, 0.30, 0.0), (0.33, 0.42, 1.0), (0.45, 1.0, 0.0)]
            }
            ClockDuckCoursePattern::Shortcut => &[
                (0.0, 0.24, 0.0),
                (0.255, 0.315, 0.45),
                (0.33, 0.64, 0.0),
                (0.70, 1.0, 0.0),
            ],
            ClockDuckCoursePattern::Terraces => &[
                (0.0, 0.25, 0.0),
                (0.28, 0.42, 0.9),
                (0.45, 0.59, 2.0),
                (0.62, 0.76, 1.0),
                (0.79, 1.0, 0.0),
            ],
            ClockDuckCoursePattern::Platforms => return Self::fixed(width, radius, 0),
        };
        let mut surfaces: Vec<_> = spans
            .iter()
            .map(|&(start, end, height)| Surface {
                start: start * width,
                end: end * width,
                height: height * radius,
            })
            .collect();
        surfaces.first_mut().unwrap().start = -radius * 8.0;
        surfaces.last_mut().unwrap().end = width + radius * 8.0;
        Self {
            surfaces,
            pattern,
            attempts: 0,
            fallback: false,
        }
    }

    pub fn valid_routes(&self, radius: f32, capabilities: Capabilities) -> bool {
        if !(2..=MAX_SURFACES).contains(&self.surfaces.len())
            || self.surfaces.iter().any(|s| {
                !s.start.is_finite()
                    || !s.end.is_finite()
                    || !s.height.is_finite()
                    || s.height < 0.0
                    || s.start >= s.end
            })
            || self
                .surfaces
                .windows(2)
                .any(|pair| pair[0].end > pair[1].start)
        {
            return false;
        }
        (0..self.surfaces.len() - 1).all(|i| {
            plan(self, i, i + 1, radius, capabilities).is_ok()
                && plan(self, i + 1, i, radius, capabilities).is_ok()
        })
    }

    /// Fixed regression fixtures: single platform, ascending steps, and gap.
    pub fn fixed(width: f32, radius: f32, fixture: usize) -> Self {
        let surfaces = match fixture {
            0 => vec![
                Surface {
                    start: -radius * 8.0,
                    end: width * 0.35,
                    height: 0.0,
                },
                Surface {
                    start: width * 0.39,
                    end: width * 0.61,
                    height: radius * 1.3,
                },
                Surface {
                    start: width * 0.65,
                    end: width + radius * 8.0,
                    height: 0.0,
                },
            ],
            1 => vec![
                Surface {
                    start: -radius * 8.0,
                    end: width * 0.25,
                    height: 0.0,
                },
                Surface {
                    start: width * 0.28,
                    end: width * 0.42,
                    height: radius * 0.9,
                },
                Surface {
                    start: width * 0.45,
                    end: width * 0.59,
                    height: radius * 2.0,
                },
                Surface {
                    start: width * 0.62,
                    end: width * 0.76,
                    height: radius * 1.0,
                },
                Surface {
                    start: width * 0.79,
                    end: width + radius * 8.0,
                    height: 0.0,
                },
            ],
            _ => vec![
                Surface {
                    start: -radius * 8.0,
                    end: width * 0.46,
                    height: 0.0,
                },
                Surface {
                    start: width * 0.54,
                    end: width + radius * 8.0,
                    height: 0.0,
                },
            ],
        };
        Self {
            surfaces,
            pattern: ClockDuckCoursePattern::Platforms,
            attempts: 0,
            fallback: false,
        }
    }
}
