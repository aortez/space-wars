//! Narrow Rapier boundary for articulated Spacewars mechanics.
//!
//! Rapier owns every pose and velocity exposed by this crate. Callers provide
//! gameplay intent, kinematic planet targets, and acceleration fields; they do
//! not integrate the returned bodies a second time.

use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

use engine_core::Vec2;
use rapier2d::prelude::*;
use serde::{Deserialize, Serialize};

const PLANET_ENTITY_TAG: u128 = 1;
const ROVER_CHASSIS_ENTITY_TAG: u128 = 2;
const ROVER_WHEEL_ENTITY_TAG: u128 = 3;
const ENTITY_TAG_SHIFT: u32 = 72;
const WHEEL_INDEX_SHIFT: u32 = 64;

/// Gameplay identity associated with a Rapier collider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsEntity {
    Planet(u64),
    RoverChassis(u64),
    RoverWheel { rover_id: u64, wheel: usize },
}

/// World-space state read directly from a Rapier rigid body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyMotion {
    pub position: Vec2,
    pub angle: f32,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
}

/// Axis-aligned playfield for a bulk circle workload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallPhysicsBounds {
    pub width: f32,
    pub height: f32,
}

/// One dynamic circle inserted into [`BallPhysics`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallBodySpec {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub density: f32,
    pub restitution: f32,
}

/// Rapier-owned motion projected without allocating a snapshot collection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallMotion {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub angular_velocity: f32,
}

/// Per-step Rapier counters used by the Pizza performance lab.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BallPhysicsMetrics {
    pub wall_time: Duration,
    pub rapier_step_time: Duration,
    pub broad_phase_time: Duration,
    pub narrow_phase_time: Duration,
    pub island_time: Duration,
    pub solver_time: Duration,
    pub ccd_time: Duration,
    pub active_bodies: usize,
    pub sleeping_bodies: usize,
    pub candidate_pairs: usize,
    pub contact_pairs: usize,
    pub contacts: usize,
}

#[derive(Debug, Clone, Copy)]
struct BallBody {
    id: u64,
    body: RigidBodyHandle,
}

/// Bulk-oriented Rapier world for circle collision and lifecycle benchmarks.
///
/// Unlike [`RoverPhysics`], this keeps bodies in a dense vector and exposes
/// allocation-free motion iteration. Scenario metadata such as colors remains
/// outside Rapier.
pub struct BallPhysics {
    world: PhysicsWorld,
    balls: Vec<BallBody>,
    indices: HashMap<u64, usize>,
    allow_sleeping: bool,
}

impl BallPhysics {
    pub fn new(
        bounds: BallPhysicsBounds,
        gravity: Vec2,
        allow_sleeping: bool,
        solver_iterations: usize,
    ) -> Self {
        let bounds = normalized_ball_bounds(bounds);
        let gravity = finite_vec2_or_zero(gravity);
        let mut world = PhysicsWorld {
            gravity: to_rapier(gravity),
            ..PhysicsWorld::default()
        };
        world.integration_parameters.dt = 1.0 / 60.0;
        world.integration_parameters.length_unit = bounds.width.min(bounds.height).max(0.001);
        world.integration_parameters.num_solver_iterations = solver_iterations.max(1);
        world
            .integration_parameters
            .num_internal_stabilization_iterations = 1;
        world.integration_parameters.max_ccd_substeps = 1;

        let thickness = (bounds.width.min(bounds.height) * 0.02).max(0.001);
        let half_width = bounds.width * 0.5;
        let half_height = bounds.height * 0.5;
        for (position, collider) in [
            (
                Vector::new(half_width, -thickness * 0.5),
                ColliderBuilder::cuboid(half_width + thickness, thickness * 0.5),
            ),
            (
                Vector::new(half_width, bounds.height + thickness * 0.5),
                ColliderBuilder::cuboid(half_width + thickness, thickness * 0.5),
            ),
            (
                Vector::new(-thickness * 0.5, half_height),
                ColliderBuilder::cuboid(thickness * 0.5, half_height + thickness),
            ),
            (
                Vector::new(bounds.width + thickness * 0.5, half_height),
                ColliderBuilder::cuboid(thickness * 0.5, half_height + thickness),
            ),
        ] {
            world.insert(
                RigidBodyBuilder::fixed().translation(position),
                collider.friction(0.4).restitution(0.9),
            );
        }

        Self {
            world,
            balls: Vec::new(),
            indices: HashMap::new(),
            allow_sleeping,
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.balls.reserve(additional);
        self.indices.reserve(additional);
    }

    pub fn insert_ball(&mut self, spec: BallBodySpec) -> bool {
        if self.indices.contains_key(&spec.id) || !valid_ball_spec(spec) {
            return false;
        }

        let body = RigidBodyBuilder::dynamic()
            .translation(to_rapier(spec.position))
            .linvel(to_rapier(spec.velocity))
            .can_sleep(self.allow_sleeping)
            .ccd_enabled(false)
            .user_data(spec.id as u128)
            .build();
        let collider = ColliderBuilder::ball(spec.radius)
            .density(spec.density)
            .friction(0.3)
            .restitution(spec.restitution.clamp(0.0, 1.0))
            .user_data(spec.id as u128)
            .build();
        let (body, _) = self.world.insert(body, collider);
        let index = self.balls.len();
        self.balls.push(BallBody { id: spec.id, body });
        self.indices.insert(spec.id, index);
        true
    }

    /// Remove a body by dense index and swap the final entry into its slot.
    pub fn swap_remove_ball(&mut self, index: usize) -> Option<u64> {
        if index >= self.balls.len() {
            return None;
        }
        let removed = self.balls.swap_remove(index);
        self.indices.remove(&removed.id);
        self.world.remove_body(removed.body);
        if let Some(replacement) = self.balls.get(index) {
            self.indices.insert(replacement.id, index);
        }
        Some(removed.id)
    }

    pub fn remove_ball(&mut self, id: u64) -> bool {
        self.indices
            .get(&id)
            .copied()
            .and_then(|index| self.swap_remove_ball(index))
            .is_some()
    }

    pub fn len(&self) -> usize {
        self.balls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.balls.is_empty()
    }

    pub fn motions(&self) -> impl ExactSizeIterator<Item = BallMotion> + '_ {
        self.balls.iter().map(|entry| {
            let body = &self.world.bodies[entry.body];
            BallMotion {
                id: entry.id,
                position: from_rapier(body.translation()),
                velocity: from_rapier(body.linvel()),
                angular_velocity: body.angvel(),
            }
        })
    }

