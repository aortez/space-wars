//! Navigation policy over the C3 flight encoder: the same inspected inputs and
//! weights as the flight policy, with the three heads read as a body-relative
//! direction pair and a speed for the composed motor stack. The exported motor
//! network owns the actuators; this layer only chooses where to go and how fast.
use crate::learned_flight::{ARCHITECTURE, FlightPolicy, FlightPolicyFile};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    flight_environment::motor::{self, MotorCommand},
    flight_policy_observation::FlightPolicyInputs,
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
/// The v3 command: the v2 command with the advisory vector of the motor's
/// v3 contract, whose turn sense a fourth head sets.
pub const COMMAND_CONTRACT_V3: &str = "navigation-command-v3";
pub const FORMAT_VERSION_V3: u32 = 6;
/// A sense head beyond this, through tanh, either way is a turn sense.
pub const SENSE_THRESHOLD: f32 = 0.5;

/// The speed cap of a navigation command contract version.
pub fn max_speed(contract: u32) -> Result<f32, &'static str> {
    match contract {
        1 => Ok(MAX_SPEED),
        2 | 3 => Ok(MAX_SPEED_V2),
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
    /// The v3 contract's advisory vector, carried to the motor command;
    /// absent, as zero, under the earlier contracts.
    #[serde(default, skip_serializing_if = "no_advisory")]
    pub advisory: [f32; motor::ADVISORY_ELEMENTS],
}

fn no_advisory(advisory: &[f32; motor::ADVISORY_ELEMENTS]) -> bool {
    advisory.iter().all(|v| *v == 0.0)
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
            advisory: self.advisory,
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
                advisory: own.advisory,
            });
        }
        Ok(MotorCommand {
            heading: (-velocity.x).atan2(velocity.y),
            speed: speed.min(max_speed),
            advisory: own.advisory,
        })
    }

    /// The absolute motor command under a contract: v1 commands are relative
    /// to the pilot's planet frame already, v2 and v3 commands relative to
    /// the goal.
    pub fn absolute_under(
        self,
        contract: u32,
        angle: f32,
        goal_velocity: Vec2,
        max_speed: f32,
    ) -> Result<MotorCommand, &'static str> {
        match contract {
            2 | 3 => self.absolute_with_goal(angle, goal_velocity, max_speed),
            _ => self.absolute_within(angle, max_speed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NavigationOutput {
    /// Raw heads: the pair before tanh and the speed before the scaled sigmoid.
    pub latents: [f32; 3],
    /// The v3 sense head before tanh and its threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sense_latent: Option<f32>,
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
            FORMAT_VERSION_V3 => 3,
            _ => return Err("incompatible navigation policy contract"),
        };
        if file.architecture != ARCHITECTURE {
            return Err("incompatible navigation policy contract");
        }
        Ok(Self {
            policy: FlightPolicy::validated(file, false, if contract == 3 { 4 } else { 3 })?,
            contract,
        })
    }

    /// The command contract version: 1 for the stationary-frame command to
    /// 20, 2 for the planet-frame command to 60, 3 for that command with the
    /// advisory vector.
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
        let (heads, value) = self.policy.heads(inputs)?;
        let latents = [heads[0], heads[1], heads[2]];
        let sense_latent = (self.contract == 3).then(|| heads[3]);
        let mut speed = self.max_speed() / (1.0 + (-latents[2]).exp());
        if self.contract >= 2 && speed < SPEED_DEADBAND_V2 {
            speed = 0.0;
        }
        let mut advisory = [0.0; motor::ADVISORY_ELEMENTS];
        if let Some(latent) = sense_latent {
            let sense = latent.tanh();
            advisory[0] = if sense > SENSE_THRESHOLD {
                1.0
            } else if sense < -SENSE_THRESHOLD {
                -1.0
            } else {
                0.0
            };
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
            sense_latent,
            value,
            command: NavigationCommand {
                heading_sin,
                heading_cos,
                speed,
                advisory,
            },
        })
    }
}

