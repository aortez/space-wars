//! Domain assembly helpers for a wheeled rover on circular terrain.

use engine_core::Vec2;
use rapier2d::prelude::{JointAxesMask, JointAxis, MotorModel, PinSlotJointBuilder, Vector};
use serde::{Deserialize, Serialize};

use crate::world::{
    BodyId, BodyKind, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    ContactPoint, JointId, JointRole, PhysicsId, PhysicsWorld,
};

const PLANET_BODY_ROLE: BodyRole = BodyRole::new(10);
const PLANET_SURFACE_ROLE: ColliderRole = ColliderRole::new(10);
const ROVER_CHASSIS_BODY_ROLE: BodyRole = BodyRole::new(20);
const ROVER_LEFT_WHEEL_BODY_ROLE: BodyRole = BodyRole::new(21);
const ROVER_RIGHT_WHEEL_BODY_ROLE: BodyRole = BodyRole::new(22);
const ROVER_CHASSIS_COLLIDER_ROLE: ColliderRole = ColliderRole::new(20);
const ROVER_WHEEL_COLLIDER_ROLE: ColliderRole = ColliderRole::new(21);
const ROVER_LEFT_SUSPENSION_ROLE: JointRole = JointRole::new(20);
const ROVER_RIGHT_SUSPENSION_ROLE: JointRole = JointRole::new(21);

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

    /// Local-space trapezoid used by collision and presentation.
    pub fn local_vertices(self) -> [Vec2; 4] {
        let top_half_width = self.half_width * 0.1;
        [
            Vec2::new(-self.half_width, -self.half_height),
            Vec2::new(self.half_width, -self.half_height),
            Vec2::new(top_half_width, self.half_height),
            Vec2::new(-top_half_width, self.half_height),
        ]
    }

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

#[derive(Debug, Clone, PartialEq)]
pub struct RoverSnapshot {
    pub chassis: BodyMotion,
    pub wheels: [BodyMotion; 2],
    pub suspension_anchors: [Vec2; 2],
    pub contacts: Vec<ContactPoint>,
}

/// Stable description of one kinematic circular planet assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetAssembly {
    entity: PhysicsId,
    body: BodyId,
}

impl PlanetAssembly {
    pub fn insert(
        physics: &mut PhysicsWorld,
        entity: PhysicsId,
        spec: PlanetSpec,
        bumps: &[BumpSpec],
    ) -> Option<Self> {
        if physics.contains_entity(entity)
            || !valid_planet_spec(spec)
            || bumps.iter().copied().any(|bump| !valid_bump_spec(bump))
        {
            return None;
        }

        let body = BodyId::new(entity, PLANET_BODY_ROLE);
        let mut surface =
            ColliderSpec::ball(ColliderId::new(entity, PLANET_SURFACE_ROLE, 0), spec.radius);
        surface.friction = 1.25;
        surface.restitution = 0.0;
        if !physics.insert_body(
            body,
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: spec.center,
                angle: spec.angle,
                ..BodySpec::default()
            },
            &[surface],
        ) {
            return None;
        }

        for (index, bump) in bumps.iter().copied().enumerate() {
            let normal = Vec2::new(bump.surface_angle.cos(), bump.surface_angle.sin());
            let mut collider = ColliderSpec::convex_polygon(
                ColliderId::new(entity, PLANET_SURFACE_ROLE, u16::try_from(index + 1).ok()?),
                bump.local_vertices().to_vec(),
            );
            collider.local_position = normal * bump.radial_center_distance(spec.radius);
            collider.local_angle = bump.surface_angle - std::f32::consts::FRAC_PI_2;
            collider.friction = 1.25;
            collider.restitution = 0.0;
            if !physics.insert_collider(body, &collider) {
                physics.remove_entity(entity);
                return None;
            }
        }

        Some(Self { entity, body })
    }

    pub fn entity(&self) -> PhysicsId {
        self.entity
    }

    pub fn body(&self) -> BodyId {
        self.body
    }

    pub fn set_next_pose(&self, physics: &mut PhysicsWorld, center: Vec2, angle: f32) -> bool {
        physics.set_next_kinematic_pose(self.body, center, angle)
    }
}

/// Stable IDs and controller parameters for one articulated rover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoverAssembly {
    entity: PhysicsId,
    chassis: BodyId,
    wheels: [BodyId; 2],
    chassis_collider: ColliderId,
    wheel_colliders: [ColliderId; 2],
    suspension_joints: [JointId; 2],
    suspension_anchors: [Vec2; 2],
    target_wheel_speed: f32,
    motor_torque: f32,
    brake_torque: f32,
}

