//! Versioned, bounded dense models, independent of any particular organism.
//!
//! V1 stores row-major weights followed by per-output biases in each layer.
//! Inference is feed-forward; episode state and fitness do not belong here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const MODEL_VERSION: u32 = 1;
pub const MAX_MODEL_BYTES: usize = 2 * 1024 * 1024;
const MAX_WIDTH: usize = 4096;
const MAX_PARAMETERS: usize = 65_536;
const MAX_LAYERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelContract {
    pub organism: String,
    pub role: String,
    pub observation: String,
    pub action: String,
    pub inputs: usize,
    pub outputs: usize,
    pub cadence_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Linear,
    Tanh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMemory {
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenseLayer {
    pub outputs: usize,
    pub activation: Activation,
    pub weights: Vec<f32>,
    pub biases: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenseGenome {
    pub version: u32,
    pub contract: ModelContract,
    pub memory: ModelMemory,
    pub layers: Vec<DenseLayer>,
}

impl DenseGenome {
    pub fn parameter_count(&self) -> usize {
        self.layers
            .iter()
            .map(|layer| layer.weights.len() + layer.biases.len())
            .sum()
    }

    pub fn parameters(&self) -> impl Iterator<Item = &f32> {
        self.layers
            .iter()
            .flat_map(|layer| layer.weights.iter().chain(&layer.biases))
    }

    pub fn parameters_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.layers
            .iter_mut()
            .flat_map(|layer| layer.weights.iter_mut().chain(&mut layer.biases))
    }

    /// Equal interfaces do not imply compatible mutation/seed layouts.
    pub fn same_architecture(&self, other: &Self) -> bool {
        self.version == other.version
            && self.contract == other.contract
            && self.memory == other.memory
            && self.layers.len() == other.layers.len()
            && self.layers.iter().zip(&other.layers).all(|(a, b)| {
                a.outputs == b.outputs
                    && a.activation == b.activation
                    && a.weights.len() == b.weights.len()
                    && a.biases.len() == b.biases.len()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError(pub String);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

#[derive(Debug, Clone)]
pub struct DenseModel {
    genome: DenseGenome,
    id: String,
}

impl DenseModel {
    pub fn new(genome: DenseGenome) -> Result<Self, ModelError> {
        validate(&genome)?;
        // Struct field order and f32 encoding are part of the V1 identity format.
        let bytes = serde_json::to_vec(&genome).map_err(|e| ModelError(e.to_string()))?;
        let mut hash = Sha256::new();
        hash.update(b"spacewars.dense_genome.v1\0");
        hash.update(&bytes);
        Ok(Self {
            genome,
            id: format!("{:x}", hash.finalize()),
        })
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ModelError> {
        if bytes.len() > MAX_MODEL_BYTES {
            return Err(ModelError("model file exceeds the size limit".into()));
        }
        let genome = serde_json::from_slice(bytes).map_err(|e| ModelError(e.to_string()))?;
        Self::new(genome)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn genome(&self) -> &DenseGenome {
        &self.genome
    }

    pub fn require_contract(&self, expected: &ModelContract) -> Result<(), ModelError> {
        if &self.genome.contract != expected {
            return Err(ModelError(format!(
                "incompatible model contract: expected {}/{} with {} -> {} inputs/outputs",
                expected.organism, expected.role, expected.inputs, expected.outputs
            )));
        }
        Ok(())
    }

    pub fn infer(&self, inputs: &[f32]) -> Result<Vec<f32>, ModelError> {
        if inputs.len() != self.genome.contract.inputs || inputs.iter().any(|v| !v.is_finite()) {
            return Err(ModelError("invalid model inputs".into()));
        }
        let mut values = inputs.to_vec();
        for layer in &self.genome.layers {
            let mut next = Vec::with_capacity(layer.outputs);
            for (weights, bias) in layer.weights.chunks_exact(values.len()).zip(&layer.biases) {
                let mut sum = *bias;
                for (weight, value) in weights.iter().zip(&values) {
                    sum += weight * value;
                }
                if !sum.is_finite() {
                    return Err(ModelError("non-finite model output".into()));
                }
                next.push(match layer.activation {
                    Activation::Linear => sum,
                    Activation::Tanh => sum.tanh(),
                });
            }
            values = next;
        }
        Ok(values)
    }
}

fn validate(genome: &DenseGenome) -> Result<(), ModelError> {
    let c = &genome.contract;
    if genome.version != MODEL_VERSION {
        return Err(ModelError("unsupported model version".into()));
    }
    for name in [&c.organism, &c.role, &c.observation, &c.action] {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-/".contains(&b))
        {
            return Err(ModelError("invalid contract identifier".into()));
        }
    }
    if !(1..=MAX_WIDTH).contains(&c.inputs)
        || !(1..=MAX_WIDTH).contains(&c.outputs)
        || c.cadence_ticks == 0
        || !(1..=MAX_LAYERS).contains(&genome.layers.len())
    {
        return Err(ModelError("invalid model dimensions or cadence".into()));
    }
    let mut inputs = c.inputs;
    let mut count = 0;
    for layer in &genome.layers {
        if !(1..=MAX_WIDTH).contains(&layer.outputs)
            || layer.weights.len() != inputs * layer.outputs
            || layer.biases.len() != layer.outputs
        {
            return Err(ModelError(
                "layer parameter layout does not match dimensions".into(),
            ));
        }
        count += layer.weights.len() + layer.biases.len();
        if count > MAX_PARAMETERS {
            return Err(ModelError("model parameter limit exceeded".into()));
        }
        inputs = layer.outputs;
    }
    if inputs != c.outputs || genome.parameters().any(|p| !p.is_finite()) {
        return Err(ModelError(
            "invalid output dimension or non-finite parameter".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