    pub fn step(&mut self, dt_seconds: f32) -> BallPhysicsMetrics {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return BallPhysicsMetrics::default();
        }
        self.world.integration_parameters.dt = dt_seconds;

        let started = Instant::now();
        self.world.step();
        let wall_time = started.elapsed();
        let counters = self.world.physics_pipeline.counters;
        let mut candidate_pairs = 0;
        let mut contact_pairs = 0;
        let mut contacts = 0;
        for pair in self.world.narrow_phase.contact_pairs() {
            candidate_pairs += 1;
            if pair.has_any_active_contact() {
                contact_pairs += 1;
                contacts += pair
                    .manifolds
                    .iter()
                    .map(|manifold| manifold.data.solver_contacts.len())
                    .sum::<usize>();
            }
        }
        BallPhysicsMetrics {
            wall_time,
            rapier_step_time: counters.step_time(),
            broad_phase_time: counters.cd.broad_phase_time.time(),
            narrow_phase_time: counters.cd.narrow_phase_time.time(),
            island_time: counters.stages.island_construction_time.time(),
            solver_time: counters.stages.solver_time.time(),
            ccd_time: counters.stages.ccd_time.time(),
            active_bodies: self.world.islands.active_bodies().count(),
            sleeping_bodies: self
                .balls
                .iter()
                .filter(|entry| self.world.bodies[entry.body].is_sleeping())
                .count(),
            candidate_pairs,
            contact_pairs,
            contacts,
        }
    }
}

