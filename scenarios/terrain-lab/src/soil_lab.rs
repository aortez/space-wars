//! Headless material experiments. The same inputs drive MPM, a zero-internal-
//! friction control, and the earlier Rapier grain model. No game integration.

use engine_core::Vec2;
use engine_soil::{Config, Environment, Material, Seed, Soil, SoilError, StepStats, Surface};
use engine_terrain::MaterialId;

mod grains;
#[cfg(test)]
mod tests;
use grains::Grains;

pub const DT: f64 = 1.0 / 60.0;
pub const SPACING: f32 = 0.125;
const RADIUS: f32 = 8.0;
const GRAVITY: f32 = 18.0;
pub const SUPPORT_TICK: u64 = 120;
pub const BLAST_TICKS: [u64; 2] = [120, 300];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    Flat,
    MovingPlanet,
}
impl Fixture {
    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::MovingPlanet => "moving-planet",
        }
    }

    pub fn pose(self, seconds: f64) -> (Vec2, f32) {
        match self {
            Self::Flat => (Vec2::ZERO, 0.0),
            Self::MovingPlanet => (
                Vec2::new(0.3 * (seconds as f32 * 0.2).sin(), -RADIUS),
                seconds as f32 * 0.04,
            ),
        }
    }

    pub fn surface_velocity(self, position: Vec2, seconds: f64) -> Vec2 {
        match self {
            Self::Flat => Vec2::ZERO,
            Self::MovingPlanet => {
                let r = position - self.pose(seconds).0;
                Vec2::new(0.06 * (seconds as f32 * 0.2).cos(), 0.0) + Vec2::new(-r.y, r.x) * 0.04
            }
        }
    }

    /// Arc length at this height, and height above the prescribed base. This
    /// map has unit area Jacobian, so the curved seed keeps its reference mass
    /// and initial density rather than stretching more soil into existence.
    pub fn to_world(self, local: Vec2, seconds: f64) -> Vec2 {
        let (center, angle) = self.pose(seconds);
        match self {
            Self::Flat => local,
            Self::MovingPlanet => {
                let theta = local.x / (RADIUS + local.y);
                center
                    + (Vec2::new(theta.sin(), theta.cos()) * (RADIUS + local.y))
                        .rotate_radians(angle)
            }
        }
    }

    pub fn to_local(self, position: Vec2, seconds: f64) -> Vec2 {
        let (center, angle) = self.pose(seconds);
        match self {
            Self::Flat => position,
            Self::MovingPlanet => {
                let r = (position - center).rotate_radians(-angle);
                Vec2::new(r.x.atan2(r.y) * r.length(), r.length() - RADIUS)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experiment {
    Pour,
    Bank,
    Blasts,
}
impl Experiment {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pour => "pour",
            Self::Bank => "bank",
            Self::Blasts => "blasts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Mpm,
    Frictionless,
    Grains,
}
impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Self::Mpm => "mpm",
            Self::Frictionless => "frictionless",
            Self::Grains => "grains",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LabConfig {
    pub fixture: Fixture,
    pub experiment: Experiment,
    pub model: Model,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Ground {
    pub fixture: Fixture,
    pub experiment: Experiment,
    pub support_removed: bool,
}

impl Ground {
    /// Convex support in the ground's rigid frame. The curved fixture uses a
    /// polygonal cap so both contact backends see the identical shape.
    pub fn platform(self) -> Vec<Vec2> {
        if self.experiment != Experiment::Bank {
            return Vec::new();
        }
        let left = if self.support_removed { 0.0 } else { -4.0 };
        match self.fixture {
            Fixture::Flat => vec![
                Vec2::new(left, 0.0),
                Vec2::new(4.0, 0.0),
                Vec2::new(4.0, 2.0),
                Vec2::new(left, 2.0),
            ],
            Fixture::MovingPlanet => {
                let mut points = vec![Vec2::new(0.0, 0.0)];
                // Counterclockwise: bottom pivot, right end, then top arc.
                for i in 0..=32 {
                    let x = 4.0 + (left - 4.0) * i as f32 / 32.0;
                    let theta = x / RADIUS;
                    points.push(Vec2::new(theta.sin(), theta.cos()) * (RADIUS + 2.0));
                }
                points
            }
        }
    }

    pub fn blast_center(self, seconds: f64) -> Vec2 {
        self.fixture.to_world(Vec2::new(0.0, 0.8), seconds)
    }

    fn prepared(self) -> PreparedGround {
        let polygon = self.platform();
        let planes = (0..polygon.len())
            .map(|i| {
                let a = polygon[i];
                let edge = polygon[(i + 1) % polygon.len()] - a;
                let normal = Vec2::new(edge.y, -edge.x).normalized();
                (normal, a.dot(normal))
            })
            .collect();
        PreparedGround {
            ground: self,
            planes,
        }
    }
}

struct PreparedGround {
    ground: Ground,
    planes: Vec<(Vec2, f32)>,
}
impl PreparedGround {
    fn platform_surface(&self, p: Vec2, seconds: f64) -> Option<Surface> {
        if self.planes.is_empty() {
            return None;
        }
        let (center, angle) = self.ground.fixture.pose(seconds);
        let local = (p - center).rotate_radians(-angle);
        let mut distance = f32::NEG_INFINITY;
        let mut normal = Vec2::Y;
        for &(n, offset) in &self.planes {
            let d = local.dot(n) - offset;
            if d > distance {
                distance = d;
                normal = n;
            }
        }
        Some(Surface {
            distance,
            normal: normal.rotate_radians(angle),
            velocity: self.ground.fixture.surface_velocity(p, seconds),
            friction: 0.6,
        })
    }
}

impl Environment for Ground {
    fn gravity(&self, p: Vec2, seconds: f64) -> Vec2 {
        match self.fixture {
            Fixture::Flat => Vec2::new(0.0, -GRAVITY),
            Fixture::MovingPlanet => {
                // One prescribed uniform spherical source: inverse square
                // outside, linear inside, matching engine-gravity's field.
                let r = self.fixture.pose(seconds).0 - p;
                r * (GRAVITY * RADIUS.powi(2) / r.length().max(RADIUS).powi(3))
            }
        }
    }

    fn surface(&self, p: Vec2, seconds: f64) -> Option<Surface> {
        self.prepared().surface(p, seconds)
    }
}

impl Environment for PreparedGround {
    fn gravity(&self, p: Vec2, seconds: f64) -> Vec2 {
        self.ground.gravity(p, seconds)
    }

    fn surface(&self, p: Vec2, seconds: f64) -> Option<Surface> {
        let (distance, normal) = match self.ground.fixture {
            Fixture::Flat => (p.y, Vec2::Y),
            Fixture::MovingPlanet => {
                let r = p - self.ground.fixture.pose(seconds).0;
                (
                    r.length() - RADIUS,
                    if r.length_squared() > 0.0 {
                        r.normalized()
                    } else {
                        Vec2::Y
                    },
                )
            }
        };
        let base = Surface {
            distance,
            normal,
            velocity: self.ground.fixture.surface_velocity(p, seconds),
            friction: 0.6,
        };
        Some(
            self.platform_surface(p, seconds)
                .filter(|s| s.distance < base.distance)
                .unwrap_or(base),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub position: Vec2,
    pub velocity: Vec2,
    pub mass: f32,
    pub material: MaterialId,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Metrics {
    pub particles: usize,
    pub mass: [f64; 2],
    pub reservoir_mass: [f64; 2],
    pub rms_speed: f32,
    pub height: f32,
    pub width: f32,
    pub below_platform_mass: f32,
    /// 90th percentile height in |u|<0.5, zero if no material is there.
    pub center_height: f32,
    pub kinetic_energy: f64,
}

#[derive(Clone)]
enum Solver {
    Mpm(Soil),
    Grains(Box<Grains>),
}

#[derive(Clone)]
pub struct SoilLab {
    pub config: LabConfig,
    pub tick: u64,
    pub ground: Ground,
    pub last_step: StepStats,
    pub last_blast_hits: usize,
    initial: [f64; 2],
    reservoir: Vec<(u64, Vec2, MaterialId)>,
    emitted: usize,
    solver: Solver,
}

impl SoilLab {
    pub fn new(config: LabConfig) -> Result<Self, SoilError> {
        let ground = Ground {
            fixture: config.fixture,
            experiment: config.experiment,
            support_removed: false,
        };
        let material = Material {
            friction_angle_degrees: if config.model == Model::Frictionless {
                0.0
            } else {
                35.0
            },
            ..Material::default()
        };
        let solver = match config.model {
            Model::Mpm | Model::Frictionless => Solver::Mpm(Soil::new(Config {
                origin: Vec2::new(-32.0, -24.0),
                nodes: [257, 193],
                material,
                ..Config::default()
            })?),
            Model::Grains => Solver::Grains(Box::new(Grains::new(ground))),
        };
        let mut reservoir = Vec::new();
        let mut add = |tick, x: f32, y: f32, row: usize| {
            let id = reservoir.len() as u64;
            let hash = (id ^ config.seed).wrapping_mul(0x9e3779b97f4a7c15);
            let jitter = ((hash >> 40) as f32 / (1_u32 << 24) as f32 - 0.5) * SPACING * 0.02;
            reservoir.push((
                tick,
                Vec2::new(x + jitter, y),
                MaterialId(1 + (row / 4 % 2) as u8),
            ));
        };
        match config.experiment {
            Experiment::Pour => {
                for row in 0..24 {
                    for x in 0..12 {
                        add(row as u64 * 6, (x as f32 - 5.5) * SPACING, 4.5, row);
                    }
                }
            }
            Experiment::Bank => {
                for y in 0..12 {
                    for x in 0..60 {
                        let x = (x as f32 - 29.5) * SPACING;
                        let height = (1.5 - (x.abs() - 1.5).max(0.0) * 0.7).max(0.0);
                        if (y as f32 + 0.5) * SPACING <= height {
                            add(0, x, 2.0 + (y as f32 + 0.5) * SPACING, y);
                        }
                    }
                }
            }
            Experiment::Blasts => {
                for y in 0..16 {
                    for x in 0..64 {
                        add(
                            0,
                            (x as f32 - 31.5) * SPACING,
                            (y as f32 + 0.5) * SPACING,
                            y,
                        );
                    }
                }
            }
        }
        let mut initial = [0.0; 2];
        for (_, _, material) in &reservoir {
            initial[(material.0 - 1) as usize] += SPACING.powi(2) as f64;
        }
        let mut lab = Self {
            config,
            tick: 0,
            ground,
            last_step: StepStats::default(),
            last_blast_hits: 0,
            initial,
            reservoir,
            emitted: 0,
            solver,
        };
        lab.emit()?;
        Ok(lab)
    }

    fn emit(&mut self) -> Result<(), SoilError> {
        let seconds = self.tick as f64 * DT;
        let mut seeds = Vec::new();
        let mut end = self.emitted;
        while end < self.reservoir.len() && self.reservoir[end].0 <= self.tick {
            let (_, local, material) = self.reservoir[end];
            let position = self.config.fixture.to_world(local, seconds);
            let down = self.ground.gravity(position, seconds).normalized();
            seeds.push(Seed {
                position,
                velocity: self.config.fixture.surface_velocity(position, seconds)
                    + down
                        * if self.config.experiment == Experiment::Pour {
                            1.0
                        } else {
                            0.0
                        },
                reference_area: SPACING.powi(2),
                material,
            });
            end += 1;
        }
        match &mut self.solver {
            Solver::Mpm(soil) => soil.insert(&seeds)?,
            Solver::Grains(grains) => grains.insert(&seeds),
        }
        self.emitted = end;
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), SoilError> {
        self.emit()?;
        self.last_blast_hits = 0;
        if self.config.experiment == Experiment::Bank && self.tick == SUPPORT_TICK {
            self.ground.support_removed = true;
            if let Solver::Grains(grains) = &mut self.solver {
                grains.replace_support(self.ground, self.tick as f64 * DT);
            }
        }
        if self.config.experiment == Experiment::Blasts && BLAST_TICKS.contains(&self.tick) {
            let center = self.ground.blast_center(self.tick as f64 * DT);
            self.last_blast_hits = match &mut self.solver {
                Solver::Mpm(soil) => soil.blast(center, 1.5, 12.0)?,
                Solver::Grains(grains) => grains.blast(center, 1.5, 12.0),
            };
        }
        self.last_step = match &mut self.solver {
            Solver::Mpm(soil) => soil.advance(DT, &self.ground.prepared())?,
            Solver::Grains(grains) => {
                grains.step(self.ground, self.tick as f64 * DT);
                StepStats::default()
            }
        };
        self.tick += 1;
        Ok(())
    }

    pub fn points(&self) -> Vec<Point> {
        match &self.solver {
            Solver::Mpm(soil) => soil
                .particles()
                .iter()
                .map(|p| Point {
                    position: p.position(),
                    velocity: p.velocity(),
                    mass: p.mass(),
                    material: p.material(),
                })
                .collect(),
            Solver::Grains(grains) => grains.points(),
        }
    }

    pub fn audit(&self) -> Result<Metrics, SoilError> {
        let mut metrics = Metrics::default();
        let points = self.points();
        if points.len() != self.emitted {
            return Err(SoilError::NumericalFailure);
        }
        let seconds = self.tick as f64 * DT;
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut center = Vec::new();
        for p in &points {
            if !p.position.x.is_finite()
                || !p.position.y.is_finite()
                || !p.velocity.length_squared().is_finite()
            {
                return Err(SoilError::NumericalFailure);
            }
            let local = self.config.fixture.to_local(p.position, seconds);
            let relative = p.velocity - self.config.fixture.surface_velocity(p.position, seconds);
            metrics.mass[(p.material.0 - 1) as usize] += p.mass as f64;
            metrics.rms_speed += relative.length_squared();
            metrics.kinetic_energy += 0.5 * p.mass as f64 * relative.length_squared() as f64;
            metrics.height = metrics.height.max(local.y);
            if local.y < 1.5 {
                metrics.below_platform_mass += p.mass;
            }
            min_x = min_x.min(local.x);
            max_x = max_x.max(local.x);
            if local.x.abs() < 0.5 {
                center.push(local.y);
            }
        }
        for (_, _, material) in &self.reservoir[self.emitted..] {
            metrics.reservoir_mass[(material.0 - 1) as usize] += SPACING.powi(2) as f64;
        }
        for i in 0..2 {
            if (metrics.mass[i] + metrics.reservoir_mass[i] - self.initial[i]).abs() > 1.0e-8 {
                return Err(SoilError::NumericalFailure);
            }
        }
        if let Solver::Mpm(soil) = &self.solver
            && soil.particles().iter().any(|p| {
                !p.elastic_area_ratio().is_finite()
                    || p.elastic_area_ratio() <= 0.0
                    || !p.log_dilation().is_finite()
                    || p.log_dilation() < 0.0
            })
        {
            return Err(SoilError::NumericalFailure);
        }
        metrics.particles = points.len();
        metrics.rms_speed = (metrics.rms_speed / points.len().max(1) as f32).sqrt();
        metrics.width = if points.is_empty() {
            0.0
        } else {
            max_x - min_x
        };
        center.sort_by(f32::total_cmp);
        metrics.center_height = center.get(center.len() * 9 / 10).copied().unwrap_or(0.0);
        Ok(metrics)
    }

    pub fn state_hash(&self) -> u64 {
        let solver = match &self.solver {
            Solver::Mpm(soil) => soil.state_hash(),
            Solver::Grains(grains) => grains.state_hash(),
        };
        solver
            ^ self.tick.rotate_left(17)
            ^ (self.emitted as u64).rotate_left(31)
            ^ u64::from(self.ground.support_removed)
    }
}
