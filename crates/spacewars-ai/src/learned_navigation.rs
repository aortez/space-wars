//! Navigation policy over the C3 flight encoder: the same inspected inputs and
//! weights as the flight policy, with the three heads read as a body-relative
//! direction pair and a speed for the composed motor stack. The exported motor
//! network owns the actuators; this layer only chooses where to go and how fast.
use crate::learned_flight::{ARCHITECTURE, FlightPolicy, FlightPolicyFile};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight_environment::motor::MotorCommand, flight_policy_observation::FlightPolicyInputs,
};
use serde::{Deserialize, Serialize};

pub const COMMAND_CONTRACT: &str = "navigation-command-v1";
/// Flight policy files at this version carry the navigation command contract.
pub const FORMAT_VERSION: u32 = 4;
pub const MAX_SPEED: f32 = 20.0;
/// The v2 command: the ship's desired velocity relative to the goal, as a
/// heading relative to the ship's heading and a speed to 60, for the
/// planet-frame motor contract. The host adds the goal's velocity relative to
/// the pilot's planet frame, so a network parks at a moving goal without
/// encoding that velocity in its output, and a speed under the deadband is a
/// stop.
pub const COMMAND_CONTRACT_V2: &str = "navigation-command-v2";
/// Speeds under this are stops under the v2 contract: a policy's near-zero
/// output is a stop the motor brakes to, while the law's crawl into the
/// arrival radius, under the velocity tolerance, still passes.
pub const SPEED_DEADBAND_V2: f32 = 0.5;
pub const FORMAT_VERSION_V2: u32 = 5;
pub const MAX_SPEED_V2: f32 = 60.0;

/// The speed cap of a navigation command contract version.
pub fn max_speed(contract: u32) -> Result<f32, &'static str> {
    match contract {
        1 => Ok(MAX_SPEED),
        2 => Ok(MAX_SPEED_V2),
        _ => Err("unsupported navigation contract"),
    }
}

/// Desired heading relative to the ship's heading at the decision tick, as a
/// direction pair, plus a nonnegative speed. The pair avoids a seam behind the
/// ship; only its direction is used.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationCommand {
    pub heading_sin: f32,
    pub heading_cos: f32,
    pub speed: f32,
}

