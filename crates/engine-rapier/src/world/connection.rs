//! Structural connections: a weld between two bodies whose own material, a
//! beam, carries the bending and the pull the weld transmits. Each step charges
//! the sustained connection law for the load the weld carried, so a safe load
//! costs nothing however long it rests. A connection the law exhausts severs:
//! its weld leaves at the end of that step and its record stays, failed.
//! Callers name a connection by a stable joint identity and never see the
//! backend's joint types. See `docs/design/material-connection-loads.md`.

use std::collections::{BTreeMap, HashMap};

use engine_core::Vec2;
use engine_material::{Beam, Connection, ConnectionLoad, FailureMode, Material};
use rapier2d::math::{Rotation, Vector};
use rapier2d::prelude::{
    FixedJointBuilder, ImpulseJoint, JointAxesMask, Pose, RigidBody, RigidBodySet,
};
use serde::{Deserialize, Serialize};

use super::{BodyId, JointEntry, JointId, PhysicsId, PhysicsWorld, PhysicsWorldError, finite_vec2};

/// Marks the connection section that follows a snapshot's other sections.
pub(super) const CONNECTION_SNAPSHOT_TAG: u64 = 0x434f_4e4e_4543_5453;

/// Where a connection is welded, along which direction, and what it is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionSpec {
    /// The body the connection grows from, such as a trunk.
    pub root_body: BodyId,
    /// The body it holds, such as a branch.
    pub body: BodyId,
    /// The center of the root section, where the load is measured, in world
    /// coordinates. Both bodies are welded there as they now lie.
    pub root: Vec2,
    /// The beam's direction from the root into the held body, in world
    /// coordinates. Its length does not matter.
    pub axis: Vec2,
    /// The beam and its material, usually fresh from [`Connection::new`].
    pub connection: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionRejection {
    /// The identity already names a joint or a connection, failed or not.
    Taken,
    /// A body is missing, or both ends are one body.
    Bodies,
    /// The root or the axis is not finite, or the axis has no direction.
    Geometry,
    /// The connection has no capacity left to carry a load.
    Exhausted,
}

/// The load the held body put on the root section through the weld in a
/// step: the solver's final-substep impulse over that substep, as for a
/// tether's tension. A jolt that ends before the final substep does not show
/// here. Directions are the beam's as welded, so they turn with the root body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionLoadSample {
    /// Force along the beam, `mu wu / s^2`: positive in tension, where the
    /// held body pulls away from the root, and negative in compression.
    pub axial: f32,
    /// Force across the beam, positive toward the counterclockwise side of
    /// its direction. The law does not model shear, so this charges nothing.
    pub shear: f32,
    /// Bending moment at the root section, `mu wu^2 / s^2`, positive
    /// counterclockwise. A branch reaching right under gravity bends its root
    /// clockwise.
    pub moment: f32,
    /// The law's combined stress over the material's safe limit. Above one is
    /// an overload.
    pub utilization: f32,
    /// Work this load charged against the capacity.
    pub charged: f32,
}

/// How the step that exhausted a connection ended it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConnectionFailure {
    /// Work that step's overload asked beyond what was left, which the law
    /// reports as unallocated.
    pub excess: f32,
}

/// A connection as the last completed step left it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionState {
    pub root_body: BodyId,
    pub body: BodyId,
    /// The beam, its material and the capacity it has left.
    pub connection: Connection,
    /// The last load its weld carried. A severed connection keeps the load of
    /// the step that severed it; a new one reports zero until its first step.
    pub load: ConnectionLoadSample,
    /// Set once, by the step that exhausted it.
    pub failure: Option<ConnectionFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ConnectionRecord {
    root_body: BodyId,
    body: BodyId,
    connection: Connection,
    load: ConnectionLoadSample,
    failure: Option<ConnectionFailure>,
}

impl ConnectionRecord {
    /// Whether either end belongs to `entity`, as for a joint.
    pub(super) fn touches(&self, entity: PhysicsId) -> bool {
        self.root_body.entity == entity || self.body.entity == entity
    }

    /// A live connection has its own weld between its own bodies; a severed
    /// one has none.
    pub(super) fn consistent(&self, weld: Option<&JointEntry>) -> bool {
        match weld {
            Some(entry) => {
                self.failure.is_none()
                    && entry.body_a == self.root_body
                    && entry.body_b == self.body
            }
            None => self.failure.is_some(),
        }
    }
}

