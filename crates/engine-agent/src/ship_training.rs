//! First bounded ship-learning measurement. Only the training partition enters
//! selection; stationary validation and moving-target transfer are reports.

use crate::{
    evolution::{EvaluationTask, EvolutionConfig, EvolutionError, EvolutionRun, TaskManifest},
    ship_arrival::{self, ArrivalCase, ArrivalEpisode, ArrivalFitness, ArrivalTask},
};
use engine_core::Vec2;
use serde::Serialize;
use spacewars_ai::{
    guidance::{self, Policy},
    model::{Activation, DenseGenome, DenseLayer, DenseModel, MODEL_VERSION, ModelMemory},
};

pub const EXPERIMENT: &str = "material_arrival_learning_v1";
pub const TRAINING_RECIPE: &str = "material_arrival_stationary_train_v1";
pub const SEEDS: [u64; 3] = [1, 2, 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Training,
    StationaryValidation,
    MovingTransfer,
}

pub fn cases(partition: Partition) -> Vec<ArrivalCase> {
    let fixed = ship_arrival::cases();
    match partition {
        Partition::Training => fixed[..4].to_vec(),
        Partition::MovingTransfer => fixed[5..].to_vec(),
        Partition::StationaryValidation => {
            let mut radial = fixed[0].clone();
            radial.name = "validation_radial_close".into();
            radial.target_offset = Vec2::new(0.0, 12.0);
            radial.start.radial_speed = 2.0;
            let mut right = fixed[1].clone();
            right.name = "validation_right_drift".into();
            right.target_offset = Vec2::new(14.0, 8.0);
            right.start.heading_offset = -0.2;
            right.start.lateral_speed = -2.0;
            let mut left = fixed[2].clone();
            left.name = "validation_left_rotated".into();
            left.target_offset = Vec2::new(-14.0, 8.0);
            left.start.bearing = -std::f32::consts::FRAC_PI_2;
            left.start.heading_offset = 0.2;
            left.start.lateral_speed = 2.0;
            vec![radial, right, left, fixed[4].clone()]
        }
    }
}

/// Zero-valued architecture template, not an initialization seed. The runner
/// randomizes every weight and bias independently for each initial candidate.
pub fn template() -> DenseGenome {
    DenseGenome {
        version: MODEL_VERSION,
        contract: guidance::contract(),
        memory: ModelMemory::None,
        layers: vec![
            DenseLayer {
                outputs: 8,
                activation: Activation::Tanh,
                weights: vec![0.0; 9 * 8],
                biases: vec![0.0; 8],
            },
            DenseLayer {
                outputs: 3,
                activation: Activation::Linear,
                weights: vec![0.0; 8 * 3],
                biases: vec![0.0; 3],
            },
        ],
    }
}

pub fn config(seed: u64) -> Result<EvolutionConfig, EvolutionError> {
    if !SEEDS.contains(&seed) {
        return Err(EvolutionError("experiment seed must be 1, 2, or 3".into()));
    }
    Ok(EvolutionConfig {
        seed,
        population: 32,
        generations: 48,
        elites: 2,
        tournament_size: 3,
        mutation_probability: 0.1,
        mutation_scale: 0.35,
        initial_scale: 0.5,
        parameter_limit: 8.0,
    })
}

#[derive(Debug, Clone, Default)]
pub struct TrainingTask;

impl EvaluationTask for TrainingTask {
    type Fitness = ArrivalFitness;

    fn manifest(&self) -> TaskManifest {
        // Reuse the calibrated physics, gates, and scoring without changing the
        // identity or case list of the original eight-case diagnostic.
        let mut manifest = ArrivalTask.manifest();
        manifest.id = TRAINING_RECIPE.into();
        manifest.configuration["cases"] = serde_json::json!(cases(Partition::Training));
        manifest.configuration["parent_recipe"] = ship_arrival::RECIPE.into();
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<ArrivalFitness, EvolutionError> {
        let mut total = ArrivalFitness::default();
        for case in cases(Partition::Training) {
            add(
                &mut total,
                ship_arrival::run_episode(case, Policy::Model(model.clone()), false)?.fitness,
            );
        }
        Ok(total)
    }
}

fn add(total: &mut ArrivalFitness, score: ArrivalFitness) {
    total.completed += score.completed;
    total.position_points += score.position_points;
    total.velocity_points += score.velocity_points;
    total.spare_ticks += score.spare_ticks;
}

/// The example accepts this experiment only; generic/custom runs use the shared
/// EvolutionRun API and must identify their own recipe instead.
pub fn validate_run(run: &EvolutionRun<ArrivalFitness>) -> Result<(), EvolutionError> {
    let template = template();
    if run.task() != &TrainingTask.manifest()
        || run.config() != &config(run.config().seed)?
        || run
            .population()
            .iter()
            .any(|candidate| !candidate.genome.same_architecture(&template))
    {
        return Err(EvolutionError(
            "checkpoint is not this ship experiment".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ExperimentManifest {
    pub version: u32,
    pub id: &'static str,
    pub runtime: String,
    pub training: TaskManifest,
    pub architecture_template: DenseGenome,
    pub runs: Vec<EvolutionConfig>,
    pub operators: &'static str,
    pub seed_genomes: Vec<DenseGenome>,
    pub finalist_selection: &'static str,
    pub stationary_validation: Vec<ArrivalCase>,
    pub moving_transfer: Vec<ArrivalCase>,
}

pub fn manifest() -> Result<ExperimentManifest, EvolutionError> {
    Ok(ExperimentManifest {
        version: 1,
        id: EXPERIMENT,
        runtime: crate::evolution::runtime_id()?,
        training: TrainingTask.manifest(),
        architecture_template: template(),
        runs: SEEDS.into_iter().map(config).collect::<Result<_, _>>()?,
        operators: "sha256_counter_uniform_initialization_tournament_mutation_elitism_v1",
        seed_genomes: vec![],
        finalist_selection: "best_training_fitness_in_last_generation_per_seed_v1",
        stationary_validation: cases(Partition::StationaryValidation),
        moving_transfer: cases(Partition::MovingTransfer),
    })
}

#[derive(Debug, Serialize)]
pub struct PartitionResult {
    pub partition: Partition,
    pub fitness: ArrivalFitness,
    pub episodes: Vec<ArrivalEpisode>,
}

#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub manifest: ExperimentManifest,
    pub controller: String,
    pub partitions: Vec<PartitionResult>,
}

/// Called explicitly after training. These results never enter EvolutionRun.
pub fn evaluate(policy: Policy) -> Result<Evaluation, EvolutionError> {
    let mut partitions = Vec::new();
    for partition in [
        Partition::Training,
        Partition::StationaryValidation,
        Partition::MovingTransfer,
    ] {
        let mut episodes = Vec::new();
        let mut fitness = ArrivalFitness::default();
        for case in cases(partition) {
            let episode = ship_arrival::run_episode(case, policy.clone(), false)?;
            add(&mut fitness, episode.fitness);
            episodes.push(episode);
        }
        partitions.push(PartitionResult {
            partition,
            fitness,
            episodes,
        });
    }
    Ok(Evaluation {
        manifest: manifest()?,
        controller: policy.id().into(),
        partitions,
    })
}

#[cfg(test)]
mod tests;