/// The direction pair's floor: above the command's refusal norm with room for
/// the single-precision parse of a pair rescaled in double precision.
pub const MIN_PAIR_NORM: f32 = 0.125;
impl NavigationCommand {
    /// The absolute motor command under the v1 speed cap.
    pub fn absolute(self, angle: f32) -> Result<MotorCommand, &'static str> {
        self.absolute_within(angle, MAX_SPEED)
    }

    /// The absolute motor command under the given speed cap.
    pub fn absolute_within(self, angle: f32, max_speed: f32) -> Result<MotorCommand, &'static str> {
        let norm = self.heading_sin.hypot(self.heading_cos);
        if !norm.is_finite()
            || norm < 0.1
            || !self.speed.is_finite()
            || !(0.0..=max_speed).contains(&self.speed)
        {
            return Err("invalid navigation command");
        }
        let heading = angle + self.heading_sin.atan2(self.heading_cos);
        Ok(MotorCommand {
            heading: heading.sin().atan2(heading.cos()),
            speed: self.speed,
        })
    }

    /// The absolute motor command under the v2 contract: this command's
    /// velocity plus the goal's velocity relative to the pilot's planet frame,
    /// capped at the given speed. A sum without speed keeps the command's
    /// heading as a stop.
    pub fn absolute_with_goal(
        self,
        angle: f32,
        goal_velocity: Vec2,
        max_speed: f32,
    ) -> Result<MotorCommand, &'static str> {
        let own = self.absolute_within(angle, max_speed)?;
        if !goal_velocity.x.is_finite() || !goal_velocity.y.is_finite() {
            return Err("non-finite goal velocity");
        }
        let velocity = Vec2::new(-own.heading.sin(), own.heading.cos()) * own.speed + goal_velocity;
        let speed = velocity.length();
        if speed < 1e-6 {
            return Ok(MotorCommand {
                heading: own.heading,
                speed: 0.0,
            });
        }
        Ok(MotorCommand {
            heading: (-velocity.x).atan2(velocity.y),
            speed: speed.min(max_speed),
        })
    }

    /// The absolute motor command under a contract: v1 commands are relative
    /// to the pilot's planet frame already, v2 commands relative to the goal.
    pub fn absolute_under(
        self,
        contract: u32,
        angle: f32,
        goal_velocity: Vec2,
        max_speed: f32,
    ) -> Result<MotorCommand, &'static str> {
        match contract {
            2 => self.absolute_with_goal(angle, goal_velocity, max_speed),
            _ => self.absolute_within(angle, max_speed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NavigationOutput {
    /// Raw heads: the pair before tanh and the speed before the scaled sigmoid.
    pub latents: [f32; 3],
    pub value: f32,
    pub command: NavigationCommand,
}

pub struct NavigationPolicy {
    policy: FlightPolicy,
    contract: u32,
}

impl NavigationPolicy {
    pub fn from_file(file: FlightPolicyFile) -> Result<Self, &'static str> {
        let contract = match file.format_version {
            FORMAT_VERSION => 1,
            FORMAT_VERSION_V2 => 2,
            _ => return Err("incompatible navigation policy contract"),
        };
        if file.architecture != ARCHITECTURE {
            return Err("incompatible navigation policy contract");
        }
        Ok(Self {
            policy: FlightPolicy::validated(file, false)?,
            contract,
        })
    }

    /// The command contract version: 1 for the stationary-frame command to
    /// 20, 2 for the planet-frame command to 60.
    pub fn contract(&self) -> u32 {
        self.contract
    }

    pub fn max_speed(&self) -> f32 {
        max_speed(self.contract).expect("validated contract")
    }

    /// The command from the heads. A direction pair under the floor keeps its
    /// direction at the floor, forward if it has none, as the training side
    /// maps its latents, and a v2 speed under the deadband is a stop: a
    /// policy's command is always one the motor can fly.
    pub fn infer(&self, inputs: &FlightPolicyInputs) -> Result<NavigationOutput, &'static str> {
        let (latents, value) = self.policy.heads(inputs)?;
        let mut speed = self.max_speed() / (1.0 + (-latents[2]).exp());
        if self.contract == 2 && speed < SPEED_DEADBAND_V2 {
            speed = 0.0;
        }
        let (mut heading_sin, mut heading_cos) = (latents[0].tanh(), latents[1].tanh());
        let norm = heading_sin.hypot(heading_cos);
        if norm < MIN_PAIR_NORM {
            (heading_sin, heading_cos) = if norm == 0.0 {
                (0.0, MIN_PAIR_NORM)
            } else {
                (
                    heading_sin * MIN_PAIR_NORM / norm,
                    heading_cos * MIN_PAIR_NORM / norm,
                )
            };
        }
        Ok(NavigationOutput {
            latents,
            value,
            command: NavigationCommand {
                heading_sin,
                heading_cos,
                speed,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learned_flight::Tensor;
    use scenario_spacewars::surface_sortie::flight_policy_observation::schema;

    fn file(format_version: u32, cos_bias: f32) -> FlightPolicyFile {
        let weights = FlightPolicy::expected_shapes(false)
            .into_iter()
            .map(|(name, shape)| {
                let mut values = vec![0.0; shape.iter().product()];
                if name == "actor.bias" {
                    values[1] = cos_bias;
                }
                (name, Tensor { shape, values })
            })
            .collect();
        FlightPolicyFile {
            format_version,
            architecture: ARCHITECTURE.into(),
            observation_schema: schema(),
            weights,
        }
    }

    fn inputs() -> FlightPolicyInputs {
        FlightPolicyInputs {
            self_goal: vec![0.0; 35],
            cells: vec![[0.0; 2]; 1472],
            rays: vec![[0.0; 4]; 128],
            nearest: [0.0; 6],
            objects: vec![[0.0; 16]; 32],
        }
    }

    #[test]
    fn relative_commands_become_wrapped_absolute_headings() {
        let command = NavigationCommand {
            heading_sin: 0.0,
            heading_cos: -1.0,
            speed: 12.0,
        };
        let absolute = command.absolute(3.0).unwrap();
        assert!((absolute.heading - (3.0 - std::f32::consts::PI)).abs() < 1e-5);
        assert_eq!(absolute.speed, 12.0);
        for bad in [
            NavigationCommand {
                heading_sin: 0.0,
                heading_cos: 0.0,
                speed: 1.0,
            },
            NavigationCommand {
                heading_sin: 1.0,
                heading_cos: 0.0,
                speed: 21.0,
            },
            NavigationCommand {
                heading_sin: 1.0,
                heading_cos: 0.0,
                speed: -1.0,
            },
        ] {
            assert!(bad.absolute(0.0).is_err());
        }
    }

    #[test]
    fn heads_map_to_a_pair_and_a_scaled_speed_under_the_navigation_version_only() {
        assert!(NavigationPolicy::from_file(file(2, 1.0)).is_err());
        assert!(FlightPolicy::from_file(file(FORMAT_VERSION, 1.0)).is_err());
        let policy = NavigationPolicy::from_file(file(FORMAT_VERSION, 1.0)).unwrap();
        let output = policy.infer(&inputs()).unwrap();
        assert_eq!(output.latents, [0.0, 1.0, 0.0]);
        assert_eq!(output.command.heading_sin, 0.0);
        assert!((output.command.heading_cos - 1.0_f32.tanh()).abs() < 1e-6);
        assert!((output.command.speed - 10.0).abs() < 1e-6);
        assert_eq!(output.command.absolute(0.5).unwrap().heading, 0.5);
        // A collapsed pair is floored forward, so the command stays flyable;
        // a hand-written zero pair is still refused.
        let zero = NavigationPolicy::from_file(file(FORMAT_VERSION, 0.0)).unwrap();
        let floored = zero.infer(&inputs()).unwrap().command;
        assert_eq!(
            (floored.heading_sin, floored.heading_cos),
            (0.0, MIN_PAIR_NORM)
        );
        assert_eq!(floored.absolute(0.5).unwrap().heading, 0.5);
        assert!(
            NavigationCommand {
                heading_sin: 0.0,
                heading_cos: 0.0,
                speed: 1.0
            }
            .absolute(0.0)
            .is_err()
        );
        // The v2 contract reads the same heads with a 60-unit speed cap.
        let fast = NavigationPolicy::from_file(file(FORMAT_VERSION_V2, 1.0)).unwrap();
        assert_eq!(fast.contract(), 2);
        assert_eq!(policy.contract(), 1);
        let command = fast.infer(&inputs()).unwrap().command;
        assert!((command.speed - 30.0).abs() < 1e-5);
        assert!(command.absolute(0.0).is_err());
        assert!(command.absolute_within(0.0, fast.max_speed()).is_ok());
        assert!(NavigationPolicy::from_file(file(6, 1.0)).is_err());
    }

    #[test]
    fn v2_commands_are_relative_to_the_goal_and_small_speeds_are_stops() {
        // A stop at a goal moving in the frame becomes the goal's velocity.
        let stop = NavigationCommand {
            heading_sin: 0.0,
            heading_cos: 1.0,
            speed: 0.0,
        };
        let moving = stop
            .absolute_with_goal(0.0, Vec2::new(-3.0, 4.0), 60.0)
            .unwrap();
        assert!((moving.speed - 5.0).abs() < 1e-6);
        assert!((moving.heading - 3.0_f32.atan2(4.0)).abs() < 1e-6);
        // An approach that cancels the goal's velocity is a stop that keeps
        // the command's heading; the sum is capped at the motor's speed.
        let forward = NavigationCommand {
            heading_sin: 0.0,
            heading_cos: 1.0,
            speed: 10.0,
        };
        let still = forward
            .absolute_with_goal(
                0.5,
                Vec2::new(10.0 * 0.5_f32.sin(), -10.0 * 0.5_f32.cos()),
                60.0,
            )
            .unwrap();
        assert_eq!(still.speed, 0.0);
        assert!((still.heading - 0.5).abs() < 1e-6);
        let capped = forward
            .absolute_with_goal(0.0, Vec2::new(0.0, 55.0), 60.0)
            .unwrap();
        assert_eq!(capped.speed, 60.0);
        assert_eq!(
            forward
                .absolute_under(1, 0.0, Vec2::new(0.0, 55.0), 20.0)
                .unwrap()
                .speed,
            10.0
        );
        assert!(
            forward
                .absolute_under(2, 0.0, Vec2::new(f32::NAN, 0.0), 60.0)
                .is_err()
        );
        // The deadband: a v2 policy's speed under one unit is a stop, a v1
        // policy's is not.
        let mut slow = file(FORMAT_VERSION_V2, 1.0);
        slow.weights.get_mut("actor.bias").unwrap().values[2] = -5.0;
        let v2 = NavigationPolicy::from_file(slow.clone()).unwrap();
        assert_eq!(v2.infer(&inputs()).unwrap().command.speed, 0.0);
        slow.format_version = FORMAT_VERSION;
        let v1 = NavigationPolicy::from_file(slow).unwrap();
        assert!(v1.infer(&inputs()).unwrap().command.speed > 0.1);
    }
}
