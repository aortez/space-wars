//! Version 2 models: a dense trunk fed by direct inputs and by shared set
//! encoders. An encoder applies one dense layer to every item of a
//! fixed-capacity set and max-pools the results, so a set's capacity changes
//! the input size but not the parameter count. Layers keep V1's layout; the
//! trunk computes exactly as `DenseModel::infer` does.

use super::*;

pub const SHARED_MODEL_VERSION: u32 = 2;
const MAX_ENCODERS: usize = 4;
const MAX_ITEMS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    /// Per output, the largest value over the pooled items; 0 when none is.
    Max,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetEncoder {
    pub items: usize,
    pub item_width: usize,
    /// Position of a presence flag within each item. Items whose flag is not
    /// positive are left out of the pool.
    pub presence: Option<usize>,
    pub pool: Pool,
    /// Reads one item; its outputs are pooled across items.
    pub layer: DenseLayer,
}

impl SetEncoder {
    fn values(&self) -> usize {
        self.items * self.item_width
    }
}

/// Inputs are the direct inputs, then each encoder's items in order. The
/// trunk reads the direct inputs, then each encoder's pooled outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedGenome {
    pub version: u32,
    pub contract: ModelContract,
    pub memory: ModelMemory,
    pub direct_inputs: usize,
    pub encoders: Vec<SetEncoder>,
    pub trunk: Vec<DenseLayer>,
}

impl SharedGenome {
    /// A genome that computes exactly what `dense` computes, whatever the
    /// encoders read: the trunk is `dense`, and its first layer's weights for
    /// the pooled outputs are zero.
    pub fn from_dense(
        dense: &DenseGenome,
        contract: ModelContract,
        encoders: Vec<SetEncoder>,
    ) -> Result<Self, ModelError> {
        validate(dense)?;
        let direct_inputs = dense.contract.inputs;
        let pooled: usize = encoders.iter().map(|e| e.layer.outputs).sum();
        let mut trunk = dense.layers.clone();
        let first = &mut trunk[0];
        first.weights = first
            .weights
            .chunks_exact(direct_inputs)
            .flat_map(|row| row.iter().copied().chain(std::iter::repeat_n(0.0, pooled)))
            .collect();
        let genome = Self {
            version: SHARED_MODEL_VERSION,
            contract,
            memory: dense.memory,
            direct_inputs,
            encoders,
            trunk,
        };
        validate_shared(&genome)?;
        Ok(genome)
    }

    pub fn parameter_count(&self) -> usize {
        self.layers()
            .map(|layer| layer.weights.len() + layer.biases.len())
            .sum()
    }

    /// Encoder layers in order, then trunk layers; weights before biases.
    pub fn parameters(&self) -> impl Iterator<Item = &f32> {
        self.layers()
            .flat_map(|layer| layer.weights.iter().chain(&layer.biases))
    }

    pub fn parameters_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.encoders
            .iter_mut()
            .map(|e| &mut e.layer)
            .chain(&mut self.trunk)
            .flat_map(|layer| layer.weights.iter_mut().chain(&mut layer.biases))
    }

    fn layers(&self) -> impl Iterator<Item = &DenseLayer> {
        self.encoders.iter().map(|e| &e.layer).chain(&self.trunk)
    }

    /// Equal interfaces do not imply compatible mutation/seed layouts.
    pub fn same_architecture(&self, other: &Self) -> bool {
        let shape = |g: &Self| {
            (
                g.encoders
                    .iter()
                    .map(|e| (e.items, e.item_width, e.presence, e.pool))
                    .collect::<Vec<_>>(),
                g.layers()
                    .map(|l| (l.outputs, l.activation, l.weights.len(), l.biases.len()))
                    .collect::<Vec<_>>(),
            )
        };
        self.version == other.version
            && self.contract == other.contract
            && self.memory == other.memory
            && self.direct_inputs == other.direct_inputs
            && shape(self) == shape(other)
    }
}

#[derive(Debug, Clone)]
pub struct SharedModel {
    genome: SharedGenome,
    id: String,
}

