//! Structural connections: a weld between two bodies whose own material, a
//! beam, carries the bending and the pull the weld transmits. Each step charges
//! the sustained connection law for the load the weld carried, so a safe load
//! costs nothing however long it rests. A connection the law exhausts severs:
//! its weld leaves at the end of that step and its record stays, failed.
//! A connection that bends is a hinge rather than a rigid weld: a spring with
//! the beam's own bending stiffness holds the held body's turn about the root.
//! An anchor holds a body in its support as the ground holds a root: springs
//! with the beam's own stiffness hold it along the beam, across it and in its
//! turn, so that several anchors share a load by where they hold.
//! Callers name a connection by a stable joint identity and never see the
//! backend's joint types. See `docs/design/material-connection-loads.md`.

use std::collections::{BTreeMap, HashMap};

use engine_core::Vec2;
use engine_material::{Beam, Connection, ConnectionLoad, FailureMode, Material};
use rapier2d::math::{Rotation, Vector};
use rapier2d::prelude::{
    FixedJointBuilder, GenericJoint, GenericJointBuilder, ImpulseJoint, JointAxesMask, JointAxis,
    MotorModel, Pose, RigidBody, RigidBodySet,
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
    /// Whether the held body bends about the root. A connection that bends
    /// is a hinge at the root whose spring has the beam's bending stiffness,
    /// [`hinge_stiffness`], damped by its material. One that does not is a
    /// rigid weld.
    pub bends: bool,
}

/// Where a body is anchored in its support, and what holds it there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorSpec {
    /// The body that holds, such as the ground around a root.
    pub support: BodyId,
    /// The body it holds, such as a tree's roots.
    pub body: BodyId,
    /// Where it holds, in world coordinates. Both bodies are joined there as
    /// they now lie, and the load is measured there.
    pub at: Vec2,
    /// The held length's direction from the support into the held body, in
    /// world coordinates, along which the load reads as axial. Its length
    /// does not matter.
    pub axis: Vec2,
    /// What holds, and its beam: the held length's section and length.
    pub connection: Connection,
}

/// How a connection joins its two bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joining {
    Weld,
    Hinge,
    Anchor,
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
    /// The identity names no connection.
    Missing,
}

/// The load the held body put on the root section through the weld in a
/// step: the solver's final-substep impulse over that substep, as for a
/// tether's tension. A jolt that ends before the final substep does not show
/// here. Directions are the beam's as welded, so they turn with the root body.
/// An anchor's whole force charges the law as its pull, whichever way it
/// points, so its shear charges too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionLoadSample {
    /// Force along the beam, `mu wu / s^2`: positive in tension, where the
    /// held body pulls away from the root, and negative in compression.
    pub axial: f32,
    /// Force across the beam, positive toward the counterclockwise side of
    /// its direction. The law does not model shear, so on a weld or a hinge
    /// this charges nothing.
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
    /// How far the held body has turned about the root since it was joined,
    /// in radians, positive counterclockwise, from the bodies as they now lie.
    /// A rigid weld turns only by the solver's slight give. A severed
    /// connection has no joint to measure.
    pub deflection: Option<f32>,
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

/// The bending stiffness of a connection that bends, `mu wu^2 / s^2` per
/// radian: the connection law's own `stiffness * I / length` for the beam. Its
/// hinge therefore turns by the law's elastic rotation for the moment it
/// carries.
pub fn hinge_stiffness(connection: &Connection) -> f32 {
    let beam = connection.beam();
    connection.region().material().stiffness * beam.second_moment() / beam.length
}

/// The stiffness with which an anchor holds along its beam and across it,
/// `mu / s^2`: the connection law's own axial `stiffness * area / length`. An
/// anchor therefore gives by the law's elastic extension for the force it
/// carries.
pub fn anchor_stiffness(connection: &Connection) -> f32 {
    let beam = connection.beam();
    connection.region().material().stiffness * beam.area() / beam.length
}

/// The damping ratio at which a free swing loses `damping`, the material's
/// fraction of stored elastic energy, each cycle: `-ln(1 - damping) / 4 pi`,
/// at most critical.
fn damping_ratio(damping: f32) -> f32 {
    if damping >= 1.0 {
        return 1.0;
    }
    (-libm::log1pf(-damping) / (4.0 * std::f32::consts::PI)).min(1.0)
}