/// A connection in a snapshot. Material names are presentation only, so only
/// the numeric parameters are saved, as for registered regions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredConnection {
    root_body: BodyId,
    body: BodyId,
    /// Density, stiffness, strength, toughness and damping.
    parameters: [f32; 5],
    brittle: bool,
    /// Thickness, height and length.
    beam: [f32; 3],
    remaining: f64,
    load: ConnectionLoadSample,
    failure: Option<ConnectionFailure>,
}

impl StoredConnection {
    fn new(record: &ConnectionRecord) -> Self {
        let m = record.connection.region().material();
        let beam = record.connection.beam();
        Self {
            root_body: record.root_body,
            body: record.body,
            parameters: [m.density, m.stiffness, m.strength, m.toughness, m.damping],
            brittle: m.mode == FailureMode::Brittle,
            beam: [beam.thickness, beam.height, beam.length],
            remaining: record.connection.region().remaining_precise(),
            load: record.load,
            failure: record.failure,
        }
    }

    fn record(&self) -> Option<ConnectionRecord> {
        let [density, stiffness, strength, toughness, damping] = self.parameters;
        let [thickness, height, length] = self.beam;
        let material = Material {
            name: "connection",
            density,
            stiffness,
            strength,
            toughness,
            damping,
            mode: if self.brittle {
                FailureMode::Brittle
            } else {
                FailureMode::Ductile
            },
        };
        let connection = Connection::with_remaining(
            material,
            Beam::new(thickness, height, length),
            self.remaining,
        )
        .ok()?;
        Some(ConnectionRecord {
            root_body: self.root_body,
            body: self.body,
            connection,
            load: self.load,
            failure: self.failure,
        })
    }
}

impl PhysicsWorld {
    /// Weld two existing bodies where they now lie, with a connection that
    /// carries the load. Contacts between the two stay off while they are
    /// welded, so the weld carries the whole load; after a severing they
    /// collide, so their colliders should meet at the root without overlapping.
    pub fn insert_connection(
        &mut self,
        id: JointId,
        spec: ConnectionSpec,
    ) -> Result<(), ConnectionRejection> {
        use ConnectionRejection as R;
        if self.joint_indices.contains_key(&id) || self.connections.contains_key(&id) {
            return Err(R::Taken);
        }
        let (Some(root_handle), Some(body_handle)) = (
            self.body_handle(spec.root_body),
            self.body_handle(spec.body),
        ) else {
            return Err(R::Bodies);
        };
        if spec.root_body == spec.body {
            return Err(R::Bodies);
        }
        let length = spec.axis.length();
        if !finite_vec2(spec.root)
            || !finite_vec2(spec.axis)
            || !length.is_finite()
            || length <= 0.0
        {
            return Err(R::Geometry);
        }
        if spec.connection.failed() {
            return Err(R::Exhausted);
        }
        // The frame's x axis runs along the beam, so the weld's first locked
        // axis carries the axial load and its second the shear. Built from the
        // axis itself, with no trigonometry that could round differently
        // elsewhere.
        let direction = spec.axis / length;
        let frame = Pose::from_parts(
            super::to_rapier(spec.root),
            Rotation::from_cos_sin_unchecked(direction.x, direction.y),
        );
        let root_pose = *self.raw.bodies[root_handle].position();
        let body_pose = *self.raw.bodies[body_handle].position();
        let weld = FixedJointBuilder::new()
            .local_frame1(root_pose.inverse() * frame)
            .local_frame2(body_pose.inverse() * frame)
            .contacts_enabled(false)
            .build();
        let inserted = self.insert_raw_joint(id, spec.root_body, spec.body, weld.into());
        debug_assert!(inserted, "a checked identity and bodies");
        self.connections.insert(
            id,
            ConnectionRecord {
                root_body: spec.root_body,
                body: spec.body,
                connection: spec.connection,
                load: ConnectionLoadSample::default(),
                failure: None,
            },
        );
        Ok(())
    }

    /// A connection's bodies, capacity, last load and failure, or None when
    /// `id` names no connection.
    pub fn connection(&self, id: JointId) -> Option<ConnectionState> {
        let record = self.connections.get(&id)?;
        Some(ConnectionState {
            root_body: record.root_body,
            body: record.body,
            connection: record.connection,
            load: record.load,
            failure: record.failure,
        })
    }

