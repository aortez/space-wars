//! One ordinary dynamic box, deliberately outside the terrain material ledger.
use super::*;
use engine_rapier::world::{
    BodyRole, ColliderId, ColliderMassProperties, ColliderRole, ColliderSpec, RayCastOptions,
};

const ID: PhysicsId = PhysicsId::new(u64::MAX - 1);
pub const HALF: Vec2 = Vec2::new(0.75, 0.45);
pub(super) fn body() -> BodyId {
    BodyId::new(ID, BodyRole::PRIMARY)
}
pub(super) fn collider() -> ColliderId {
    ColliderId::new(ID, ColliderRole::PRIMARY, 0)
}

#[derive(Debug, Clone, Copy)]
pub struct ProbeSnapshot {
    pub motion: BodyMotion,
    pub half_extents: Vec2,
    pub grain_contacts: usize,
    pub ground_contacts: usize,
    pub relative_speed: f32,
}

impl BlastLab {
    pub fn up_at(&self, position: Vec2) -> Vec2 {
        match self.config.fixture {
            Fixture::Flat | Fixture::Slope => Vec2::Y,
            Fixture::MovingPlanet => {
                let radial = position - self.ground_motion().position;
                if radial.length_squared() > 0.0001 {
                    radial.normalized()
                } else {
                    Vec2::Y
                }
            }
        }
    }

    /// Drop/reposition the single test box above the actual first surface along
    /// gravity. Queries include loose grains and changed terrain, not the old
    /// field envelope. This does not create or consume any terrain material.
    pub fn drop_probe(&mut self, aim: Vec2) -> Result<(), TerrainError> {
        if !aim.x.is_finite() || !aim.y.is_finite() || aim.length() > 1000.0 {
            return Err(TerrainError("invalid probe aim"));
        }
        let up = self.up_at(aim);
        let hit = self.physics.cast_ray(
            aim + up * 12.0,
            -up,
            RayCastOptions {
                max_distance: 24.0,
                exclude_entity: Some(ID),
                ..RayCastOptions::default()
            },
        );
        let position = hit.map_or(aim, |hit| hit.point) + up * 3.0;
        let ground = self.ground_motion();
        let velocity = self
            .physics
            .velocity_at_point(self.bodies[0].assembly.body(), position)
            .unwrap();
        let angle = -up.x.atan2(up.y);
        if self.probe {
            self.physics.set_pose(body(), position, angle, true);
            self.physics
                .set_velocity(body(), velocity, ground.angular_velocity, true);
        } else {
            let mut shape = ColliderSpec::cuboid(collider(), HALF.x, HALF.y);
            shape.density = 0.0;
            shape.friction = 0.8;
            shape.restitution = 0.0;
            shape.mass_properties = Some(ColliderMassProperties {
                mass: 1.0,
                center: Vec2::ZERO,
                inertia: (HALF.x.powi(2) + HALF.y.powi(2)) / 3.0,
            });
            if !self.physics.insert_body(
                body(),
                BodySpec {
                    position,
                    angle,
                    linear_velocity: velocity,
                    angular_velocity: ground.angular_velocity,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[shape],
            ) {
                return Err(TerrainError("probe insertion failed"));
            }
            self.probe = true;
        }
        Ok(())
    }

    pub fn probe_snapshot(&self) -> Option<ProbeSnapshot> {
        let motion = self.physics.motion(body())?;
        let up = self.up_at(motion.position);
        let mut snapshot = ProbeSnapshot {
            motion,
            half_extents: HALF,
            grain_contacts: 0,
            ground_contacts: 0,
            relative_speed: motion.linear_velocity.distance_to(
                self.physics
                    .velocity_at_point(self.bodies[0].assembly.body(), motion.position)
                    .unwrap(),
            ),
        };
        for contact in self.physics.surface_contacts(collider()) {
            if contact.separation > 0.02 || contact.normal.dot(up) < 0.2 {
                continue;
            }
            if self
                .grains
                .iter()
                .any(|grain| grain.id() == contact.collider.entity)
            {
                snapshot.grain_contacts += 1;
            }
            if self
                .bodies
                .iter()
                .any(|body| body.id == contact.collider.entity)
            {
                snapshot.ground_contacts += 1;
            }
        }
        Some(snapshot)
    }
}