/// A dynamic body's moment of inertia about `point`, or zero for a body that
/// does not move.
fn inertia_about(body: &RigidBody, point: Vector) -> f32 {
    if !body.is_dynamic() {
        return 0.0;
    }
    let mass = body.mass_properties();
    let arm = point - mass.world_com;
    mass.local_mprops.principal_inertia() + mass.local_mprops.mass() * arm.length_squared()
}

/// The springs of an anchor of `connection` that holds `held` at `point`:
/// its stiffness and damping along and across the beam, then in its turn.
/// Each is damped as a hinge's spring is, for the held body alone.
fn anchor_springs(connection: &Connection, held: &RigidBody, point: Vector) -> [(f32, f32); 2] {
    let ratio = damping_ratio(connection.region().material().damping);
    let mass = if held.is_dynamic() { held.mass() } else { 0.0 };
    let (pull, turn) = (anchor_stiffness(connection), hinge_stiffness(connection));
    [
        (pull, 2.0 * ratio * (pull * mass).sqrt()),
        (
            turn,
            2.0 * ratio * (turn * inertia_about(held, point)).sqrt(),
        ),
    ]
}

impl PhysicsWorld {
    /// Join two existing bodies where they now lie, with a connection that
    /// carries the load: a rigid weld, or a hinge that bends. Contacts between
    /// the two stay off while they are joined, so the joint carries the whole
    /// load; after a severing they collide, so their colliders should meet at
    /// the root without overlapping.
    ///
    /// A hinge's spring is [`hinge_stiffness`]. Its damping is set once, here,
    /// from the held body's inertia about the root, so that a swing of that
    /// body alone loses the material's damping fraction of its energy each
    /// cycle.
    pub fn insert_connection(
        &mut self,
        id: JointId,
        spec: ConnectionSpec,
    ) -> Result<(), ConnectionRejection> {
        let joining = if spec.bends {
            Joining::Hinge
        } else {
            Joining::Weld
        };
        self.join(
            id,
            spec.root_body,
            spec.body,
            spec.root,
            spec.axis,
            spec.connection,
            joining,
        )
    }