impl SharedModel {
    pub fn new(genome: SharedGenome) -> Result<Self, ModelError> {
        validate_shared(&genome)?;
        // Struct field order and f32 encoding are part of the V2 identity format.
        let bytes = serde_json::to_vec(&genome).map_err(|e| ModelError(e.to_string()))?;
        let mut hash = Sha256::new();
        hash.update(b"spacewars.shared_genome.v2\0");
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

    pub fn genome(&self) -> &SharedGenome {
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
        let genome = &self.genome;
        if inputs.len() != genome.contract.inputs || inputs.iter().any(|v| !v.is_finite()) {
            return Err(ModelError("invalid model inputs".into()));
        }
        let (direct, mut rest) = inputs.split_at(genome.direct_inputs);
        let mut values = direct.to_vec();
        for encoder in &genome.encoders {
            let (items, next) = rest.split_at(encoder.values());
            rest = next;
            let start = values.len();
            values.resize(start + encoder.layer.outputs, f32::NEG_INFINITY);
            let mut pooled = false;
            for item in items.chunks_exact(encoder.item_width) {
                if encoder.presence.is_some_and(|flag| item[flag] <= 0.0) {
                    continue;
                }
                pooled = true;
                let outputs = forward(&encoder.layer, item)?;
                for (best, value) in values[start..].iter_mut().zip(outputs) {
                    *best = best.max(value);
                }
            }
            if !pooled {
                values[start..].fill(0.0);
            }
        }
        for layer in &genome.trunk {
            values = forward(layer, &values)?;
        }
        Ok(values)
    }
}

/// One layer, summed in the same order as `DenseModel::infer`.
fn forward(layer: &DenseLayer, values: &[f32]) -> Result<Vec<f32>, ModelError> {
    let mut next = Vec::with_capacity(layer.outputs);
    for (weights, bias) in layer.weights.chunks_exact(values.len()).zip(&layer.biases) {
        let mut sum = *bias;
        for (weight, value) in weights.iter().zip(values) {
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
    Ok(next)
}

fn validate_shared(genome: &SharedGenome) -> Result<(), ModelError> {
    let c = &genome.contract;
    if genome.version != SHARED_MODEL_VERSION {
        return Err(ModelError("unsupported model version".into()));
    }
    validate_identifiers(c)?;
    if !(1..=MAX_WIDTH).contains(&c.inputs)
        || !(1..=MAX_WIDTH).contains(&c.outputs)
        || c.cadence_ticks == 0
        || genome.direct_inputs > MAX_WIDTH
        || genome.encoders.len() > MAX_ENCODERS
        || !(1..=MAX_LAYERS).contains(&genome.trunk.len())
    {
        return Err(ModelError("invalid model dimensions or cadence".into()));
    }
    let mut inputs = genome.direct_inputs;
    let mut trunk_inputs = genome.direct_inputs;
    for encoder in &genome.encoders {
        let layer = &encoder.layer;
        if !(1..=MAX_ITEMS).contains(&encoder.items)
            || !(1..=MAX_WIDTH).contains(&encoder.item_width)
            || encoder
                .presence
                .is_some_and(|flag| flag >= encoder.item_width)
            || !(1..=MAX_WIDTH).contains(&layer.outputs)
            || layer.weights.len() != encoder.item_width * layer.outputs
            || layer.biases.len() != layer.outputs
        {
            return Err(ModelError(
                "encoder parameter layout does not match dimensions".into(),
            ));
        }
        inputs += encoder.values();
        trunk_inputs += layer.outputs;
    }
    if inputs != c.inputs || trunk_inputs > MAX_WIDTH {
        return Err(ModelError(
            "encoder sets do not match the contract inputs".into(),
        ));
    }
    for layer in &genome.trunk {
        if !(1..=MAX_WIDTH).contains(&layer.outputs)
            || layer.weights.len() != trunk_inputs * layer.outputs
            || layer.biases.len() != layer.outputs
        {
            return Err(ModelError(
                "layer parameter layout does not match dimensions".into(),
            ));
        }
        trunk_inputs = layer.outputs;
    }
    if genome.parameter_count() > MAX_PARAMETERS {
        return Err(ModelError("model parameter limit exceeded".into()));
    }
    if trunk_inputs != c.outputs || genome.parameters().any(|p| !p.is_finite()) {
        return Err(ModelError(
            "invalid output dimension or non-finite parameter".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
