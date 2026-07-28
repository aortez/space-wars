//! Canonical Rapier-backed mechanics world.
//!
//! This module owns every raw Rapier handle. Callers identify physical
//! entities and their parts with stable engine IDs and read authoritative
//! motion or normalized contact data after each step.

use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

use engine_core::Vec2;
use rapier2d::prelude::{
    BroadPhaseBvh, CCDSolver, Collider, ColliderBuilder, ColliderHandle, ColliderSet, GenericJoint,
    Group, ImpulseJoint, ImpulseJointHandle, ImpulseJointSet, IntegrationParameters,
    InteractionGroups, InteractionTestMode, IslandManager, MultibodyJointSet, NarrowPhase,
    PhysicsPipeline, PhysicsWorld as RapierWorld, Pose, QueryFilter, QueryFilterFlags, Ray,
    RigidBody, RigidBodyBuilder, RigidBodyHandle, RigidBodySet, RigidBodyType, Vector,
};
use serde::{Deserialize, Serialize};

const SNAPSHOT_VERSION: u32 = 1;
const USER_DATA_COLLIDER_TAG: u128 = 0x43;
const USER_DATA_BODY_TAG: u128 = 0x42;
const USER_DATA_ROLE_SHIFT: u32 = 64;
const USER_DATA_PART_SHIFT: u32 = 96;
const USER_DATA_TAG_SHIFT: u32 = 112;

/// Stable scenario-selected identity for one physical entity or assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PhysicsId(u64);

impl PhysicsId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Semantic role for one rigid body within an entity assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BodyRole(u32);

