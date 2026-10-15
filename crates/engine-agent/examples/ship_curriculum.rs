//! Bounded continuation from the retained ship-arrival genome.

use clap::{Parser, Subcommand, ValueEnum};
use engine_agent::{
    evolution::{EvolutionRun, atomic_write, read_bounded},
    ship_arrival,
    ship_curriculum::{self, CurriculumFitness, TrainingTask},
    ship_qualification,
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
#[command(about = "Run the fixed material_arrival_mixed_learning_v1 experiment locally")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyName {
    Reference,
    Baseline,
    Idle,
    Model,
}

#[derive(clap::Args)]
struct ControllerArgs {
    #[arg(long, value_enum, default_value = "baseline")]
    policy: PolicyName,
    #[arg(long, required_if_eq("policy", "model"))]
    genome: Option<PathBuf>,
}

impl ControllerArgs {
    fn load(&self) -> Result<Policy, Box<dyn Error>> {
        if !matches!(self.policy, PolicyName::Model) && self.genome.is_some() {
            return Err("--genome requires --policy model".into());
        }
        Ok(match self.policy {
            PolicyName::Model => Policy::Model(DenseModel::from_json(&read_bounded(
                self.genome.as_ref().ok_or("model requires --genome")?,
                MAX_MODEL_BYTES,
            )?)?),
            PolicyName::Baseline => Policy::Model(ship_qualification::candidate()?),
            PolicyName::Reference => Policy::Reference,
            PolicyName::Idle => Policy::Idle,
        })
    }
}

#[derive(Subcommand)]
enum Command {
    /// Print the seed artifact, immutable recipe, all cases, and acceptance gate.
    Manifest,
    Train {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long)]
        checkpoint: PathBuf,
        /// Pause after N candidate batches (0 saves just the initial state).
        #[arg(long)]
        steps: Option<usize>,
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
    Resume {
        #[arg(long)]
        checkpoint: PathBuf,
        #[arg(long)]
        steps: Option<usize>,
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
    /// Report training, original, development, and three fresh groups separately.
    Evaluate {
        #[command(flatten)]
        controller: ControllerArgs,
    },
    /// Trace a named case from any partition, using the same physical evaluator.
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
    mut run: EvolutionRun<CurriculumFitness>,
    checkpoint: &Path,
    steps: Option<usize>,
    best_genome: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    ship_curriculum::validate_run(&run)?;
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
        "candidates_retaining_stationary": h.fitness.iter().filter(|f| f.stationary_completed == 4).count(),
        "candidates_completing_all_training": h.fitness.iter().filter(|f| f.total.completed == 12).count(),
    })).collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "experiment": ship_curriculum::EXPERIMENT, "runtime": engine_agent::evolution::runtime_id()?,
            "task": run.task(), "config": run.config(), "seed_id": ship_qualification::CANDIDATE,
            "complete": run.complete(), "generation": run.generation(),
            "evaluated_in_generation": run.fitness().iter().filter(|f| f.is_some()).count(),
            "advanced_candidates": advanced, "checkpoint": checkpoint,
            "exported_best_genome": exported, "best_id": best.map(|b| &b.best_id), "history": history,
        }))?
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    match Args::parse().command {
        Command::Manifest => println!(
            "{}",
            serde_json::to_string_pretty(&ship_curriculum::manifest()?)?
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
            progress(
                ship_curriculum::new_run(seed)?,
                &checkpoint,
                steps,
                best_genome.as_deref(),
            )?;
        }
        Command::Resume {
            checkpoint,
            steps,
            best_genome,
        } => {
            progress(
                EvolutionRun::load(&checkpoint, &TrainingTask)?,
                &checkpoint,
                steps,
                best_genome.as_deref(),
            )?;
        }
        Command::Evaluate { controller } => {
            let report = ship_curriculum::evaluate(controller.load()?)?;
            for partition in &report.partitions {
                eprintln!(
                    "{:?}: {} / {} arrived",
                    partition.partition,
                    partition.arrived,
                    partition.episodes.len()
                );
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Episode { case, controller } => {
            let case = ship_curriculum::PARTITIONS
                .into_iter()
                .flat_map(ship_curriculum::cases)
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
