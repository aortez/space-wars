//! Contact-based arcade character mechanics. No terrain or gravity ownership.

use engine_core::Vec2;

use crate::world::{
    BodyId, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    CollisionGroups, PhysicsId, PhysicsWorld, SurfaceContact,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingSpec {
    pub collision_groups: CollisionGroups,
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
    pub balance: SpacelingBalanceSpec,
}

/// Arcade thresholds, independent of terrain identity and controller source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingBalanceSpec {
    /// Unexpected linear velocity change between controller ticks, in units/s.
    pub knockdown_velocity_change: f32,
    /// Angular speed relative to support (world-frame when airborne), in rad/s.
    pub knockdown_angular_speed: f32,
    pub settle_speed: f32,
    pub settle_angular_speed: f32,
    pub settle_seconds: f32,
    pub recovery_seconds: f32,
    /// Brief gaps during physical self-righting do not restart the whole attempt.
    pub support_grace_seconds: f32,
}

impl Default for SpacelingBalanceSpec {
    fn default() -> Self {
        Self {
            knockdown_velocity_change: 12.0,
            knockdown_angular_speed: 8.0,
            settle_speed: 2.0,
            settle_angular_speed: 2.0,
            settle_seconds: 0.25,
            recovery_seconds: 0.8,
            support_grace_seconds: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpacelingBalance {
    #[default]
    Balanced,
    KnockedDown,
    Recovering,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacelingDisturbance {
    pub velocity_change: f32,
    pub angular_speed: f32,
}

impl Default for SpacelingSpec {
    fn default() -> Self {
        Self {
            collision_groups: CollisionGroups::ALL,
            radius: 0.3,
            half_segment: 0.6,
            walk_speed: 5.0,
            ground_acceleration: 32.0,
            air_acceleration: 7.0,
            jump_speed: 8.0,
            min_support_alignment: 0.65,
            max_angular_speed: 5.0,
            angular_acceleration: 60.0,
            balance: SpacelingBalanceSpec::default(),
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
            self.balance.knockdown_velocity_change,
            self.balance.knockdown_angular_speed,
            self.balance.settle_speed,
            self.balance.settle_angular_speed,
            self.balance.settle_seconds,
            self.balance.recovery_seconds,
            self.balance.support_grace_seconds,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
            && self.min_support_alignment.is_finite()
            && self.min_support_alignment > 0.0
            && self.min_support_alignment <= 1.0
            && self.balance.knockdown_angular_speed > self.max_angular_speed
            && self.balance.settle_angular_speed < self.balance.knockdown_angular_speed
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
    pub balance: SpacelingBalance,
    pub recovery_progress: f32,
    pub settled_seconds: f32,
    pub knockdowns: u64,
    pub recoveries: u64,
    pub last_knockdown: Option<SpacelingDisturbance>,
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
    balance: SpacelingBalance,
    settled_seconds: f32,
    recovery_seconds: f32,
    unsupported_seconds: f32,
    recovery_support: Option<ColliderId>,
    expected_velocity: Option<Vec2>,
    knockdowns: u64,
    recoveries: u64,
    last_knockdown: Option<SpacelingDisturbance>,
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
        shape.collision_groups = spec.collision_groups;
        shape.solver_groups = spec.collision_groups;
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
            balance: SpacelingBalance::Balanced,
            settled_seconds: 0.0,
            recovery_seconds: 0.0,
            unsupported_seconds: 0.0,
            recovery_support: None,
            expected_velocity: None,
            knockdowns: 0,
            recoveries: 0,
            last_knockdown: None,
        })
    }

    pub fn body(&self) -> BodyId {
        self.body
    }

    /// Call once before each world step. The caller applies the supplied gravity
    /// acceleration over that same `dt`. The previous commanded velocity plus
    /// gravity is a disturbance reference only, never a second pose integrator.
    /// Solver impacts are noticed on the next controller tick; impulses applied
    /// before this call are noticed immediately.
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
        let gravity = if gravity.x.is_finite() && gravity.y.is_finite() {
            gravity
        } else {
            Vec2::ZERO
        };
        let has_gravity = gravity.length_squared() > 1e-6;
        if has_gravity {
            self.up = gravity.normalized() * -1.0;
        }
        let Some(snapshot) = self.snapshot(physics) else {
            return false;
        };
        let motion = snapshot.motion;
        self.update_balance(physics, &snapshot, has_gravity, dt);
        // Consume held jump even while disabled; recovery must not buffer it.
        let jump_pressed = control.jump_held && !self.jump_was_held;
        self.jump_was_held = control.jump_held;
        if self.balance == SpacelingBalance::KnockedDown || !has_gravity {
            self.expected_velocity = Some(motion.linear_velocity + gravity * dt);
            return false;
        }

        let recovering = self.balance == SpacelingBalance::Recovering;
        let strength = if recovering {
            (self.recovery_seconds / self.spec.balance.recovery_seconds).clamp(0.1, 1.0)
        } else if snapshot.grounded() {
            1.0
        } else {
            0.15
        };
        let desired_rate = (angle_error(self.up, motion.angle) * 12.0)
            .clamp(-self.spec.max_angular_speed, self.spec.max_angular_speed);
        let rate = motion.angular_velocity
            + (desired_rate - motion.angular_velocity).clamp(
                -self.spec.angular_acceleration * strength * dt,
                self.spec.angular_acceleration * strength * dt,
            );
        physics.set_velocity(self.body, motion.linear_velocity, rate, true);

        let walk = if !recovering && control.walk.is_finite() {
            control.walk.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let normal = snapshot.support.map_or(self.up, |contact| contact.normal);
        let tangent = right(normal);
        let acceleration = if snapshot.grounded() {
            self.spec.ground_acceleration * if recovering { strength } else { 1.0 }
        } else {
            self.spec.air_acceleration
        };
        // Releasing movement in free flight does not provide invisible braking.
        if snapshot.grounded() || walk != 0.0 {
            let delta = (walk * self.spec.walk_speed - snapshot.relative_speed)
                .clamp(-acceleration * dt, acceleration * dt);
            physics.apply_velocity_delta(self.body, tangent * delta, true);
        }
        let jump = jump_pressed && !recovering && snapshot.grounded();
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
        self.expected_velocity = physics
            .motion(self.body)
            .map(|motion| motion.linear_velocity + gravity * dt);
        jump
    }

    fn update_balance(
        &mut self,
        physics: &mut PhysicsWorld,
        snapshot: &SpacelingSnapshot,
        has_gravity: bool,
        dt: f32,
    ) {
        let support_spin = snapshot
            .support
            .map_or(0.0, |support| support.angular_velocity);
        let spin = (snapshot.motion.angular_velocity - support_spin).abs();
        let shock = self.expected_velocity.map_or(0.0, |velocity| {
            (snapshot.motion.linear_velocity - velocity).length()
        });
        let severe = shock >= self.spec.balance.knockdown_velocity_change
            || spin >= self.spec.balance.knockdown_angular_speed;
        if severe && self.balance != SpacelingBalance::KnockedDown {
            self.set_balance(physics, SpacelingBalance::KnockedDown);
            self.knockdowns += 1;
            self.last_knockdown = Some(SpacelingDisturbance {
                velocity_change: shock,
                angular_speed: spin,
            });
        }
        let stable = has_gravity
            && snapshot.support.is_some_and(|support| {
                contact_relative_velocity(snapshot.motion, support).length()
                    <= self.spec.balance.settle_speed
            })
            && spin <= self.spec.balance.settle_angular_speed
            && !severe;
        match self.balance {
            SpacelingBalance::Balanced => {}
            SpacelingBalance::KnockedDown => {
                let support = snapshot.support.map(|contact| contact.collider);
                if !stable || support != self.recovery_support {
                    self.settled_seconds = 0.0;
                }
                self.recovery_support = support;
                if stable {
                    self.settled_seconds += dt;
                    if self.settled_seconds >= self.spec.balance.settle_seconds {
                        self.set_balance(physics, SpacelingBalance::Recovering);
                    }
                }
            }
            SpacelingBalance::Recovering => {
                let support = snapshot.support.map(|contact| contact.collider);
                if support.is_none() {
                    self.unsupported_seconds += dt;
                } else {
                    self.unsupported_seconds = 0.0;
                }
                let support_removed = self
                    .recovery_support
                    .is_none_or(|collider| physics.collider_handle(collider).is_none());
                if !has_gravity
                    || support_removed
                    || (support.is_some() && support != self.recovery_support)
                    || self.unsupported_seconds > self.spec.balance.support_grace_seconds
                {
                    self.set_balance(physics, SpacelingBalance::KnockedDown);
                } else if support.is_some() {
                    self.recovery_seconds += dt;
                    if self.recovery_seconds >= self.spec.balance.recovery_seconds
                        && stable
                        && angle_error(self.up, snapshot.motion.angle).abs() < 0.15
                        && spin < 0.8
                    {
                        self.set_balance(physics, SpacelingBalance::Balanced);
                        self.recoveries += 1;
                    }
                }
            }
        }
    }

    fn set_balance(&mut self, physics: &mut PhysicsWorld, balance: SpacelingBalance) {
        self.balance = balance;
        self.recovery_seconds = 0.0;
        self.unsupported_seconds = 0.0;
        self.settled_seconds = 0.0;
        if balance != SpacelingBalance::Recovering {
            self.recovery_support = None;
        }
        // A limp body needs ordinary contact friction to settle. Walking and
        // recovering supply their own bounded traction, as in the baseline.
        if let Some(collider) = physics
            .collider_handle(self.collider)
            .and_then(|handle| physics.raw.colliders.get_mut(handle))
        {
            collider.set_friction(if balance == SpacelingBalance::KnockedDown {
                0.6
            } else {
                0.0
            });
        }
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
                // A prone capsule can be supported too; support is not balance.
                || offset.dot(self.up) > -self.spec.radius * 0.25
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
            balance: self.balance,
            recovery_progress: if self.balance == SpacelingBalance::Recovering {
                (self.recovery_seconds / self.spec.balance.recovery_seconds).min(1.0)
            } else {
                0.0
            },
            settled_seconds: self.settled_seconds,
            knockdowns: self.knockdowns,
            recoveries: self.recoveries,
            last_knockdown: self.last_knockdown,
        })
    }
}

fn angle_error(up: Vec2, angle: f32) -> f32 {
    let desired = up.y.atan2(up.x) - std::f32::consts::FRAC_PI_2;
    (desired - angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

fn contact_relative_velocity(motion: BodyMotion, contact: SurfaceContact) -> Vec2 {
    let offset = contact.position - motion.position;
    motion.linear_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity
        - contact.velocity
}

fn right(up: Vec2) -> Vec2 {
    Vec2::new(up.y, -up.x)
}

#[cfg(test)]
mod tests;
