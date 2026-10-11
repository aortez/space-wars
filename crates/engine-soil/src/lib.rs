//! Experimental 2D MLS-MPM soil. Material lives on particles; a temporary
//! quadratic grid transfers momentum and stress. No renderer, game clock,
//! gravity policy, terrain editing, or rigid-body engine is owned here.

use engine_core::Vec2;
use engine_terrain::MaterialId;

mod material;
mod math;
pub use material::Material;
use math::Mat;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilError {
    InvalidInput,
    Capacity,
    OutsideGrid,
    SubstepLimit,
    NumericalFailure,
}

impl std::fmt::Display for SoilError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid soil input or environment sample",
            Self::Capacity => "soil particle/grid capacity exceeded",
            Self::OutsideGrid => "soil particle stencil left the grid",
            Self::SubstepLimit => "soil stability substep budget exhausted",
            Self::NumericalFailure => "non-finite or inverted soil state",
        })
    }
}
impl std::error::Error for SoilError {}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub origin: Vec2,
    pub nodes: [usize; 2],
    pub cell_size: f32,
    pub max_particles: usize,
    pub max_substeps: usize,
    pub material: Material,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            origin: Vec2::new(-16.0, -18.0),
            nodes: [129, 145],
            cell_size: 0.25,
            max_particles: 8192,
            max_substeps: 128,
            material: Material::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Positive area at rest; mass = reference_area * configured density.
    pub reference_area: f32,
    /// Provenance/color only in this prototype; all IDs share one material law.
    pub material: MaterialId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    position: Vec2,
    velocity: Vec2,
    reference_area: f32,
    mass: f32,
    material: MaterialId,
    elastic: Mat,
    affine: Mat,
    log_dilation: f32,
}

impl Particle {
    pub fn position(&self) -> Vec2 {
        self.position
    }
    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }
    pub fn mass(&self) -> f32 {
        self.mass
    }
    pub fn reference_area(&self) -> f32 {
        self.reference_area
    }
    pub fn material(&self) -> MaterialId {
        self.material
    }
    pub fn elastic_area_ratio(&self) -> f32 {
        self.elastic.determinant()
    }
    pub fn log_dilation(&self) -> f32 {
        self.log_dilation
    }
}

/// Signed distance is positive outside the prescribed solid; normal is an
/// outward unit vector. Contact is separating Coulomb friction in its moving
/// frame. Reaction forces are not yet returned to a rigid-body simulation.
#[derive(Debug, Clone, Copy)]
pub struct Surface {
    pub distance: f32,
    pub normal: Vec2,
    pub velocity: Vec2,
    pub friction: f32,
}

