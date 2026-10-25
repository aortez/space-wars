//! Exact cardinal poses without trigonometric rounding or implicit angle snapping.
use super::*;

/// Counterclockwise quarter turns. Components are permuted and negated exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarterTurn {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl QuarterTurn {
    /// Rotate a vector without a trigonometric approximation.
    pub fn rotate(self, v: Vec2) -> Vec2 {
        match self {
            Self::Deg0 => v,
            Self::Deg90 => Vec2::new(-v.y, v.x),
            Self::Deg180 => -v,
            Self::Deg270 => Vec2::new(v.y, -v.x),
        }
    }

    pub(super) fn rotation(self) -> rapier2d::math::Rotation {
        let n = self.rotate(Vec2::X);
        rapier2d::math::Rotation::from_cos_sin_unchecked(n.x, n.y)
    }

    // Persisted endpoint angles are read from exact rotations. This recognizes
    // those witnesses only; live admission also checks the full rotation.
    pub(super) fn from_angle(angle: f32) -> Option<Self> {
        use std::f32::consts::{FRAC_PI_2, PI};
        if angle == 0.0 {
            Some(Self::Deg0)
        } else if angle == FRAC_PI_2 {
            Some(Self::Deg90)
        } else if angle == PI || angle == -PI {
            Some(Self::Deg180)
        } else if angle == -FRAC_PI_2 {
            Some(Self::Deg270)
        } else {
            None
        }
    }
}

impl PhysicsWorld {
    /// Set an exact cardinal pose. Use before enrolling a material gravity scene.
    /// A changed pose invalidates any existing gravity continuation, just like
    /// `set_pose`. Ordinary radian-based pose construction is unchanged.
    pub fn set_quarter_turn_pose(
        &mut self,
        id: BodyId,
        position: Vec2,
        turn: QuarterTurn,
        wake_up: bool,
    ) -> bool {
        if !finite_vec2(position) {
            return false;
        }
        let Some(handle) = self.body_handle(id) else {
            return false;
        };
        let mut pose = Pose::IDENTITY;
        pose.translation = to_rapier(position);
        pose.rotation = turn.rotation();
        let body = &self.raw.bodies[handle];
        let before = *body.position();
        // Even obtaining a mutable backend body marks it as modified. Avoid
        // changing snapshot bookkeeping when both poses already match.
        if before == pose && *body.next_position() == pose {
            return true;
        }
        let body = &mut self.raw.bodies[handle];
        body.set_position(pose, wake_up);
        if *body.position() != before {
            self.invalidate_gravity(GravityInvalidation::ExternalEdit);
        }
        true
    }
}