impl BodyRole {
    pub const PRIMARY: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Semantic role for a collider. Several collider parts may share one role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ColliderRole(u32);

impl ColliderRole {
    pub const PRIMARY: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Semantic role for a joint within an entity assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JointRole(u32);

impl JointRole {
    pub const PRIMARY: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BodyId {
    pub entity: PhysicsId,
    pub role: BodyRole,
}

impl BodyId {
    pub const fn new(entity: PhysicsId, role: BodyRole) -> Self {
        Self { entity, role }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ColliderId {
    pub entity: PhysicsId,
    pub role: ColliderRole,
    pub part: u16,
}

impl ColliderId {
    pub const fn new(entity: PhysicsId, role: ColliderRole, part: u16) -> Self {
        Self { entity, role, part }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JointId {
    pub entity: PhysicsId,
    pub role: JointRole,
}

impl JointId {
    pub const fn new(entity: PhysicsId, role: JointRole) -> Self {
        Self { entity, role }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Dynamic,
    Fixed,
    KinematicPosition,
    KinematicVelocity,
}

impl BodyKind {
    fn to_rapier(self) -> RigidBodyType {
        match self {
            Self::Dynamic => RigidBodyType::Dynamic,
            Self::Fixed => RigidBodyType::Fixed,
            Self::KinematicPosition => RigidBodyType::KinematicPositionBased,
            Self::KinematicVelocity => RigidBodyType::KinematicVelocityBased,
        }
    }
}

/// Engine-native initial state and integration settings for one body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySpec {
    pub kind: BodyKind,
    pub position: Vec2,
    pub angle: f32,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub can_sleep: bool,
    pub ccd_enabled: bool,
    pub additional_solver_iterations: usize,
}

impl Default for BodySpec {
    fn default() -> Self {
        Self {
            kind: BodyKind::Dynamic,
            position: Vec2::ZERO,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity_scale: 1.0,
            can_sleep: true,
            ccd_enabled: false,
            additional_solver_iterations: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColliderShape {
    Ball { radius: f32 },
    Cuboid { half_width: f32, half_height: f32 },
    ConvexPolygon { vertices: Vec<Vec2> },
    Polyline { vertices: Vec<Vec2> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionGroups {
    pub memberships: u32,
    pub filter: u32,
}

impl CollisionGroups {
    pub const ALL: Self = Self {
        memberships: u32::MAX,
        filter: u32::MAX,
    };

    pub const NONE: Self = Self {
        memberships: 0,
        filter: 0,
    };

    pub const fn new(memberships: u32, filter: u32) -> Self {
        Self {
            memberships,
            filter,
        }
    }

    fn to_rapier(self) -> InteractionGroups {
        InteractionGroups::new(
            Group::from_bits_retain(self.memberships),
            Group::from_bits_retain(self.filter),
            InteractionTestMode::And,
        )
    }
}

impl Default for CollisionGroups {
    fn default() -> Self {
        Self::ALL
    }
}

/// One collider attached to the body passed to [`PhysicsWorld::insert_body`]
/// or [`PhysicsWorld::insert_collider`].
#[derive(Debug, Clone, PartialEq)]
pub struct ColliderSpec {
    pub id: ColliderId,
    pub shape: ColliderShape,
    pub local_position: Vec2,
    pub local_angle: f32,
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub sensor: bool,
    pub collision_groups: CollisionGroups,
    pub solver_groups: CollisionGroups,
}

impl ColliderSpec {
    pub fn ball(id: ColliderId, radius: f32) -> Self {
        Self {
            id,
            shape: ColliderShape::Ball { radius },
            local_position: Vec2::ZERO,
            local_angle: 0.0,
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            collision_groups: CollisionGroups::ALL,
            solver_groups: CollisionGroups::ALL,
        }
    }

    pub fn cuboid(id: ColliderId, half_width: f32, half_height: f32) -> Self {
        Self {
            id,
            shape: ColliderShape::Cuboid {
                half_width,
                half_height,
            },
            local_position: Vec2::ZERO,
            local_angle: 0.0,
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            collision_groups: CollisionGroups::ALL,
            solver_groups: CollisionGroups::ALL,
        }
    }

    pub fn convex_polygon(id: ColliderId, vertices: Vec<Vec2>) -> Self {
        Self {
            id,
            shape: ColliderShape::ConvexPolygon { vertices },
            local_position: Vec2::ZERO,
            local_angle: 0.0,
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            collision_groups: CollisionGroups::ALL,
            solver_groups: CollisionGroups::ALL,
        }
    }

    pub fn polyline(id: ColliderId, vertices: Vec<Vec2>) -> Self {
        Self {
            id,
            shape: ColliderShape::Polyline { vertices },
            local_position: Vec2::ZERO,
            local_angle: 0.0,
            density: 0.0,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            collision_groups: CollisionGroups::ALL,
            solver_groups: CollisionGroups::ALL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsWorldConfig {
    pub gravity: Vec2,
    pub length_unit: f32,
    pub solver_iterations: usize,
    pub internal_stabilization_iterations: usize,
    pub max_ccd_substeps: usize,
    pub collect_events: bool,
}

impl Default for PhysicsWorldConfig {
    fn default() -> Self {
        Self {
            gravity: Vec2::ZERO,
            length_unit: 1.0,
            solver_iterations: 4,
            internal_stabilization_iterations: 1,
            max_ccd_substeps: 1,
            collect_events: true,
        }
    }
}

/// Authoritative motion read directly from one Rapier rigid body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyMotion {
    pub position: Vec2,
    pub angle: f32,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyMotionRecord {
    pub id: BodyId,
    pub motion: BodyMotion,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    pub position: Vec2,
    pub normal: Vec2,
}

/// One active, solver-backed contact pair normalized to stable collider order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactEvent {
    pub collider_a: ColliderId,
    pub collider_b: ColliderId,
    pub point: Option<Vec2>,
    /// Normal and impulse point from `collider_a` toward `collider_b`.
    pub normal: Vec2,
    pub impulse: Vec2,
    pub impulse_magnitude: f32,
    pub solver_contacts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorIntersection {
    pub collider_a: ColliderId,
    pub collider_b: ColliderId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayCastOptions {
    pub max_distance: f32,
    pub solid: bool,
    pub include_sensors: bool,
    pub collision_groups: CollisionGroups,
    pub exclude_entity: Option<PhysicsId>,
}

impl Default for RayCastOptions {
    fn default() -> Self {
        Self {
            max_distance: f32::MAX,
            solid: true,
            include_sensors: false,
            collision_groups: CollisionGroups::ALL,
            exclude_entity: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub collider: ColliderId,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
}

/// Per-step timings and population counters from the canonical world.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PhysicsStepMetrics {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsWorldError(String);

impl std::fmt::Display for PhysicsWorldError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PhysicsWorldError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct BodyEntry {
    id: BodyId,
    handle: RigidBodyHandle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ColliderEntry {
    id: ColliderId,
    parent: BodyId,
    handle: ColliderHandle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct JointEntry {
    id: JointId,
    body_a: BodyId,
    body_b: BodyId,
    handle: ImpulseJointHandle,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EntityRecord {
    bodies: Vec<BodyId>,
    colliders: Vec<ColliderId>,
    joints: Vec<JointId>,
}

/// Canonical application mechanics world.
pub struct PhysicsWorld {
    pub(crate) raw: RapierWorld,
    bodies: Vec<BodyEntry>,
    body_indices: HashMap<BodyId, usize>,
    colliders: Vec<ColliderEntry>,
    collider_indices: HashMap<ColliderId, usize>,
    joints: Vec<JointEntry>,
    joint_indices: HashMap<JointId, usize>,
    entities: BTreeMap<PhysicsId, EntityRecord>,
    contact_events: Vec<ContactEvent>,
    sensor_intersections: Vec<SensorIntersection>,
    collect_events: bool,
}

impl Clone for PhysicsWorld {
    fn clone(&self) -> Self {
        let snapshot = self
            .snapshot_bytes()
            .expect("an in-memory physics world must serialize");
        let mut clone =
            Self::from_snapshot_bytes(&snapshot).expect("a same-build physics snapshot must load");
        if clone.collect_events {
            clone.rebuild_events();
        }
        clone
    }
}

impl std::fmt::Debug for PhysicsWorld {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PhysicsWorld")
            .field("entities", &self.entities.len())
            .field("bodies", &self.bodies.len())
            .field("colliders", &self.colliders.len())
            .field("joints", &self.joints.len())
            .field("contacts", &self.contact_events.len())
            .field("sensor_intersections", &self.sensor_intersections.len())
            .field("collect_events", &self.collect_events)
            .finish()
    }
}

#[derive(Serialize)]
struct PhysicsSnapshotRef<'a> {
    version: u32,
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
    body_entries: &'a [BodyEntry],
    collider_entries: &'a [ColliderEntry],
    joint_entries: &'a [JointEntry],
    entities: &'a BTreeMap<PhysicsId, EntityRecord>,
    collect_events: bool,
}

#[derive(Deserialize)]
struct PhysicsSnapshot {
    version: u32,
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
    body_entries: Vec<BodyEntry>,
    collider_entries: Vec<ColliderEntry>,
    joint_entries: Vec<JointEntry>,
    entities: BTreeMap<PhysicsId, EntityRecord>,
    collect_events: bool,
}

impl PhysicsWorld {
    pub fn new(config: PhysicsWorldConfig) -> Self {
        let config = normalized_world_config(config);
        let mut raw = RapierWorld {
            gravity: to_rapier(config.gravity),
            ..RapierWorld::default()
        };
        raw.integration_parameters.dt = 1.0 / 60.0;
        raw.integration_parameters.length_unit = config.length_unit;
        raw.integration_parameters.num_solver_iterations = config.solver_iterations;
        raw.integration_parameters
            .num_internal_stabilization_iterations = config.internal_stabilization_iterations;
        raw.integration_parameters.max_ccd_substeps = config.max_ccd_substeps;

        Self {
            raw,
            bodies: Vec::new(),
            body_indices: HashMap::new(),
            colliders: Vec::new(),
            collider_indices: HashMap::new(),
            joints: Vec::new(),
            joint_indices: HashMap::new(),
            entities: BTreeMap::new(),
            contact_events: Vec::new(),
            sensor_intersections: Vec::new(),
            collect_events: config.collect_events,
        }
    }

    pub fn reserve(&mut self, bodies: usize, colliders: usize, joints: usize) {
        self.bodies.reserve(bodies);
        self.body_indices.reserve(bodies);
        self.colliders.reserve(colliders);
        self.collider_indices.reserve(colliders);
        self.joints.reserve(joints);
        self.joint_indices.reserve(joints);
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn contains_entity(&self, id: PhysicsId) -> bool {
        self.entities.contains_key(&id)
    }

    pub fn contains_body(&self, id: BodyId) -> bool {
        self.body_indices.contains_key(&id)
    }

    pub fn insert_body(&mut self, id: BodyId, spec: BodySpec, colliders: &[ColliderSpec]) -> bool {
        if self.body_indices.contains_key(&id)
            || !valid_body_spec(spec)
            || colliders
                .iter()
                .any(|collider| collider.id.entity != id.entity)
        {
            return false;
        }

        let mut collider_builders = Vec::with_capacity(colliders.len());
        for collider in colliders {
            if self.collider_indices.contains_key(&collider.id)
                || colliders
                    .iter()
                    .filter(|candidate| candidate.id == collider.id)
                    .count()
                    != 1
            {
                return false;
            }
            let Some(builder) = build_collider(collider) else {
                return false;
            };
            collider_builders.push((collider.id, builder));
        }

        let body = build_body(id, spec);
        let handle = self.raw.insert_body(body);
        let body_index = self.bodies.len();
        self.bodies.push(BodyEntry { id, handle });
        self.body_indices.insert(id, body_index);
        self.entities.entry(id.entity).or_default().bodies.push(id);

        for (collider_id, collider) in collider_builders {
            let collider_handle = self.raw.insert_collider(collider, Some(handle));
            self.register_collider(collider_id, id, collider_handle);
        }
        true
    }

    pub fn insert_collider(&mut self, parent: BodyId, spec: &ColliderSpec) -> bool {
        if spec.id.entity != parent.entity || self.collider_indices.contains_key(&spec.id) {
            return false;
        }
        let Some(parent_handle) = self.body_handle(parent) else {
            return false;
        };
        let Some(collider) = build_collider(spec) else {
            return false;
        };
        let handle = self.raw.insert_collider(collider, Some(parent_handle));
        self.register_collider(spec.id, parent, handle);
        true
    }

    pub fn remove_entity(&mut self, entity: PhysicsId) -> bool {
        let Some(record) = self.entities.get(&entity).cloned() else {
            return false;
        };

        for joint in record.joints {
            self.remove_joint(joint);
        }
        for body in record.bodies {
            if let Some(handle) = self.body_handle(body) {
                self.raw.remove_body(handle);
            }
            self.remove_body_mapping(body);
        }
        for collider in record.colliders {
            self.remove_collider_mapping(collider);
        }
        self.entities.remove(&entity);
        true
    }

    pub fn motion(&self, id: BodyId) -> Option<BodyMotion> {
        let handle = self.body_handle(id)?;
        self.raw.bodies.get(handle).map(body_motion)
    }

    pub fn motions(&self) -> impl ExactSizeIterator<Item = BodyMotionRecord> + '_ {
        self.bodies.iter().map(|entry| BodyMotionRecord {
            id: entry.id,
            motion: body_motion(&self.raw.bodies[entry.handle]),
        })
    }

    pub fn body_mass(&self, id: BodyId) -> Option<f32> {
        let handle = self.body_handle(id)?;
        self.raw.bodies.get(handle).map(RigidBody::mass)
    }

    pub fn set_body_kind(&mut self, id: BodyId, kind: BodyKind, wake_up: bool) -> bool {
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.set_body_type(kind.to_rapier(), wake_up);
        true
    }

    pub fn set_pose(&mut self, id: BodyId, position: Vec2, angle: f32, wake_up: bool) -> bool {
        if !finite_vec2(position) || !angle.is_finite() {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.set_position(Pose::new(to_rapier(position), angle), wake_up);
        true
    }

    pub fn set_next_kinematic_pose(&mut self, id: BodyId, position: Vec2, angle: f32) -> bool {
        if !finite_vec2(position) || !angle.is_finite() {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.set_next_kinematic_position(Pose::new(to_rapier(position), angle));
        true
    }

    pub fn set_velocity(
        &mut self,
        id: BodyId,
        linear_velocity: Vec2,
        angular_velocity: f32,
        wake_up: bool,
    ) -> bool {
        if !finite_vec2(linear_velocity) || !angular_velocity.is_finite() {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.set_linvel(to_rapier(linear_velocity), wake_up);
        body.set_angvel(angular_velocity, wake_up);
        true
    }

    pub fn clear_forces(&mut self) {
        let bodies = &self.bodies;
        let raw_bodies = &mut self.raw.bodies;
        for entry in bodies {
            if let Some(body) = raw_bodies.get_mut(entry.handle) {
                body.reset_forces(false);
                body.reset_torques(false);
            }
        }
    }

    pub fn apply_force(&mut self, id: BodyId, force: Vec2, wake_up: bool) -> bool {
        if !finite_vec2(force) {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.add_force(to_rapier(force), wake_up);
        true
    }

    pub fn apply_acceleration(&mut self, id: BodyId, acceleration: Vec2, wake_up: bool) -> bool {
        if !finite_vec2(acceleration) {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        let force = to_rapier(acceleration) * body.mass();
        body.add_force(force, wake_up);
        true
    }

    pub fn apply_impulse(&mut self, id: BodyId, impulse: Vec2, wake_up: bool) -> bool {
        if !finite_vec2(impulse) {
            return false;
        }
        let Some(body) = self.body_mut(id) else {
            return false;
        };
        body.apply_impulse(to_rapier(impulse), wake_up);
        true
    }

    pub fn set_gravity(&mut self, gravity: Vec2) -> bool {
        if !finite_vec2(gravity) {
            return false;
        }
        self.raw.gravity = to_rapier(gravity);
        true
    }

    pub fn gravity(&self) -> Vec2 {
        from_rapier(self.raw.gravity)
    }

    pub fn step(&mut self, dt_seconds: f32) -> PhysicsStepMetrics {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            self.contact_events.clear();
            self.sensor_intersections.clear();
            return PhysicsStepMetrics::default();
        }
        self.raw.integration_parameters.dt = dt_seconds;

        let started = Instant::now();
        self.raw.step();
        let wall_time = started.elapsed();
        let counters = self.raw.physics_pipeline.counters;

        let mut candidate_pairs = 0;
        let mut contact_pairs = 0;
        let mut contacts = 0;
        for pair in self.raw.contact_pairs() {
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

        if self.collect_events {
            self.rebuild_events();
        } else {
            self.contact_events.clear();
            self.sensor_intersections.clear();
        }

        PhysicsStepMetrics {
            wall_time,
            rapier_step_time: counters.step_time(),
            broad_phase_time: counters.cd.broad_phase_time.time(),
            narrow_phase_time: counters.cd.narrow_phase_time.time(),
            island_time: counters.stages.island_construction_time.time(),
            solver_time: counters.stages.solver_time.time(),
            ccd_time: counters.stages.ccd_time.time(),
            active_bodies: self.raw.islands.active_bodies().count(),
            sleeping_bodies: self
                .bodies
                .iter()
                .filter_map(|entry| self.raw.bodies.get(entry.handle))
                .filter(|body| body.is_dynamic() && body.is_sleeping())
                .count(),
            candidate_pairs,
            contact_pairs,
            contacts,
        }
    }

    pub fn contact_events(&self) -> &[ContactEvent] {
        &self.contact_events
    }

    pub fn sensor_intersections(&self) -> &[SensorIntersection] {
        &self.sensor_intersections
    }

    pub fn contact_points(&self, collider: ColliderId) -> Vec<ContactPoint> {
        let Some(handle) = self.collider_handle(collider) else {
            return Vec::new();
        };
        let mut points = Vec::new();
        for pair in self.raw.contact_pairs_with(handle) {
            let direction = if pair.collider1 == handle { 1.0 } else { -1.0 };
            for manifold in &pair.manifolds {
                for contact in &manifold.data.solver_contacts {
                    points.push(ContactPoint {
                        position: from_rapier(contact.point),
                        normal: from_rapier(manifold.data.normal) * direction,
                    });
                }
            }
        }
        points
    }

    pub fn cast_ray(
        &self,
        origin: Vec2,
        direction: Vec2,
        options: RayCastOptions,
    ) -> Option<RayHit> {
        if !finite_vec2(origin)
            || !finite_vec2(direction)
            || direction.length_squared() <= f32::EPSILON
            || !options.max_distance.is_finite()
            || options.max_distance < 0.0
        {
            return None;
        }

        let direction = direction.normalized();
        let excluded = options.exclude_entity;
        let predicate = |_: ColliderHandle, collider: &Collider| {
            decode_collider(collider.user_data).is_none_or(|id| Some(id.entity) != excluded)
        };
        let mut flags = QueryFilterFlags::empty();
        if !options.include_sensors {
            flags |= QueryFilterFlags::EXCLUDE_SENSORS;
        }
        let filter = QueryFilter {
            flags,
            groups: Some(options.collision_groups.to_rapier()),
            predicate: Some(&predicate),
            ..QueryFilter::default()
        };
        let ray = Ray::new(to_rapier(origin), to_rapier(direction));
        let (handle, intersection) =
            self.raw
                .cast_ray_and_get_normal(&ray, options.max_distance, options.solid, filter)?;
        let collider = self.raw.colliders.get(handle)?;
        let id = decode_collider(collider.user_data)?;
        Some(RayHit {
            collider: id,
            point: origin + direction * intersection.time_of_impact,
            normal: from_rapier(intersection.normal),
            distance: intersection.time_of_impact,
        })
    }

    /// Serialize authoritative Rapier state and all stable handle mappings.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, PhysicsWorldError> {
        bincode::serialize(&PhysicsSnapshotRef {
            version: SNAPSHOT_VERSION,
            gravity: &self.raw.gravity,
            integration_parameters: &self.raw.integration_parameters,
            islands: &self.raw.islands,
            broad_phase: &self.raw.broad_phase,
            narrow_phase: &self.raw.narrow_phase,
            bodies: &self.raw.bodies,
            colliders: &self.raw.colliders,
            impulse_joints: &self.raw.impulse_joints,
            multibody_joints: &self.raw.multibody_joints,
            ccd_solver: &self.raw.ccd_solver,
            body_entries: &self.bodies,
            collider_entries: &self.colliders,
            joint_entries: &self.joints,
            entities: &self.entities,
            collect_events: self.collect_events,
        })
        .map_err(|error| PhysicsWorldError(error.to_string()))
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, PhysicsWorldError> {
        let snapshot: PhysicsSnapshot =
            bincode::deserialize(bytes).map_err(|error| PhysicsWorldError(error.to_string()))?;
        if snapshot.version != SNAPSHOT_VERSION {
            return Err(PhysicsWorldError(format!(
                "unsupported physics snapshot version {}",
                snapshot.version
            )));
        }

        let body_indices = snapshot
            .body_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id, index))
            .collect();
        let collider_indices = snapshot
            .collider_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id, index))
            .collect();
        let joint_indices = snapshot
            .joint_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id, index))
            .collect();

        Ok(Self {
            raw: RapierWorld {
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
            bodies: snapshot.body_entries,
            body_indices,
            colliders: snapshot.collider_entries,
            collider_indices,
            joints: snapshot.joint_entries,
            joint_indices,
            entities: snapshot.entities,
            contact_events: Vec::new(),
            sensor_intersections: Vec::new(),
            collect_events: snapshot.collect_events,
        })
    }

    pub(crate) fn body_handle(&self, id: BodyId) -> Option<RigidBodyHandle> {
        self.body_indices
            .get(&id)
            .and_then(|index| self.bodies.get(*index))
            .map(|entry| entry.handle)
    }

    pub(crate) fn collider_handle(&self, id: ColliderId) -> Option<ColliderHandle> {
        self.collider_indices
            .get(&id)
            .and_then(|index| self.colliders.get(*index))
            .map(|entry| entry.handle)
    }

    pub(crate) fn insert_raw_joint(
        &mut self,
        id: JointId,
        body_a: BodyId,
        body_b: BodyId,
        joint: GenericJoint,
    ) -> bool {
        if self.joint_indices.contains_key(&id) {
            return false;
        }
        let Some(handle_a) = self.body_handle(body_a) else {
            return false;
        };
        let Some(handle_b) = self.body_handle(body_b) else {
            return false;
        };
        let handle = self.raw.insert_impulse_joint(handle_a, handle_b, joint);
        let index = self.joints.len();
        self.joints.push(JointEntry {
            id,
            body_a,
            body_b,
            handle,
        });
        self.joint_indices.insert(id, index);
        for entity in [body_a.entity, body_b.entity] {
            let joints = &mut self.entities.entry(entity).or_default().joints;
            if !joints.contains(&id) {
                joints.push(id);
            }
        }
        true
    }

    pub(crate) fn impulse_joint_mut(&mut self, id: JointId) -> Option<&mut ImpulseJoint> {
        let index = *self.joint_indices.get(&id)?;
        let handle = self.joints.get(index)?.handle;
        self.raw.impulse_joints.get_mut(handle, true)
    }

    fn register_collider(&mut self, id: ColliderId, parent: BodyId, handle: ColliderHandle) {
        let index = self.colliders.len();
        self.colliders.push(ColliderEntry { id, parent, handle });
        self.collider_indices.insert(id, index);
        self.entities
            .entry(id.entity)
            .or_default()
            .colliders
            .push(id);
    }

    fn body_mut(&mut self, id: BodyId) -> Option<&mut RigidBody> {
        let handle = self.body_handle(id)?;
        self.raw.bodies.get_mut(handle)
    }

    fn remove_joint(&mut self, id: JointId) -> bool {
        let Some(index) = self.joint_indices.remove(&id) else {
            return false;
        };
        let removed = self.joints.swap_remove(index);
        self.raw.remove_impulse_joint(removed.handle);
        if let Some(replacement) = self.joints.get(index) {
            self.joint_indices.insert(replacement.id, index);
        }
        for entity in [removed.body_a.entity, removed.body_b.entity] {
            if let Some(record) = self.entities.get_mut(&entity) {
                record.joints.retain(|joint| *joint != id);
            }
        }
        true
    }

    fn remove_body_mapping(&mut self, id: BodyId) {
        let Some(index) = self.body_indices.remove(&id) else {
            return;
        };
        self.bodies.swap_remove(index);
        if let Some(replacement) = self.bodies.get(index) {
            self.body_indices.insert(replacement.id, index);
        }
    }

    fn remove_collider_mapping(&mut self, id: ColliderId) {
        let Some(index) = self.collider_indices.remove(&id) else {
            return;
        };
        self.colliders.swap_remove(index);
        if let Some(replacement) = self.colliders.get(index) {
            self.collider_indices.insert(replacement.id, index);
        }
    }

    fn rebuild_events(&mut self) {
        self.contact_events.clear();
        self.sensor_intersections.clear();

        for pair in self.raw.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let Some(collider_a) = self.raw.colliders.get(pair.collider1) else {
                continue;
            };
            let Some(collider_b) = self.raw.colliders.get(pair.collider2) else {
                continue;
            };
            let Some(mut id_a) = decode_collider(collider_a.user_data) else {
                continue;
            };
            let Some(mut id_b) = decode_collider(collider_b.user_data) else {
                continue;
            };

            let (magnitude, strongest_normal) = pair.max_impulse();
            let mut normal = from_rapier(strongest_normal);
            let mut impulse = from_rapier(pair.total_impulse());
            if id_b < id_a {
                std::mem::swap(&mut id_a, &mut id_b);
                normal = -normal;
                impulse = -impulse;
            }
            let point = pair
                .manifolds
                .iter()
                .flat_map(|manifold| &manifold.data.solver_contacts)
                .next()
                .map(|contact| from_rapier(contact.point));
            let solver_contacts = pair
                .manifolds
                .iter()
                .map(|manifold| manifold.data.solver_contacts.len())
                .sum();
            self.contact_events.push(ContactEvent {
                collider_a: id_a,
                collider_b: id_b,
                point,
                normal,
                impulse,
                impulse_magnitude: magnitude,
                solver_contacts,
            });
        }
        self.contact_events
            .sort_unstable_by_key(|event| (event.collider_a, event.collider_b));

        for (_, collider_a, _, collider_b, intersecting) in self.raw.intersection_pairs() {
            if !intersecting {
                continue;
            }
            let Some(mut id_a) = decode_collider(collider_a.user_data) else {
                continue;
            };
            let Some(mut id_b) = decode_collider(collider_b.user_data) else {
                continue;
            };
            if id_b < id_a {
                std::mem::swap(&mut id_a, &mut id_b);
            }
            self.sensor_intersections.push(SensorIntersection {
                collider_a: id_a,
                collider_b: id_b,
            });
        }
        self.sensor_intersections
            .sort_unstable_by_key(|event| (event.collider_a, event.collider_b));
    }
}

fn build_body(id: BodyId, spec: BodySpec) -> RigidBody {
    RigidBodyBuilder::new(spec.kind.to_rapier())
        .translation(to_rapier(spec.position))
        .rotation(spec.angle)
        .linvel(to_rapier(spec.linear_velocity))
        .angvel(spec.angular_velocity)
        .linear_damping(spec.linear_damping)
        .angular_damping(spec.angular_damping)
        .gravity_scale(spec.gravity_scale)
        .can_sleep(spec.can_sleep)
        .ccd_enabled(spec.ccd_enabled)
        .additional_solver_iterations(spec.additional_solver_iterations)
        .user_data(encode_body(id))
        .build()
}

fn build_collider(spec: &ColliderSpec) -> Option<Collider> {
    if !valid_collider_spec(spec) {
        return None;
    }
    let builder = match &spec.shape {
        ColliderShape::Ball { radius } => ColliderBuilder::ball(*radius),
        ColliderShape::Cuboid {
            half_width,
            half_height,
        } => ColliderBuilder::cuboid(*half_width, *half_height),
        ColliderShape::ConvexPolygon { vertices } => {
            let vertices = vertices.iter().copied().map(to_rapier).collect::<Vec<_>>();
            ColliderBuilder::convex_hull(&vertices)?
        }
        ColliderShape::Polyline { vertices } => {
            ColliderBuilder::polyline(vertices.iter().copied().map(to_rapier).collect(), None)
        }
    };
    Some(
        builder
            .position(Pose::new(to_rapier(spec.local_position), spec.local_angle))
            .density(spec.density)
            .friction(spec.friction)
            .restitution(spec.restitution.clamp(0.0, 1.0))
            .sensor(spec.sensor)
            .collision_groups(spec.collision_groups.to_rapier())
            .solver_groups(spec.solver_groups.to_rapier())
            .user_data(encode_collider(spec.id))
            .build(),
    )
}

fn normalized_world_config(config: PhysicsWorldConfig) -> PhysicsWorldConfig {
    PhysicsWorldConfig {
        gravity: if finite_vec2(config.gravity) {
            config.gravity
        } else {
            Vec2::ZERO
        },
        length_unit: finite_positive_or(config.length_unit, 1.0),
        solver_iterations: config.solver_iterations.max(1),
        internal_stabilization_iterations: config.internal_stabilization_iterations.max(1),
        max_ccd_substeps: config.max_ccd_substeps.max(1),
        collect_events: config.collect_events,
    }
}

fn valid_body_spec(spec: BodySpec) -> bool {
    finite_vec2(spec.position)
        && spec.angle.is_finite()
        && finite_vec2(spec.linear_velocity)
        && spec.angular_velocity.is_finite()
        && spec.linear_damping.is_finite()
        && spec.linear_damping >= 0.0
        && spec.angular_damping.is_finite()
        && spec.angular_damping >= 0.0
        && spec.gravity_scale.is_finite()
}

fn valid_collider_spec(spec: &ColliderSpec) -> bool {
    let valid_shape = match &spec.shape {
        ColliderShape::Ball { radius } => radius.is_finite() && *radius > 0.0,
        ColliderShape::Cuboid {
            half_width,
            half_height,
        } => {
            half_width.is_finite()
                && *half_width > 0.0
                && half_height.is_finite()
                && *half_height > 0.0
        }
        ColliderShape::ConvexPolygon { vertices } => {
            vertices.len() >= 3 && vertices.iter().copied().all(finite_vec2)
        }
        ColliderShape::Polyline { vertices } => {
            vertices.len() >= 2 && vertices.iter().copied().all(finite_vec2)
        }
    };
    valid_shape
        && finite_vec2(spec.local_position)
        && spec.local_angle.is_finite()
        && spec.density.is_finite()
        && spec.density >= 0.0
        && spec.friction.is_finite()
        && spec.friction >= 0.0
        && spec.restitution.is_finite()
}

fn body_motion(body: &RigidBody) -> BodyMotion {
    BodyMotion {
        position: from_rapier(body.translation()),
        angle: body.rotation().angle(),
        linear_velocity: from_rapier(body.linvel()),
        angular_velocity: body.angvel(),
    }
}

fn finite_positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn finite_vec2(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

fn encode_body(id: BodyId) -> u128 {
    id.entity.value() as u128
        | ((id.role.value() as u128) << USER_DATA_ROLE_SHIFT)
        | (USER_DATA_BODY_TAG << USER_DATA_TAG_SHIFT)
}

fn encode_collider(id: ColliderId) -> u128 {
    id.entity.value() as u128
        | ((id.role.value() as u128) << USER_DATA_ROLE_SHIFT)
        | ((id.part as u128) << USER_DATA_PART_SHIFT)
        | (USER_DATA_COLLIDER_TAG << USER_DATA_TAG_SHIFT)
}

fn decode_collider(user_data: u128) -> Option<ColliderId> {
    if user_data >> USER_DATA_TAG_SHIFT != USER_DATA_COLLIDER_TAG {
        return None;
    }
    Some(ColliderId {
        entity: PhysicsId::new(user_data as u64),
        role: ColliderRole::new((user_data >> USER_DATA_ROLE_SHIFT) as u32),
        part: (user_data >> USER_DATA_PART_SHIFT) as u16,
    })
}

pub(crate) fn to_rapier(value: Vec2) -> Vector {
    Vector::new(value.x, value.y)
}

pub(crate) fn from_rapier(value: Vector) -> Vec2 {
    Vec2::new(value.x, value.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALL_ROLE: BodyRole = BodyRole::new(1);
    const BALL_COLLIDER: ColliderRole = ColliderRole::new(1);

    fn ball_ids(value: u64) -> (PhysicsId, BodyId, ColliderId) {
        let entity = PhysicsId::new(value);
        (
            entity,
            BodyId::new(entity, BALL_ROLE),
            ColliderId::new(entity, BALL_COLLIDER, 0),
        )
    }

    fn insert_ball(world: &mut PhysicsWorld, value: u64, position: Vec2) -> BodyId {
        let (_, body, collider) = ball_ids(value);
        assert!(world.insert_body(
            body,
            BodySpec {
                position,
                can_sleep: false,
                ..BodySpec::default()
            },
            &[ColliderSpec::ball(collider, 0.25)],
        ));
        body
    }

    #[test]
    fn stable_ids_hide_raw_handles_and_survive_dense_removal() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let first = insert_ball(&mut world, 1, Vec2::new(-1.0, 0.0));
        let second = insert_ball(&mut world, 2, Vec2::ZERO);
        let third = insert_ball(&mut world, 3, Vec2::new(1.0, 0.0));

        assert!(world.remove_entity(second.entity));
        assert_eq!(world.body_count(), 2);
        assert!(world.motion(first).is_some());
        assert!(world.motion(second).is_none());
        assert!(world.motion(third).is_some());
        assert_eq!(
            world.motions().map(|motion| motion.id).collect::<Vec<_>>(),
            vec![first, third]
        );
    }

    #[test]
    fn contacts_are_reported_in_stable_collider_order() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            ..PhysicsWorldConfig::default()
        });
        insert_ball(&mut world, 9, Vec2::new(0.0, 0.0));
        insert_ball(&mut world, 2, Vec2::new(0.2, 0.0));

        let metrics = world.step(1.0 / 60.0);
        assert!(metrics.contact_pairs >= 1);
        let event = world.contact_events().first().unwrap();
        assert!(event.collider_a < event.collider_b);
        assert_eq!(event.collider_a.entity, PhysicsId::new(2));
        assert_eq!(event.collider_b.entity, PhysicsId::new(9));
    }

    #[test]
    fn sensors_report_stable_intersections_without_solver_contacts() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let sensor_entity = PhysicsId::new(8);
        let sensor_body = BodyId::new(sensor_entity, BodyRole::new(2));
        let sensor_id = ColliderId::new(sensor_entity, ColliderRole::new(2), 0);
        let mut sensor = ColliderSpec::ball(sensor_id, 0.5);
        sensor.sensor = true;
        assert!(world.insert_body(
            sensor_body,
            BodySpec {
                kind: BodyKind::Fixed,
                ..BodySpec::default()
            },
            &[sensor],
        ));
        insert_ball(&mut world, 3, Vec2::new(0.25, 0.0));

        let metrics = world.step(1.0 / 60.0);

        assert_eq!(metrics.contact_pairs, 0);
        assert!(world.contact_events().is_empty());
        assert_eq!(world.sensor_intersections().len(), 1);
        let intersection = world.sensor_intersections()[0];
        assert!(intersection.collider_a < intersection.collider_b);
        assert!([intersection.collider_a, intersection.collider_b].contains(&sensor_id));
    }

    #[test]
    fn ray_cast_returns_engine_id_and_honors_entity_exclusion() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let near = insert_ball(&mut world, 4, Vec2::new(2.0, 0.0));
        let far = insert_ball(&mut world, 7, Vec2::new(4.0, 0.0));
        world.step(1.0 / 60.0);

        let first = world
            .cast_ray(
                Vec2::ZERO,
                Vec2::X,
                RayCastOptions {
                    max_distance: 10.0,
                    ..RayCastOptions::default()
                },
            )
            .unwrap();
        assert_eq!(first.collider.entity, near.entity);

        let second = world
            .cast_ray(
                Vec2::ZERO,
                Vec2::X,
                RayCastOptions {
                    max_distance: 10.0,
                    exclude_entity: Some(near.entity),
                    ..RayCastOptions::default()
                },
            )
            .unwrap();
        assert_eq!(second.collider.entity, far.entity);
        assert!(second.distance > first.distance);
    }

    #[test]
    fn snapshot_restores_authoritative_motion_and_mappings() {
        let mut original = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -1.0),
            ..PhysicsWorldConfig::default()
        });
        let body = insert_ball(&mut original, 1, Vec2::new(0.0, 2.0));
        for _ in 0..10 {
            original.step(1.0 / 60.0);
        }

        let bytes = original.snapshot_bytes().unwrap();
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(original.motion(body), restored.motion(body));

        for _ in 0..10 {
            original.step(1.0 / 60.0);
            restored.step(1.0 / 60.0);
        }
        assert_eq!(original.motion(body), restored.motion(body));
    }

    #[test]
    fn kinematic_target_pushes_a_dynamic_body() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let kinematic_entity = PhysicsId::new(1);
        let kinematic = BodyId::new(kinematic_entity, BALL_ROLE);
        let collider = ColliderId::new(kinematic_entity, BALL_COLLIDER, 0);
        assert!(world.insert_body(
            kinematic,
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: Vec2::new(-0.5, 0.0),
                can_sleep: false,
                ..BodySpec::default()
            },
            &[ColliderSpec::ball(collider, 0.25)],
        ));
        let dynamic = insert_ball(&mut world, 2, Vec2::new(0.0, 0.0));

        assert!(world.set_next_kinematic_pose(kinematic, Vec2::new(0.25, 0.0), 0.0));
        world.step(1.0 / 60.0);

        assert!(world.motion(dynamic).unwrap().linear_velocity.x > 0.0);
    }
}
