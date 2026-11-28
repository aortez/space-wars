//! Polar inputs measured in body lengths and seconds, in the observing actor's frame.
use super::*;

mod creature;
pub use creature::{
    CREATURE_OBJECT_WIDTH, CREATURE_SELF_FEATURES, CreatureBody, CreatureContext,
    CreatureObservation,
};

pub const BAND_EDGES_BODY_LENGTHS: [f32; 24] = [
    0., 0.25, 0.5, 0.75, 1., 1.375, 1.75, 2.25, 2.875, 3.625, 4.5, 5.5, 6.75, 8.25, 10., 12.,
    14.25, 16.75, 19.5, 22.5, 25.75, 29.25, 33.25, 37.5,
];
/// Local distance, speed and acceleration scales in body lengths and seconds.
pub const DISTANCE_SCALE: f32 = BAND_EDGES_BODY_LENGTHS[23];
pub const SPEED_SCALE: f32 = 8.75;
pub const ACCELERATION_SCALE: f32 = 5.625;
pub const SPIN_SCALE: f32 = 1.8;
pub const TIME_SCALE: f32 = 30.;
pub const OBJECT_CAPACITY: usize = 32;

/// Columns 1 through 4 also occur in the saved flight schema. New kinds
/// extend the creature vocabulary after them. Column zero is presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum ObjectKind {
    Planet = 1,
    Sun,
    EnclosingEdge,
    OtherSolid,
    ClockEntrance,
    ClockExit,
}

impl PolarConfig {
    pub fn body_relative(body_length: f32) -> Self {
        Self {
            sectors: 64,
            radial_edges: BAND_EDGES_BODY_LENGTHS
                .iter()
                .map(|edge| edge * body_length)
                .collect(),
            angular_samples: 2,
            radial_samples: 2,
        }
    }
}

