//! Small independent actor/critic motor network. No scripted action overrides.
use crate::learned_flight::{FlightPolicyOutput, Tensor};
use scenario_spacewars::surface_sortie::flight_environment::{FlightAction, motor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ARCHITECTURE: &str = "motor18-actor32x32-critic32x32-v1";
pub const RESIDUAL_ARCHITECTURE: &str = "motor18-protected-actuator20x32x32-v2";
pub const MOVEMENT_ARCHITECTURE: &str = "motor18-protected-movement20x32x32-v3";
/// The plain actor with 64-unit hidden layers, for laws the 32-unit actor
/// cannot fit closely enough; the critic stays at 32.
pub const WIDE_ARCHITECTURE: &str = "motor18-actor64x64-critic32x32-v4";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotorPolicyFile {
    pub format_version: u32,
    pub architecture: String,
    pub observation_schema: serde_json::Value,
    pub weights: BTreeMap<String, Tensor>,
}
pub struct MotorPolicy {
    weights: BTreeMap<String, Tensor>,
    residual: bool,
    movement: bool,
    contract: u32,
}
impl MotorPolicy {
    pub fn from_file(file: MotorPolicyFile) -> Result<Self, &'static str> {
        let movement = file.format_version == 3 && file.architecture == MOVEMENT_ARCHITECTURE;
        let residual =
            (file.format_version == 2 && file.architecture == RESIDUAL_ARCHITECTURE) || movement;
        let wide = file.format_version == 4 && file.architecture == WIDE_ARCHITECTURE;
        let plain = (file.format_version == 1 && file.architecture == ARCHITECTURE) || wide;
        // The planet-frame v2 contract is carried by the plain actors only.
        let contract = if file.observation_schema == motor::schema() {
            motor::CONTRACT_V1
        } else if file.observation_schema == motor::schema_v2() && plain {
            motor::CONTRACT_V2
        } else {
            return Err("incompatible motor policy contract");
        };
        if !plain && !residual {
            return Err("incompatible motor policy contract");
        }
        let width = if wide { 64 } else { 32 };
        let mut expected = BTreeMap::new();
        for (name, input, output) in [
            ("actor1", 18, width),
            ("actor2", width, width),
            ("actor", width, 3),
            ("critic1", 18, 32),
            ("critic2", 32, 32),
            ("critic", 32, 1),
        ] {
            expected.insert(format!("{name}.weight"), vec![output, input]);
            expected.insert(format!("{name}.bias"), vec![output]);
        }
        expected.insert("log_std".into(), vec![3]);
        if residual {
            for (name, input, output) in [
                ("residual1", 20, 32),
                ("residual2", 32, 32),
                ("residual", 32, 2),
            ] {
                expected.insert(format!("{name}.weight"), vec![output, input]);
                expected.insert(format!("{name}.bias"), vec![output]);
            }
            expected.insert("residual_log_std".into(), vec![2]);
        }
        if file.weights.len() != expected.len() {
            return Err("wrong motor tensor count");
        }
        for (name, shape) in expected {
            let tensor = file.weights.get(&name).ok_or("missing motor tensor")?;
            if tensor.shape != shape
                || tensor.values.len() != shape.iter().product::<usize>()
                || !tensor
                    .values
                    .iter()
                    .all(|x| x.is_finite() && x.abs() <= 1e6)
            {
                return Err("invalid motor tensor");
            }
        }
        Ok(Self {
            weights: file.weights,
            residual,
            movement,
            contract,
        })
    }

    /// The motor command contract the inputs must follow.
    pub fn contract(&self) -> u32 {
        self.contract
    }
    fn linear(&self, name: &str, x: &[f32], tanh: bool) -> Vec<f32> {
        self.weights[&format!("{name}.weight")]
            .values
            .chunks_exact(x.len())
            .zip(&self.weights[&format!("{name}.bias")].values)
            .map(|(w, b)| {
                let v = w.iter().zip(x).fold(*b, |sum, (w, x)| sum + w * x);
                if tanh { v.tanh() } else { v }
            })
            .collect()
    }
    pub fn infer(&self, inputs: &[f32]) -> Result<FlightPolicyOutput, &'static str> {
        if inputs.len() != 18 || !inputs.iter().all(|v| v.is_finite()) {
            return Err("invalid motor inputs");
        }
        let mut a = self.linear(
            "actor",
            &self.linear("actor2", &self.linear("actor1", inputs, true), true),
            false,
        );
        if self.residual {
            if inputs[2] < 0.0 {
                return Err("negative residual command speed");
            }
            let mut features = inputs.to_vec();
            features.push(-inputs[0] * inputs[2] - inputs[3]);
            features.push(inputs[1] * inputs[2] - inputs[4]);
            let first = if self.movement {
                self.weights["residual1.weight"]
                    .values
                    .chunks_exact(20)
                    .zip(&self.weights["residual1.bias"].values)
                    .map(|(w, b)| {
                        let original = w[..18]
                            .iter()
                            .zip(inputs)
                            .fold(*b, |sum, (w, x)| sum + w * x);
                        let error = w[18..]
                            .iter()
                            .zip(&features[18..])
                            .fold(0.0, |sum, (w, x)| sum + w * x);
                        (original + error).tanh()
                    })
                    .collect::<Vec<_>>()
            } else {
                self.linear("residual1", &features, true)
            };
            let delta = self.linear("residual", &self.linear("residual2", &first, true), false);
            let speed = inputs[2] * 20.0;
            let gate = speed / (speed + 1.0);
            let correction = if self.movement {
                [(delta[0] - a[1]) + 2.0, (delta[1] - a[2]) - 2.0]
            } else {
                [delta[0], delta[1]]
            };
            a[1] += gate * correction[0];
            a[2] += gate * correction[1];
        }
        let value = self.linear(
            "critic",
            &self.linear("critic2", &self.linear("critic1", inputs, true), true),
            false,
        )[0];
        if !a
            .iter()
            .chain(std::iter::once(&value))
            .all(|v| v.is_finite())
        {
            return Err("non-finite motor output");
        }
        Ok(FlightPolicyOutput {
            steering_latent: a[0],
            thrust_logit: a[1],
            brake_logit: a[2],
            value,
            action: FlightAction {
                steering: a[0].tanh(),
                thrust: 1.0 / (1.0 + (-a[1]).exp()),
                brake: 1.0 / (1.0 + (-a[2]).exp()),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(residual: bool) -> MotorPolicyFile {
        let mut weights = BTreeMap::new();
        let mut layers = vec![
            ("actor1", 18, 32),
            ("actor2", 32, 32),
            ("actor", 32, 3),
            ("critic1", 18, 32),
            ("critic2", 32, 32),
            ("critic", 32, 1),
        ];
        if residual {
            layers.extend([
                ("residual1", 20, 32),
                ("residual2", 32, 32),
                ("residual", 32, 2),
            ]);
            weights.insert(
                "residual_log_std".into(),
                Tensor {
                    shape: vec![2],
                    values: vec![0.0; 2],
                },
            );
        }
        for (name, input, output) in layers {
            weights.insert(
                format!("{name}.weight"),
                Tensor {
                    shape: vec![output, input],
                    values: vec![0.0; input * output],
                },
            );
            weights.insert(
                format!("{name}.bias"),
                Tensor {
                    shape: vec![output],
                    values: vec![0.0; output],
                },
            );
        }
        weights.insert(
            "log_std".into(),
            Tensor {
                shape: vec![3],
                values: vec![0.0; 3],
            },
        );
        MotorPolicyFile {
            format_version: if residual { 2 } else { 1 },
            architecture: if residual {
                RESIDUAL_ARCHITECTURE
            } else {
                ARCHITECTURE
            }
            .into(),
            observation_schema: motor::schema(),
            weights,
        }
    }

    #[test]
    fn movement_blend_preserves_stopping_and_replaces_positive_speed_response() {
        let mut f = file(true);
        f.format_version = 3;
        f.architecture = MOVEMENT_ARCHITECTURE.into();
        f.weights.get_mut("actor.bias").unwrap().values = vec![0.2, -3.0, 0.8];
        f.weights.get_mut("residual.bias").unwrap().values = vec![-1.0, 0.5];
        let model = MotorPolicy::from_file(f).unwrap();
        let mut x = [0.0; 18];
        let stopped = model.infer(&x).unwrap();
        assert_eq!((stopped.thrust_logit, stopped.brake_logit), (-3.0, 0.8));
        x[2] = 0.5;
        let moving = model.infer(&x).unwrap();
        assert_eq!(moving.steering_latent, 0.2);
        assert!((moving.thrust_logit - (-3.0 + (10.0 / 11.0) * 4.0)).abs() < 1e-6);
        assert!((moving.brake_logit - (0.8 + (10.0 / 11.0) * -2.3)).abs() < 1e-6);
    }

    #[test]
    fn movement_format_rejects_incompatible_version_and_missing_weights() {
        let mut f = file(true);
        f.architecture = MOVEMENT_ARCHITECTURE.into();
        assert!(MotorPolicy::from_file(f.clone()).is_err());
        f.format_version = 3;
        assert!(MotorPolicy::from_file(f.clone()).is_ok());
        f.weights.remove("residual1.weight");
        assert!(MotorPolicy::from_file(f).is_err());
    }

    #[test]
    fn protected_zero_speed_and_continuous_positive_speed_residual() {
        let mut f = file(true);
        f.weights.get_mut("actor.bias").unwrap().values = vec![0.2, -3.0, 0.8];
        f.weights.get_mut("residual.bias").unwrap().values = vec![2.0, -2.0];
        let model = MotorPolicy::from_file(f).unwrap();
        let mut x = [0.0; 18];
        let stopped = model.infer(&x).unwrap();
        assert_eq!(
            (
                stopped.steering_latent,
                stopped.thrust_logit,
                stopped.brake_logit
            ),
            (0.2, -3.0, 0.8)
        );
        x[2] = 0.5;
        let moving = model.infer(&x).unwrap();
        assert_eq!(moving.steering_latent, stopped.steering_latent);
        assert!((moving.thrust_logit - (-3.0 + 20.0 / 11.0)).abs() < 1e-6);
        assert!((moving.brake_logit - (0.8 - 20.0 / 11.0)).abs() < 1e-6);
    }

    #[test]
    fn residual_uses_command_relative_velocity_error() {
        let mut f = file(true);
        f.weights.get_mut("residual1.weight").unwrap().values[18] = 1.0;
        f.weights.get_mut("residual1.weight").unwrap().values[20 + 19] = 1.0;
        f.weights.get_mut("residual2.weight").unwrap().values[0] = 1.0;
        f.weights.get_mut("residual2.weight").unwrap().values[32 + 1] = 1.0;
        f.weights.get_mut("residual.weight").unwrap().values[0] = 1.0;
        f.weights.get_mut("residual.weight").unwrap().values[32 + 1] = 1.0;
        let m = MotorPolicy::from_file(f).unwrap();
        let mut x = [0.0; 18];
        x[..5].copy_from_slice(&[0.6, 0.8, 0.5, 0.2, -0.1]);
        let out = m.infer(&x).unwrap();
        assert!((out.thrust_logit - (10.0 / 11.0) * (-0.5_f32).tanh().tanh()).abs() < 1e-6);
        assert!((out.brake_logit - (10.0 / 11.0) * 0.5_f32.tanh().tanh()).abs() < 1e-6);
    }

    #[test]
    fn legacy_and_residual_contracts_reject_mixed_or_invalid_weights() {
        assert!(MotorPolicy::from_file(file(false)).is_ok());
        let mut wrong = file(true);
        wrong.format_version = 1;
        assert!(MotorPolicy::from_file(wrong).is_err());
        let mut missing = file(true);
        missing.weights.remove("residual_log_std");
        assert!(MotorPolicy::from_file(missing).is_err());
        let mut invalid = file(true);
        invalid.weights.get_mut("residual.bias").unwrap().values[0] = f32::NAN;
        assert!(MotorPolicy::from_file(invalid).is_err());
        let m = MotorPolicy::from_file(file(true)).unwrap();
        let mut x = [0.0; 18];
        x[2] = -0.1;
        assert!(m.infer(&x).is_err());
    }
}