    /// Anchor a body in its support where they now lie. An anchor is neither
    /// rigid nor a hinge: springs hold the held body at the anchor along its
    /// beam and across it, each with [`anchor_stiffness`], and hold its turn
    /// with [`hinge_stiffness`]. Several anchors between the same two bodies
    /// therefore share a load by where they hold: one farther from the center
    /// the held body turns about gives further and carries more. The whole
    /// force an anchor carries charges the law as its pull, whichever way it
    /// points, with its moment. Contacts between the two bodies stay off
    /// while any of their anchors holds.
    ///
    /// Each spring is damped once, here, for the held body alone, as a
    /// hinge's is.
    pub fn insert_anchor(
        &mut self,
        id: JointId,
        spec: AnchorSpec,
    ) -> Result<(), ConnectionRejection> {
        self.join(
            id,
            spec.support,
            spec.body,
            spec.at,
            spec.axis,
            spec.connection,
            Joining::Anchor,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn join(
        &mut self,
        id: JointId,
        root_body: BodyId,
        body: BodyId,
        root: Vec2,
        axis: Vec2,
        connection: Connection,
        joining: Joining,
    ) -> Result<(), ConnectionRejection> {
        use ConnectionRejection as R;
        if self.joint_indices.contains_key(&id) || self.connections.contains_key(&id) {
            return Err(R::Taken);
        }
        let (Some(root_handle), Some(body_handle)) =
            (self.body_handle(root_body), self.body_handle(body))
        else {
            return Err(R::Bodies);
        };
        if root_body == body {
            return Err(R::Bodies);
        }
        let length = axis.length();
        if !finite_vec2(root) || !finite_vec2(axis) || !length.is_finite() || length <= 0.0 {
            return Err(R::Geometry);
        }
        if connection.failed() {
            return Err(R::Exhausted);
        }
        // The frame's x axis runs along the beam, so the weld's first locked
        // axis carries the axial load and its second the shear. Built from the
        // axis itself, with no trigonometry that could round differently
        // elsewhere.
        let direction = axis / length;
        let frame = Pose::from_parts(
            super::to_rapier(root),
            Rotation::from_cos_sin_unchecked(direction.x, direction.y),
        );
        let root_pose = *self.raw.bodies[root_handle].position();
        let body_pose = *self.raw.bodies[body_handle].position();
        let joint: GenericJoint = match joining {
            Joining::Hinge => {
                let stiffness = hinge_stiffness(&connection);
                let ratio = damping_ratio(connection.region().material().damping);
                let inertia = inertia_about(&self.raw.bodies[body_handle], frame.translation);
                // A force-based spring, so that its stiffness is a real moment
                // per radian rather than an acceleration scaled by the bodies'
                // masses.
                GenericJointBuilder::new(JointAxesMask::LOCKED_REVOLUTE_AXES)
                    .local_frame1(root_pose.inverse() * frame)
                    .local_frame2(body_pose.inverse() * frame)
                    .contacts_enabled(false)
                    .motor_model(JointAxis::AngX, MotorModel::ForceBased)
                    .motor_position(
                        JointAxis::AngX,
                        0.0,
                        stiffness,
                        2.0 * ratio * (stiffness * inertia).sqrt(),
                    )
                    .build()
            }
            Joining::Anchor => {
                let [pull, turn] = anchor_springs(
                    &connection,
                    &self.raw.bodies[body_handle],
                    frame.translation,
                );
                // No axis is locked: a force-based spring holds each.
                GenericJointBuilder::new(JointAxesMask::empty())
                    .local_frame1(root_pose.inverse() * frame)
                    .local_frame2(body_pose.inverse() * frame)
                    .contacts_enabled(false)
                    .motor_model(JointAxis::LinX, MotorModel::ForceBased)
                    .motor_position(JointAxis::LinX, 0.0, pull.0, pull.1)
                    .motor_model(JointAxis::LinY, MotorModel::ForceBased)
                    .motor_position(JointAxis::LinY, 0.0, pull.0, pull.1)
                    .motor_model(JointAxis::AngX, MotorModel::ForceBased)
                    .motor_position(JointAxis::AngX, 0.0, turn.0, turn.1)
                    .build()
            }
            Joining::Weld => FixedJointBuilder::new()
                .local_frame1(root_pose.inverse() * frame)
                .local_frame2(body_pose.inverse() * frame)
                .contacts_enabled(false)
                .build()
                .into(),
        };
        let inserted = self.insert_raw_joint(id, root_body, body, joint);
        debug_assert!(inserted, "a checked identity and bodies");
        self.connections.insert(
            id,
            ConnectionRecord {
                root_body,
                body,
                connection,
                load: ConnectionLoadSample::default(),
                failure: None,
            },
        );
        Ok(())
    }

    /// A connection's bodies, capacity, last load, failure and deflection, or
    /// None when `id` names no connection.
    pub fn connection(&self, id: JointId) -> Option<ConnectionState> {
        let record = self.connections.get(&id)?;
        let deflection = self.connection_joint(id).map(|joint| {
            let root = self.raw.bodies[joint.body1()].position().rotation;
            let held = self.raw.bodies[joint.body2()].position().rotation;
            let turn = (root * joint.data.local_frame1.rotation).inverse()
                * (held * joint.data.local_frame2.rotation);
            turn.angle()
        });
        Some(ConnectionState {
            root_body: record.root_body,
            body: record.body,
            connection: record.connection,
            load: record.load,
            failure: record.failure,
            deflection,
        })
    }

    /// The live joint of a connection, weld or hinge.
    fn connection_joint(&self, id: JointId) -> Option<&ImpulseJoint> {
        self.joint_indices
            .get(&id)
            .and_then(|&index| self.joints.get(index))
            .and_then(|entry| self.raw.impulse_joints.get(entry.handle))
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

    /// Give a live connection a new beam where it is joined, as wood does
    /// when it thickens. Its frames stay, so a hinge keeps the rest it was
    /// joined at rather than taking the turn it now has. Its spring takes the
    /// new beam's [`hinge_stiffness`], damped as [`Self::insert_connection`]
    /// damps one for the held body as it now is, so a held body whose mass
    /// changed first is damped for its new mass. An anchor's springs take the
    /// new beam's stiffnesses the same way. The work the connection has
    /// absorbed stays absorbed: it keeps the new beam's capacity less what the
    /// old one used.
    ///
    /// Refused, with nothing changed, when `id` names no connection, the
    /// connection has severed, the beam is not valid for its material, or the
    /// new beam's capacity is no more than the work already absorbed.
    pub fn reshape_connection(
        &mut self,
        id: JointId,
        beam: Beam,
    ) -> Result<(), ConnectionRejection> {
        use ConnectionRejection as R;
        let record = self.connections.get(&id).ok_or(R::Missing)?;
        if record.failure.is_some() {
            return Err(R::Exhausted);
        }
        let region = record.connection.region();
        let used = f64::from(region.capacity()) - region.remaining_precise();
        let material = region.material();
        let fresh = Connection::new(material, beam).map_err(|_| R::Geometry)?;
        let remaining = f64::from(fresh.region().capacity()) - used;
        if remaining <= 0.0 {
            return Err(R::Exhausted);
        }
        let connection =
            Connection::with_remaining(material, beam, remaining).map_err(|_| R::Geometry)?;
        let handle = self
            .joint_indices
            .get(&id)
            .and_then(|&index| self.joints.get(index))
            .map(|entry| entry.handle)
            .expect("a live connection has its joint");
        let joint = self
            .raw
            .impulse_joints
            .get(handle)
            .expect("a live connection has its joint");
        if is_hinge(joint) {
            let stiffness = hinge_stiffness(&connection);
            let ratio = damping_ratio(material.damping);
            let held = &self.raw.bodies[joint.body2()];
            let root = (*held.position() * joint.data.local_frame2).translation;
            let damping = 2.0 * ratio * (stiffness * inertia_about(held, root)).sqrt();
            self.raw
                .impulse_joints
                .get_mut(handle, true)
                .expect("a live connection has its joint")
                .data
                .set_motor_position(JointAxis::AngX, 0.0, stiffness, damping);
        } else if is_anchor(joint) {
            let held = &self.raw.bodies[joint.body2()];
            let at = (*held.position() * joint.data.local_frame2).translation;
            let [pull, turn] = anchor_springs(&connection, held, at);
            let data = &mut self
                .raw
                .impulse_joints
                .get_mut(handle, true)
                .expect("a live connection has its joint")
                .data;
            data.set_motor_position(JointAxis::LinX, 0.0, pull.0, pull.1);
            data.set_motor_position(JointAxis::LinY, 0.0, pull.0, pull.1);
            data.set_motor_position(JointAxis::AngX, 0.0, turn.0, turn.1);
        }
        self.connections
            .get_mut(&id)
            .expect("checked above")
            .connection = connection;
        self.invalidate_gravity(super::GravityInvalidation::ExternalEdit);
        Ok(())
    }

    /// The connections the last step severed, in identity order. The next
    /// step replaces the list; a snapshot does not keep it.
    pub fn severed_connections(&self) -> &[JointId] {
        &self.severed_connections
    }

    /// Charge each live connection for the load its joint carried in the step
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
            let anchor = is_anchor(joint);
            let [axial, shear, moment] = if anchor {
                anchor_impulses(joint)
            } else if is_hinge(joint) {
                hinge_impulses(joint, &self.raw.bodies)
            } else {
                root_impulses(joint, &self.raw.bodies)
            };
            let mut load = ConnectionLoadSample {
                axial: axial * per_impulse,
                shear: shear * per_impulse,
                moment: moment * per_impulse,
                ..ConnectionLoadSample::default()
            };
            // What holds an anchor gives way to the force it carries,
            // whichever way that points.
            let pull = if anchor {
                load.axial.hypot(load.shear)
            } else {
                load.axial
            };
            let Ok(outcome) = engine_material::connection(
                &record.connection,
                ConnectionLoad {
                    moment: load.moment,
                    axial: pull,
                    duration: dt,
                    // A weld is rigid, and a hinge's turn and an anchor's
                    // give are all elastic, so none measures plastic rotation
                    // or extension, and the law's documented progression
                    // stands in for both.
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

/// Whether a connection's joint is a bending hinge rather than a rigid weld.
fn is_hinge(joint: &ImpulseJoint) -> bool {
    joint.data.locked_axes == JointAxesMask::LOCKED_REVOLUTE_AXES
}

/// A hinge as [`PhysicsWorld::insert_connection`] builds one for
/// `connection`: two locked linear axes and a force-based spring on its turn,
/// at rest where it was joined, with the beam's stiffness and any damping.
fn is_connection_hinge(joint: &GenericJoint, connection: &Connection) -> bool {
    let Some(spring) = joint.motor(JointAxis::AngX) else {
        return false;
    };
    joint.locked_axes == JointAxesMask::LOCKED_REVOLUTE_AXES
        && joint.motor_axes == JointAxesMask::ANG_X
        && spring.model == MotorModel::ForceBased
        && spring.target_pos == 0.0
        && spring.target_vel == 0.0
        && spring.stiffness == hinge_stiffness(connection)
        && spring.damping.is_finite()
        && spring.damping >= 0.0
        && spring.max_force == f32::MAX
}

/// Whether a connection's joint is an anchor: no axis locked, and a spring on
/// each.
fn is_anchor(joint: &ImpulseJoint) -> bool {
    joint.data.locked_axes.is_empty() && joint.data.motor_axes == ANCHOR_AXES
}

/// The axes an anchor's springs hold: along its beam, across it and its turn.
const ANCHOR_AXES: JointAxesMask = JointAxesMask::LIN_X
    .union(JointAxesMask::LIN_Y)
    .union(JointAxesMask::ANG_X);

/// An anchor as [`PhysicsWorld::insert_anchor`] builds one for `connection`:
/// no locked axis and a force-based spring on each, at rest where it was
/// joined, with the beam's stiffnesses and any damping.
fn is_connection_anchor(joint: &GenericJoint, connection: &Connection) -> bool {
    let spring = |axis, stiffness: f32| {
        joint.motor(axis).is_some_and(|spring| {
            spring.model == MotorModel::ForceBased
                && spring.target_pos == 0.0
                && spring.target_vel == 0.0
                && spring.stiffness == stiffness
                && spring.damping.is_finite()
                && spring.damping >= 0.0
                && spring.max_force == f32::MAX
        })
    };
    joint.locked_axes.is_empty()
        && joint.motor_axes == ANCHOR_AXES
        && spring(JointAxis::LinX, anchor_stiffness(connection))
        && spring(JointAxis::LinY, anchor_stiffness(connection))
        && spring(JointAxis::AngX, hinge_stiffness(connection))
}

/// The impulses an anchor carried on its support, at the anchor and in its
/// own frame: along the beam, across it and about the anchor. Each spring is
/// a row of its own, which the solver leaves as it is, so each impulse is the
/// load itself.
fn anchor_impulses(joint: &ImpulseJoint) -> [f32; 3] {
    [JointAxis::LinX, JointAxis::LinY, JointAxis::AngX]
        .map(|axis| joint.data.motor(axis).map_or(0.0, |motor| motor.impulse))
}

/// The impulses a hinge carried on the root body, at the root and in its own
/// frame: along the beam, across it and about the root. Its spring is a row
/// of its own whose impulse is the moment. Rapier orthogonalizes the two
/// locked linear rows in the bodies' mass metric, the one along the beam
/// first, so this undoes that as [`root_impulses`] does for a weld. Both
/// linear rows act at the root, so neither adds to the moment about it.
fn hinge_impulses(joint: &ImpulseJoint, bodies: &RigidBodySet) -> [f32; 3] {
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
    let cross = |a: Vector, b: Vector| a.x * b.y - a.y * b.x;
    let linear = |axis: Vector| Row {
        linear: axis,
        root: cross(root_arm, axis),
        held: cross(held_arm, axis),
    };
    let (cos, sin) = (frame1.rotation.re, frame1.rotation.im);
    let along = linear(Vector::new(cos, sin));
    let across = linear(Vector::new(-sin, cos));
    let norm = dot(along, along);
    let across_along = if norm.abs() <= 1.0e-20 {
        0.0
    } else {
        dot(across, along) / norm
    };
    let written = joint.impulses;
    let spring = joint
        .data
        .motor(JointAxis::AngX)
        .map_or(0.0, |motor| motor.impulse);
    [written.x - across_along * written.y, written.y, spring]
}

/// Rebuild saved connections and check each against the saved joints: a live
/// connection keeps its weld, hinge or anchor between its own bodies, and a
/// failed one has none.
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
            if joint.data.locked_axes != JointAxesMask::LOCKED_FIXED_AXES
                && !is_connection_hinge(&joint.data, &record.connection)
                && !is_connection_anchor(&joint.data, &record.connection)
            {
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