impl PolarSensor {
    /// Prepare once per body size. The conservative overlap margin scales with
    /// the geometry, too. At an eight-unit hull it is the original 0.001.
    pub fn for_body_length(body_length: f32) -> Result<Self, &'static str> {
        Self::with_margin(
            PolarConfig::body_relative(body_length),
            body_length * 0.000125,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Observer {
    pub origin: Vec2,
    pub origin_velocity: Vec2,
    /// Rotation taking local +x to world space.
    pub angle: f32,
    pub body_length: f32,
}

impl Observer {
    pub fn upright_angle(gravity: Vec2) -> f32 {
        if gravity == Vec2::ZERO {
            0.0
        } else {
            gravity.x.atan2(-gravity.y)
        }
    }

    pub fn local(self, vector: Vec2) -> Vec2 {
        vector.rotate_radians(-self.angle)
    }

    pub fn displacement(self, delta: Vec2) -> Vec2 {
        self.local(delta) / (DISTANCE_SCALE * self.body_length)
    }

    pub fn speed(self, velocity: Vec2) -> Vec2 {
        self.local(velocity) / (SPEED_SCALE * self.body_length)
    }

    pub fn motion_inputs(self, motion: PolicyMotion) -> [f32; 16] {
        let goal = self.displacement(motion.goal - motion.center);
        let approach = self.speed(motion.goal_velocity - motion.center_velocity);
        let velocity = self.speed(motion.center_velocity);
        let origin_velocity = self.speed(self.origin_velocity);
        let offset = self.displacement(motion.center - self.origin);
        let gravity = self.local(motion.gravity) / (ACCELERATION_SCALE * self.body_length);
        let frame_velocity = self.speed(motion.frame_velocity);
        [
            goal.x,
            goal.y,
            approach.x,
            approach.y,
            velocity.x,
            velocity.y,
            origin_velocity.x,
            origin_velocity.y,
            offset.x,
            offset.y,
            motion.spin / SPIN_SCALE,
            gravity.x,
            gravity.y,
            frame_velocity.x,
            frame_velocity.y,
            motion.frame_spin / SPIN_SCALE,
        ]
    }

    pub fn object_inputs(
        self,
        world: &PhysicsWorld,
        record: BodyMotionRecord,
        extent: ObjectExtent,
    ) -> Result<[f32; 11], &'static str> {
        let motion = record.motion;
        let position = self.local(motion.position - self.origin) / extent.distance_scale;
        let velocity = if world.body_kind(record.id) == Some(BodyKind::Fixed) {
            Vec2::ZERO
        } else {
            world
                .velocity_at_point(record.id, motion.position)
                .ok_or("missing object origin velocity")?
        };
        let velocity = self.speed(velocity - self.origin_velocity);
        let angle = motion.angle - self.angle;
        Ok([
            position.x,
            position.y,
            velocity.x,
            velocity.y,
            angle.sin(),
            angle.cos(),
            motion.angular_velocity / SPIN_SCALE,
            extent.radius / extent.distance_scale,
            extent.hazard_radius / extent.distance_scale,
            f32::from(extent.hazard_enabled),
            f32::from(extent.active_frame),
        ])
    }

    pub fn observe(
        self,
        world: &PhysicsWorld,
        sensor: &PolarSensor,
        filter: PolarFilter<'_>,
    ) -> Result<PolicyGeometry, &'static str> {
        if !finite_vec2(self.origin_velocity)
            || sensor.config() != &PolarConfig::body_relative(self.body_length)
        {
            return Err("incompatible observer or polar sensor scale");
        }
        let geometry = sensor.observe(world, self.origin, self.angle, filter)?;
        let distance = DISTANCE_SCALE * self.body_length;
        let mut rays = Vec::with_capacity(geometry.rays.len());
        let mut ray_sources = Vec::with_capacity(geometry.rays.len());
        for ray in geometry.rays {
            rays.push(match (ray.distance, ray.surface_velocity, ray.collider) {
                (Some(d), Some(v), Some(_)) => {
                    let v = self.speed(v - self.origin_velocity);
                    [1.0, d / distance, v.x, v.y]
                }
                (None, None, None) => [0.0; 4],
                _ => return Err("inconsistent polar ray hit"),
            });
            ray_sources.push(ray.collider);
        }
        let nearest = match (
            geometry.nearest_distance,
            geometry.nearest_point,
            geometry.nearest_surface_velocity,
            geometry.nearest_collider,
        ) {
            (Some(d), Some(point), Some(v), Some(_)) => {
                let point = self.displacement(point - self.origin);
                let v = self.speed(v - self.origin_velocity);
                [1.0, d / distance, point.x, point.y, v.x, v.y]
            }
            (None, None, None, None) => [0.0; 6],
            _ => return Err("inconsistent nearest polar hit"),
        };
        Ok(PolicyGeometry {
            cells: geometry
                .cells
                .into_iter()
                .map(|c| [c.sampled_occupancy, c.overlap_coverage])
                .collect(),
            rays,
            nearest,
            ray_sources,
            nearest_source: geometry.nearest_collider,
        })
    }
}

/// World-space facts at one completed step. Frame motion is the motion of
/// the support or control frame at the actor, including its angular velocity.
pub struct PolicyMotion {
    pub center: Vec2,
    pub center_velocity: Vec2,
    pub spin: f32,
    pub gravity: Vec2,
    pub frame_velocity: Vec2,
    pub frame_spin: f32,
    pub goal: Vec2,
    pub goal_velocity: Vec2,
}

pub struct ObjectExtent {
    /// The legacy flight export uses the world radius for object positions
    /// and extents. Creatures use the same scale as their local geometry.
    pub distance_scale: f32,
    pub radius: f32,
    pub hazard_radius: f32,
    pub hazard_enabled: bool,
    pub active_frame: bool,
}

#[derive(Debug, PartialEq)]
pub struct PolicyGeometry {
    pub cells: Vec<[f32; 2]>,
    pub rays: Vec<[f32; 4]>,
    pub nearest: [f32; 6],
    /// Source identities for replay diagnostics, outside the network inputs.
    pub ray_sources: Vec<Option<ColliderId>>,
    pub nearest_source: Option<ColliderId>,
}
