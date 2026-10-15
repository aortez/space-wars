//! Local diagnostic only; does not start a game or change live bot assignments.

use clap::{Parser, Subcommand, ValueEnum};
use engine_agent::{
    evolution::{EvolutionConfig, EvolutionRun, atomic_write, read_bounded},
    line_diagnostic::{
        KnownPolicy, LineController, LineFitness, LineTask, known_genome, run_episode,
    },
};
use scenario_line_mover::LineConfig;
use spacewars_ai::model::{DenseGenome, DenseModel, MAX_MODEL_BYTES};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(about = "Trace and train a bounded diagnostic line mover")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Policy {
    Oracle,
    Idle,
    Wrong,
}

#[derive(Subcommand)]
enum Command {
    /// Export a hand-written model artifact with known weights.
    Genome {
        #[arg(long, value_enum, default_value = "oracle")]
        policy: Policy,
        #[arg(long)]
        out: PathBuf,
    },
    /// Evaluate an artifact and print the complete tick trace as JSON.
    Episode {
        #[arg(long)]
        genome: PathBuf,
        #[arg(long, default_value_t = -2, allow_hyphen_values = true)]
        start: i32,
        #[arg(long, default_value_t = 1, allow_hyphen_values = true)]
        target: i32,
        #[arg(long, default_value_t = 2)]
        dwell_ticks: u32,
        #[arg(long, default_value_t = 16)]
        max_ticks: u32,
    },
    /// Start a fixed recipe: all 81 start/target pairs per candidate.
    Train {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 16)]
        population: usize,
        #[arg(long, default_value_t = 8)]
        generations: usize,
        #[arg(long, default_value_t = 2)]
        elites: usize,
        #[arg(long, default_value_t = 3)]
        tournament_size: usize,
        /// Each artifact fills one slot unchanged; remaining slots are random.
        #[arg(long)]
        seed_genome: Vec<PathBuf>,
        #[arg(long)]
        checkpoint: PathBuf,
        /// Stop after this many complete candidate batches (0 saves initial state).
        #[arg(long)]
        steps: Option<usize>,
        /// Export the best model from the latest completed generation, if any.
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
    /// Continue using the saved immutable recipe and the same executable build.
    Resume {
        #[arg(long)]
        checkpoint: PathBuf,
        #[arg(long)]
        steps: Option<usize>,
        #[arg(long)]
        best_genome: Option<PathBuf>,
    },
}

fn load(path: &Path) -> Result<DenseModel, Box<dyn Error>> {
    Ok(DenseModel::from_json(&read_bounded(
        path,
        MAX_MODEL_BYTES,
    )?)?)
}

fn save_genome(path: &Path, genome: &DenseGenome) -> Result<(), Box<dyn Error>> {
    atomic_write(path, &serde_json::to_vec_pretty(genome)?)?;
    Ok(())
}

fn resolved_output(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or("output must be a file path")?;
    Ok(parent.canonicalize()?.join(name))
}

fn progress(
    mut run: EvolutionRun<LineFitness>,
    task: &LineTask,
    path: &Path,
    steps: Option<usize>,
    best_path: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    if let Some(best_path) = best_path
        && resolved_output(path)? == resolved_output(best_path)?
    {
        return Err("checkpoint and best genome must have different output paths".into());
    }
    // Save before evaluating too, so a fresh job has a durable starting point.
    run.save(path)?;
    let mut advanced = 0;
    while advanced < steps.unwrap_or(usize::MAX) && run.advance(task)? {
        advanced += 1;
        // Candidate batches are small; persist every batch in this prototype.
        run.save(path)?;
        if advanced == 1 || run.fitness().iter().all(Option::is_none) || run.complete() {
            eprintln!(
                "{}",
                serde_json::json!({"event": "checkpoint", "generation": run.generation(),
                "evaluated_in_generation": run.fitness().iter().filter(|f| f.is_some()).count(),
                "complete": run.complete(), "best_fitness": run.history().last().map(|h| h.best_fitness)})
            );
        }
    }
    let best = run.history().last();
    let exported = if let (Some(path), Some(best)) = (best_path, best) {
        save_genome(path, &best.best_genome)?;
        Some(path)
    } else {
        None
    };
    let history = run
        .history()
        .iter()
        .map(|h| {
            serde_json::json!({
                "generation": h.generation, "best_id": h.best_id, "best_fitness": h.best_fitness,
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "task": run.task().id, "cases_per_candidate": task.cases().len(),
            "config": run.config(), "complete": run.complete(), "generation": run.generation(),
            "evaluated_in_generation": run.fitness().iter().filter(|f| f.is_some()).count(),
            "advanced_candidates": advanced, "checkpoint": path,
            "exported_best_genome": exported, "history": history,
        }))?
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    match Args::parse().command {
        Command::Genome { policy, out } => {
            let policy = match policy {
                Policy::Oracle => KnownPolicy::Oracle,
                Policy::Idle => KnownPolicy::Idle,
                Policy::Wrong => KnownPolicy::Wrong,
            };
            let model = DenseModel::new(known_genome(policy))?;
            save_genome(&out, model.genome())?;
            println!(
                "{}",
                serde_json::json!({"genome_id": model.id(), "path": out})
            );
        }
        Command::Episode {
            genome,
            start,
            target,
            dwell_ticks,
            max_ticks,
        } => {
            let model = load(&genome)?;
            let episode = run_episode(
                LineController::Model(&model),
                LineConfig {
                    start,
                    target,
                    dwell_ticks,
                    max_ticks,
                },
                true,
            )?;
            println!("{}", serde_json::to_string_pretty(&episode)?);
        }
        Command::Train {
            seed,
            population,
            generations,
            elites,
            tournament_size,
            seed_genome,
            checkpoint,
            steps,
            best_genome,
        } => {
            if checkpoint.try_exists()? {
                return Err("checkpoint already exists; use resume or choose a new path".into());
            }
            let task = LineTask::default();
            let seeds = seed_genome
                .iter()
                .map(|p| load(p).map(|m| m.genome().clone()))
                .collect::<Result<Vec<_>, _>>()?;
            let config = EvolutionConfig {
                seed,
                population,
                generations,
                elites,
                tournament_size,
                ..EvolutionConfig::default()
            };
            let run = EvolutionRun::new(&task, known_genome(KnownPolicy::Oracle), seeds, config)?;
            progress(run, &task, &checkpoint, steps, best_genome.as_deref())?;
        }
        Command::Resume {
            checkpoint,
            steps,
            best_genome,
        } => {
            let task = LineTask::default();
            progress(
                EvolutionRun::load(&checkpoint, &task)?,
                &task,
                &checkpoint,
                steps,
                best_genome.as_deref(),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genome_export_cannot_overwrite_the_resumable_job() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.json");
        let alias = dir.path().join(".").join("run.json");
        let task = LineTask::default();
        let run = EvolutionRun::new(
            &task,
            known_genome(KnownPolicy::Oracle),
            vec![],
            EvolutionConfig::default(),
        )
        .unwrap();
        let error = progress(run, &task, &path, Some(0), Some(&alias)).unwrap_err();
        assert!(error.to_string().contains("different output paths"));
        assert!(!path.exists());
    }
}
