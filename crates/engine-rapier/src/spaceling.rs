//! Contact-based arcade character mechanics. No terrain or gravity ownership.

use engine_core::Vec2;

use crate::world::{
    BodyId, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, SurfaceContact,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingSpec {
    pub radius: f32,
    pub half_segment: f32,
    pub walk_speed: f32,
    pub ground_acceleration: f32,
    pub air_acceleration: f32,
    pub jump_speed: f32,
    /// Minimum dot product of support normal and gravity-relative up.
    pub min_support_alignment: f32,
    pub max_angular_speed: f32,
    pub angular_acceleration: f32,
}

impl Default for SpacelingSpec {
    fn default() -> Self {
        Self {
            radius: 0.3,
            half_segment: 0.6,
            walk_speed: 5.0,
            ground_acceleration: 32.0,
            air_acceleration: 7.0,
            jump_speed: 8.0,
            min_support_alignment: 0.65,
            max_angular_speed: 5.0,
            angular_acceleration: 60.0,
        }
    }
}

impl SpacelingSpec {
    pub fn half_height(self) -> f32 {
        self.radius + self.half_segment
    }

    fn valid(self) -> bool {
        [
            self.radius,
            self.half_segment,
            self.walk_speed,
            self.ground_acceleration,
            self.air_acceleration,
            self.jump_speed,
            self.max_angular_speed,
            self.angular_acceleration,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
            && self.min_support_alignment.is_finite()
            && self.min_support_alignment > 0.0
            && self.min_support_alignment <= 1.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpacelingControl {
    pub walk: f32,
    pub jump_held: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingSnapshot {
    pub motion: BodyMotion,
    pub up: Vec2,
    pub support: Option<SurfaceContact>,
    pub relative_speed: f32,
    pub contacts: usize,
    pub jumps: u64,
}

impl SpacelingSnapshot {
    pub fn grounded(self) -> bool {
        self.support.is_some()
    }
}

/// One dynamic body and collider. Cached controller intent never integrates pose.
pub struct SpacelingAssembly {
    body: BodyId,
    collider: ColliderId,
    spec: SpacelingSpec,
    up: Vec2,
    jump_was_held: bool,
    jumps: u64,
}

impl SpacelingAssembly {
    /// Angle is the body's local +X direction; local +Y points toward the head.
    pub fn insert(
        physics: &mut PhysicsWorld,
        entity: PhysicsId,
        position: Vec2,
        angle: f32,
        spec: SpacelingSpec,
    ) -> Option<Self> {
        if !spec.valid() || physics.contains_entity(entity) {
            return None;
        }
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        let mut shape = ColliderSpec::capsule(collider, spec.half_segment, spec.radius);
        // Traction is the bounded controller, not a large passive friction torque.
        shape.friction = 0.0;
        if !physics.insert_body(
            body,
            BodySpec {
                position,
                angle,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &[shape],
        ) {
            return None;
        }
        // Averaging zero with a rough planet's friction still produces a large
        // tipping torque. The controller supplies traction, so choose zero for
        // the pair too, not just for this collider's material.
        let handle = physics.collider_handle(collider)?;
        physics
            .raw
            .colliders
            .get_mut(handle)?
            .set_friction_combine_rule(rapier2d::prelude::CoefficientCombineRule::Min);
        Some(Self {
            body,
            collider,
            spec,
            up: Vec2::new(0.0, 1.0).rotate_radians(angle),
            jump_was_held: false,
            jumps: 0,
        })
    }

    pub fn body(&self) -> BodyId {
        self.body
    }

    /// Call before the world step. Gravity itself is applied by the caller.
    /// Returns true only when this tick actually launches a supported jump.
    pub fn apply_control(
        &mut self,
        physics: &mut PhysicsWorld,
        control: SpacelingControl,
        gravity: Vec2,
        dt: f32,
    ) -> bool {
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        if gravity.x.is_finite() && gravity.y.is_finite() && gravity.length_squared() > 1e-6 {
            self.up = gravity.normalized() * -1.0;
        }
        let Some(snapshot) = self.snapshot(physics) else {
            return false;
        };
        let motion = snapshot.motion;
        let desired_angle = self.up.y.atan2(self.up.x) - std::f32::consts::FRAC_PI_2;
        let error = (desired_angle - motion.angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let desired_rate =
            (error * 12.0).clamp(-self.spec.max_angular_speed, self.spec.max_angular_speed);
        let rate = motion.angular_velocity
            + (desired_rate - motion.angular_velocity).clamp(
                -self.spec.angular_acceleration * dt,
                self.spec.angular_acceleration * dt,
            );
        physics.set_velocity(self.body, motion.linear_velocity, rate, true);

        let walk = if control.walk.is_finite() {
            control.walk.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let normal = snapshot.support.map_or(self.up, |contact| contact.normal);
        let tangent = right(normal);
        let acceleration = if snapshot.grounded() {
            self.spec.ground_acceleration
        } else {
            self.spec.air_acceleration
        };
        // Releasing movement in free flight does not provide invisible braking.
        if snapshot.grounded() || walk != 0.0 {
            let delta = (walk * self.spec.walk_speed - snapshot.relative_speed)
                .clamp(-acceleration * dt, acceleration * dt);
            physics.apply_velocity_delta(self.body, tangent * delta, true);
        }
        let jump = control.jump_held && !self.jump_was_held && snapshot.grounded();
        self.jump_was_held = control.jump_held;
        if jump {
            let support_velocity = snapshot.support.unwrap().velocity;
            let outward_speed = (motion.linear_velocity - support_velocity).dot(self.up);
            physics.apply_velocity_delta(
                self.body,
                self.up * (self.spec.jump_speed - outward_speed).max(0.0),
                true,
            );
            self.jumps += 1;
        }
        jump
    }

    pub fn snapshot(&self, physics: &PhysicsWorld) -> Option<SpacelingSnapshot> {
        let motion = physics.motion(self.body)?;
        let mut support: Option<SurfaceContact> = None;
        let mut contacts = 0;
        for contact in physics.surface_contacts(self.collider) {
            contacts += 1;
            let offset = contact.position - motion.position;
            let point_velocity =
                motion.linear_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity;
            let alignment = contact.normal.dot(self.up);
            if contact.separation > 0.04
                || alignment < self.spec.min_support_alignment
                || offset.dot(self.up) > -self.spec.half_segment * 0.5
                || (point_velocity - contact.velocity).dot(contact.normal) > 1.0
            {
                continue;
            }
            // Stable ties, independent of broad-phase pair traversal order.
            let better = support.is_none_or(|previous| {
                alignment > previous.normal.dot(self.up)
                    || (alignment == previous.normal.dot(self.up)
                        && (
                            contact.collider,
                            contact.position.x.to_bits(),
                            contact.position.y.to_bits(),
                        ) < (
                            previous.collider,
                            previous.position.x.to_bits(),
                            previous.position.y.to_bits(),
                        ))
            });
            if better {
                support = Some(contact);
            }
        }
        let normal = support.map_or(self.up, |contact| contact.normal);
        let relative_velocity = support.map_or(motion.linear_velocity, |contact| {
            let offset = contact.position - motion.position;
            motion.linear_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity
                - contact.velocity
        });
        Some(SpacelingSnapshot {
            motion,
            up: self.up,
            support,
            relative_speed: relative_velocity.dot(right(normal)),
            contacts,
            jumps: self.jumps,
        })
    }
}

fn right(up: Vec2) -> Vec2 {
    Vec2::new(up.y, -up.x)
}

#[cfg(test)]
mod tests;
