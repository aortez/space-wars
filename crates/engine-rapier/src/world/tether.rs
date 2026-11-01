//! One rope between two bodies: a distance limit that pulls the two together
//! only while taut and lets them close freely. Callers name it by a stable
//! joint identity and never see the backend's joint types.

use engine_core::Vec2;
use rapier2d::prelude::{JointAxesMask, JointAxis, RopeJointBuilder};

use super::{BodyId, JointId, PhysicsWorld, finite_vec2, from_rapier, to_rapier};

/// Where a rope is tied, in each body's own frame, and how long it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TetherSpec {
    /// The body the rope is tied to, and the point it is tied at.
    pub anchor_body: BodyId,
    pub anchor: Vec2,
    /// The body on the rope's other end, and the point the rope leaves it.
    pub body: BodyId,
    pub attachment: Vec2,
    /// The farthest the two points may be apart.
    pub length: f32,
}

/// A rope as the last completed step left it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TetherState {
    pub anchor_body: BodyId,
    pub body: BodyId,
    /// Both ends in world coordinates.
    pub anchor: Vec2,
    pub attachment: Vec2,
    pub length: f32,
    pub distance: f32,
    /// The pull holding the ends together as the last step ended, as a force:
    /// the solver's final substep impulse over that substep. Zero while slack.
    /// A catch that jerks the rope taut early in a step does not show here.
    pub tension: f32,
}

impl TetherState {
    /// How much shorter than its length the span between the ends is.
    pub fn slack(&self) -> f32 {
        (self.length - self.distance).max(0.0)
    }
}

impl PhysicsWorld {
    /// Tie a rope between two existing bodies. Refused when the identity is
    /// already a joint, either body is missing, the two are one body, or the
    /// length is not a positive finite distance. Contacts between the two
    /// bodies stay enabled, so the rope never lets one pass through the other.
    pub fn insert_tether(&mut self, id: JointId, spec: TetherSpec) -> bool {
        if spec.anchor_body == spec.body
            || !finite_vec2(spec.anchor)
            || !finite_vec2(spec.attachment)
            || !valid_length(spec.length)
        {
            return false;
        }
        let rope = RopeJointBuilder::new(spec.length)
            .local_anchor1(to_rapier(spec.anchor))
            .local_anchor2(to_rapier(spec.attachment))
            .contacts_enabled(true)
            .build();
        self.insert_raw_joint(id, spec.anchor_body, spec.body, rope.into())
    }

    /// Change a rope's length. Lengthening only relaxes the limit; shortening
    /// below the current span makes the next step pull the ends together as
    /// hard as it takes, so callers that need a bounded pull shorten it no
    /// further than the span their own bounded impulses have closed.
    pub fn set_tether_length(&mut self, id: JointId, length: f32) -> bool {
        if !valid_length(length) || self.tether(id).is_none() {
            return false;
        }
        let Some(joint) = self.impulse_joint_mut(id) else {
            return false;
        };
        joint.data.set_limits(JointAxis::LinX, [0.0, length]);
        true
    }

    /// A rope's ends, length, span and last pull, or None when `id` names no
    /// joint or a joint that is not a rope.
    pub fn tether(&self, id: JointId) -> Option<TetherState> {
        let index = *self.joint_indices.get(&id)?;
        let entry = self.joints.get(index)?;
        let joint = self.raw.impulse_joints.get(entry.handle)?;
        let data = &joint.data;
        if data.coupled_axes != JointAxesMask::LIN_AXES
            || !data.limit_axes.contains(JointAxesMask::LIN_X)
        {
            return None;
        }
        let limits = data.limits(JointAxis::LinX)?;
        let body1 = self.raw.bodies.get(joint.body1())?;
        let body2 = self.raw.bodies.get(joint.body2())?;
        let params = &self.raw.integration_parameters;
        // Each solver iteration is a substep, and the solver keeps the limit
        // impulse of the last one.
        let substeps = params.num_solver_iterations.max(1);
        let anchor = from_rapier(*body1.position() * data.local_anchor1());
        let attachment = from_rapier(*body2.position() * data.local_anchor2());
        Some(TetherState {
            anchor_body: entry.body_a,
            body: entry.body_b,
            anchor,
            attachment,
            length: limits.max,
            distance: (attachment - anchor).length(),
            tension: limits.impulse.max(0.0) * substeps as f32 / params.dt,
        })
    }
}

fn valid_length(length: f32) -> bool {
    length.is_finite() && length > 0.0
}