    /// Take a connection out with its weld, if it still has one, leaving both
    /// bodies in place. This is the host's own cut: it records no failure and
    /// frees the identity. Refused when `id` names no connection.
    pub fn remove_connection(&mut self, id: JointId) -> bool {
        if self.connections.remove(&id).is_none() {
            return false;
        }
        self.remove_joint(id);
        true
    }

    /// The connections the last step severed, in identity order. The next
    /// step replaces the list; a snapshot does not keep it.
    pub fn severed_connections(&self) -> &[JointId] {
        &self.severed_connections
    }

    /// Charge each live connection for the load its weld carried in the step
    /// just solved, in identity order, and sever those the charge exhausts.
    pub(super) fn charge_connections(&mut self, dt: f32) {
        self.severed_connections.clear();
        if self.connections.is_empty() {
            return;
        }
        // Each solver iteration is a substep, and the solver keeps the impulse
        // of the last one.
        let per_impulse = self.raw.integration_parameters.num_solver_iterations.max(1) as f32 / dt;
        for (&id, record) in &mut self.connections {
            if record.failure.is_some() {
                continue;
            }
            let Some(joint) = self
                .joint_indices
                .get(&id)
                .and_then(|&index| self.joints.get(index))
                .and_then(|entry| self.raw.impulse_joints.get(entry.handle))
            else {
                continue;
            };
            let [axial, shear, moment] = root_impulses(joint, &self.raw.bodies);
            let mut load = ConnectionLoadSample {
                axial: axial * per_impulse,
                shear: shear * per_impulse,
                moment: moment * per_impulse,
                ..ConnectionLoadSample::default()
            };
            let Ok(outcome) = engine_material::connection(
                &record.connection,
                ConnectionLoad {
                    moment: load.moment,
                    axial: load.axial,
                    duration: dt,
                    // The weld is rigid, so the law's documented progression
                    // stands in for a measured plastic rotation or extension.
                    measured_plastic_rotation: None,
                    measured_plastic_extension: None,
                },
            ) else {
                // A load the law refuses, not finite or past its bound, is
                // recorded with no charge.
                load.utilization = f32::INFINITY;
                record.load = load;
                continue;
            };
            load.utilization =
                outcome.ledger.peak_stress[0] / record.connection.region().effective_strength();
            load.charged = outcome.consumed[0];
            record
                .connection
                .absorb(load.charged)
                .expect("the law charges finite, non-negative work");
            record.load = load;
            if record.connection.failed() {
                record.failure = Some(ConnectionFailure {
                    excess: outcome.ledger.excess,
                });
                self.severed_connections.push(id);
            }
        }
        for index in 0..self.severed_connections.len() {
            let id = self.severed_connections[index];
            self.remove_joint(id);
        }
    }

    pub(super) fn serialize_connections(
        &self,
        bytes: &mut Vec<u8>,
    ) -> Result<(), PhysicsWorldError> {
        if self.connections.is_empty() {
            return Ok(());
        }
        let stored: Vec<(JointId, StoredConnection)> = self
            .connections
            .iter()
            .map(|(&id, record)| (id, StoredConnection::new(record)))
            .collect();
        bincode::serialize_into(&mut *bytes, &CONNECTION_SNAPSHOT_TAG)
            .map_err(|error| PhysicsWorldError(error.to_string()))?;
        bincode::serialize_into(&mut *bytes, &stored)
            .map_err(|error| PhysicsWorldError(error.to_string()))
    }
}

/// One locked row of a weld, as the solver builds it: its linear direction,
/// and its angular parts on the root body and on the held body.
#[derive(Debug, Clone, Copy)]
struct Row {
    linear: Vector,
    root: f32,
    held: f32,
}

impl Row {
    fn less(self, other: Row, coefficient: f32) -> Row {
        Row {
            linear: self.linear - other.linear * coefficient,
            root: self.root - other.root * coefficient,
            held: self.held - other.held * coefficient,
        }
    }
}

/// A body's inverse mass and inertia as the solver sees them, zero unless it
/// is dynamic, and the arm from its center of mass to `anchor`.
fn solver_terms(body: &RigidBody, anchor: Vector) -> (Vector, f32, Vector) {
    if !body.is_dynamic() {
        return (Vector::ZERO, 0.0, Vector::ZERO);
    }
    let mass = body.mass_properties();
    (
        mass.effective_inv_mass,
        mass.effective_world_inv_inertia,
        anchor - mass.world_com,
    )
}

