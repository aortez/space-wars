//! Persistent bounds for numerical force/work errors, never material budgets.
use super::*;

mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Ledger {
    pub energy_scale: f64,
    pub momentum_scale: f64,
    pub distance_scale: f64,
    pub work_error: f64,
    pub impulse_error: f64,
    pub position_error: f64,
    motion: (f64, f64, f64),
    acceleration: f64,
    origin: [f64; 2],
}

impl Ledger {
    pub(super) fn new(world: &PhysicsWorld, acceleration: f64) -> Result<Self, Rejection> {
        let motion = state(world);
        let energy_scale = mechanical(world, acceleration).max(0.0);
        let momentum_scale = libm::sqrt(2.0 * motion.2 * energy_scale);
        if !energy_scale.is_finite() || !momentum_scale.is_finite() {
            return Err(Rejection::Energy);
        }
        Ok(Self {
            energy_scale,
            momentum_scale,
            // The certified plate has unit longitudinal extent. This is a
            // trajectory error cap, not a contact/departure uncertainty floor.
            distance_scale: motion.0.abs().max(1.0),
            work_error: 0.0,
            impulse_error: 0.0,
            position_error: 0.0,
            motion,
            acceleration,
            origin: origin(world),
        })
    }

    pub(super) fn validate(
        &self,
        world: &PhysicsWorld,
        acceleration: f64,
    ) -> Result<(), Rejection> {
        if self.origin != origin(world)
            || self.motion != state(world)
            || self.acceleration != acceleration
            || [
                (self.work_error, self.energy_scale),
                (self.impulse_error, self.momentum_scale),
                (self.position_error, self.distance_scale),
            ]
            .iter()
            .any(|(used, scale)| {
                !scale.is_finite()
                    || *scale < 0.0
                    || !used.is_finite()
                    || used.abs() > 1.0e-5 * scale
            })
        {
            return Err(Rejection::Evidence);
        }
        Ok(())
    }

    // An already-accounted impact or settling projection changes physical
    // motion, but cannot reset the numerical discrepancies or their bounds.
    pub(super) fn after_response(&mut self, world: &PhysicsWorld) -> Result<(), Rejection> {
        let next = state(world);
        if self.motion.0 != next.0 || self.motion.2 != next.2 {
            return Err(Rejection::Evidence);
        }
        self.motion = next;
        Ok(())
    }

    pub(super) fn charge(
        &mut self,
        world: &PhysicsWorld,
        work: f64,
        impulse: f64,
        position: f64,
    ) -> Result<(), Rejection> {
        fn sum(previous: f64, error: f64, scale: f64) -> Result<f64, Rejection> {
            // Opposite observed errors can cancel; a new frame/root cannot.
            // Every accepted prefix, not just the final endpoint, is bounded.
            let next = previous + error;
            if !next.is_finite()
                || next.abs() > 1.0e-5 * scale
                || (error != 0.0 && next == previous)
            {
                return Err(Rejection::Energy);
            }
            Ok(next)
        }
        let work_error = sum(self.work_error, work, self.energy_scale)?;
        let impulse_error = sum(self.impulse_error, impulse, self.momentum_scale)?;
        let position_error = sum(self.position_error, position, self.distance_scale)?;
        self.work_error = work_error;
        self.impulse_error = impulse_error;
        self.position_error = position_error;
        self.motion = state(world);
        Ok(())
    }
}

pub(super) fn drift_uncertainty(
    world: &PhysicsWorld,
    x: f64,
    middle: f32,
    dt: f32,
) -> Result<f64, Rejection> {
    let n = world
        .raw
        .integration_parameters
        .num_solver_iterations
        .checked_add(
            world
                .raw
                .bodies
                .iter()
                .map(|(_, b)| b.additional_solver_iterations())
                .max()
                .unwrap_or(0),
        )
        .ok_or(Rejection::Geometry)?;
    // Force-free, rotation-locked drift has one rounded dt division, velocity
    // multiplication and n position additions. Include COM/writeback overhead
    // conservatively, plus an absolute subnormal allowance. No unit-size floor.
    let factor = 16.0 * (n as f64 + 4.0);
    let relative = factor * f64::from(f32::EPSILON);
    if n == 0 || relative >= 0.01 {
        return Err(Rejection::Geometry);
    }
    let movement = f64::from(middle) * f64::from(dt);
    Ok(relative * (x.abs() + movement.abs())
        + factor * f64::from(f32::from_bits(1)) * (f64::from(middle).abs() + 1.0))
}
