//! Pinned local experiment. Finalist validation is a separate explicit command.

use clap::{Parser, Subcommand, ValueEnum};
use engine_agent::{
    evolution::{EvolutionRun, atomic_write, read_bounded},
    ship_arrival::{self, ArrivalFitness},
    ship_qualification,
    ship_training::{self, Partition, TrainingTask},
};
use spacewars_ai::{
    guidance::Policy,
    model::{DenseModel, MAX_MODEL_BYTES},
};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(about = "Run the fixed material_arrival_learning_v1 experiment locally")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyName {
    Reference,
    Idle,
    Model,
}

#[derive(clap::Args)]
struct ControllerArgs {
    #[arg(long, value_enum, default_value = "reference")]
    policy: PolicyName,
    #[arg(long, required_if_eq("policy", "model"))]
    genome: Option<PathBuf>,
}

impl ControllerArgs {
    fn load(&self) -> Result<Policy, Box<dyn Error>> {
        Ok(match self.policy {
            PolicyName::Model => Policy::Model(DenseModel::from_json(&read_bounded(
                self.genome.as_ref().ok_or("model requires --genome")?,
                MAX_MODEL_BYTES,
            )?)?),
            PolicyName::Reference | PolicyName::Idle if self.genome.is_some() => {
                return Err("--genome requires --policy model".into());
            }
            PolicyName::Reference => Policy::Reference,
            PolicyName::Idle => Policy::Idle,
        })
    }
}

#[derive(Subcommand)]
enum Command {
    /// Print architecture, all case partitions, and every declared run setting.
    Manifest,
    /// Print the frozen fresh-case schedule and pass gate for seed 3.
    QualificationManifest,
    /// Compare frozen seed 3 with the reference on 96 fresh and 11 old cases.
    Qualify,
    /// Trace one fresh qualification case with the frozen model or reference.
    QualificationEpisode {
        #[arg(long)]
        case: String,
        #[arg(long, value_enum, default_value = "model")]
        policy: PolicyName,
    },
    /// Train on four stationary cases; all settings except seed are fixed.
    Train {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long)]
        checkpoint: PathBuf,
        /// Pause after N complete candidate batches (0 saves initial state).
        #[arg(long)]
        steps: Option<usize>,
        /// Export the winner of the latest complete generation, if available.
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
    /// Resume the immutable recipe using this exact executable build.
    Resume {
        #[arg(long)]
        checkpoint: PathBuf,
        #[arg(long)]
        steps: Option<usize>,
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
    /// Report training, stationary validation, and moving transfer separately.
    Evaluate {
        #[command(flatten)]
        controller: ControllerArgs,
    },
    /// Retain a full trace of a named case from any experiment partition.
    Episode {
        #[arg(long)]
        case: String,
        #[command(flatten)]
        controller: ControllerArgs,
    },
}

fn resolved_output(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent
        .canonicalize()?
        .join(path.file_name().ok_or("output must be a file path")?))
}

fn progress(
    mut run: EvolutionRun<ArrivalFitness>,
    checkpoint: &Path,
    steps: Option<usize>,
    best_genome: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    ship_training::validate_run(&run)?;
    if let Some(best) = best_genome
        && resolved_output(checkpoint)? == resolved_output(best)?
    {
        return Err("checkpoint and best genome must have different output paths".into());
    }
    run.save(checkpoint)?;
    let mut advanced = 0;
    while advanced < steps.unwrap_or(usize::MAX) && run.advance(&TrainingTask)? {
        advanced += 1;
        run.save(checkpoint)?;
        if advanced == 1 || run.fitness().iter().all(Option::is_none) || run.complete() {
            eprintln!(
                "{}",
                serde_json::json!({
                    "event": "checkpoint", "seed": run.config().seed,
                    "completed_generations": run.history().len(),
                    "evaluated_in_generation": run.fitness().iter().filter(|f| f.is_some()).count(),
                    "complete": run.complete(),
                    "best_fitness": run.history().last().map(|h| h.best_fitness),
                })
            );
        }
    }
    let best = run.history().last();
    let exported = if let (Some(path), Some(best)) = (best_genome, best) {
        atomic_write(path, &serde_json::to_vec_pretty(&best.best_genome)?)?;
        Some(path)
    } else {
        None
    };
    let history = run.history().iter().map(|h| serde_json::json!({
        "generation": h.generation, "best_id": h.best_id, "best_fitness": h.best_fitness,
        "candidates_with_arrivals": h.fitness.iter().filter(|f| f.completed > 0).count(),
        "candidates_with_zero_fitness": h.fitness.iter().filter(|f| **f == ArrivalFitness::default()).count(),
    })).collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "experiment": ship_training::EXPERIMENT, "runtime": engine_agent::evolution::runtime_id()?,
            "task": run.task(), "config": run.config(), "complete": run.complete(),
            "generation": run.generation(),
            "evaluated_in_generation": run.fitness().iter().filter(|f| f.is_some()).count(),
            "advanced_candidates": advanced, "checkpoint": checkpoint,
            "exported_best_genome": exported, "best_id": best.map(|b| &b.best_id), "history": history,
        }))?
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    match Args::parse().command {
        Command::QualificationManifest => println!(
            "{}",
            serde_json::to_string_pretty(&ship_qualification::manifest()?)?
        ),
        Command::Qualify => println!(
            "{}",
            serde_json::to_string_pretty(&ship_qualification::run()?)?
        ),
        Command::QualificationEpisode { case, policy } => {
            let case = ship_qualification::cases()
                .into_iter()
                .find(|c| c.case.name == case)
                .ok_or("unknown qualification case")?
                .case;
            let policy = match policy {
                PolicyName::Model => Policy::Model(ship_qualification::candidate()?),
                PolicyName::Reference => Policy::Reference,
                PolicyName::Idle => Policy::Idle,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&ship_arrival::run_episode(case, policy, true)?)?
            );
        }
        Command::Manifest => println!(
            "{}",
            serde_json::to_string_pretty(&ship_training::manifest()?)?
        ),
        Command::Train {
            seed,
            checkpoint,
            steps,
            best_genome,
        } => {
            if checkpoint.try_exists()? {
                return Err("checkpoint already exists; use resume or choose a new path".into());
            }
            let run = EvolutionRun::new(
                &TrainingTask,
                ship_training::template(),
                vec![],
                ship_training::config(seed)?,
            )?;
            progress(run, &checkpoint, steps, best_genome.as_deref())?;
        }
        Command::Resume {
            checkpoint,
            steps,
            best_genome,
        } => {
            let run = EvolutionRun::load(&checkpoint, &TrainingTask)?;
            progress(run, &checkpoint, steps, best_genome.as_deref())?;
        }
        Command::Evaluate { controller } => {
            let report = ship_training::evaluate(controller.load()?)?;
            for result in &report.partitions {
                eprintln!(
                    "{:?}: {} / {} arrived",
                    result.partition,
                    result.fitness.completed,
                    result.episodes.len()
                );
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Episode { case, controller } => {
            let case = [
                Partition::Training,
                Partition::StationaryValidation,
                Partition::MovingTransfer,
            ]
            .into_iter()
            .flat_map(ship_training::cases)
            .find(|c| c.name == case)
            .ok_or("unknown experiment case")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ship_arrival::run_episode(
                    case,
                    controller.load()?,
                    true
                )?)?
            );
        }
    }
    Ok(())
}