impl RoverAssembly {
    pub fn insert(
        physics: &mut PhysicsWorld,
        entity: PhysicsId,
        planet: &PlanetAssembly,
        planet_spec: PlanetSpec,
        spec: RoverSpec,
    ) -> Option<Self> {
        if entity == planet.entity()
            || physics.contains_entity(entity)
            || !physics.contains_body(planet.body())
            || !valid_rover_spec(spec)
        {
            return None;
        }

        let normal = Vec2::new(spec.surface_angle.cos(), spec.surface_angle.sin());
        let tangent_angle = spec.surface_angle - std::f32::consts::FRAC_PI_2;
        let wheel_surface_distance = planet_spec.radius + spec.wheel_radius + 0.03;
        let wheel_center = planet_spec.center + normal * wheel_surface_distance;
        let anchor_to_center = spec.suspension_rest_length - spec.suspension_anchor_height;
        let chassis_center = wheel_center + normal * anchor_to_center;

        let chassis = BodyId::new(entity, ROVER_CHASSIS_BODY_ROLE);
        let chassis_collider = ColliderId::new(entity, ROVER_CHASSIS_COLLIDER_ROLE, 0);
        let mut chassis_shape = ColliderSpec::cuboid(
            chassis_collider,
            spec.chassis_half_width,
            spec.chassis_half_height,
        );
        chassis_shape.density = 1.0;
        chassis_shape.friction = 0.7;
        chassis_shape.restitution = 0.0;
        if !physics.insert_body(
            chassis,
            BodySpec {
                position: chassis_center,
                angle: tangent_angle,
                linear_damping: 0.05,
                angular_damping: 0.25,
                ccd_enabled: true,
                additional_solver_iterations: 4,
                ..BodySpec::default()
            },
            &[chassis_shape],
        ) {
            return None;
        }

        let suspension_anchors = [
            Vec2::new(-spec.wheel_offset, spec.suspension_anchor_height),
            Vec2::new(spec.wheel_offset, spec.suspension_anchor_height),
        ];
        let tangent = Vec2::new(tangent_angle.cos(), tangent_angle.sin());
        let wheels = [
            BodyId::new(entity, ROVER_LEFT_WHEEL_BODY_ROLE),
            BodyId::new(entity, ROVER_RIGHT_WHEEL_BODY_ROLE),
        ];
        let wheel_colliders = [
            ColliderId::new(entity, ROVER_WHEEL_COLLIDER_ROLE, 0),
            ColliderId::new(entity, ROVER_WHEEL_COLLIDER_ROLE, 1),
        ];
        let suspension_joints = [
            JointId::new(entity, ROVER_LEFT_SUSPENSION_ROLE),
            JointId::new(entity, ROVER_RIGHT_SUSPENSION_ROLE),
        ];

        for index in 0..2 {
            let side = if index == 0 { -1.0 } else { 1.0 };
            let mut wheel_shape = ColliderSpec::ball(wheel_colliders[index], spec.wheel_radius);
            wheel_shape.density = 0.8;
            wheel_shape.friction = 1.6;
            wheel_shape.restitution = 0.0;
            if !physics.insert_body(
                wheels[index],
                BodySpec {
                    position: wheel_center + tangent * (side * spec.wheel_offset),
                    angle: tangent_angle,
                    linear_damping: 0.02,
                    angular_damping: 0.01,
                    ccd_enabled: true,
                    additional_solver_iterations: 4,
                    ..BodySpec::default()
                },
                &[wheel_shape],
            ) {
                physics.remove_entity(entity);
                return None;
            }

            let mut suspension = PinSlotJointBuilder::new(Vector::new(0.0, -1.0))
                .local_anchor1(Vector::new(
                    suspension_anchors[index].x,
                    suspension_anchors[index].y,
                ))
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
            if !physics.insert_raw_joint(
                suspension_joints[index],
                chassis,
                wheels[index],
                suspension.into(),
            ) {
                physics.remove_entity(entity);
                return None;
            }
        }

        Some(Self {
            entity,
            chassis,
            wheels,
            chassis_collider,
            wheel_colliders,
            suspension_joints,
            suspension_anchors,
            target_wheel_speed: spec.wheel_target_speed,
            motor_torque: spec.wheel_motor_torque,
            brake_torque: spec.wheel_brake_torque,
        })
    }

    pub fn entity(&self) -> PhysicsId {
        self.entity
    }

    pub fn bodies(&self) -> [BodyId; 3] {
        [self.chassis, self.wheels[0], self.wheels[1]]
    }