pub trait Environment {
    fn gravity(&self, position: Vec2, seconds: f64) -> Vec2;
    fn surface(&self, position: Vec2, seconds: f64) -> Option<Surface>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepStats {
    pub substeps: usize,
    pub peak_active_nodes: usize,
    /// Number of particle/substep yield events (not distinct particles).
    pub yield_events: usize,
    /// Safety projections after advection, separate from normal grid contact.
    pub contact_corrections: usize,
    pub max_correction: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Node {
    mass: f32,
    velocity: Vec2,
}

#[derive(Clone)]
pub struct Soil {
    config: Config,
    particles: Vec<Particle>,
    seconds: f64,
    grid: Vec<Node>,
    active: Vec<usize>,
}

fn finite(v: Vec2) -> bool {
    v.x.is_finite() && v.y.is_finite()
}

impl Soil {
    pub fn new(config: Config) -> Result<Self, SoilError> {
        config.material.validate()?;
        if !finite(config.origin)
            || !config.cell_size.is_finite()
            || !(0.01..=10.0).contains(&config.cell_size)
            || config.nodes.iter().any(|n| !(4..=2048).contains(n))
            || config.max_substeps == 0
            || config.max_substeps > 1024
            || config.max_particles == 0
            || config.max_particles > 65536
        {
            return Err(SoilError::InvalidInput);
        }
        let size = config.nodes[0] * config.nodes[1];
        if size > 262144 {
            return Err(SoilError::Capacity);
        }
        Ok(Self {
            config,
            particles: Vec::new(),
            seconds: 0.0,
            grid: vec![Node::default(); size],
            active: Vec::new(),
        })
    }

    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }
    pub fn seconds(&self) -> f64 {
        self.seconds
    }
    pub fn config(&self) -> Config {
        self.config
    }

    /// Entire batch is validated before ownership is transferred.
    pub fn insert(&mut self, seeds: &[Seed]) -> Result<(), SoilError> {
        if seeds.len() > self.config.max_particles - self.particles.len() {
            return Err(SoilError::Capacity);
        }
        for seed in seeds {
            if !finite(seed.position)
                || !finite(seed.velocity)
                || !seed.reference_area.is_finite()
                || !(1.0e-8..=100.0).contains(&seed.reference_area)
                || seed.material == MaterialId::VOID
            {
                return Err(SoilError::InvalidInput);
            }
            stencil(self.config, seed.position)?;
        }
        self.particles.extend(seeds.iter().map(|seed| Particle {
            position: seed.position,
            velocity: seed.velocity,
            reference_area: seed.reference_area,
            mass: seed.reference_area * self.config.material.density,
            material: seed.material,
            elastic: Mat::IDENTITY,
            affine: Mat::default(),
            log_dilation: 0.0,
        }));
        Ok(())
    }

    /// Radial velocity impulse with linear falloff. No cutout, deletion, or
    /// prescribed fracture. The direction at the exact center is zero.
    pub fn blast(&mut self, center: Vec2, radius: f32, speed: f32) -> Result<usize, SoilError> {
        if !finite(center)
            || !radius.is_finite()
            || !(0.01..=100.0).contains(&radius)
            || !speed.is_finite()
            || !(0.0..=100.0).contains(&speed)
        {
            return Err(SoilError::InvalidInput);
        }
        let mut hit = 0;
        for p in &mut self.particles {
            let offset = p.position - center;
            let distance = offset.length();
            if distance > 0.0 && distance < radius {
                p.velocity += offset * (speed * (1.0 - distance / radius) / distance);
                hit += 1;
            }
        }
        Ok(hit)
    }

    /// On any failure the entire requested frame is rolled back, including
    /// time and affine/elastic state. Grid scratch is rebuilt on the next call.
    pub fn advance(
        &mut self,
        seconds: f64,
        environment: &impl Environment,
    ) -> Result<StepStats, SoilError> {
        if !seconds.is_finite() || !(1.0e-6..=1.0 / 30.0).contains(&seconds) {
            return Err(SoilError::InvalidInput);
        }
        let original = self.particles.clone();
        let start = self.seconds;
        let result = self.advance_inner(seconds, environment);
        if result.is_err() {
            self.particles = original;
            self.seconds = start;
        }
        result
    }

    fn advance_inner(
        &mut self,
        seconds: f64,
        environment: &impl Environment,
    ) -> Result<StepStats, SoilError> {
        let end = self.seconds + seconds;
        let mut stats = StepStats::default();
        while end - self.seconds > 1.0e-9 {
            if stats.substeps >= self.config.max_substeps {
                return Err(SoilError::SubstepLimit);
            }
            let transport = self
                .particles
                .iter()
                .map(|p| p.velocity.length() + self.config.cell_size * p.affine.norm())
                .fold(0.0_f32, f32::max);
            let stable =
                0.35 * self.config.cell_size / (self.config.material.wave_speed() + transport);
            let dt = ((end - self.seconds) as f32).min(stable);
            if !dt.is_finite() || dt < 1.0e-9 {
                return Err(SoilError::NumericalFailure);
            }
            self.substep(dt, environment, &mut stats)?;
            self.seconds += dt as f64;
            stats.substeps += 1;
        }
        self.seconds = end;
        Ok(stats)
    }

    fn substep(
        &mut self,
        dt: f32,
        environment: &impl Environment,
        stats: &mut StepStats,
    ) -> Result<(), SoilError> {
        for index in self.active.drain(..) {
            self.grid[index] = Node::default();
        }
        let inverse_moment = 4.0 / self.config.cell_size.powi(2);
        for p in &mut self.particles {
            let trial = Mat::IDENTITY.add(p.affine.scale(dt)).mul(p.elastic);
            let (elastic, stress, yielded) =
                self.config.material.project(trial, &mut p.log_dilation)?;
            p.elastic = elastic;
            stats.yield_events += usize::from(yielded);
            let momentum_affine = p
                .affine
                .scale(p.mass)
                .add(stress.scale(-dt * p.reference_area * inverse_moment));
            for (index, weight, offset) in stencil(self.config, p.position)? {
                if weight == 0.0 {
                    continue;
                }
                let node = &mut self.grid[index];
                if node.mass == 0.0 {
                    self.active.push(index);
                }
                node.mass += weight * p.mass;
                node.velocity += (p.velocity * p.mass + momentum_affine.vector(offset)) * weight;
            }
        }
        stats.peak_active_nodes = stats.peak_active_nodes.max(self.active.len());
        for &index in &self.active {
            let position = self.config.origin
                + Vec2::new(
                    (index % self.config.nodes[0]) as f32,
                    (index / self.config.nodes[0]) as f32,
                ) * self.config.cell_size;
            let gravity = environment.gravity(position, self.seconds);
            if !finite(gravity) {
                return Err(SoilError::InvalidInput);
            }
            let node = &mut self.grid[index];
            node.velocity = node.velocity / node.mass + gravity * dt;
            if let Some(surface) = environment.surface(position, self.seconds) {
                validate_surface(surface)?;
                // Only intervene for penetrating or crossing grid nodes.
                let closing = (node.velocity - surface.velocity).dot(surface.normal);
                if surface.distance <= 0.0 || surface.distance + closing * dt < 0.0 {
                    node.velocity = contact(node.velocity, surface, surface.distance.max(0.0) / dt);
                }
            }
            if !finite(node.velocity) {
                return Err(SoilError::NumericalFailure);
            }
        }
        for p in &mut self.particles {
            let mut velocity = Vec2::ZERO;
            let mut affine = Mat::default();
            for (index, weight, offset) in stencil(self.config, p.position)? {
                let node_velocity = self.grid[index].velocity;
                velocity += node_velocity * weight;
                affine =
                    affine.add(Mat::outer(node_velocity, offset).scale(weight * inverse_moment));
            }
            p.velocity = velocity;
            p.affine = affine;
            p.position += velocity * dt;
            if let Some(surface) = environment.surface(p.position, self.seconds + dt as f64) {
                validate_surface(surface)?;
                if surface.distance < 0.0 {
                    p.position -= surface.normal * surface.distance;
                    p.velocity = contact(p.velocity, surface, 0.0);
                    stats.contact_corrections += 1;
                    stats.max_correction = stats.max_correction.max(-surface.distance);
                }
            }
            if !finite(p.position) || !finite(p.velocity) || !p.affine.finite() {
                return Err(SoilError::NumericalFailure);
            }
            stencil(self.config, p.position)?;
        }
        Ok(())
    }

    /// All authoritative state, including hidden affine and elastic matrices.
    /// Replay guarantee is same build/architecture, not cross-platform bits.
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        let mut word = |v: u64| {
            for b in v.to_le_bytes() {
                hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
            }
        };
        for v in [
            self.config.origin.x,
            self.config.origin.y,
            self.config.cell_size,
            self.config.material.density,
            self.config.material.young_modulus,
            self.config.material.poisson_ratio,
            self.config.material.friction_angle_degrees,
        ] {
            word(v.to_bits() as u64);
        }
        for v in [
            self.config.nodes[0],
            self.config.nodes[1],
            self.config.max_particles,
            self.config.max_substeps,
        ] {
            word(v as u64);
        }
        word(self.seconds.to_bits());
        word(self.particles.len() as u64);
        for p in &self.particles {
            word(p.material.0 as u64);
            for v in [
                p.position.x,
                p.position.y,
                p.velocity.x,
                p.velocity.y,
                p.mass,
                p.reference_area,
                p.log_dilation,
            ]
            .into_iter()
            .chain(p.elastic.0)
            .chain(p.affine.0)
            {
                word(v.to_bits() as u64);
            }
        }
        hash
    }
}

