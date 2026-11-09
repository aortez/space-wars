use super::*;
use engine_rapier::rover::{RoverAssembly, RoverSpawnPose, RoverSpec};

/// Physical comparison settings shared by manual and CPU driving.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WheelTuning {
    pub radius: f32,
    pub motor_torque: f32,
    /// Velocity-error response of the force-based drive and brake motors.
    /// Omit the original value so retained diagnostic observations still match.
    #[serde(skip_serializing_if = "reference_motor_damping")]
    pub motor_damping: f32,
}

fn reference_motor_damping(value: &f32) -> bool {
    *value == WheelTuning::REFERENCE.motor_damping
}

impl Default for WheelTuning {
    fn default() -> Self {
        Self {
            radius: 0.65,
            motor_torque: 300.0,
            motor_damping: 240.0,
        }
    }
}

impl WheelTuning {
    /// Original wheel settings retained for reproducible diagnostic captures.
    pub const REFERENCE: Self = Self {
        radius: 0.65,
        motor_torque: 90.0,
        motor_damping: 80.0,
    };

    pub(super) fn bounded(self) -> Self {
        let defaults = Self::default();
        Self {
            radius: if self.radius.is_finite() {
                self.radius.clamp(0.5, 1.0)
            } else {
                defaults.radius
            },
            motor_torque: if self.motor_torque.is_finite() {
                self.motor_torque.clamp(30.0, 480.0)
            } else {
                defaults.motor_torque
            },
            motor_damping: if self.motor_damping.is_finite() {
                self.motor_damping.clamp(1.0, 2_400.0)
            } else {
                defaults.motor_damping
            },
        }
    }
}

/// A wheeled comparison, not a flexible belt: both tires have native circular
/// contacts, independent suspension travel and torque-limited drive/brakes.
pub(super) fn spec(tuning: WheelTuning) -> RoverSpec {
    RoverSpec {
        chassis_half_width: 1.6,
        chassis_half_height: 0.45,
        chassis_density: 10.0,
        wheel_radius: tuning.radius,
        // Keep wheel mass fixed when comparing geometry. Radius still changes
        // rotational inertia, clearance and the leverage at a terrain corner.
        wheel_density: 2.2 * (WheelTuning::default().radius / tuning.radius).powi(2),
        wheel_offset: 1.2,
        suspension_anchor_height: -0.1,
        suspension_rest_length: 0.55,
        suspension_travel: 0.22,
        suspension_stiffness: 3_500.0,
        suspension_damping: 300.0,
        suspension_max_force: 2_400.0,
        wheel_target_speed: 2.5 / tuning.radius,
        wheel_motor_torque: tuning.motor_torque,
        wheel_brake_torque: 120.0,
        wheel_motor_damping: tuning.motor_damping,
        ..Default::default()
    }
}

pub(super) const fn enabled(config: ScorchedConfig) -> bool {
    config.suspension
        && !config.undercut
        && config.drop_height.is_none()
        && config.bombardment_seconds == 0
        && !((config.hillside || config.roof) && config.demo)
}

#[cfg(test)]
pub(super) fn insert(
    physics: &mut PhysicsWorld,
    entity: PhysicsId,
    position: Vec2,
    tuning: WheelTuning,
) -> RoverAssembly {
    insert_scaled(physics, entity, position, tuning, 1.0)
}

pub(super) fn insert_scaled(
    physics: &mut PhysicsWorld,
    entity: PhysicsId,
    position: Vec2,
    tuning: WheelTuning,
    scale: f32,
) -> RoverAssembly {
    let mut spec = spec(tuning);
    spec.chassis_half_width *= scale;
    spec.chassis_half_height *= scale;
    spec.wheel_radius *= scale;
    spec.wheel_offset *= scale;
    spec.suspension_anchor_height *= scale;
    spec.suspension_rest_length *= scale;
    spec.suspension_travel *= scale;
    // Density stays fixed: masses scale with area, forces with length cubed,
    // and torques/inertias with length to the fourth. Time stays in seconds.
    spec.suspension_stiffness *= scale.powi(2);
    spec.suspension_damping *= scale.powi(2);
    spec.suspension_max_force *= scale.powi(3);
    spec.wheel_motor_torque *= scale.powi(4);
    spec.wheel_brake_torque *= scale.powi(4);
    spec.wheel_motor_damping *= scale.powi(4);
    RoverAssembly::insert_at(
        physics,
        entity,
        RoverSpawnPose {
            wheel_center: position
                + Vec2::Y * (tuning.radius - WheelTuning::default().radius) * scale
                - Vec2::Y * (spec.suspension_rest_length - spec.suspension_anchor_height),
            up_angle: std::f32::consts::FRAC_PI_2,
        },
        spec,
    )
    .expect("valid tank suspension")
}

#[cfg(test)]
mod tests;
