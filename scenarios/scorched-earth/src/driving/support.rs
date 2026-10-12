use super::*;
use engine_rapier::world::SurfaceContact;

#[derive(Clone, Copy)]
pub(crate) struct SupportContext {
    angle: f32,
    allowed: bool,
    min_up: f32,
}

impl SupportContext {
    pub(crate) fn accepts(self, contact: SurfaceContact) -> bool {
        self.allowed
            && contact.separation <= 0.03
            && contact.normal.y > 0.0
            && contact.normal.y >= self.min_up
            && contact.normal.rotate_radians(-self.angle).y >= 0.5
    }
}

impl ScorchedState {
    pub(crate) fn reference_support_context(&self, player: usize) -> SupportContext {
        SupportContext {
            angle: self.physics.motion(self.tanks[player].body).unwrap().angle,
            allowed: true,
            min_up: 40.0_f32.to_radians().cos(),
        }
    }

    pub(crate) fn support_context(&self, player: usize) -> SupportContext {
        let mut context = self.reference_support_context(player);
        if let Some(gear) = &self.running_gear[player] {
            let [_, left, right] = gear.bodies();
            let span = self.physics.motion(right).unwrap().position
                - self.physics.motion(left).unwrap().position;
            // Cell corners may have 45° contact normals on a shallow hill.
            // Limit the grade across the actual wheelbase instead, and retain
            // live contacts below an upright chassis. Neither proximity rays
            // nor the chassis alone can authorize airborne propulsion.
            context.allowed = span.x > 0.0
                && span.y.abs() <= span.x * 40.0_f32.to_radians().tan()
                && context.angle.cos() >= 40.0_f32.to_radians().cos();
            context.min_up = 0.0;
        }
        context
    }

    fn support_colliders(&self, player: usize) -> [Option<ColliderId>; 2] {
        self.running_gear[player].as_ref().map_or(
            [
                Some(ColliderId::new(
                    self.tanks[player].body.entity,
                    ColliderRole::PRIMARY,
                    0,
                )),
                None,
            ],
            |gear| gear.wheel_colliders().map(Some),
        )
    }

    fn has_support_with_context(&self, player: usize, context: SupportContext) -> bool {
        self.support_colliders(player)
            .into_iter()
            .flatten()
            .any(|collider| {
                self.physics
                    .surface_contacts(collider)
                    .any(|c| context.accepts(c))
            })
    }

    pub(crate) fn has_track_support(&self, player: usize) -> bool {
        self.has_support_with_context(player, self.support_context(player))
    }

    /// Retained solely for exact diagnostic prefixes and same-state controls.
    pub(crate) fn has_reference_track_support(&self, player: usize) -> bool {
        self.has_support_with_context(player, self.reference_support_context(player))
    }
}