/// A v2 navigation export under the v3 contract with a fourth head of zero
/// weight: it issues the same commands with a free sense, so a stack lifted
/// this way must fly the v2 flights identically.
#[cfg(test)]
pub(crate) fn lifted_to_v3(mut file: FlightPolicyFile) -> FlightPolicyFile {
    assert_eq!(file.format_version, FORMAT_VERSION_V2);
    let actor = file.weights.get_mut("actor.weight").unwrap();
    actor.shape = vec![4, 64];
    actor.values.extend(std::iter::repeat_n(0.0, 64));
    for name in ["actor.bias", "log_std"] {
        let tensor = file.weights.get_mut(name).unwrap();
        tensor.shape = vec![4];
        tensor.values.push(0.0);
    }
    file.format_version = FORMAT_VERSION_V3;
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learned_flight::Tensor;
    use scenario_spacewars::surface_sortie::flight_policy_observation::schema;

    fn file(format_version: u32, cos_bias: f32) -> FlightPolicyFile {
        file_with_heads(format_version, cos_bias, 3, 0.0)
    }

    fn file_with_heads(
        format_version: u32,
        cos_bias: f32,
        heads: usize,
        sense_bias: f32,
    ) -> FlightPolicyFile {
        let weights = FlightPolicy::expected_shapes(false, heads)
            .into_iter()
            .map(|(name, shape)| {
                let mut values = vec![0.0; shape.iter().product()];
                if name == "actor.bias" {
                    values[1] = cos_bias;
                    if heads == 4 {
                        values[3] = sense_bias;
                    }
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
            advisory: [0.0; 4],
        };
        let absolute = command.absolute(3.0).unwrap();
        assert!((absolute.heading - (3.0 - std::f32::consts::PI)).abs() < 1e-5);
        assert_eq!(absolute.speed, 12.0);
        for bad in [
            NavigationCommand {
                heading_sin: 0.0,
                heading_cos: 0.0,
                speed: 1.0,
                advisory: [0.0; 4],
            },
            NavigationCommand {
                heading_sin: 1.0,
                heading_cos: 0.0,
                speed: 21.0,
                advisory: [0.0; 4],
            },
            NavigationCommand {
                heading_sin: 1.0,
                heading_cos: 0.0,
                speed: -1.0,
                advisory: [0.0; 4],
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
                speed: 1.0,
                advisory: [0.0; 4],
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
            advisory: [0.0; 4],
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
            advisory: [0.0; 4],
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

    #[test]
    fn v3_heads_add_a_turn_sense_read_through_its_threshold() {
        assert_eq!(max_speed(3).unwrap(), 60.0);
        for (bias, sense) in [(0.0, 0.0), (0.4, 0.0), (2.0, 1.0), (-2.0, -1.0)] {
            let policy =
                NavigationPolicy::from_file(file_with_heads(FORMAT_VERSION_V3, 1.0, 4, bias))
                    .unwrap();
            assert_eq!(policy.contract(), 3);
            let output = policy.infer(&inputs()).unwrap();
            assert_eq!(output.latents, [0.0, 1.0, 0.0]);
            assert_eq!(output.sense_latent, Some(bias));
            assert_eq!(output.command.advisory, [sense, 0.0, 0.0, 0.0]);
            assert!((output.command.speed - 30.0).abs() < 1e-5);
            // The advisory rides the command to the motor and is written
            // only when it is set.
            let absolute = output
                .command
                .absolute_under(3, 0.0, Vec2::new(0.0, 5.0), 60.0)
                .unwrap();
            assert_eq!(absolute.advisory, output.command.advisory);
            let value = serde_json::to_value(output.command).unwrap();
            assert_eq!(value.get("advisory").is_some(), sense != 0.0);
            let back: NavigationCommand = serde_json::from_value(value).unwrap();
            assert_eq!(back, output.command);
        }
        // A fourth head needs the v3 version, and the v3 version a fourth head.
        assert!(
            NavigationPolicy::from_file(file_with_heads(FORMAT_VERSION_V2, 1.0, 4, 0.0)).is_err()
        );
        assert!(NavigationPolicy::from_file(file(FORMAT_VERSION_V3, 1.0)).is_err());
        let v2 = NavigationPolicy::from_file(file(FORMAT_VERSION_V2, 1.0)).unwrap();
        assert_eq!(v2.infer(&inputs()).unwrap().sense_latent, None);
        let lifted =
            NavigationPolicy::from_file(lifted_to_v3(file(FORMAT_VERSION_V2, 1.0))).unwrap();
        let output = lifted.infer(&inputs()).unwrap();
        assert_eq!(output.sense_latent, Some(0.0));
        assert_eq!(output.command.advisory, [0.0; 4]);
        assert_eq!(output.latents, v2.infer(&inputs()).unwrap().latents);
    }
}