/// The impulses a weld's locked axes carried on the root body, at the root
/// and in the weld's own frame: along the beam, across it and about the root.
/// Before it solves a joint, Rapier orthogonalizes the locked rows in the
/// bodies' mass metric by modified Gram-Schmidt, the angular row first and
/// then the two linear ones. The impulses it writes back are coefficients of
/// those changed rows, so for a weld whose root is not at the held body's
/// center of mass the moment hides in the linear rows. This undoes that with
/// the solver's own inputs at the poses the step ended with; a weld turns as
/// one piece, which leaves the coefficients unchanged.
fn root_impulses(joint: &ImpulseJoint, bodies: &RigidBodySet) -> [f32; 3] {
    let root = &bodies[joint.body1()];
    let held = &bodies[joint.body2()];
    let frame1 = *root.position() * joint.data.local_frame1;
    let frame2 = *held.position() * joint.data.local_frame2;
    let (root_inv_mass, root_inv_inertia, root_arm) = solver_terms(root, frame1.translation);
    let (held_inv_mass, held_inv_inertia, held_arm) = solver_terms(held, frame2.translation);
    let inv_mass = root_inv_mass + held_inv_mass;
    let dot = |a: Row, b: Row| {
        a.linear.x * inv_mass.x * b.linear.x
            + a.linear.y * inv_mass.y * b.linear.y
            + a.root * root_inv_inertia * b.root
            + a.held * held_inv_inertia * b.held
    };
    // The solver's reciprocal, which treats a vanishing norm as no row.
    let inv = |value: f32| {
        if value.abs() <= 1.0e-20 {
            0.0
        } else {
            1.0 / value
        }
    };
    let cross = |a: Vector, b: Vector| a.x * b.y - a.y * b.x;
    let linear = |axis: Vector| Row {
        linear: axis,
        root: cross(root_arm, axis),
        held: cross(held_arm, axis),
    };
    let (cos, sin) = (frame1.rotation.re, frame1.rotation.im);
    let angular = Row {
        linear: Vector::ZERO,
        root: 1.0,
        held: 1.0,
    };
    let along = linear(Vector::new(cos, sin));
    let across = linear(Vector::new(-sin, cos));
    let angular_norm = inv(dot(angular, angular));
    let along_angular = dot(along, angular) * angular_norm;
    let across_angular = dot(across, angular) * angular_norm;
    let along = along.less(angular, along_angular);
    let across_along = dot(across.less(angular, across_angular), along) * inv(dot(along, along));
    let written = joint.impulses;
    [
        written.x - across_along * written.y,
        written.y,
        written.z - along_angular * written.x
            + (across_along * along_angular - across_angular) * written.y,
    ]
}

/// Rebuild saved connections and check each against the saved joints: a live
/// connection keeps its weld between its own bodies, and a failed one has none.
pub(super) fn restore_connections(
    reader: &mut std::io::Cursor<&[u8]>,
    joints: &[JointEntry],
    impulse_joints: &rapier2d::prelude::ImpulseJointSet,
) -> Result<BTreeMap<JointId, ConnectionRecord>, PhysicsWorldError> {
    let invalid = || PhysicsWorldError("invalid connection snapshot".into());
    let stored: Vec<(JointId, StoredConnection)> =
        bincode::deserialize_from(reader).map_err(|error| PhysicsWorldError(error.to_string()))?;
    let welds: HashMap<JointId, &JointEntry> =
        joints.iter().map(|entry| (entry.id, entry)).collect();
    let mut connections = BTreeMap::new();
    for (id, saved) in stored {
        let record = saved.record().ok_or_else(invalid)?;
        let weld = welds.get(&id).copied();
        if record.connection.failed() != record.failure.is_some() || !record.consistent(weld) {
            return Err(invalid());
        }
        if let Some(entry) = weld {
            let joint = impulse_joints.get(entry.handle).ok_or_else(invalid)?;
            if joint.data.locked_axes != JointAxesMask::LOCKED_FIXED_AXES {
                return Err(invalid());
            }
        }
        if connections.insert(id, record).is_some() {
            return Err(invalid());
        }
    }
    if connections.is_empty() {
        return Err(invalid());
    }
    Ok(connections)
}
