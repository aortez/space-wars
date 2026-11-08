//! Versioned CPU inference for the C3 learned-flight experiment. The caller owns
//! goals, observations and physical outcome evaluation; no native fallback.
use scenario_spacewars::surface_sortie::{
    flight_environment::FlightAction,
    flight_policy_observation::{FlightPolicyInputs, schema},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod layers;
use layers::*;

pub const ARCHITECTURE: &str = "polar-conv8x8-coordinates-set-attention32-mlp64x64-v1";
pub const GOAL_RESIDUAL_ARCHITECTURE: &str =
    "polar-conv8x8-coordinates-set-attention32-mlp64x64-goal16-v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub values: Vec<f32>,
}

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
        let mut expected = BTreeMap::new();
        for (name, input, output) in [
            ("map_fc", 192, 32),
            ("rays_fc", 512, 32),
            ("object_fc1", 15, 32),
            ("object_fc2", 32, 32),
            ("query", 35, 32),
            ("fusion1", 137, 64),
            ("fusion2", 64, 64),
            ("actor", 64, heads),
            ("critic", 64, 1),
        ] {
            expected.insert(format!("{name}.weight"), vec![output, input]);
            expected.insert(format!("{name}.bias"), vec![output]);
        }
        expected.insert("conv1.weight".into(), vec![8, 4, 3, 3]);
        expected.insert("conv1.bias".into(), vec![8]);
        expected.insert("conv2.weight".into(), vec![8, 8, 3, 3]);
        expected.insert("conv2.bias".into(), vec![8]);
        expected.insert("log_std".into(), vec![heads]);
        if goal_residual {
            expected.insert("goal_hidden.weight".into(), vec![16, 2]);
            expected.insert("goal_hidden.bias".into(), vec![16]);
            expected.insert("goal_output.weight".into(), vec![3, 16]);
            expected.insert("goal_output.bias".into(), vec![3]);
        }
        expected
    }

    fn linear(&self, name: &str, x: &[f32], activation: bool) -> Vec<f32> {
        linear(
            x,
            &self.weights[&format!("{name}.weight")],
            &self.weights[&format!("{name}.bias")],
            activation,
        )
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
        if inputs.self_goal.len() != 35
            || inputs.cells.len() != 1472
            || inputs.rays.len() != 128
            || inputs.objects.len() != 32
            || !inputs.values().all(f32::is_finite)
        {
            return Err("invalid flight policy input shape or value");
        }
        if inputs.objects.iter().any(|o| o[0] != 0.0 && o[0] != 1.0) {
            return Err("invalid object mask");
        }
        let edges = scenario_spacewars::surface_sortie::flight_environment::flight_sensor_config()
            .radial_edges;
        let mut map = vec![0.0; 4 * 23 * 64];
        for r in 0..23 {
            for a in 0..64 {
                let i = r * 64 + a;
                map[i] = inputs.cells[i][0];
                map[23 * 64 + i] = inputs.cells[i][1];
                map[2 * 23 * 64 + i] = edges[r] / 300.0;
                map[3 * 23 * 64 + i] = edges[r + 1] / 300.0;
            }
        }
        let map = conv(
            &map,
            4,
            23,
            64,
            &self.weights["conv1.weight"],
            &self.weights["conv1.bias"],
        );
        let map = conv(
            &map,
            8,
            12,
            32,
            &self.weights["conv2.weight"],
            &self.weights["conv2.bias"],
        );
        let map = self.linear("map_fc", &pool(&map), true);
        let rays = self.linear(
            "rays_fc",
            &inputs.rays.iter().flatten().copied().collect::<Vec<_>>(),
            true,
        );
        let query = self.linear("query", &inputs.self_goal, false);
        let mut objects = Vec::new();
        let mut scores = Vec::new();
        for row in &inputs.objects {
            if row[0] == 0.0 {
                continue;
            }
            let x = self.linear("object_fc1", &row[1..], true);
            let x = self.linear("object_fc2", &x, true);
            scores.push(x.iter().zip(&query).map(|(a, b)| a * b).sum::<f32>() / (32.0_f32).sqrt());
            objects.push(x);
        }
        let mut pooled = vec![0.0; 32];
        if !scores.is_empty() {
            let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let weights: Vec<_> = scores.iter().map(|s| (s - max).exp()).collect();
            let total: f32 = weights.iter().sum();
            for (x, w) in objects.iter().zip(weights) {
                for (out, v) in pooled.iter_mut().zip(x) {
                    *out += v * (w / total);
                }
            }
        }
        let mut x = inputs.self_goal.clone();
        x.extend(map);
        x.extend(rays);
        x.extend(inputs.nearest);
        x.extend(pooled);
        let x = self.linear("fusion1", &x, true);
        let x = self.linear("fusion2", &x, true);
        let mut actor = self.linear("actor", &x, false);
        if self.goal_residual {
            let goal = [inputs.self_goal[0] * 25.0, inputs.self_goal[1] * 25.0];
            let goal = self.linear("goal_hidden", &goal, true);
            let correction = self.linear("goal_output", &goal, false);
            for (output, delta) in actor.iter_mut().zip(correction) {
                *output += delta;
            }
        }
        let value = self.linear("critic", &x, false)[0];
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
