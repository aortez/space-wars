//! Seeded mixed-arrival experiment. Stationary retention precedes aggregate
//! fitness; development and fresh evaluation never participate in selection.

use crate::{
    evolution::{EvaluationTask, EvolutionConfig, EvolutionError, EvolutionRun, TaskManifest},
    ship_arrival::{
        self, ArrivalCase, ArrivalEpisode, ArrivalFitness, ArrivalOutcome, ArrivalTask,
    },
    ship_qualification, ship_training,
};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::pilot::MaterialFlightStart;
use serde::{Deserialize, Serialize};
use spacewars_ai::{guidance::Policy, model::DenseModel};
use std::f32::consts::{FRAC_PI_2, PI};

pub const EXPERIMENT: &str = "material_arrival_mixed_learning_v1";
pub const TRAINING_RECIPE: &str = "material_arrival_mixed_train_v1";
pub const SEEDS: [u64; 3] = [1, 2, 3];
pub const FITNESS_ORDER: &str =
    "stationary_arrivals_then_total_arrivals_position_velocity_spare_ticks_v1";
pub const GATE: &str = "all_training_original_development_and_fresh_cases_arrived_no_exclusions_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Training,
    Original,
    Development,
    FreshStationary,
    FreshMoving,
    FreshInward,
}

pub const PARTITIONS: [Partition; 6] = [
    Partition::Training,
    Partition::Original,
    Partition::Development,
    Partition::FreshStationary,
    Partition::FreshMoving,
    Partition::FreshInward,
];

pub fn cases(partition: Partition) -> Vec<ArrivalCase> {
    match partition {
        Partition::Training => {
            let mut result = ship_training::cases(ship_training::Partition::Training);
            let development = ship_qualification::cases();
            // These cases are deliberately reused from inspected development
            // evidence. Evaluation reports keep their overlap explicit.
            for name in [
                "qualification_target_motion_b0_p0",
                "qualification_target_motion_b2_p1",
                "qualification_target_motion_b0_p4",
                "qualification_target_motion_b2_p5",
                "qualification_combined_a0_b0_p2",
                "qualification_combined_a0_b1_p2",
            ] {
                result.push(
                    development
                        .iter()
                        .find(|c| c.case.name == name)
                        .expect("frozen development case")
                        .case
                        .clone(),
                );
            }
            let mut inward = result[8].clone();
            inward.name = "mixed_train_inward_stationary".into();
            inward.start.bearing = 0.0;
            inward.start.heading_offset = 0.0;
            inward.start.lateral_speed = 0.0;
            inward.target_offset = Vec2::new(0.0, -12.0);
            inward.target_velocity = Vec2::ZERO;
            result.push(inward);
            let mut inward = result[8].clone();
            inward.name = "mixed_train_inward_mirrored".into();
            inward.start.bearing = FRAC_PI_2;
            inward.start.altitude = 75.0;
            inward.start.heading_offset = -FRAC_PI_2;
            inward.start.lateral_speed = -3.0;
            result.push(inward);
            result
        }
        Partition::Original => ship_qualification::regressions(),
        Partition::Development => ship_qualification::cases()
            .into_iter()
            .map(|c| c.case)
            .collect(),
        Partition::FreshStationary | Partition::FreshMoving | Partition::FreshInward => {
            let (name, profiles) = match partition {
                // Altitude, offset, heading, initial (lateral, radial) speed,
                // target velocity. Bearings and profiles are fixed, not drawn.
                Partition::FreshStationary => (
                    "stationary",
                    [
                        (85.0, (0.0, 15.0), 0.2, (0.0, 3.0), (0.0, 0.0)),
                        (85.0, (24.0, 6.0), -0.4, (-4.0, 0.0), (0.0, 0.0)),
                        (85.0, (-24.0, 6.0), 0.4, (4.0, 0.0), (0.0, 0.0)),
                        (85.0, (8.0, 22.0), 2.8, (0.0, -3.0), (0.0, 0.0)),
                    ],
                ),
                Partition::FreshMoving => (
                    "moving",
                    [
                        (95.0, (22.0, 9.0), 0.4, (0.0, -2.0), (2.75, 0.0)),
                        (95.0, (-22.0, 9.0), -0.4, (0.0, 2.0), (-2.75, 0.0)),
                        (95.0, (12.0, 12.0), 0.6, (-2.0, 0.0), (1.75, 0.75)),
                        (95.0, (-12.0, 12.0), -0.6, (2.0, 0.0), (-1.75, -0.75)),
                    ],
                ),
                Partition::FreshInward => (
                    "inward",
                    [
                        (68.0, (0.0, -7.0), 1.3, (2.5, 0.0), (0.0, -0.8)),
                        (74.0, (0.0, -10.0), -1.3, (-2.5, 0.0), (0.0, -0.8)),
                        (82.0, (8.0, -10.0), 0.5, (0.0, -2.0), (0.0, 0.0)),
                        (70.0, (-8.0, -6.0), -0.5, (2.0, -1.0), (-0.5, -0.6)),
                    ],
                ),
                _ => unreachable!(),
            };
            let mut result = Vec::new();
            for (b, bearing) in [PI / 6.0, 2.0 * PI / 3.0, -PI / 3.0, -5.0 * PI / 6.0]
                .into_iter()
                .enumerate()
            {
                for (p, (altitude, offset, heading, speed, velocity)) in
                    profiles.into_iter().enumerate()
                {
                    result.push(ArrivalCase {
                        name: format!("mixed_fresh_{name}_b{b}_p{p}"),
                        seed: 42,
                        start: MaterialFlightStart {
                            bearing,
                            altitude,
                            heading_offset: heading,
                            lateral_speed: speed.0,
                            radial_speed: speed.1,
                        },
                        target_offset: Vec2::new(offset.0, offset.1),
                        target_velocity: Vec2::new(velocity.0, velocity.1),
                        max_ticks: 1200,
                    });
                }
            }
            result
        }
    }
}

