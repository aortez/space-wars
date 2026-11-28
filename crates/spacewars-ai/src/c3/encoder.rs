//! Shared C3 feature extraction. The profile sizes its self block, masked
//! object records and output head. Every profile uses the same polar geometry.
use super::layers::{conv, linear, pool};
use engine_rapier::world::polar::policy::{BAND_EDGES_BODY_LENGTHS, DISTANCE_SCALE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub values: Vec<f32>,
}

pub(crate) fn shapes(
    self_width: usize,
    object_width: usize,
    heads: usize,
) -> BTreeMap<String, Vec<usize>> {
    let mut expected = BTreeMap::new();
    for (name, input, output) in [
        ("map_fc", 192, 32),
        ("rays_fc", 512, 32),
        ("object_fc1", object_width - 1, 32),
        ("object_fc2", 32, 32),
        ("query", self_width, 32),
        ("fusion1", self_width + 102, 64),
        ("fusion2", 64, 64),
        ("actor", 64, heads),
    ] {
        expected.insert(format!("{name}.weight"), vec![output, input]);
        expected.insert(format!("{name}.bias"), vec![output]);
    }
    expected.insert("conv1.weight".into(), vec![8, 4, 3, 3]);
    expected.insert("conv1.bias".into(), vec![8]);
    expected.insert("conv2.weight".into(), vec![8, 8, 3, 3]);
    expected.insert("conv2.bias".into(), vec![8]);
    expected
}

pub(crate) struct Inputs<'a, const N: usize> {
    pub self_goal: &'a [f32],
    pub cells: &'a [[f32; 2]],
    pub rays: &'a [[f32; 4]],
    pub nearest: &'a [f32; 6],
    pub objects: &'a [[f32; N]],
}

pub(crate) struct Encoder<'a>(pub &'a BTreeMap<String, Tensor>);

impl Encoder<'_> {
    pub(crate) fn linear(&self, name: &str, x: &[f32], activation: bool) -> Vec<f32> {
        linear(
            x,
            &self.0[&format!("{name}.weight")],
            &self.0[&format!("{name}.bias")],
            activation,
        )
    }

    pub(crate) fn encode<const N: usize>(
        &self,
        inputs: Inputs<'_, N>,
    ) -> Result<Vec<f32>, &'static str> {
        if inputs.self_goal.len() != self.0["query.weight"].shape[1]
            || N != self.0["object_fc1.weight"].shape[1] + 1
            || inputs.cells.len() != 1472
            || inputs.rays.len() != 128
            || inputs.objects.len() != 32
            || !inputs
                .self_goal
                .iter()
                .chain(inputs.cells.iter().flatten())
                .chain(inputs.rays.iter().flatten())
                .chain(inputs.nearest)
                .chain(inputs.objects.iter().flatten())
                .all(|v| v.is_finite())
        {
            return Err("invalid polar policy input shape or value");
        }
        if inputs.objects.iter().any(|o| o[0] != 0.0 && o[0] != 1.0) {
            return Err("invalid object mask");
        }
        let mut map = vec![0.0; 4 * 23 * 64];
        for r in 0..23 {
            for a in 0..64 {
                let i = r * 64 + a;
                map[i] = inputs.cells[i][0];
                map[23 * 64 + i] = inputs.cells[i][1];
                map[2 * 23 * 64 + i] = BAND_EDGES_BODY_LENGTHS[r] / DISTANCE_SCALE;
                map[3 * 23 * 64 + i] = BAND_EDGES_BODY_LENGTHS[r + 1] / DISTANCE_SCALE;
            }
        }
        let map = conv::<64>(&map, 4, 23, &self.0["conv1.weight"], &self.0["conv1.bias"]);
        let map = conv::<32>(&map, 8, 12, &self.0["conv2.weight"], &self.0["conv2.bias"]);
        let map = self.linear("map_fc", &pool(&map), true);
        let rays = self.linear("rays_fc", inputs.rays.as_flattened(), true);
        let query = self.linear("query", inputs.self_goal, false);
        let mut objects = Vec::new();
        let mut scores = Vec::new();
        for row in inputs.objects {
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
        let mut x = inputs.self_goal.to_vec();
        x.extend(map);
        x.extend(rays);
        x.extend(inputs.nearest);
        x.extend(pooled);
        let x = self.linear("fusion1", &x, true);
        Ok(self.linear("fusion2", &x, true))
    }
}
