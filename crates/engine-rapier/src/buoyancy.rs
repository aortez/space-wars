//! Opt-in pool -> body forces, with explicit restricted box-displacement feedback.
//! This first adapter owns one centered box/circle collider per dynamic body.
use crate::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld,
};
use engine_core::Vec2;
use engine_water::{
    MAX_STEP, WaterError, WaterWorld,
    displacement::DisplacementBox,
    immersion::{HullShape, WaterHull},
};

#[derive(Debug, Clone, Copy)]
pub struct BuoyancyConfig {
    /// Fluid mass per unit area (unit depth). Compare with collider density.
    pub density: f32,
    /// Distributed linear drag rate, including resistance to rotation.
    pub drag: f32,
}

impl Default for BuoyancyConfig {
    fn default() -> Self {
        Self {
            density: 1.0,
            drag: 5.0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BuoyancyReport {
    pub submerged_area: f64,
    pub submerged_fraction: f64,
    pub center_of_buoyancy: Option<Vec2>,
    pub force: Vec2,
    pub torque: f32,
}

pub struct BuoyantBody {
    body: BodyId,
    hull: WaterHull,
    shape: HullShape,
}

impl BuoyantBody {
    /// The collider and water hull are built together, preventing mismatched
    /// shape/density assumptions. The caller owns removal and must not replace
    /// this body's geometry behind the adapter. No partial insertion on error.
    pub fn insert(
        world: &mut PhysicsWorld,
        entity: PhysicsId,
        spec: BodySpec,
        shape: HullShape,
        density: f32,
    ) -> Option<Self> {
        if spec.kind != BodyKind::Dynamic
            || spec.gravity_scale != 1.0
            || !density.is_finite()
            || !(0.001..=10_000.0).contains(&density)
            || world.contains_entity(entity)
        {
            return None;
        }
        let hull = WaterHull::new(shape).ok()?;
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let collider_id = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        let mut collider = match shape {
            HullShape::Box {
                half_width,
                half_height,
            } => ColliderSpec::cuboid(collider_id, half_width, half_height),
            HullShape::Circle { radius } => ColliderSpec::ball(collider_id, radius),
        };
        collider.density = density;
        collider.restitution = 0.1;
        if !world.insert_body(body, spec, &[collider]) {
            return None;
        }
        world.refresh_mass_properties(body);
        Some(Self { body, hull, shape })
    }

    pub fn body(&self) -> BodyId {
        self.body
    }
    pub fn shape(&self) -> HullShape {
        self.shape
    }

    /// Submit this body's authoritative pose as the pool's sole displacer.
    /// Only unrotated, rotation-locked dynamic boxes are supported. The water
    /// model validates the closed/flat basin and footprint. Failure is atomic.
    ///
    /// Submit before water stepping/force sampling and after physics stepping
    /// so rendering sees the final pose. The caller owns the pool's occupancy
    /// input: clear it with `set_displacer(pool, None)` when removing the body
    /// or switching feedback off. This does not step either simulation or apply
    /// forces; one-way buoyancy remains independently available.
    pub fn sync_displacement(
        &self,
        world: &PhysicsWorld,
        water: &mut WaterWorld,
        pool: usize,
    ) -> Result<(), WaterError> {
        let HullShape::Box {
            half_width,
            half_height,
        } = self.shape
        else {
            return Err(WaterError::InvalidGeometry);
        };
        let motion = world.motion(self.body).ok_or(WaterError::InvalidInput)?;
        if world.body_rotation_locked(self.body) != Some(true)
            || motion.angle != 0.0
            || motion.angular_velocity != 0.0
            || world.dynamic_body_inertia(self.body).is_none()
        {
            return Err(WaterError::InvalidGeometry);
        }
        water.set_displacer(
            pool,
            Some(DisplacementBox {
                center: motion.position,
                half_extents: Vec2::new(half_width, half_height),
            }),
        )
    }

    /// Call once after `world.clear_forces()` and before each physics step.
    /// Adds forces alongside other force fields; never clears another force.
    /// Gravity must point downward, matching
    /// the pool model; its magnitude comes from the mechanics world. Water does
    /// not change, and no per-body force or contact cache is retained.
    pub fn apply_forces(
        &self,
        world: &mut PhysicsWorld,
        water: &WaterWorld,
        config: BuoyancyConfig,
        dt: f64,
    ) -> Option<BuoyancyReport> {
        let gravity = world.gravity();
        if !dt.is_finite()
            || dt <= 0.0
            || dt > MAX_STEP
            || !config.density.is_finite()
            || !(0.001..=10_000.0).contains(&config.density)
            || !config.drag.is_finite()
            || !(0.0..=100.0).contains(&config.drag)
            || gravity.x != 0.0
            || gravity.y > 0.0
        {
            return None;
        }
        let motion = world.motion(self.body)?;
        let center = world.center_of_mass(self.body)?;
        let mass = world.body_mass(self.body)? as f64;
        let inertia = world.dynamic_body_inertia(self.body)? as f64;
        if mass <= 0.0 || inertia <= 0.0 {
            return None;
        }
        let m = self
            .hull
            .measure(water, motion.position, motion.angle, center)
            .ok()?;
        // Overlapping water regions are not meaningful pressure layers. Reject
        // obvious double-counting, rather than applying more than full lift.
        if m.area > self.hull.area() * (1.0 + 1.0e-6) {
            return None;
        }
        if m.area <= 1.0e-10 {
            return Some(BuoyancyReport::default());
        }
        let fraction = (m.area / self.hull.area()).clamp(0.0, 1.0);
        let k = config.density as f64 * config.drag as f64 * dt;
        // The coupled translation/rotation block's trace bounds the eigenvalues
        // of the mass-weighted drag matrix (the other translation mode is A/m).
        // This caps each dissipative eigenmode response below 1, even for a
        // very light body or a large dt.
        let drag = k / (dt * (1.0 + k * (m.area / mass + m.polar_moment.max(0.0) / inertia)));
        let v = motion.linear_velocity;
        let w = motion.angular_velocity as f64;
        let [x, y] = m.first_moment;
        let lift = config.density as f64 * -(gravity.y as f64);
        let force = Vec2::new(
            (drag * (m.flow[0] - m.area * v.x as f64 + w * y)) as f32,
            (lift * m.area + drag * (m.flow[1] - m.area * v.y as f64 - w * x)) as f32,
        );
        let torque = (lift * x
            + drag * (m.flow_torque - v.y as f64 * x + v.x as f64 * y - w * m.polar_moment))
            as f32;
        if !force.x.is_finite() || !force.y.is_finite() || !torque.is_finite() {
            return None;
        }
        world.apply_force(self.body, force, true);
        world.apply_torque(self.body, torque, true);
        Some(BuoyancyReport {
            submerged_area: m.area,
            submerged_fraction: fraction,
            center_of_buoyancy: Some(center + Vec2::new((x / m.area) as f32, (y / m.area) as f32)),
            force,
            torque,
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod displacement_tests;
