//! Evolvable polar policy for the creature senses and held control profile.
use crate::model::{MAX_MODEL_BYTES, ModelError};
use engine_common::brain_controls::{
    BrainBinding, CREATURE_PROFILE, CREATURE_SENSES, CREATURE_SLOTS,
};
use engine_rapier::world::polar::policy::{
    CREATURE_OBJECT_WIDTH, CREATURE_SELF_FEATURES, CreatureObservation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub use crate::evolution_draws::EvolutionDraws;
pub(crate) mod encoder;
mod layers;
use encoder::{Encoder, Inputs, Tensor};

pub const ARCHITECTURE: &str = "polar-conv8x8-coordinates-set-attention32-mlp64x64-v1";
pub const DECISION_TICKS: u32 = 3;
pub const PARAMETER_LIMIT: f32 = 8.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenomeFile {
    version: u32,
    architecture: String,
    binding: BrainBinding,
    cadence_ticks: u32,
    weights: BTreeMap<String, Tensor>,
}

/// A validated, immutable set of weights with a content-addressed identity.
#[derive(Debug, Clone)]
pub struct C3Genome {
    file: GenomeFile,
    id: String,
}

fn shapes() -> BTreeMap<String, Vec<usize>> {
    encoder::shapes(
        CREATURE_SELF_FEATURES.len(),
        CREATURE_OBJECT_WIDTH,
        CREATURE_SLOTS.len(),
    )
}

impl C3Genome {
    pub fn random(seed: u64, candidate: usize) -> Self {
        let mut rng = EvolutionDraws::new(seed, 0, candidate, b"initial");
        let weights = shapes()
            .into_iter()
            .map(|(name, shape)| {
                // A fan-in scaled uniform initialization keeps large ray and fusion
                // layers responsive to input. Biases start at zero.
                let fan_in = shape[1..].iter().product::<usize>();
                let limit = (3.0 / fan_in as f32).sqrt();
                let count = shape.iter().product::<usize>();
                let values = if shape.len() == 1 {
                    vec![0.0; count]
                } else {
                    (0..count).map(|_| rng.signed() * limit).collect()
                };
                (name, Tensor { shape, values })
            })
            .collect();
        Self::from_file(GenomeFile {
            version: 1,
            architecture: ARCHITECTURE.into(),
            binding: BrainBinding::creature(),
            cadence_ticks: DECISION_TICKS,
            weights,
        })
    }

    fn from_file(file: GenomeFile) -> Self {
        // Field order, tensor names in ascending byte order and row-major f32
        // values are the v1 identity and mutation traversal contracts.
        let bytes = serde_json::to_vec(&file).expect("finite C3 parameters");
        let mut hash = Sha256::new();
        hash.update(b"spacewars.c3_genome.v1\0");
        hash.update(bytes);
        Self {
            file,
            id: format!("{:x}", hash.finalize()),
        }
    }

    pub fn mutated(
        &self,
        draws: &mut EvolutionDraws,
        probability: f32,
        scale: f32,
    ) -> Result<Self, ModelError> {
        if !(0.0..=1.0).contains(&probability) || !(0.0..=PARAMETER_LIMIT).contains(&scale) {
            return Err(ModelError(
                "invalid C3 mutation probability or scale".into(),
            ));
        }
        let mut file = self.file.clone();
        draws.mutate(
            file.weights.values_mut().flat_map(|t| t.values.iter_mut()),
            probability,
            scale,
            PARAMETER_LIMIT,
        );
        Ok(Self::from_file(file))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn binding(&self) -> &BrainBinding {
        &self.file.binding
    }

    pub fn parameter_count(&self) -> usize {
        self.file.weights.values().map(|t| t.values.len()).sum()
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ModelError> {
        serde_json::to_vec(&self.file).map_err(|e| ModelError(e.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ModelError> {
        if bytes.len() > MAX_MODEL_BYTES {
            return Err(ModelError("C3 file exceeds the size limit".into()));
        }
        let file: GenomeFile =
            serde_json::from_slice(bytes).map_err(|e| ModelError(e.to_string()))?;
        if file.version != 1
            || file.architecture != ARCHITECTURE
            || file.binding.profile != CREATURE_PROFILE
            || file.binding.senses != CREATURE_SENSES
            || file.cadence_ticks != DECISION_TICKS
        {
            return Err(ModelError("incompatible C3 genome contract".into()));
        }
        for required in ["move", "jump"] {
            if !file
                .binding
                .required_controls
                .iter()
                .any(|slot| slot == required)
            {
                return Err(ModelError(format!("C3 genome must require {required}")));
            }
        }
        let expected = shapes();
        if file.weights.len() != expected.len() {
            return Err(ModelError("invalid C3 tensor count".into()));
        }
        for (name, shape) in expected {
            let tensor = file
                .weights
                .get(&name)
                .ok_or_else(|| ModelError(format!("missing C3 tensor {name}")))?;
            if tensor.shape != shape
                || tensor.values.len() != shape.iter().product::<usize>()
                || tensor
                    .values
                    .iter()
                    .any(|v| !(-PARAMETER_LIMIT..=PARAMETER_LIMIT).contains(v))
            {
                return Err(ModelError(format!("invalid C3 tensor {name}")));
            }
        }
        Ok(Self::from_file(file))
    }

    /// The body decoder validates the four post-activation outputs together
    /// and records a neutral decision if any slot is invalid.
    pub fn infer(&self, observation: &CreatureObservation) -> Result<[f32; 4], ModelError> {
        let encoder = Encoder(&self.file.weights);
        let encoded = encoder
            .encode(Inputs {
                self_goal: &observation.self_goal,
                cells: &observation.geometry.cells,
                rays: &observation.geometry.rays,
                nearest: &observation.geometry.nearest,
                objects: &observation.objects,
            })
            .map_err(|err| ModelError(err.into()))?;
        let actions = encoder.linear("actor", &encoded, true);
        Ok(actions.try_into().expect("validated action head shape"))
    }
}

#[cfg(test)]
mod tests;