/// A contact point reported by Rapier for debug presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    pub position: Vec2,
    pub normal: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanetSpec {
    pub center: Vec2,
    pub radius: f32,
    pub angle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BumpSpec {
    /// Counter-clockwise surface angle, where zero points along world +X.
    pub surface_angle: f32,
    /// Half-width of the bump where it meets the planet surface.
    pub half_width: f32,
    /// Half of the bump's height above the planet surface.
    pub half_height: f32,
}

impl BumpSpec {
    const EMBED_RATIO: f32 = 0.35;

    /// Local-space trapezoid used by both collision and presentation.
    ///
    /// Local +X follows the rover's forward tangent and local +Y points away
    /// from the planet. Sloped faces make this a suspension obstacle instead
    /// of a wheel-height vertical wall.
    pub fn local_vertices(self) -> [Vec2; 4] {
        let top_half_width = self.half_width * 0.1;
        [
            Vec2::new(-self.half_width, -self.half_height),
            Vec2::new(self.half_width, -self.half_height),
            Vec2::new(top_half_width, self.half_height),
            Vec2::new(-top_half_width, self.half_height),
        ]
    }

    /// Radial distance from the planet center to this bump's local origin.
    ///
    /// The base is embedded slightly below the curved surface so the exposed
    /// terrain has no collider seam capable of trapping a wheel.
    pub fn radial_center_distance(self, planet_radius: f32) -> f32 {
        planet_radius + self.half_height * (1.0 - Self::EMBED_RATIO)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoverSpec {
    /// Counter-clockwise surface angle, where zero points along world +X.
    pub surface_angle: f32,
    pub chassis_half_width: f32,
    pub chassis_half_height: f32,
    pub wheel_radius: f32,
    pub wheel_offset: f32,
    pub suspension_anchor_height: f32,
    pub suspension_rest_length: f32,
    pub suspension_travel: f32,
    pub suspension_stiffness: f32,
    pub suspension_damping: f32,
    pub suspension_max_force: f32,
    pub wheel_target_speed: f32,
    pub wheel_motor_torque: f32,
    pub wheel_brake_torque: f32,
}

impl Default for RoverSpec {
    fn default() -> Self {
        Self {
            surface_angle: std::f32::consts::FRAC_PI_2,
            chassis_half_width: 2.2,
            chassis_half_height: 0.6,
            wheel_radius: 0.75,
            wheel_offset: 1.45,
            suspension_anchor_height: -0.35,
            suspension_rest_length: 0.85,
            suspension_travel: 0.35,
            suspension_stiffness: 240.0,
            suspension_damping: 24.0,
            suspension_max_force: 300.0,
            wheel_target_speed: 7.0,
            wheel_motor_torque: 80.0,
            wheel_brake_torque: 100.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RoverControl {
    pub throttle: f32,
    pub brake: bool,
}

/// Authoritative Rapier state projected into engine-native value types.
#[derive(Debug, Clone, PartialEq)]
pub struct RoverSnapshot {
    pub chassis: BodyMotion,
    pub wheels: [BodyMotion; 2],
    pub suspension_anchors: [Vec2; 2],
    pub contacts: Vec<ContactPoint>,
}

#[derive(Serialize, Deserialize)]
struct PlanetBodies {
    body: RigidBodyHandle,
}

#[derive(Serialize, Deserialize)]
struct RoverBodies {
    chassis: RigidBodyHandle,
    chassis_collider: ColliderHandle,
    wheels: [RigidBodyHandle; 2],
    wheel_colliders: [ColliderHandle; 2],
    suspension_joints: [ImpulseJointHandle; 2],
    suspension_anchors: [Vector; 2],
    target_wheel_speed: f32,
    motor_torque: f32,
    brake_torque: f32,
}

/// Owns Rapier's mechanical state and keeps raw handles behind typed IDs.
pub struct RoverPhysics {
    world: PhysicsWorld,
    planets: BTreeMap<u64, PlanetBodies>,
    rovers: BTreeMap<u64, RoverBodies>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsSnapshotError(String);

impl std::fmt::Display for PhysicsSnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PhysicsSnapshotError {}

#[derive(Serialize)]
struct RoverPhysicsSnapshotRef<'a> {
    gravity: &'a Vector,
    integration_parameters: &'a IntegrationParameters,
    islands: &'a IslandManager,
    broad_phase: &'a BroadPhaseBvh,
    narrow_phase: &'a NarrowPhase,
    bodies: &'a RigidBodySet,
    colliders: &'a ColliderSet,
    impulse_joints: &'a ImpulseJointSet,
    multibody_joints: &'a MultibodyJointSet,
    ccd_solver: &'a CCDSolver,
    planets: &'a BTreeMap<u64, PlanetBodies>,
    rovers: &'a BTreeMap<u64, RoverBodies>,
}

#[derive(Deserialize)]
struct RoverPhysicsSnapshot {
    gravity: Vector,
    integration_parameters: IntegrationParameters,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    planets: BTreeMap<u64, PlanetBodies>,
    rovers: BTreeMap<u64, RoverBodies>,
}

impl RoverPhysics {
    /// Create a zero-global-gravity world.
    ///
    /// `units_per_meter` tells Rapier how the caller's world units relate to
    /// its meter-tuned tolerances. Spacewars can therefore retain its existing
    /// coordinate system without spreading conversion constants through game
    /// code.
    pub fn new(units_per_meter: f32) -> Self {
        let mut world = PhysicsWorld {
            gravity: Vector::ZERO,
            ..PhysicsWorld::default()
        };
        world.integration_parameters.dt = 1.0 / 60.0;
        world.integration_parameters.length_unit = finite_positive_or(units_per_meter, 1.0);
        world.integration_parameters.num_solver_iterations = 8;
        world
            .integration_parameters
            .num_internal_stabilization_iterations = 2;
        world.integration_parameters.max_ccd_substeps = 2;
        Self {
            world,
            planets: BTreeMap::new(),
            rovers: BTreeMap::new(),
        }
    }

    /// Serialize every authoritative Rapier structure plus the typed handle
    /// mapping. `PhysicsPipeline` is intentionally omitted because it contains
    /// only step scratch state and is reconstructed when loading.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, PhysicsSnapshotError> {
        bincode::serialize(&RoverPhysicsSnapshotRef {
            gravity: &self.world.gravity,
            integration_parameters: &self.world.integration_parameters,
            islands: &self.world.islands,
            broad_phase: &self.world.broad_phase,
            narrow_phase: &self.world.narrow_phase,
            bodies: &self.world.bodies,
            colliders: &self.world.colliders,
            impulse_joints: &self.world.impulse_joints,
            multibody_joints: &self.world.multibody_joints,
            ccd_solver: &self.world.ccd_solver,
            planets: &self.planets,
            rovers: &self.rovers,
        })
        .map_err(|error| PhysicsSnapshotError(error.to_string()))
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, PhysicsSnapshotError> {
        let snapshot: RoverPhysicsSnapshot =
            bincode::deserialize(bytes).map_err(|error| PhysicsSnapshotError(error.to_string()))?;
        Ok(Self {
            world: PhysicsWorld {
                gravity: snapshot.gravity,
                integration_parameters: snapshot.integration_parameters,
                physics_pipeline: PhysicsPipeline::new(),
                islands: snapshot.islands,
                broad_phase: snapshot.broad_phase,
                narrow_phase: snapshot.narrow_phase,
                bodies: snapshot.bodies,
                colliders: snapshot.colliders,
                impulse_joints: snapshot.impulse_joints,
                multibody_joints: snapshot.multibody_joints,
                ccd_solver: snapshot.ccd_solver,
            },
            planets: snapshot.planets,
            rovers: snapshot.rovers,
        })
    }

    pub fn insert_planet(&mut self, id: u64, spec: PlanetSpec, bumps: &[BumpSpec]) -> bool {
        if self.planets.contains_key(&id) || !valid_planet_spec(spec) {
            return false;
        }

        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(to_rapier(spec.center))
            .rotation(spec.angle)
            .user_data(encode_entity(PhysicsEntity::Planet(id)))
            .build();
        let collider = ColliderBuilder::ball(spec.radius)
            .friction(1.25)
            .restitution(0.0)
            .user_data(encode_entity(PhysicsEntity::Planet(id)))
            .build();
        let (body, _) = self.world.insert(body, collider);

        for bump in bumps.iter().copied().filter(|bump| valid_bump_spec(*bump)) {
            let normal = Vector::new(bump.surface_angle.cos(), bump.surface_angle.sin());
            let local_center = normal * bump.radial_center_distance(spec.radius);
            let local_rotation = bump.surface_angle - std::f32::consts::FRAC_PI_2;
            let vertices = bump.local_vertices().map(to_rapier);
            self.world.insert_collider(
                ColliderBuilder::convex_hull(&vertices)
                    .expect("validated bump dimensions always form a convex hull")
                    .position(Pose::new(local_center, local_rotation))
                    .friction(1.25)
                    .restitution(0.0)
                    .user_data(encode_entity(PhysicsEntity::Planet(id))),
                Some(body),
            );
        }

        self.planets.insert(id, PlanetBodies { body });
        true
    }

    /// Queue the authoritative gameplay pose for a kinematic planet.
    pub fn set_next_planet_pose(&mut self, id: u64, center: Vec2, angle: f32) -> bool {
        let Some(planet) = self.planets.get(&id) else {
            return false;
        };
        let Some(body) = self.world.bodies.get_mut(planet.body) else {
            return false;
        };
        body.set_next_kinematic_position(Pose::new(to_rapier(center), angle));
        true
    }

    pub fn insert_rover(
        &mut self,
        id: u64,
        planet_id: u64,
        planet: PlanetSpec,
        spec: RoverSpec,
    ) -> bool {
        if self.rovers.contains_key(&id)
            || !self.planets.contains_key(&planet_id)
            || !valid_rover_spec(spec)
        {
            return false;
        }

        let normal = Vector::new(spec.surface_angle.cos(), spec.surface_angle.sin());
        let tangent_angle = spec.surface_angle - std::f32::consts::FRAC_PI_2;
        let wheel_surface_distance = planet.radius + spec.wheel_radius + 0.03;
        let wheel_center = to_rapier(planet.center) + normal * wheel_surface_distance;
        let anchor_to_center = spec.suspension_rest_length - spec.suspension_anchor_height;
        let chassis_center = wheel_center + normal * anchor_to_center;

        let chassis = RigidBodyBuilder::dynamic()
            .translation(chassis_center)
            .rotation(tangent_angle)
            .linear_damping(0.05)
            .angular_damping(0.25)
            .ccd_enabled(true)
            .additional_solver_iterations(4)
            .user_data(encode_entity(PhysicsEntity::RoverChassis(id)))
            .build();
        let chassis_collider =
            ColliderBuilder::cuboid(spec.chassis_half_width, spec.chassis_half_height)
                .density(1.0)
                .friction(0.7)
                .restitution(0.0)
                .user_data(encode_entity(PhysicsEntity::RoverChassis(id)))
                .build();
        let (chassis, chassis_collider) = self.world.insert(chassis, chassis_collider);

        let suspension_anchors = [
            Vector::new(-spec.wheel_offset, spec.suspension_anchor_height),
            Vector::new(spec.wheel_offset, spec.suspension_anchor_height),
        ];
        let local_suspension_axis = Vector::new(0.0, -1.0);
        let tangent = Vector::new(tangent_angle.cos(), tangent_angle.sin());
        let mut wheels = [RigidBodyHandle::invalid(); 2];
        let mut wheel_colliders = [ColliderHandle::invalid(); 2];
        let mut suspension_joints = [ImpulseJointHandle::invalid(); 2];

        for index in 0..2 {
            let wheel_position = wheel_center
                + tangent
                    * if index == 0 {
                        -spec.wheel_offset
                    } else {
                        spec.wheel_offset
                    };
            let wheel = RigidBodyBuilder::dynamic()
                .translation(wheel_position)
                .rotation(tangent_angle)
                .linear_damping(0.02)
                .angular_damping(0.01)
                .ccd_enabled(true)
                .additional_solver_iterations(4)
                .user_data(encode_entity(PhysicsEntity::RoverWheel {
                    rover_id: id,
                    wheel: index,
                }))
                .build();
            let wheel_collider = ColliderBuilder::ball(spec.wheel_radius)
                .density(0.8)
                .friction(1.6)
                .restitution(0.0)
                .user_data(encode_entity(PhysicsEntity::RoverWheel {
                    rover_id: id,
                    wheel: index,
                }))
                .build();
            let (wheel, wheel_collider) = self.world.insert(wheel, wheel_collider);

            let mut suspension = PinSlotJointBuilder::new(local_suspension_axis)
                .local_anchor1(suspension_anchors[index])
                .local_anchor2(Vector::ZERO)
                .limits([
                    spec.suspension_rest_length - spec.suspension_travel,
                    spec.suspension_rest_length + spec.suspension_travel,
                ])
                .motor_position(
                    spec.suspension_rest_length,
                    spec.suspension_stiffness,
                    spec.suspension_damping,
                )
                .motor_max_force(spec.suspension_max_force)
                .contacts_enabled(false)
                .build();
            suspension
                .data
                .set_motor_model(JointAxis::LinX, MotorModel::ForceBased);
            let joint = self.world.insert_impulse_joint(chassis, wheel, suspension);

            wheels[index] = wheel;
            wheel_colliders[index] = wheel_collider;
            suspension_joints[index] = joint;
        }

        self.rovers.insert(
            id,
            RoverBodies {
                chassis,
                chassis_collider,
                wheels,
                wheel_colliders,
                suspension_joints,
                suspension_anchors,
                target_wheel_speed: spec.wheel_target_speed,
                motor_torque: spec.wheel_motor_torque,
                brake_torque: spec.wheel_brake_torque,
            },
        );
        true
    }

    /// Translate driving intent into Rapier joint-motor targets.
    pub fn set_rover_control(&mut self, id: u64, control: RoverControl) -> bool {
        let Some(rover) = self.rovers.get(&id) else {
            return false;
        };
        let throttle = if control.throttle.is_finite() {
            control.throttle.clamp(-1.0, 1.0)
        } else {
            0.0
        };

        for joint_handle in rover.suspension_joints {
            let Some(joint) = self.world.impulse_joints.get_mut(joint_handle, true) else {
                continue;
            };
            if control.brake {
                joint
                    .data
                    .set_motor_velocity(JointAxis::AngX, 0.0, 1.0)
                    .set_motor_max_force(JointAxis::AngX, rover.brake_torque);
                joint
                    .data
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased);
            } else if throttle.abs() > f32::EPSILON {
                joint
                    .data
                    .set_motor_velocity(JointAxis::AngX, -throttle * rover.target_wheel_speed, 1.0)
                    .set_motor_max_force(JointAxis::AngX, rover.motor_torque);
                joint
                    .data
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased);
            } else {
                joint.data.motor_axes.remove(JointAxesMask::ANG_X);
            }
        }
        true
    }

    /// Apply the caller's acceleration field as `mass × acceleration`, then
    /// advance Rapier exactly once.
    pub fn step(&mut self, dt_seconds: f32, acceleration_at: impl Fn(Vec2) -> Vec2) {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return;
        }
        self.world.integration_parameters.dt = dt_seconds;

        let handles = self
            .rovers
            .values()
            .flat_map(|rover| std::iter::once(rover.chassis).chain(rover.wheels))
            .collect::<Vec<_>>();
        for handle in handles {
            let Some(body) = self.world.bodies.get_mut(handle) else {
                continue;
            };
            let acceleration = acceleration_at(from_rapier(body.translation()));
            let acceleration = if acceleration.x.is_finite() && acceleration.y.is_finite() {
                acceleration
            } else {
                Vec2::ZERO
            };
            body.reset_forces(false);
            body.add_force(to_rapier(acceleration) * body.mass(), true);
        }

        self.world.step();
    }

    pub fn rover_snapshot(&self, id: u64) -> Option<RoverSnapshot> {
        let rover = self.rovers.get(&id)?;
        let chassis = self.world.bodies.get(rover.chassis)?;
        let wheel_bodies = [
            self.world.bodies.get(rover.wheels[0])?,
            self.world.bodies.get(rover.wheels[1])?,
        ];
        let anchors = rover
            .suspension_anchors
            .map(|anchor| from_rapier(chassis.position().transform_point(anchor)));

        let mut contacts = Vec::new();
        for collider in rover
            .wheel_colliders
            .into_iter()
            .chain(std::iter::once(rover.chassis_collider))
        {
            for pair in self.world.contact_pairs_with(collider) {
                for manifold in &pair.manifolds {
                    for contact in &manifold.data.solver_contacts {
                        contacts.push(ContactPoint {
                            position: from_rapier(contact.point),
                            normal: from_rapier(manifold.data.normal),
                        });
                    }
                }
            }
        }

        Some(RoverSnapshot {
            chassis: body_motion(chassis),
            wheels: [body_motion(wheel_bodies[0]), body_motion(wheel_bodies[1])],
            suspension_anchors: anchors,
            contacts,
        })
    }

    pub fn remove_rover(&mut self, id: u64) -> bool {
        let Some(rover) = self.rovers.remove(&id) else {
            return false;
        };
        // Removing the chassis also removes its attached impulse joints.
        self.world.remove_body(rover.chassis);
        for wheel in rover.wheels {
            self.world.remove_body(wheel);
        }
        true
    }

    pub fn entity_for_user_data(user_data: u128) -> Option<PhysicsEntity> {
        decode_entity(user_data)
    }
}

fn body_motion(body: &RigidBody) -> BodyMotion {
    BodyMotion {
        position: from_rapier(body.translation()),
        angle: body.rotation().angle(),
        linear_velocity: from_rapier(body.linvel()),
        angular_velocity: body.angvel(),
    }
}

fn valid_planet_spec(spec: PlanetSpec) -> bool {
    spec.center.x.is_finite()
        && spec.center.y.is_finite()
        && spec.radius.is_finite()
        && spec.radius > 0.0
        && spec.angle.is_finite()
}

fn valid_bump_spec(spec: BumpSpec) -> bool {
    spec.surface_angle.is_finite()
        && spec.half_width.is_finite()
        && spec.half_width > 0.0
        && spec.half_height.is_finite()
        && spec.half_height > 0.0
}

fn valid_rover_spec(spec: RoverSpec) -> bool {
    [
        spec.chassis_half_width,
        spec.chassis_half_height,
        spec.wheel_radius,
        spec.wheel_offset,
        spec.suspension_rest_length,
        spec.suspension_travel,
        spec.suspension_stiffness,
        spec.suspension_damping,
        spec.suspension_max_force,
        spec.wheel_target_speed,
        spec.wheel_motor_torque,
        spec.wheel_brake_torque,
    ]
    .into_iter()
    .all(|value| value.is_finite() && value > 0.0)
        && spec.suspension_anchor_height.is_finite()
        && spec.surface_angle.is_finite()
        && spec.suspension_travel < spec.suspension_rest_length
}

fn finite_positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn normalized_ball_bounds(bounds: BallPhysicsBounds) -> BallPhysicsBounds {
    BallPhysicsBounds {
        width: finite_positive_or(bounds.width, 1.0),
        height: finite_positive_or(bounds.height, 1.0),
    }
}

fn finite_vec2_or_zero(value: Vec2) -> Vec2 {
    if value.x.is_finite() && value.y.is_finite() {
        value
    } else {
        Vec2::ZERO
    }
}

fn valid_ball_spec(spec: BallBodySpec) -> bool {
    spec.id != 0
        && spec.position.x.is_finite()
        && spec.position.y.is_finite()
        && spec.velocity.x.is_finite()
        && spec.velocity.y.is_finite()
        && spec.radius.is_finite()
        && spec.radius > 0.0
        && spec.density.is_finite()
        && spec.density > 0.0
        && spec.restitution.is_finite()
}

fn encode_entity(entity: PhysicsEntity) -> u128 {
    match entity {
        PhysicsEntity::Planet(id) => (PLANET_ENTITY_TAG << ENTITY_TAG_SHIFT) | id as u128,
        PhysicsEntity::RoverChassis(id) => {
            (ROVER_CHASSIS_ENTITY_TAG << ENTITY_TAG_SHIFT) | id as u128
        }
        PhysicsEntity::RoverWheel { rover_id, wheel } => {
            (ROVER_WHEEL_ENTITY_TAG << ENTITY_TAG_SHIFT)
                | ((wheel as u128) << WHEEL_INDEX_SHIFT)
                | rover_id as u128
        }
    }
}

fn decode_entity(user_data: u128) -> Option<PhysicsEntity> {
    let tag = user_data >> ENTITY_TAG_SHIFT;
    let id = user_data as u64;
    match tag {
        PLANET_ENTITY_TAG => Some(PhysicsEntity::Planet(id)),
        ROVER_CHASSIS_ENTITY_TAG => Some(PhysicsEntity::RoverChassis(id)),
        ROVER_WHEEL_ENTITY_TAG => Some(PhysicsEntity::RoverWheel {
            rover_id: id,
            wheel: ((user_data >> WHEEL_INDEX_SHIFT) & 0xff) as usize,
        }),
        _ => None,
    }
}

fn to_rapier(value: Vec2) -> Vector {
    Vector::new(value.x, value.y)
}

fn from_rapier(value: Vector) -> Vec2 {
    Vec2::new(value.x, value.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLANET_ID: u64 = 10;
    const ROVER_ID: u64 = 20;

    fn ball_spec(id: u64, position: Vec2) -> BallBodySpec {
        BallBodySpec {
            id,
            position,
            velocity: Vec2::ZERO,
            radius: 0.05,
            density: 1.0,
            restitution: 0.9,
        }
    }

    fn test_world() -> (RoverPhysics, PlanetSpec) {
        test_world_with_bumps(&[])
    }

    fn test_world_with_bumps(bumps: &[BumpSpec]) -> (RoverPhysics, PlanetSpec) {
        let planet = PlanetSpec {
            center: Vec2::ZERO,
            radius: 20.0,
            angle: 0.0,
        };
        let mut physics = RoverPhysics::new(1.0);
        assert!(physics.insert_planet(PLANET_ID, planet, bumps));
        assert!(physics.insert_rover(ROVER_ID, PLANET_ID, planet, RoverSpec::default()));
        (physics, planet)
    }

    fn gravity(position: Vec2) -> Vec2 {
        let distance = position.length();
        if distance > f32::EPSILON {
            position * (-18.0 / distance)
        } else {
            Vec2::ZERO
        }
    }

    fn driven_suspension_excursion(mut physics: RoverPhysics) -> (f32, RoverSnapshot) {
        for _ in 0..240 {
            physics.set_rover_control(ROVER_ID, RoverControl::default());
            physics.step(1.0 / 60.0, gravity);
        }

        let mut minimum_length = f32::INFINITY;
        let mut maximum_length = 0.0_f32;
        for _ in 0..900 {
            physics.set_rover_control(
                ROVER_ID,
                RoverControl {
                    throttle: 0.7,
                    brake: false,
                },
            );
            physics.step(1.0 / 60.0, gravity);
            let snapshot = physics.rover_snapshot(ROVER_ID).unwrap();
            for index in 0..2 {
                let length =
                    snapshot.suspension_anchors[index].distance_to(snapshot.wheels[index].position);
                minimum_length = minimum_length.min(length);
                maximum_length = maximum_length.max(length);
            }
        }
        (
            maximum_length - minimum_length,
            physics.rover_snapshot(ROVER_ID).unwrap(),
        )
    }

    #[test]
    fn entity_user_data_round_trips() {
        let entities = [
            PhysicsEntity::Planet(42),
            PhysicsEntity::RoverChassis(98),
            PhysicsEntity::RoverWheel {
                rover_id: 120,
                wheel: 1,
            },
        ];
        for entity in entities {
            assert_eq!(
                RoverPhysics::entity_for_user_data(encode_entity(entity)),
                Some(entity)
            );
        }
    }

    #[test]
    fn bulk_ball_world_keeps_dense_motion_order_through_removal() {
        let mut physics = BallPhysics::new(
            BallPhysicsBounds {
                width: 1.0,
                height: 0.6,
            },
            Vec2::ZERO,
            false,
            4,
        );
        physics.reserve(3);
        assert!(physics.insert_ball(ball_spec(1, Vec2::new(0.2, 0.3))));
        assert!(physics.insert_ball(ball_spec(2, Vec2::new(0.4, 0.3))));
        assert!(physics.insert_ball(ball_spec(3, Vec2::new(0.6, 0.3))));
        assert_eq!(
            physics
                .motions()
                .map(|motion| motion.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        assert_eq!(physics.swap_remove_ball(1), Some(2));
        assert_eq!(
            physics
                .motions()
                .map(|motion| motion.id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(physics.remove_ball(1));
        assert!(!physics.remove_ball(99));
        assert_eq!(physics.len(), 1);
    }

    #[test]
    fn bulk_ball_world_reports_contacts_and_finite_motion() {
        let mut physics = BallPhysics::new(
            BallPhysicsBounds {
                width: 1.0,
                height: 0.6,
            },
            Vec2::new(0.0, 0.5),
            false,
            4,
        );
        assert!(physics.insert_ball(ball_spec(1, Vec2::new(0.48, 0.2))));
        assert!(physics.insert_ball(ball_spec(2, Vec2::new(0.52, 0.2))));

        let metrics = physics.step(1.0 / 60.0);
        assert_eq!(metrics.active_bodies, 2);
        assert!(metrics.contact_pairs >= 1);
        for motion in physics.motions() {
            assert!(motion.position.x.is_finite());
            assert!(motion.position.y.is_finite());
        }
    }

    #[test]
    fn rover_settles_on_the_planet_without_a_second_integrator() {
        let (mut physics, _) = test_world();
        for _ in 0..600 {
            physics.set_rover_control(ROVER_ID, RoverControl::default());
            physics.step(1.0 / 60.0, gravity);
        }

        let snapshot = physics.rover_snapshot(ROVER_ID).unwrap();
        for motion in std::iter::once(snapshot.chassis).chain(snapshot.wheels) {
            assert!(motion.position.x.is_finite());
            assert!(motion.position.y.is_finite());
            assert!(motion.linear_velocity.length() < 1.0);
        }
        for wheel in snapshot.wheels {
            let radius = wheel.position.length();
            assert!((20.6..22.0).contains(&radius), "wheel radius was {radius}");
        }
    }

    #[test]
    fn identical_commands_produce_identical_state() {
        let (mut first, _) = test_world();
        let (mut second, _) = test_world();
        for tick in 0..900 {
            let control = RoverControl {
                throttle: if tick < 600 { 0.65 } else { 0.0 },
                brake: tick >= 720,
            };
            first.set_rover_control(ROVER_ID, control);
            second.set_rover_control(ROVER_ID, control);
            first.step(1.0 / 60.0, gravity);
            second.step(1.0 / 60.0, gravity);
        }

        assert_eq!(
            first.rover_snapshot(ROVER_ID).unwrap(),
            second.rover_snapshot(ROVER_ID).unwrap()
        );
    }

    #[test]
    fn snapshot_round_trip_resumes_with_equivalent_behavior() {
        let (mut original, _) = test_world();
        for _ in 0..360 {
            original.set_rover_control(
                ROVER_ID,
                RoverControl {
                    throttle: 0.6,
                    brake: false,
                },
            );
            original.step(1.0 / 60.0, gravity);
        }

        let bytes = original.snapshot_bytes().unwrap();
        let mut restored = RoverPhysics::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(
            original.rover_snapshot(ROVER_ID),
            restored.rover_snapshot(ROVER_ID)
        );

        for tick in 0..360 {
            let control = RoverControl {
                throttle: if tick < 180 { -0.4 } else { 0.0 },
                brake: tick >= 240,
            };
            original.set_rover_control(ROVER_ID, control);
            restored.set_rover_control(ROVER_ID, control);
            original.step(1.0 / 60.0, gravity);
            restored.step(1.0 / 60.0, gravity);
        }
        assert_eq!(
            original.rover_snapshot(ROVER_ID),
            restored.rover_snapshot(ROVER_ID)
        );
    }

    #[test]
    fn wheel_motors_drive_the_rover_around_the_curved_surface() {
        let (mut physics, _) = test_world();
        for _ in 0..240 {
            physics.set_rover_control(ROVER_ID, RoverControl::default());
            physics.step(1.0 / 60.0, gravity);
        }
        let start = physics.rover_snapshot(ROVER_ID).unwrap().chassis.position;
        let start_angle = start.y.atan2(start.x);

        for _ in 0..600 {
            physics.set_rover_control(
                ROVER_ID,
                RoverControl {
                    throttle: 0.7,
                    brake: false,
                },
            );
            physics.step(1.0 / 60.0, gravity);
        }

        let end = physics.rover_snapshot(ROVER_ID).unwrap().chassis.position;
        let angular_travel = (end.y.atan2(end.x) - start_angle).abs();
        assert!(
            angular_travel > 0.2,
            "rover only traveled {angular_travel} radians"
        );
        assert!(
            (20.0..25.0).contains(&end.length()),
            "rover radius was {}",
            end.length()
        );
    }

    #[test]
    fn suspension_compresses_and_recovers_over_a_surface_bump() {
        let bump = BumpSpec {
            surface_angle: 1.08,
            half_width: 1.25,
            half_height: 0.32,
        };
        let (flat_physics, _) = test_world();
        let (bump_physics, _) = test_world_with_bumps(&[bump]);
        let (flat_excursion, _) = driven_suspension_excursion(flat_physics);
        let (bump_excursion, snapshot) = driven_suspension_excursion(bump_physics);

        assert!(
            bump_excursion > flat_excursion + 0.04,
            "bump excursion {bump_excursion} was too close to flat excursion {flat_excursion}"
        );
        assert!((20.0..25.0).contains(&snapshot.chassis.position.length()));
    }

    #[test]
    fn rover_completes_a_full_bumped_circumference_without_escaping() {
        let bump = BumpSpec {
            surface_angle: 1.08,
            half_width: 1.25,
            half_height: 0.32,
        };
        let (mut physics, _) = test_world_with_bumps(&[bump]);
        for _ in 0..240 {
            physics.set_rover_control(ROVER_ID, RoverControl::default());
            physics.step(1.0 / 60.0, gravity);
        }

        let start = physics.rover_snapshot(ROVER_ID).unwrap().chassis.position;
        let mut previous_angle = start.y.atan2(start.x);
        let mut accumulated_angle = 0.0_f32;
        for _ in 0..3600 {
            physics.set_rover_control(
                ROVER_ID,
                RoverControl {
                    throttle: 0.7,
                    brake: false,
                },
            );
            physics.step(1.0 / 60.0, gravity);
            let position = physics.rover_snapshot(ROVER_ID).unwrap().chassis.position;
            assert!(
                (20.0..25.0).contains(&position.length()),
                "rover radius was {}",
                position.length()
            );
            let angle = position.y.atan2(position.x);
            let delta = (angle - previous_angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            accumulated_angle += delta;
            previous_angle = angle;
        }

        let final_snapshot = physics.rover_snapshot(ROVER_ID).unwrap();
        let suspension_lengths = [0, 1].map(|index| {
            final_snapshot.suspension_anchors[index]
                .distance_to(final_snapshot.wheels[index].position)
        });
        assert!(
            accumulated_angle.abs() > std::f32::consts::TAU,
            "rover only traveled {accumulated_angle} radians: {final_snapshot:?}, suspension lengths: {suspension_lengths:?}"
        );
    }

    #[test]
    fn removing_a_rover_removes_its_articulated_bodies() {
        let (mut physics, _) = test_world();
        assert!(physics.remove_rover(ROVER_ID));
        assert!(physics.rover_snapshot(ROVER_ID).is_none());
        assert!(!physics.remove_rover(ROVER_ID));
    }
}