/// Field order is the selection order. Once all four original training cases
/// arrive, no candidate sacrificing one can displace that retained elite.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurriculumFitness {
    pub stationary_completed: u32,
    pub total: ArrivalFitness,
}

impl CurriculumFitness {
    fn record(&mut self, index: usize, score: ArrivalFitness) {
        if index < 4 {
            self.stationary_completed += score.completed;
        }
        self.total.completed += score.completed;
        self.total.position_points += score.position_points;
        self.total.velocity_points += score.velocity_points;
        self.total.spare_ticks += score.spare_ticks;
    }
}

pub fn config(seed: u64) -> Result<EvolutionConfig, EvolutionError> {
    if !SEEDS.contains(&seed) {
        return Err(EvolutionError("experiment seed must be 1, 2, or 3".into()));
    }
    Ok(EvolutionConfig {
        seed,
        population: 32,
        generations: 32,
        elites: 2,
        tournament_size: 3,
        mutation_probability: 0.05,
        mutation_scale: 0.15,
        initial_scale: 0.5,
        parameter_limit: 8.0,
    })
}

#[derive(Debug, Clone, Default)]
pub struct TrainingTask;

impl EvaluationTask for TrainingTask {
    type Fitness = CurriculumFitness;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = ArrivalTask.manifest();
        manifest.id = TRAINING_RECIPE.into();
        manifest.configuration["parent_recipe"] = ship_arrival::RECIPE.into();
        manifest.configuration["cases"] = serde_json::json!(cases(Partition::Training));
        manifest.configuration["comparison"] = FITNESS_ORDER.into();
        manifest.configuration["stationary_retention_case_count"] = 4.into();
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<CurriculumFitness, EvolutionError> {
        let mut fitness = CurriculumFitness::default();
        for (index, case) in cases(Partition::Training).into_iter().enumerate() {
            fitness.record(
                index,
                ship_arrival::run_episode(case, Policy::Model(model.clone()), false)?.fitness,
            );
        }
        Ok(fitness)
    }
}

