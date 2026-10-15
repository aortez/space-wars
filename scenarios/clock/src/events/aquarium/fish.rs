use super::*;
use navigation::{Navigation, Obstacle};

pub(crate) const MAX_FISH: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Species {
    Tetra,
    Danio,
    Cory,
}

impl Species {
    pub fn index(self) -> usize {
        match self {
            Self::Tetra => 0,
            Self::Danio => 1,
            Self::Cory => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Journey {
    Waiting,
    Entering,
    Swimming,
    Leaving,
    Exited,
    Failed,
}

pub(crate) struct Fish {
    pub species: Species,
    pub position: Vec2,
    pub velocity: Vec2,
    pub heading: f32,
    pub tail_phase: f32,
    pub(super) journey: Journey,
    frightened: u32,
    feeding: u32,
}

impl Fish {
    pub fn visible(&self) -> bool {
        matches!(
            self.journey,
            Journey::Entering | Journey::Swimming | Journey::Leaving
        )
    }
}

pub(super) struct School {
    pub fish: [Fish; MAX_FISH],
    pub nav: Navigation,
    rng: StdRng,
    goals: [usize; 7],
    fields: [Vec<u16>; 7],
    pub admitted: u32,
    pub exited: u32,
    pub failed: u32,
    pub scatters: u64,
    pub regroups: u64,
    pub corrections: u64,
    pub revisions: u64,
    last_admission: u64,
}

impl School {
    pub fn new(layout: Layout, seed: u64, radius: f32) -> Self {
        Self {
            fish: std::array::from_fn(|i| Fish {
                species: if i < 6 {
                    Species::Tetra
                } else if i < 10 {
                    Species::Danio
                } else {
                    Species::Cory
                },
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                heading: 0.0,
                tail_phase: i as f32 * 1.7,
                journey: Journey::Waiting,
                frightened: 0,
                feeding: 0,
            }),
            nav: Navigation::new(layout, radius),
            rng: StdRng::seed_from_u64(seed),
            goals: [0; 7],
            fields: std::array::from_fn(|_| vec![u16::MAX; navigation::CELLS]),
            admitted: 0,
            exited: 0,
            failed: 0,
            scatters: 0,
            regroups: 0,
            corrections: 0,
            revisions: 0,
            last_admission: 0,
        }
    }

    pub fn set_obstacles(&mut self, obstacles: Vec<Obstacle>, water: &WaterWorld, port: Vec2) {
        if self.nav.obstacles == obstacles {
            return;
        }
        self.nav.obstacles = obstacles;
        self.revisions += 1;
        self.refresh(water, port, true);
        // A changed glyph can occupy a fish or seal its pocket. Resolve to the
        // nearest reachable clearance point, retaining identity and counting it.
        for fish in &mut self.fish {
            if fish.journey == Journey::Swimming
                && (!self.nav.fits(fish.position, water)
                    || self.nav.exit[self.nav.index(fish.position)] == u16::MAX)
                && let Some(i) = self.nav.nearest_connected(fish.position)
            {
                fish.position = self.nav.position(i);
                fish.velocity = Vec2::ZERO;
                self.corrections += 1;
            }
        }
    }

    fn refresh(&mut self, water: &WaterWorld, port: Vec2, new_goals: bool) {
        self.nav.rebuild(water, port);
        for group in 0..7 {
            if new_goals || self.nav.exit[self.goals[group]] == u16::MAX {
                self.goals[group] = self.nav.target(&mut self.rng, group >= 5);
            }
            self.fields[group] = self.nav.field(self.goals[group]);
        }
    }

    pub fn step(
        &mut self,
        water: &WaterWorld,
        layout: Layout,
        port: Vec2,
        tick: u64,
        hull: Option<(Vec2, f64)>,
    ) {
        if tick.is_multiple_of(30) {
            self.refresh(water, port, false);
        }
        let radius = fish_radius(layout);
        let gathering = tick >= FILL_TICKS + SWIM_TICKS - 8 * 60;
        let departing = tick >= FILL_TICKS + SWIM_TICKS;
        let positions = self.fish.each_ref().map(|f| f.position);
        for i in 0..MAX_FISH {
            let group = if i < 6 { 0 } else { i - 5 };
            let fish = &mut self.fish[i];
            if fish.journey == Journey::Waiting {
                // The inlet is connected to offscreen water. Admit only once
                // its whole mouth and the first in-tank waypoint are wet.
                if tick < FILL_TICKS + SWIM_TICKS
                    && tick >= 4 * 60 + i as u64 * 5
                    && tick >= self.last_admission + 2
                    && self.nav.fits(port, water)
                    && self.nav.exit[self.nav.index(port)] != u16::MAX
                {
                    fish.journey = Journey::Entering;
                    fish.position = Vec2::new(layout.bounds_min.x - radius * 1.5, port.y);
                    fish.velocity = Vec2::new(layout.pitch * 6.0, 0.0);
                    self.admitted += 1;
                    self.last_admission = tick;
                } else {
                    continue;
                }
            }
            if !fish.visible() {
                continue;
            }
            if fish.journey == Journey::Entering {
                fish.position.x = (fish.position.x + layout.pitch * 6.0 * DT).min(port.x);
                if fish.position.x >= port.x {
                    fish.journey = Journey::Swimming;
                }
            } else if fish.journey == Journey::Leaving {
                fish.velocity = Vec2::new(-layout.pitch * 8.0, 0.0);
                fish.position.x += fish.velocity.x * DT;
                fish.position.y = port.y;
                if fish.position.x + radius < layout.bounds_min.x {
                    fish.journey = Journey::Exited;
                    self.exited += 1;
                }
            } else {
                if !gathering
                    && (fish.position - self.nav.position(self.goals[group])).length()
                        < layout.pitch * 0.7
                {
                    if fish.species == Species::Cory && fish.feeding == 0 {
                        fish.feeding = 120;
                    } else if fish.feeding <= 1 {
                        self.goals[group] = self.nav.target(&mut self.rng, group >= 5);
                        self.fields[group] = self.nav.field(self.goals[group]);
                    }
                }
                let field = if gathering {
                    &self.nav.exit
                } else {
                    &self.fields[group]
                };
                let staging = gathering
                    && !departing
                    && fish.position.y > layout.face_origin.y + layout.pitch * 9.0 + radius * 1.1;
                let target = if staging {
                    // Separate arrival positions in the unobstructed upper
                    // lane keep early arrivals visible while the slower fish
                    // gather. They converge on the actual passage only at exit.
                    port + Vec2::new(
                        radius * 2.6 * i as f32,
                        radius * 0.3 * (i as f32 * 2.1).sin(),
                    )
                } else if gathering && (fish.position - port).length() < layout.pitch * 1.5 {
                    port
                } else {
                    self.nav.waypoint(fish.position, field)
                };
                let mut steering = (target - fish.position).normalized();
                let mut speed = layout.pitch
                    * match fish.species {
                        Species::Tetra => 2.5,
                        Species::Danio => 4.0,
                        Species::Cory => 1.4,
                    };
                if gathering {
                    speed = layout.pitch.max(18.0) * 8.0;
                    if staging {
                        speed = speed.min((target - fish.position).length() * 4.0);
                    }
                } else if fish.feeding > 0 {
                    fish.feeding -= 1;
                    speed *= 0.1;
                } else if fish.species == Species::Danio && (tick + i as u64 * 37) % 180 < 40 {
                    speed *= 1.6;
                }
                if !gathering {
                    // Separation is local and bounded; only the six tetras
                    // share a destination, retaining a loose school.
                    for (j, other) in positions.iter().enumerate() {
                        let delta = fish.position - *other;
                        let distance = delta.length();
                        if i != j && distance > 0.01 && distance < radius * 2.8 {
                            steering += delta / distance * (1.0 - distance / (radius * 2.8)) * 0.8;
                        }
                    }
                }
                let threatened = hull.is_some_and(|(p, r)| {
                    (fish.position - p).length() < r as f32 + layout.pitch * 2.0
                });
                if threatened {
                    if fish.frightened == 0 {
                        self.scatters += 1;
                    }
                    fish.frightened = 75;
                    let (center, hull_radius) = hull.unwrap();
                    let delta = fish.position - center;
                    // Fade the response at the sensor boundary. A constant
                    // full-strength shove there pins fish below a floating
                    // duck, even when the route around it has ample clearance.
                    let alert = hull_radius as f32 + layout.pitch * 2.0;
                    let threat = ((alert - delta.length()) / (layout.pitch * 2.0)).clamp(0.0, 1.0);
                    steering += delta.normalized() * (threat * 1.8);
                    speed *= 1.0 + threat * 0.25;
                } else if fish.frightened > 0 {
                    fish.frightened -= 1;
                    if fish.frightened == 0 {
                        self.regroups += 1;
                    }
                }
                let current = water
                    .sample(fish.position)
                    .map_or(Vec2::ZERO, |s| s.velocity);
                let wanted = steering.normalized() * speed
                    + current.normalized() * current.length().min(layout.pitch) * 0.25;
                fish.velocity += (wanted - fish.velocity) * if gathering { 0.28 } else { 0.12 };
                // Bounded swept movement prevents fast danios from tunnelling
                // through a digit. Sliding along a face preserves momentum.
                for _ in 0..4 {
                    let delta = fish.velocity * (DT / 4.0);
                    let next = fish.position + delta;
                    if self.nav.fits(next, water) {
                        fish.position = next;
                    } else if self
                        .nav
                        .fits(fish.position + Vec2::new(delta.x, 0.0), water)
                    {
                        fish.position.x += delta.x;
                        fish.velocity.y *= 0.6;
                    } else if self
                        .nav
                        .fits(fish.position + Vec2::new(0.0, delta.y), water)
                    {
                        fish.position.y += delta.y;
                        fish.velocity.x *= 0.6;
                    } else {
                        fish.velocity *= 0.5;
                    }
                }
                if departing && (fish.position - port).length() < radius * 0.6 {
                    fish.journey = Journey::Leaving;
                }
            }
            if fish.velocity.length() > 1.0 {
                let desired = fish.velocity.y.atan2(fish.velocity.x);
                let difference = (desired - fish.heading + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                fish.heading += difference * 0.18;
            }
            fish.tail_phase += 0.13 + fish.velocity.length() / layout.pitch * 0.045;
        }
    }

    pub fn finish_departure(&mut self) {
        for fish in &mut self.fish {
            if fish.journey != Journey::Exited && fish.journey != Journey::Failed {
                fish.journey = Journey::Failed;
                self.failed += 1;
            }
        }
    }
}
