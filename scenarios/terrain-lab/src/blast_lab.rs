//! Controlled comparison of crater removal, rigid pieces, and rounded grains.
//! Experimental contact models for #165; neither is calibrated soil physics.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_gravity::{GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver};
use engine_rapier::{
    terrain::{
        GrainShape, LooseTerrain, LooseTerrainConfig, PreparedRelease, RadialImpulse,
        TerrainAssembly, TerrainBodyMut, TerrainFragment, TerrainGrain, TerrainSpec,
    },
    world::{
        BodyId, BodyKind, BodyMotion, BodySpec, PhysicsId, PhysicsStepMetrics, PhysicsWorld,
        PhysicsWorldConfig,
    },
};
use engine_terrain::{
    Brush, CellCoord, DetachedTerrain, EditMode, Material, MaterialId, Terrain, TerrainEdit,
    TerrainError, TerrainGeometry,
};

use crate::{FIXED_HZ, ORE, ROCK};

const GROUND: PhysicsId = PhysicsId::new(1);
const PLANET_RADIUS: f32 = 16.0;
const GRAVITY: f32 = 18.0;
const DT: f32 = 1.0 / FIXED_HZ as f32;
const SLOPE: f32 = 0.25;
const MAX_ACTIVE_PULSES: usize = 8;

mod diagnostics;
pub use diagnostics::MotionStats;
mod probe;
pub use probe::ProbeSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    Flat,
    Slope,
    MovingPlanet,
}

impl Fixture {
    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Slope => "slope",
            Self::MovingPlanet => "moving-planet",
        }
    }

    fn pose(self, tick: u64) -> (Vec2, f32) {
        let seconds = tick as f32 * DT;
        match self {
            Self::Flat | Self::Slope => (Vec2::ZERO, 0.0),
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
    Grains,
    GrainPulse,
}

impl BlastMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Release => "release",
            Self::Grains => "grains",
            Self::GrainPulse => "grain-pulse",
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
    pub max_loose_bodies: usize,
    pub friction: f32,
    /// Number of fixed ticks sharing a pulse's maximum velocity-change budget.
    pub pulse_ticks: u32,
    pub grain_shape: GrainShape,
    pub cell_size: f32,
    /// Enable the shared return-to-terrain lifecycle. Raw model comparisons
    /// leave this off so grain/contact behavior remains independently measurable.
    pub deposition: bool,
}

