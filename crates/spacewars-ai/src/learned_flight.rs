//! Versioned CPU inference for the C3 learned-flight experiment. The caller owns
//! goals, observations and physical outcome evaluation; no native fallback.
pub use crate::c3::encoder::Tensor;
use crate::c3::encoder::{self, Encoder, Inputs};
use scenario_spacewars::surface_sortie::{
    flight_environment::FlightAction,
    flight_policy_observation::{FlightPolicyInputs, schema},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::c3::ARCHITECTURE;
pub const GOAL_RESIDUAL_ARCHITECTURE: &str =
    "polar-conv8x8-coordinates-set-attention32-mlp64x64-goal16-v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightPolicyFile {
    pub format_version: u32,
    pub architecture: String,
    pub observation_schema: serde_json::Value,
    pub weights: BTreeMap<String, Tensor>,
}

pub struct FlightPolicy {
    weights: BTreeMap<String, Tensor>,
    goal_residual: bool,
    /// Actor heads: three for the flight and v1 and v2 navigation policies,
    /// four for a v3 navigation policy with its turn sense.
    heads: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FlightPolicyOutput {
    pub steering_latent: f32,
    pub thrust_logit: f32,
    pub brake_logit: f32,
    pub value: f32,
    pub action: FlightAction,
}

impl FlightPolicy {
    pub fn from_file(file: FlightPolicyFile) -> Result<Self, &'static str> {
        let goal_residual = match (file.format_version, file.architecture.as_str()) {
            (2, ARCHITECTURE) => false,
            (3, GOAL_RESIDUAL_ARCHITECTURE) => true,
            _ => return Err("incompatible flight policy contract"),
        };
        Self::validated(file, goal_residual, 3)
    }

    /// Checks the schema and every tensor of a file whose contract the caller
    /// has already matched; the navigation policy shares these weights under
    /// its own format version and reads the heads differently, a fourth
    /// among them under its v3 contract.
    pub(crate) fn validated(
        file: FlightPolicyFile,
        goal_residual: bool,
        heads: usize,
    ) -> Result<Self, &'static str> {
        if file.observation_schema != schema() {
            return Err("incompatible flight policy contract");
        }
        let expected = Self::expected_shapes(goal_residual, heads);
        if file.weights.len() != expected.len() {
            return Err("wrong flight policy tensor count");
        }
        for (name, shape) in expected {
            let tensor = file
                .weights
                .get(&name)
                .ok_or("missing flight policy tensor")?;
            if tensor.shape != shape
                || tensor.values.len() != shape.iter().product::<usize>()
                || !tensor
                    .values
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1e6)
            {
                return Err("invalid flight policy tensor");
            }
        }
        Ok(Self {
            weights: file.weights,
            goal_residual,
            heads,
        })
    }

    /// Tensor names and shapes of the exported encoder, heads and deviations.
    pub(crate) fn expected_shapes(
        goal_residual: bool,
        heads: usize,
    ) -> BTreeMap<String, Vec<usize>> {
        let mut expected = encoder::shapes(35, 16, heads);
        expected.insert("critic.weight".into(), vec![1, 64]);
        expected.insert("critic.bias".into(), vec![1]);
        expected.insert("log_std".into(), vec![heads]);
        if goal_residual {
            expected.insert("goal_hidden.weight".into(), vec![16, 2]);
            expected.insert("goal_hidden.bias".into(), vec![16]);
            expected.insert("goal_output.weight".into(), vec![3, 16]);
            expected.insert("goal_output.bias".into(), vec![3]);
        }
        expected
    }

    pub fn infer(&self, inputs: &FlightPolicyInputs) -> Result<FlightPolicyOutput, &'static str> {
        let (actor, value) = self.heads(inputs)?;
        Ok(FlightPolicyOutput {
            steering_latent: actor[0],
            thrust_logit: actor[1],
            brake_logit: actor[2],
            value,
            action: FlightAction {
                steering: actor[0].tanh(),
                thrust: 1.0 / (1.0 + (-actor[1]).exp()),
                brake: 1.0 / (1.0 + (-actor[2]).exp()),
            },
        })
    }

    /// The raw actor heads, three or four, and the value, before any output
    /// mapping.
    pub(crate) fn heads(
        &self,
        inputs: &FlightPolicyInputs,
    ) -> Result<(Vec<f32>, f32), &'static str> {
        let encoder = Encoder(&self.weights);
        let x = encoder.encode(Inputs {
            self_goal: &inputs.self_goal,
            cells: &inputs.cells,
            rays: &inputs.rays,
            nearest: &inputs.nearest,
            objects: &inputs.objects,
        })?;
        let mut actor = encoder.linear("actor", &x, false);
        if self.goal_residual {
            let goal = [inputs.self_goal[0] * 25.0, inputs.self_goal[1] * 25.0];
            let goal = encoder.linear("goal_hidden", &goal, true);
            let correction = encoder.linear("goal_output", &goal, false);
            for (output, delta) in actor.iter_mut().zip(correction) {
                *output += delta;
            }
        }
        let value = encoder.linear("critic", &x, false)[0];
        if !actor
            .iter()
            .chain(std::iter::once(&value))
            .all(|v| v.is_finite())
        {
            return Err("non-finite flight inference");
        }
        debug_assert_eq!(actor.len(), self.heads);
        Ok((actor, value))
    }
}
