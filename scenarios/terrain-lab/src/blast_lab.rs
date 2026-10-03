//! Controlled comparison of crater removal and conserved, coarse rigid pieces.
//! This is an experimental fixture for #165, not a granular-soil model.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_gravity::{GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver};
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainFragment, TerrainSpec},
    world::{
        BodyKind, BodyMotion, BodySpec, PhysicsId, PhysicsStepMetrics, PhysicsWorld,
        PhysicsWorldConfig,
    },
};
use engine_terrain::{
    Brush, CellCoord, DetachedTerrain, EditMode, Material, MaterialId, Terrain, TerrainEdit,
    TerrainError, TerrainGeometry,
};

use crate::{FIXED_HZ, ORE, ROCK};

const GROUND: PhysicsId = PhysicsId::new(1);
const CELL_SIZE: f32 = 0.5;
const PLANET_RADIUS: f32 = 16.0;
const GRAVITY: f32 = 18.0;
const DT: f32 = 1.0 / FIXED_HZ as f32;

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

    fn pose(self, tick: u64) -> (Vec2, f32) {
        let seconds = tick as f32 * DT;
        match self {
            Self::Flat => (Vec2::ZERO, 0.0),
            Self::MovingPlanet => (
                Vec2::new(2.0 * (seconds * 0.2).sin(), -PLANET_RADIUS),
                seconds * 0.025,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlastMode {
    Remove,
    Release,
}

impl BlastMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlastLabConfig {
    pub fixture: Fixture,
    pub mode: BlastMode,
    pub seed: u64,
    /// Experimental partition size, unrelated to terrain processing chunks.
    pub patch_cells: u32,
    /// Reject a whole blast if its prepared result exceeds this population.
    pub max_fragments: usize,
    pub friction: f32,
}

impl Default for BlastLabConfig {
    fn default() -> Self {
        Self {
            fixture: Fixture::Flat,
            mode: BlastMode::Release,
            seed: 42,
            patch_cells: 2,
            max_fragments: 128,
            friction: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blast {
    pub center: Vec2,
    pub radius: f32,
    /// Maximum radial velocity change; this is not a pressure/energy solver.
    pub speed: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlastResult {
    pub admitted: bool,
    pub selected_cells: u64,
    pub loose_bodies_hit: usize,
    pub spawned_fragments: usize,
    pub accelerated_fragments: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaterialBalance {
    pub initial: u64,
    pub ground: u64,
    pub loose: u64,
    pub removed: u64,
}

#[derive(Clone)]
pub struct BlastLab {
    config: BlastLabConfig,
    pub tick: u64,
    pub blasts: u64,
    pub rejected_blasts: u64,
    pub last_physics: PhysicsStepMetrics,
    bodies: Vec<TerrainFragment>,
    physics: PhysicsWorld,
    gravity: GravitySolver,
    next_id: u64,
    initial: [u64; 256],
    removed: [u64; 256],
}

struct PreparedBody {
    id: PhysicsId,
    terrain: Terrain,
    released: Vec<DetachedTerrain>,
    motion: BodyMotion,
    center: Vec2,
}

impl BlastLab {
    pub fn new(config: BlastLabConfig) -> Result<Self, TerrainError> {
        if !(2..=8).contains(&config.patch_cells)
            || !(1..=512).contains(&config.max_fragments)
            || !config.friction.is_finite()
            || !(0.0..=1.5).contains(&config.friction)
        {
            return Err(TerrainError(
                "invalid patch size, fragment limit or friction",
            ));
        }
        let (width, height) = match config.fixture {
            Fixture::Flat => (129, 81),
            Fixture::MovingPlanet => (65, 65),
        };
        let terrain = Terrain::generate(
            width,
            height,
            CELL_SIZE,
            vec![
                Material {
                    id: ROCK,
                    hardness: 100,
                },
                Material {
                    id: ORE,
                    hardness: 180,
                },
            ],
            |c| {
                let local = Vec2::new(
                    c.x as f32 + 0.5 - width as f32 * 0.5,
                    c.y as f32 + 0.5 - height as f32 * 0.5,
                ) * CELL_SIZE;
                let solid = match config.fixture {
                    Fixture::Flat => local.y <= 0.0,
                    Fixture::MovingPlanet => local.length_squared() <= PLANET_RADIUS.powi(2),
                };
                if !solid {
                    return MaterialId::VOID;
                }
                let mut value = config.seed
                    ^ (c.x as u64 / 5).wrapping_mul(0x9e3779b97f4a7c15)
                    ^ (c.y as u64 / 5).wrapping_mul(0xbf58476d1ce4e5b9);
                value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                if (value ^ (value >> 27)).is_multiple_of(5) {
                    ORE
                } else {
                    ROCK
                }
            },
        )?;
        let initial = quantities(&terrain);
        let geometry = TerrainGeometry::new(&terrain);
        let (position, angle) = config.fixture.pose(0);
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 4,
            ..PhysicsWorldConfig::default()
        });
        let assembly = TerrainAssembly::insert(
            &mut physics,
            GROUND,
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position,
                angle,
                can_sleep: false,
                ..BodySpec::default()
            },
            &terrain,
            &geometry,
            TerrainSpec {
                friction: config.friction,
                ..TerrainSpec::default()
            },
        )
        .expect("validated blast fixture");
        physics.step(DT);
        let body = TerrainFragment {
            id: GROUND,
            hash: terrain.hash(),
            terrain,
            geometry,
            assembly,
            edited_chunks: Vec::new(),
        };
        Ok(Self {
            config,
            tick: 0,
            blasts: 0,
            rejected_blasts: 0,
            last_physics: PhysicsStepMetrics::default(),
            bodies: vec![body],
            physics,
            gravity: GravitySolver::new(),
            next_id: 2,
            initial,
            removed: [0; 256],
        })
    }

    /// A point below the original surface, following the fixture's current pose.
    pub fn surface_point(&self, x: f32, depth: f32) -> Vec2 {
        let y = match self.config.fixture {
            Fixture::Flat => -depth,
            Fixture::MovingPlanet => (PLANET_RADIUS.powi(2) - x * x).max(0.0).sqrt() - depth,
        };
        let motion = self.physics.motion(self.bodies[0].assembly.body()).unwrap();
        motion.position + Vec2::new(x, y).rotate_radians(motion.angle)
    }

    pub fn bodies(&self) -> impl Iterator<Item = (&TerrainFragment, BodyMotion)> {
        self.bodies
            .iter()
            .map(|body| (body, self.physics.motion(body.assembly.body()).unwrap()))
    }

    /// Prepare every transfer before admitting the event. Population rejection
    /// changes no material, IDs or velocities, and is visible in the counters.
    pub fn blast(&mut self, blast: Blast) -> Result<BlastResult, TerrainError> {
        if !blast.center.x.is_finite()
            || !blast.center.y.is_finite()
            || !blast.radius.is_finite()
            || !(0.5..=5.0).contains(&blast.radius)
            || !blast.speed.is_finite()
            || !(0.0..=30.0).contains(&blast.speed)
        {
            return Err(TerrainError("invalid blast position, radius or speed"));
        }
        let mut prepared = Vec::new();
        let mut removed = [0u64; 256];
        let mut result = BlastResult::default();
        let mut emptied = 0;
        for body in &self.bodies {
            let motion = self.physics.motion(body.assembly.body()).unwrap();
            let Some(center) = body
                .terrain
                .local_to_cell((blast.center - motion.position).rotate_radians(-motion.angle))
            else {
                return Err(TerrainError("blast coordinate exceeds terrain limits"));
            };
            let brush = Brush::Circle {
                center,
                radius: (blast.radius / CELL_SIZE).round() as u32,
            };
            let selected: Vec<_> = body
                .terrain
                .brush_cells(brush)?
                .map(|(coordinate, _)| coordinate)
                .collect();
            if selected.is_empty() {
                continue;
            }
            result.selected_cells += selected.len() as u64;
            result.loose_bodies_hit += usize::from(body.id != GROUND);
            let mut terrain = body.terrain.clone();
            let mut released = Vec::new();
            match self.config.mode {
                BlastMode::Remove => {
                    for material in terrain
                        .apply(TerrainEdit {
                            brush,
                            mode: EditMode::Remove,
                        })?
                        .removed
                    {
                        removed[material.material.0 as usize] += u64::from(material.cells);
                    }
                }
                BlastMode::Release => {
                    // Coarse regular patches are a reference model. They are not
                    // the terrain cache's 32-cell processing chunks or soil grains.
                    let mut patches = BTreeMap::<(i32, i32), Vec<CellCoord>>::new();
                    for coordinate in selected {
                        let side = self.config.patch_cells as i32;
                        patches
                            .entry((coordinate.y / side, coordinate.x / side))
                            .or_default()
                            .push(coordinate);
                    }
                    for coordinates in patches.into_values() {
                        released.extend(terrain.extract_cells(&coordinates)?);
                    }
                }
            }
            released.extend(terrain.detach_disconnected()?);
            if body.id != GROUND
                && terrain
                    .cells()
                    .iter()
                    .all(|cell| cell.material == MaterialId::VOID)
            {
                emptied += 1;
            }
            result.spawned_fragments += released.len();
            prepared.push(PreparedBody {
                id: body.id,
                terrain,
                released,
                motion,
                center: self.physics.center_of_mass(body.assembly.body()).unwrap(),
            });
        }
        if self.bodies.len() - 1 - emptied + result.spawned_fragments > self.config.max_fragments {
            self.rejected_blasts += 1;
            result.spawned_fragments = 0;
            return Ok(result);
        }
        for update in prepared {
            let body = self
                .bodies
                .iter_mut()
                .find(|body| body.id == update.id)
                .unwrap();
            body.terrain = update.terrain;
            body.hash = body.terrain.hash();
            body.edited_chunks = body.geometry.refresh(&body.terrain);
            body.assembly
                .synchronize(&mut self.physics, &body.terrain, &body.geometry)
                .expect("matching terrain geometry");
            if body.id != GROUND && body.geometry.rectangle_count() == 0 {
                self.physics.remove_entity(body.id);
            }
            for released in update.released {
                let id = PhysicsId::new(self.next_id);
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .expect("blast fixture ID exhausted");
                self.bodies.push(
                    TerrainFragment::insert(
                        &mut self.physics,
                        id,
                        released,
                        update.motion,
                        update.center,
                        TerrainSpec {
                            friction: self.config.friction,
                            ..TerrainSpec::default()
                        },
                    )
                    .expect("validated transferred terrain"),
                );
            }
        }
        self.bodies
            .retain(|body| body.id == GROUND || body.geometry.rectangle_count() > 0);
        for (destination, quantity) in self.removed.iter_mut().zip(removed) {
            *destination += quantity;
        }
        // Existing loose pieces and newly released ones receive the same impulse
        // policy. Geometry/velocities are committed before the next mechanics step.
        for body in self.bodies.iter().filter(|body| body.id != GROUND) {
            let offset = self.physics.center_of_mass(body.assembly.body()).unwrap() - blast.center;
            let distance = offset.length();
            if distance > 0.0001 && distance < blast.radius {
                let delta = offset / distance * (blast.speed * (1.0 - distance / blast.radius));
                if delta.length_squared() > 0.0 {
                    self.physics
                        .apply_velocity_delta(body.assembly.body(), delta, true);
                    result.accelerated_fragments += 1;
                }
            }
        }
        self.blasts += 1;
        result.admitted = true;
        Ok(result)
    }

    pub fn step(&mut self) {
        let (position, angle) = self.config.fixture.pose(self.tick + 1);
        self.physics
            .set_next_kinematic_pose(self.bodies[0].assembly.body(), position, angle);
        self.physics.clear_forces();
        match self.config.fixture {
            Fixture::Flat => {
                for body in self.bodies.iter().skip(1) {
                    self.physics.apply_acceleration(
                        body.assembly.body(),
                        Vec2::new(0.0, -GRAVITY),
                        false,
                    );
                }
            }
            Fixture::MovingPlanet => {
                let mut participants = vec![GravityParticipant::spherical_source(
                    GravityId::new(1),
                    self.physics
                        .motion(self.bodies[0].assembly.body())
                        .unwrap()
                        .position,
                    GRAVITY * PLANET_RADIUS.powi(2),
                    PLANET_RADIUS,
                )];
                participants.extend(self.bodies.iter().skip(1).map(|body| {
                    GravityParticipant::target(
                        GravityId::new(body.id.value()),
                        self.physics.center_of_mass(body.assembly.body()).unwrap(),
                        1.0,
                    )
                }));
                let deltas = self
                    .gravity
                    .solve(
                        &participants,
                        GravityConfig {
                            backend: GravityBackend::Exact,
                            softening: 0.0,
                            interaction_scale: DT,
                        },
                    )
                    .expect("bounded gravity fixture");
                for (body, delta) in self.bodies.iter().skip(1).zip(deltas.iter().skip(1)) {
                    self.physics.apply_acceleration(
                        body.assembly.body(),
                        delta.velocity_delta / DT,
                        false,
                    );
                }
            }
        }
        self.last_physics = self.physics.step(DT);
        self.tick += 1;
    }

    /// Audit outside the timed step. Cell counts preserve material identity;
    /// matching collider mass and geometry are checked for each dynamic piece.
    pub fn audit(&self) -> Result<MaterialBalance, TerrainError> {
        let mut total = self.removed;
        let mut balance = MaterialBalance {
            initial: self.initial.iter().sum(),
            removed: self.removed.iter().sum(),
            ..MaterialBalance::default()
        };
        for (body, motion) in self.bodies() {
            let amounts = quantities(&body.terrain);
            let occupied: u64 = amounts.iter().sum();
            if body.id == GROUND {
                balance.ground += occupied;
            } else {
                balance.loose += occupied;
            }
            for (destination, amount) in total.iter_mut().zip(amounts) {
                *destination += amount;
            }
            if ![
                motion.position.x,
                motion.position.y,
                motion.angle,
                motion.linear_velocity.x,
                motion.linear_velocity.y,
                motion.angular_velocity,
            ]
            .iter()
            .all(|value| value.is_finite())
            {
                return Err(TerrainError("nonfinite material motion"));
            }
            if !body.geometry.is_current(&body.terrain)
                || body
                    .geometry
                    .chunks()
                    .iter()
                    .map(|chunk| chunk.material_cells())
                    .sum::<u64>()
                    != occupied
            {
                return Err(TerrainError("material geometry mismatch"));
            }
            if body.id != GROUND {
                let expected = occupied as f32 * CELL_SIZE.powi(2);
                let mass = self.physics.body_mass(body.assembly.body()).unwrap();
                if occupied == 0 || !mass.is_finite() || (mass - expected).abs() > expected * 0.0001
                {
                    return Err(TerrainError("transferred material mass mismatch"));
                }
            }
        }
        if total != self.initial || self.physics.body_count() != self.bodies.len() {
            return Err(TerrainError("material or body accounting mismatch"));
        }
        Ok(balance)
    }

    pub fn fragment_count(&self) -> usize {
        self.bodies.len() - 1
    }

    /// Diagnostic content/motion fingerprint, excluding timers and solver caches.
    pub fn content_motion_hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut write = |value: u64| {
            for byte in value.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        };
        for value in [self.tick, self.next_id, self.blasts, self.rejected_blasts] {
            write(value);
        }
        for value in self.removed {
            write(value);
        }
        for (body, motion) in self.bodies() {
            write(body.id.value());
            write(body.hash);
            for value in [
                motion.position.x,
                motion.position.y,
                motion.angle,
                motion.linear_velocity.x,
                motion.linear_velocity.y,
                motion.angular_velocity,
            ] {
                write(u64::from(value.to_bits()));
            }
        }
        hash
    }
}

fn quantities(terrain: &Terrain) -> [u64; 256] {
    let mut counts = [0; 256];
    for cell in terrain.cells() {
        if cell.material != MaterialId::VOID {
            counts[cell.material.0 as usize] += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests;