impl Default for BlastLabConfig {
    fn default() -> Self {
        Self {
            fixture: Fixture::Flat,
            mode: BlastMode::Release,
            seed: 42,
            patch_cells: 2,
            max_loose_bodies: 128,
            friction: 0.35,
            pulse_ticks: 6,
            grain_shape: GrainShape::Round,
            cell_size: 0.5,
            deposition: false,
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
    pub spawned_grains: usize,
    pub accelerated_bodies: usize,
    pub rejection: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BlastPulse {
    blast: Blast,
    remaining: u32,
    next_tick: u64,
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
    grains: LooseTerrain,
    pulses: Vec<BlastPulse>,
    physics: PhysicsWorld,
    gravity: GravitySolver,
    next_id: u64,
    initial: [u64; 256],
    removed: [u64; 256],
    probe: bool,
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
            || !(1..=512).contains(&config.max_loose_bodies)
            || !config.friction.is_finite()
            || !(0.0..=1.5).contains(&config.friction)
            || !(1..=30).contains(&config.pulse_ticks)
            || !config.cell_size.is_finite()
            || !(0.25..=1.0).contains(&config.cell_size)
        {
            return Err(TerrainError(
                "invalid patch size, body limit, friction, pulse duration or cell size",
            ));
        }
        let (width, height) = match config.fixture {
            Fixture::Flat | Fixture::Slope => (
                (64.0 / config.cell_size).ceil() as u32 + 1,
                (40.0 / config.cell_size).ceil() as u32 + 1,
            ),
            Fixture::MovingPlanet => {
                let side = (2.0 * PLANET_RADIUS / config.cell_size).ceil() as u32 + 1;
                (side, side)
            }
        };
        let terrain = Terrain::generate(
            width,
            height,
            config.cell_size,
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
                ) * config.cell_size;
                let solid = match config.fixture {
                    Fixture::Flat => local.y <= 0.0,
                    Fixture::Slope => local.y <= SLOPE * local.x,
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
            grains: LooseTerrain::new(LooseTerrainConfig {
                max_grains: config.max_loose_bodies,
                shape: config.grain_shape,
                friction: config.friction,
                restitution: 0.0,
                ..Default::default()
            })?,
            pulses: Vec::new(),
            physics,
            gravity: GravitySolver::new(),
            next_id: 2,
            initial,
            removed: [0; 256],
            probe: false,
        })
    }

    /// A point below the original surface, following the fixture's current pose.
    pub fn surface_point(&self, x: f32, depth: f32) -> Vec2 {
        let y = match self.config.fixture {
            Fixture::Flat => -depth,
            Fixture::Slope => SLOPE * x - depth,
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

    pub fn grains(&self) -> impl Iterator<Item = (&TerrainGrain, BodyMotion)> {
        self.grains
            .iter()
            .map(|grain| (grain, self.physics.motion(grain.body()).unwrap()))
    }

    pub fn config(&self) -> BlastLabConfig {
        self.config
    }

    pub fn ground_motion(&self) -> BodyMotion {
        self.physics.motion(self.bodies[0].assembly.body()).unwrap()
    }

    fn moving_bodies(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.bodies
            .iter()
            .skip(1)
            .map(|body| body.assembly.body())
            .chain(self.grains.iter().map(TerrainGrain::body))
            .chain(self.probe.then_some(probe::body()))
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
        let mut grain_plans = Vec::new();
        let mut removed = [0u64; 256];
        let mut result = BlastResult::default();
        let mut emptied = 0;
        for (_, motion) in self.grains() {
            if motion.position.distance_to(blast.center) < blast.radius {
                result.selected_cells += 1;
                result.loose_bodies_hit += 1;
            }
        }
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
                radius: (blast.radius / body.terrain.cell_size()).round() as u32,
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
            if matches!(self.config.mode, BlastMode::Grains | BlastMode::GrainPulse) {
                let plan = PreparedRelease::new(
                    &body.terrain,
                    &[TerrainEdit {
                        brush,
                        mode: EditMode::Remove,
                    }],
                )?;
                emptied += usize::from(body.id != GROUND && plan.source_empty());
                result.spawned_fragments += plan.fragment_count();
                result.spawned_grains += plan.grain_count();
                grain_plans.push((body.id, plan));
                continue;
            }
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
                BlastMode::Grains | BlastMode::GrainPulse => {
                    unreachable!("shared release path above")
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
        result.rejection =
            if self.loose_body_count() - emptied + result.spawned_fragments + result.spawned_grains
                > self.config.max_loose_bodies
            {
                Some("loose-body limit")
            } else if self.config.mode == BlastMode::GrainPulse
                && self.pulses.len() >= MAX_ACTIVE_PULSES
            {
                Some("active-pulse limit")
            } else {
                None
            };
        if result.rejection.is_some() {
            self.rejected_blasts += 1;
            result.spawned_fragments = 0;
            result.spawned_grains = 0;
            return Ok(result);
        }
        for (id, plan) in grain_plans {
            let body = self.bodies.iter_mut().find(|body| body.id == id).unwrap();
            let committed = self
                .grains
                .commit(
                    &mut self.physics,
                    TerrainBodyMut {
                        terrain: &mut body.terrain,
                        geometry: &mut body.geometry,
                        assembly: &mut body.assembly,
                    },
                    plan,
                    &mut self.next_id,
                )
                .expect("pre-admitted material release");
            body.hash = body.terrain.hash();
            body.edited_chunks = committed.dirty_chunks;
            if id != GROUND && body.geometry.shape_count() == 0 {
                self.physics.remove_entity(id);
            }
            self.bodies.extend(committed.fragments);
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
        let mut kick = blast;
        if self.config.mode == BlastMode::GrainPulse {
            kick.speed /= self.config.pulse_ticks as f32;
            if self.config.pulse_ticks > 1 {
                self.pulses.push(BlastPulse {
                    blast: kick,
                    remaining: self.config.pulse_ticks - 1,
                    next_tick: self.tick + 1,
                });
            }
        }
        result.accelerated_bodies = self.apply_blast_velocity(kick);
        self.blasts += 1;
        result.admitted = true;
        Ok(result)
    }

    fn apply_blast_velocity(&mut self, blast: Blast) -> usize {
        let bodies: Vec<_> = self.moving_bodies().collect();
        RadialImpulse {
            center: blast.center,
            radius: blast.radius,
            speed: blast.speed,
        }
        .apply(&mut self.physics, bodies)
    }

    pub fn step(&mut self) {
        let (position, angle) = self.config.fixture.pose(self.tick + 1);
        self.physics
            .set_next_kinematic_pose(self.bodies[0].assembly.body(), position, angle);
        self.physics.clear_forces();
        for mut pulse in std::mem::take(&mut self.pulses) {
            if self.tick >= pulse.next_tick {
                self.apply_blast_velocity(pulse.blast);
                pulse.remaining -= 1;
                pulse.next_tick += 1;
            }
            if pulse.remaining > 0 {
                self.pulses.push(pulse);
            }
        }
        let dynamic: Vec<_> = self.moving_bodies().collect();
        match self.config.fixture {
            Fixture::Flat | Fixture::Slope => {
                for body in dynamic {
                    self.physics
                        .apply_acceleration(body, Vec2::new(0.0, -GRAVITY), false);
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
                participants.extend(dynamic.iter().map(|body| {
                    GravityParticipant::target(
                        GravityId::new(body.entity.value()),
                        self.physics.center_of_mass(*body).unwrap(),
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
                for (body, delta) in dynamic.into_iter().zip(deltas.iter().skip(1)) {
                    self.physics
                        .apply_acceleration(body, delta.velocity_delta / DT, false);
                }
            }
        }
        self.last_physics = self.physics.step(DT);
        if self.config.deposition {
            let commits = self
                .grains
                .settle(
                    &mut self.physics,
                    self.bodies.iter_mut().map(|body| TerrainBodyMut {
                        terrain: &mut body.terrain,
                        geometry: &mut body.geometry,
                        assembly: &mut body.assembly,
                    }),
                    DT,
                )
                .expect("valid lab deposition");
            for commit in commits {
                let body = self
                    .bodies
                    .iter_mut()
                    .find(|b| b.assembly.body() == commit.body)
                    .unwrap();
                body.hash = body.terrain.hash();
                body.edited_chunks = commit.dirty_chunks;
            }
        }
        self.tick += 1;
    }

    pub fn deposited_cells(&self) -> u64 {
        self.grains.deposited_cells()
    }

    pub fn settling_diagnostics(&self) -> engine_rapier::terrain::SettlingDiagnostics {
        self.grains.settling_diagnostics()
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
            if !finite_motion(motion) {
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
                let expected = occupied as f32 * body.terrain.cell_size().powi(2);
                let mass = self.physics.body_mass(body.assembly.body()).unwrap();
                if occupied == 0 || !mass.is_finite() || (mass - expected).abs() > expected * 0.0001
                {
                    return Err(TerrainError("transferred material mass mismatch"));
                }
            }
        }
        self.grains.audit(&self.physics)?;
        for quantity in self.grains.quantities() {
            total[quantity.material.0 as usize] += quantity.cells;
            balance.loose += quantity.cells;
        }
        if total != self.initial
            || self.physics.body_count() != self.loose_body_count() + 1 + usize::from(self.probe)
        {
            return Err(TerrainError("material or body accounting mismatch"));
        }
        if self
            .probe_snapshot()
            .is_some_and(|probe| !finite_motion(probe.motion))
        {
            return Err(TerrainError("nonfinite probe motion"));
        }
        Ok(balance)
    }

    pub fn fragment_count(&self) -> usize {
        self.bodies.len() - 1
    }

    pub fn loose_body_count(&self) -> usize {
        self.fragment_count() + self.grains.len()
    }

    pub fn active_pulses(&self) -> usize {
        self.pulses.len()
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
        if self.config.deposition {
            write(self.grains.settling_hash());
        }
        if let Some(probe) = self.probe_snapshot() {
            write(2);
            for value in [
                probe.motion.position.x,
                probe.motion.position.y,
                probe.motion.angle,
                probe.motion.linear_velocity.x,
                probe.motion.linear_velocity.y,
                probe.motion.angular_velocity,
            ] {
                write(value.to_bits() as u64);
            }
        }
        for value in self.removed {
            write(value);
        }
        write(self.pulses.len() as u64);
        for pulse in &self.pulses {
            write(u64::from(pulse.remaining));
            write(pulse.next_tick);
            for value in [
                pulse.blast.center.x,
                pulse.blast.center.y,
                pulse.blast.radius,
                pulse.blast.speed,
            ] {
                write(u64::from(value.to_bits()));
            }
        }
        for (body, motion) in self.bodies() {
            write(0);
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
        for (grain, motion) in self.grains() {
            write(1);
            write(grain.id().value());
            write(u64::from(grain.cell().material.0));
            write(u64::from(grain.cell().durability));
            write(grain.shape() as u64);
            for value in [
                grain.cell_size(),
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

fn finite_motion(motion: BodyMotion) -> bool {
    [
        motion.position.x,
        motion.position.y,
        motion.angle,
        motion.linear_velocity.x,
        motion.linear_velocity.y,
        motion.angular_velocity,
    ]
    .iter()
    .all(|value| value.is_finite())
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