pub fn new_run(seed: u64) -> Result<EvolutionRun<CurriculumFitness>, EvolutionError> {
    EvolutionRun::new(
        &TrainingTask,
        ship_training::template(),
        vec![ship_qualification::candidate()?.genome().clone()],
        config(seed)?,
    )
}

pub fn validate_run(run: &EvolutionRun<CurriculumFitness>) -> Result<(), EvolutionError> {
    if run.task() != &TrainingTask.manifest()
        || run.config() != &config(run.config().seed)?
        || run.seeds() != [ship_qualification::candidate()?.genome().clone()]
        || run
            .population()
            .iter()
            .any(|c| !c.genome.same_architecture(&ship_training::template()))
    {
        return Err(EvolutionError(
            "checkpoint is not this seeded mixed experiment".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Schedule {
    pub partition: Partition,
    pub cases: Vec<ArrivalCase>,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub version: u32,
    pub id: &'static str,
    pub runtime: String,
    pub training: TaskManifest,
    pub runs: Vec<EvolutionConfig>,
    pub seed_genome: spacewars_ai::model::DenseGenome,
    pub seed_id: &'static str,
    pub initialization: &'static str,
    pub operators: &'static str,
    pub finalist_selection: &'static str,
    pub gate: &'static str,
    pub partitions: Vec<Schedule>,
}

pub fn manifest() -> Result<Manifest, EvolutionError> {
    Ok(Manifest {
        version: 1,
        id: EXPERIMENT,
        runtime: crate::evolution::runtime_id()?,
        training: TrainingTask.manifest(),
        runs: SEEDS.into_iter().map(config).collect::<Result<_, _>>()?,
        seed_genome: ship_qualification::candidate()?.genome().clone(),
        seed_id: ship_qualification::CANDIDATE,
        initialization: "one_unchanged_seed_then_31_independently_randomized_candidates_v1",
        operators: "sha256_counter_uniform_initialization_tournament_mutation_elitism_v1",
        finalist_selection: "best_training_fitness_in_last_generation_per_seed_v1",
        gate: GATE,
        partitions: PARTITIONS
            .into_iter()
            .map(|partition| Schedule {
                partition,
                cases: cases(partition),
            })
            .collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct PartitionResult {
    pub partition: Partition,
    pub arrived: usize,
    pub episodes: Vec<ArrivalEpisode>,
}

#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub manifest: Manifest,
    pub controller: String,
    pub training_fitness: CurriculumFitness,
    pub passed: bool,
    pub partitions: Vec<PartitionResult>,
}

/// Explicit post-training report. All three finalists are kept; fresh results
/// do not choose a seed or change the training budget.
pub fn evaluate(policy: Policy) -> Result<Evaluation, EvolutionError> {
    let mut partitions = Vec::new();
    let mut training_fitness = CurriculumFitness::default();
    for partition in PARTITIONS {
        let mut episodes = Vec::new();
        for (index, case) in cases(partition).into_iter().enumerate() {
            let episode = ship_arrival::run_episode(case, policy.clone(), false)?;
            if partition == Partition::Training {
                training_fitness.record(index, episode.fitness);
            }
            episodes.push(episode);
        }
        partitions.push(PartitionResult {
            partition,
            arrived: episodes
                .iter()
                .filter(|e| e.outcome == ArrivalOutcome::Arrived)
                .count(),
            episodes,
        });
    }
    Ok(Evaluation {
        manifest: manifest()?,
        controller: policy.id().into(),
        training_fitness,
        passed: partitions.iter().all(|p| p.arrived == p.episodes.len()),
        partitions,
    })
}

#[cfg(test)]
mod tests;
