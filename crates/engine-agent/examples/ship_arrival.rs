//! Material arrival calibration and deterministic traces; no training defaults.

use clap::{Parser, Subcommand, ValueEnum};
use engine_agent::{
    evolution::{EvaluationTask, atomic_write, read_bounded},
    ship_arrival::{self, ArrivalTask},
};
use spacewars_ai::{
    guidance::Policy,
    model::{DenseModel, MAX_MODEL_BYTES},
};
use std::{error::Error, path::PathBuf};

#[derive(Parser)]
#[command(about = "Evaluate real-physics stationary and moving ship arrival")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
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
    Suite {
        #[command(flatten)]
        controller: ControllerArgs,
    },
    Episode {
        #[arg(long)]
        case: String,
        #[command(flatten)]
        controller: ControllerArgs,
    },
    /// Export a neutral known-failure model for artifact/adapter checks.
    Genome {
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    match Args::parse().command {
        Command::Genome { out } => {
            atomic_write(
                &out,
                &serde_json::to_vec_pretty(&ship_arrival::neutral_genome())?,
            )?;
        }
        Command::Suite { controller } => {
            let policy = controller.load()?;
            let mut episodes = Vec::new();
            for case in ship_arrival::cases() {
                let episode = ship_arrival::run_episode(case, policy.clone(), false)?;
                eprintln!(
                    "{}: {:?}, {} ticks, distance {:.3}, relative speed {:.3}",
                    episode.case.name,
                    episode.outcome,
                    episode.elapsed_ticks,
                    episode.measure.distance,
                    episode.measure.relative_speed
                );
                episodes.push(episode);
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"manifest": ArrivalTask.manifest(), "episodes": episodes})
                )?
            );
        }
        Command::Episode { case, controller } => {
            let case = ship_arrival::cases()
                .into_iter()
                .find(|c| c.name == case)
                .ok_or("unknown arrival case")?;
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