    pub fn set_control(&self, physics: &mut PhysicsWorld, control: RoverControl) -> bool {
        let throttle = if control.throttle.is_finite() {
            control.throttle.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        for joint_id in self.suspension_joints {
            let Some(joint) = physics.impulse_joint_mut(joint_id) else {
                return false;
            };
            if control.brake {
                joint
                    .data
                    .set_motor_velocity(JointAxis::AngX, 0.0, 1.0)
                    .set_motor_max_force(JointAxis::AngX, self.brake_torque);
                joint
                    .data
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased);
            } else if throttle.abs() > f32::EPSILON {
                joint
                    .data
                    .set_motor_velocity(JointAxis::AngX, -throttle * self.target_wheel_speed, 1.0)
                    .set_motor_max_force(JointAxis::AngX, self.motor_torque);
                joint
                    .data
                    .set_motor_model(JointAxis::AngX, MotorModel::ForceBased);
            } else {
                joint.data.motor_axes.remove(JointAxesMask::ANG_X);
            }
        }
        true
    }

    pub fn apply_acceleration_field(
        &self,
        physics: &mut PhysicsWorld,
        acceleration_at: impl Fn(Vec2) -> Vec2,
    ) -> bool {
        for body in self.bodies() {
            let Some(motion) = physics.motion(body) else {
                return false;
            };
            let acceleration = acceleration_at(motion.position);
            if !physics.apply_acceleration(body, acceleration, true) {
                return false;
            }
        }
        true
    }

    pub fn snapshot(&self, physics: &PhysicsWorld) -> Option<RoverSnapshot> {
        let chassis = physics.motion(self.chassis)?;
        let wheels = [
            physics.motion(self.wheels[0])?,
            physics.motion(self.wheels[1])?,
        ];
        let suspension_anchors = self
            .suspension_anchors
            .map(|anchor| chassis.position + anchor.rotate_radians(chassis.angle));
        let mut contacts = Vec::new();
        for collider in self
            .wheel_colliders
            .into_iter()
            .chain(std::iter::once(self.chassis_collider))
        {
            contacts.extend(physics.contact_points(collider));
        }
        Some(RoverSnapshot {
            chassis,
            wheels,
            suspension_anchors,
            contacts,
        })
    }

    pub fn remove(self, physics: &mut PhysicsWorld) -> bool {
        physics.remove_entity(self.entity)
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
        spec.surface_angle,
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
        && spec.suspension_travel < spec.suspension_rest_length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{PhysicsWorld, PhysicsWorldConfig};

    const PLANET: PhysicsId = PhysicsId::new(1);
    const ROVER: PhysicsId = PhysicsId::new(2);

    fn gravity(position: Vec2) -> Vec2 {
        let inward = -position;
        if inward.length_squared() > f32::EPSILON {
            inward.normalized() * 18.0
        } else {
            Vec2::ZERO
        }
    }

    fn test_world() -> (PhysicsWorld, PlanetSpec, PlanetAssembly, RoverAssembly) {
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 2,
            ..PhysicsWorldConfig::default()
        });
        let planet_spec = PlanetSpec {
            center: Vec2::ZERO,
            radius: 20.0,
            angle: 0.0,
        };
        let planet = PlanetAssembly::insert(&mut physics, PLANET, planet_spec, &[]).unwrap();
        let rover = RoverAssembly::insert(
            &mut physics,
            ROVER,
            &planet,
            planet_spec,
            RoverSpec::default(),
        )
        .unwrap();
        (physics, planet_spec, planet, rover)
    }

    #[test]
    fn assembly_uses_the_canonical_world_and_settles() {
        let (mut physics, _, _, rover) = test_world();
        for _ in 0..240 {
            physics.clear_forces();
            assert!(rover.apply_acceleration_field(&mut physics, gravity));
            physics.step(1.0 / 60.0);
        }
        let snapshot = rover.snapshot(&physics).unwrap();
        assert!(!snapshot.contacts.is_empty());
        assert!(snapshot.chassis.position.length() > 20.0);
    }

    #[test]
    fn assembly_ids_remain_valid_across_world_snapshot_restore() {
        let (mut physics, _, _, rover) = test_world();
        for _ in 0..60 {
            physics.clear_forces();
            rover.apply_acceleration_field(&mut physics, gravity);
            physics.step(1.0 / 60.0);
        }
        let bytes = physics.snapshot_bytes().unwrap();
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();

        for _ in 0..60 {
            physics.clear_forces();
            restored.clear_forces();
            rover.apply_acceleration_field(&mut physics, gravity);
            rover.apply_acceleration_field(&mut restored, gravity);
            physics.step(1.0 / 60.0);
            restored.step(1.0 / 60.0);
        }
        assert_eq!(rover.snapshot(&physics), rover.snapshot(&restored));
    }
}
