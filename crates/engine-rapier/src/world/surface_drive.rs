use super::*;
use rapier2d::prelude::{ActiveHooks, ContactModificationContext, PhysicsHooks};

/// A belt's velocity relative to its collider, enforced by ordinary contact
/// friction. Dynamic supports receive the solver's equal/opposite response.
/// Normals select the powered face and acceptable support orientation; neither
/// support forces nor friction coefficients are increased by the motor.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceDrive {
    pub collider: ColliderId,
    pub local_velocity: Vec2,
    pub local_normal: Vec2,
    pub min_normal_dot: f32,
    pub world_up: Vec2,
    pub min_up_dot: f32,
}

impl SurfaceDrive {
    fn valid(self) -> bool {
        finite_vec2(self.local_velocity)
            && finite_vec2(self.local_normal)
            && finite_vec2(self.world_up)
            && self.local_normal.length_squared() > 0.01
            && self.world_up.length_squared() > 0.01
            && (0.0..=1.0).contains(&self.min_normal_dot)
            && (0.0..=1.0).contains(&self.min_up_dot)
    }
}

pub(super) struct DriveHooks<'a>(pub &'a [SurfaceDrive]);

impl DriveHooks<'_> {
    pub(super) fn enable(
        &self,
        raw: &mut RapierWorld,
        indices: &HashMap<ColliderId, usize>,
        entries: &[ColliderEntry],
    ) -> Vec<(ColliderHandle, ActiveHooks)> {
        let mut restore = Vec::new();
        for drive in self.0.iter().filter(|drive| drive.valid()) {
            let Some(entry) = indices.get(&drive.collider).map(|i| &entries[*i]) else {
                continue;
            };
            if restore.iter().any(|(handle, _)| *handle == entry.handle) {
                continue;
            }
            let Some(collider) = raw.colliders.get_mut(entry.handle) else {
                continue;
            };
            restore.push((entry.handle, collider.active_hooks()));
            collider
                .set_active_hooks(collider.active_hooks() | ActiveHooks::MODIFY_SOLVER_CONTACTS);
            if drive.local_velocity != Vec2::ZERO
                && let Some(body) = collider.parent().and_then(|h| raw.bodies.get_mut(h))
            {
                body.wake_up(true);
            }
        }
        restore
    }
}

impl PhysicsHooks for DriveHooks<'_> {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let mut velocity = Vec2::ZERO;
        for (handle, sign) in [(context.collider1, 1.0), (context.collider2, -1.0)] {
            let collider = &context.colliders[handle];
            let Some(id) = decode_collider(collider.user_data) else {
                continue;
            };
            let Some(drive) = self
                .0
                .iter()
                .find(|drive| drive.collider == id && drive.valid())
            else {
                continue;
            };
            let outward = from_rapier(*context.normal) * sign;
            let local_outward = outward.rotate_radians(-collider.position().rotation.angle());
            if local_outward.dot(drive.local_normal.normalized()) < drive.min_normal_dot
                || (-outward).dot(drive.world_up.normalized()) < drive.min_up_dot
            {
                continue;
            }
            let world_velocity = drive
                .local_velocity
                .rotate_radians(collider.position().rotation.angle());
            velocity += (world_velocity - outward * world_velocity.dot(outward)) * sign;
        }
        for contact in context.solver_contacts.iter_mut() {
            contact.tangent_velocity = to_rapier(velocity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt_world(
        dynamic_floor: bool,
        angle: f32,
        height: f32,
    ) -> (PhysicsWorld, BodyId, SurfaceDrive) {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -18.0),
            ..Default::default()
        });
        let floor = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
        let tank = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);
        let collider = ColliderId::new(tank.entity, ColliderRole::PRIMARY, 0);
        world.insert_body(
            floor,
            BodySpec {
                kind: if dynamic_floor {
                    BodyKind::Dynamic
                } else {
                    BodyKind::Fixed
                },
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(floor.entity, ColliderRole::PRIMARY, 0),
                30.0,
                0.5,
            )],
        );
        world.insert_body(
            tank,
            BodySpec {
                position: Vec2::new(0.0, height),
                angle,
                ..Default::default()
            },
            &[ColliderSpec::cuboid(collider, 1.5, 0.5)],
        );
        let drive = SurfaceDrive {
            collider,
            local_velocity: Vec2::new(-2.5, 0.0),
            local_normal: -Vec2::Y,
            min_normal_dot: 0.5,
            world_up: Vec2::Y,
            min_up_dot: 40.0_f32.to_radians().cos(),
        };
        (world, tank, drive)
    }

    #[test]
    fn native_friction_drives_stops_and_replays_without_persistent_hooks() {
        let (mut world, tank, drive) = belt_world(false, 0.0, 1.01);
        for _ in 0..120 {
            world.step_with_surface_drives(1.0 / 60.0, &[drive]);
        }
        let moving = world.motion(tank).unwrap();
        assert!(
            moving.position.x > 3.0 && moving.linear_velocity.x > 2.0,
            "{moving:?}"
        );
        assert_eq!(
            world.raw.colliders[world.collider_handle(drive.collider).unwrap()].active_hooks(),
            ActiveHooks::empty()
        );
        let mut replay = world.clone();
        for _ in 0..60 {
            world.step(1.0 / 60.0);
            replay.step(1.0 / 60.0);
        }
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        assert!(world.motion(tank).unwrap().linear_velocity.x.abs() < 0.05);
    }

    #[test]
    fn no_air_or_inverted_drive_and_dynamic_support_receives_reaction() {
        for (angle, height) in [(0.0, 20.0), (std::f32::consts::PI, 1.01)] {
            let (mut world, tank, drive) = belt_world(false, angle, height);
            for _ in 0..30 {
                world.step_with_surface_drives(1.0 / 60.0, &[drive]);
            }
            assert!(world.motion(tank).unwrap().position.x.abs() < 0.01);
        }
        let (mut world, tank, drive) = belt_world(true, 0.0, 0.99);
        world.set_gravity(Vec2::ZERO);
        // Supply a real compressive contact; the belt cannot manufacture load.
        world.set_velocity(tank, Vec2::new(0.0, -1.0), 0.0, true);
        for _ in 0..10 {
            world.step_with_surface_drives(1.0 / 60.0, &[drive]);
        }
        let floor = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
        let a = world.motion(tank).unwrap().linear_velocity.x;
        let b = world.motion(floor).unwrap().linear_velocity.x;
        assert!(a > 0.01 && b < 0.0, "tank {a}, floor {b}");
        let momentum = a * world.body_mass(tank).unwrap() + b * world.body_mass(floor).unwrap();
        assert!(momentum.abs() < 0.001, "{momentum}");
    }
}