fn validate_surface(s: Surface) -> Result<(), SoilError> {
    if !s.distance.is_finite()
        || !finite(s.normal)
        || (s.normal.length_squared() - 1.0).abs() > 0.001
        || !finite(s.velocity)
        || !s.friction.is_finite()
        || !(0.0..=2.0).contains(&s.friction)
    {
        return Err(SoilError::InvalidInput);
    }
    Ok(())
}

fn contact(velocity: Vec2, s: Surface, closing_allowance: f32) -> Vec2 {
    let relative = velocity - s.velocity;
    let normal_speed = relative.dot(s.normal);
    let change = (-normal_speed - closing_allowance).max(0.0);
    let tangent = relative - s.normal * normal_speed;
    let tangent_speed = tangent.length();
    let scale = if tangent_speed > 0.0 {
        (1.0 - s.friction * change / tangent_speed).max(0.0)
    } else {
        0.0
    };
    s.velocity + s.normal * (normal_speed + change) + tangent * scale
}

fn stencil(config: Config, position: Vec2) -> Result<[(usize, f32, Vec2); 9], SoilError> {
    let coordinate = (position - config.origin) / config.cell_size;
    let base = Vec2::new((coordinate.x - 0.5).floor(), (coordinate.y - 0.5).floor());
    if !finite(coordinate)
        || base.x < 0.0
        || base.y < 0.0
        || base.x + 2.0 >= config.nodes[0] as f32
        || base.y + 2.0 >= config.nodes[1] as f32
    {
        return Err(SoilError::OutsideGrid);
    }
    let f = coordinate - base;
    let weights = |x: f32| {
        [
            0.5 * (1.5 - x).powi(2),
            0.75 - (x - 1.0).powi(2),
            0.5 * (x - 0.5).powi(2),
        ]
    };
    let wx = weights(f.x);
    let wy = weights(f.y);
    Ok(std::array::from_fn(|i| {
        let x = i % 3;
        let y = i / 3;
        (
            (base.y as usize + y) * config.nodes[0] + base.x as usize + x,
            wx[x] * wy[y],
            (Vec2::new(x as f32, y as f32) - f) * config.cell_size,
        )
    }))
}
