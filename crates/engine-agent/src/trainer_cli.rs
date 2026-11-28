//! Command-line access to the job owner used by the Training view.
use clap::{Args, Subcommand, ValueEnum};
use engine_agent::{
    duck_recipe::{self, Partition},
    trainer::{Trainer, TrainerConfig},
    training_workers::{EpisodePool, WorkerConfig},
};
use engine_common::ClockDuckCoursePattern;
use spacewars_ai::{c3::C3Genome, model::MAX_MODEL_BYTES};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    /// Start a population and write one JSON record per completed generation.
    Train {
        #[arg(value_enum)]
        recipe: Recipe,
        /// Directory for checkpoints, genomes, replays and generation history.
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long)]
        generations: Option<u32>,
        #[arg(long)]
        workers: Option<usize>,
        /// Start from a compatible genome file or a content ID in --genome-dir.
        #[arg(long, value_name = "FILE_OR_ID")]
        start_from: Vec<String>,
        #[arg(long, requires = "start_from")]
        genome_dir: Option<PathBuf>,
        /// Return after this many generations, leaving the run resumable.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        stop_after: Option<u32>,
        #[command(flatten)]
        settings: Settings,
    },
    /// Continue an exact checkpoint from the same engine build.
    Resume {
        directory: PathBuf,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        stop_after: Option<u32>,
    },
    /// Score the last saved champion, scripted duck and random play on both doors.
    Evaluate {
        directory: PathBuf,
        /// Saved genome file or ID in the run's store. Defaults to the latest winner.
        #[arg(long, value_name = "FILE_OR_ID")]
        genome: Option<String>,
        /// Restrict the held-out suite to this course pattern.
        #[arg(long, value_parser = pattern)]
        pattern: Option<ClockDuckCoursePattern>,
        #[arg(long, value_enum, default_value = "validation")]
        partition: Suite,
        /// Independent suite seed. Use a fresh seed for final qualification.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Number of mirrored pairs per policy.
        #[arg(long, default_value_t = 4)]
        cases: usize,
        /// Fixed worker count. Omit to use the normal CPU target.
        #[arg(long)]
        workers: Option<usize>,
    },
    /// Propose an edit for the job's next generation boundary.
    Edit {
        directory: PathBuf,
        #[command(flatten)]
        settings: Settings,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum Recipe {
    DuckClock,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum Suite {
    Validation,
    Qualification,
}

fn pattern(value: &str) -> Result<ClockDuckCoursePattern, String> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(|_| {
        format!("Unknown pattern '{value}'. Choose platforms, terraces, two-jump or shortcut.")
    })
}

#[derive(Debug, Default, Args)]
pub(super) struct Settings {
    #[arg(long)]
    population: Option<usize>,
    #[arg(long)]
    mutation_probability: Option<f32>,
    #[arg(long)]
    mutation_scale: Option<f32>,
    #[arg(long)]
    tournament: Option<usize>,
    #[arg(long)]
    cases_per_generation: Option<usize>,
    /// Comma-separated course patterns.
    #[arg(long, value_delimiter = ',', value_parser = pattern)]
    case_mix: Option<Vec<ClockDuckCoursePattern>>,
}

impl Settings {
    fn apply(self, config: &mut TrainerConfig) {
        if let Some(n) = self.population {
            config.population = n;
        }
        if let Some(p) = self.mutation_probability {
            config.mutation_probability = p;
        }
        if let Some(s) = self.mutation_scale {
            config.mutation_scale = s;
        }
        if let Some(n) = self.tournament {
            config.tournament = n;
        }
        if let Some(n) = self.cases_per_generation {
            config.cases_per_generation = n;
        }
        if let Some(mix) = self.case_mix {
            config.case_mix = mix;
        }
    }
}

fn warm_genome(value: &str, store: &Path) -> Result<C3Genome, Box<dyn std::error::Error>> {
    let is_id = value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
    let path = if is_id {
        store.join(format!("{value}.json"))
    } else {
        PathBuf::from(value)
    };
    let mut bytes = Vec::new();
    File::open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(MAX_MODEL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let genome = C3Genome::from_json(&bytes)?;
    if is_id && value != genome.id() {
        return Err(format!("Stored genome {value} differs from its content identity").into());
    }
    Ok(genome)
}

fn advance(
    mut trainer: Trainer,
    stop_after: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let until = match stop_after {
        Some(n) => trainer
            .completed()
            .saturating_add(n)
            .min(trainer.config().generations),
        None => trainer.config().generations,
    };
    let mut out = io::stdout().lock();
    while trainer.completed() < until {
        let generation = trainer.run_generation()?;
        serde_json::to_writer(&mut out, generation.summary())?;
        writeln!(out)?;
        out.flush()?;
    }
    Ok(())
}

pub(super) fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Train {
            recipe: Recipe::DuckClock,
            out,
            seed,
            generations,
            workers,
            start_from,
            genome_dir,
            stop_after,
            settings,
        } => {
            let mut config = TrainerConfig::default();
            if let Some(seed) = seed {
                config.seed = seed;
            }
            if let Some(n) = generations {
                config.generations = n;
            }
            if let Some(n) = workers {
                config.workers = n;
            }
            settings.apply(&mut config);
            let store = genome_dir.unwrap_or_else(|| PathBuf::from("genomes"));
            let seeds = start_from
                .iter()
                .map(|value| warm_genome(value, &store))
                .collect::<Result<Vec<_>, _>>()?;
            advance(Trainer::seeded(config, out, seeds)?, stop_after)
        }
        Command::Resume {
            directory,
            stop_after,
        } => advance(Trainer::resume(directory)?, stop_after),
        Command::Evaluate {
            directory,
            genome,
            pattern,
            partition,
            seed,
            cases,
            workers,
        } => {
            let pool = EpisodePool::new(workers.map_or_else(WorkerConfig::default, |workers| {
                WorkerConfig::Fixed { workers }
            }))?;
            let partition = match partition {
                Suite::Validation => Partition::Validation,
                Suite::Qualification => Partition::Qualification,
            };
            let champion = match genome {
                Some(value) => warm_genome(&value, &directory.join("genomes"))?,
                None => Trainer::read_champion(&directory)?,
            };
            let report = duck_recipe::evaluate_held_out(
                &champion,
                seed,
                partition,
                cases,
                pattern,
                Some(&pool),
            )?;
            let mut out = io::stdout().lock();
            serde_json::to_writer(&mut out, &report)?;
            writeln!(out)?;
            Ok(())
        }
        Command::Edit {
            directory,
            settings,
        } => {
            let generation = Trainer::modify_edit(&directory, |config| {
                let previous = config.clone();
                settings.apply(config);
                if previous == *config {
                    return Err("The proposed settings already match this run".into());
                }
                Ok(())
            })?;
            eprintln!("Accepted settings for generation {generation}.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn train_and_edit_share_the_same_sparse_settings() {
        let parsed = crate::Args::try_parse_from([
            "engine-agent",
            "train",
            "duck-clock",
            "--population",
            "12",
            "--case-mix",
            "two-jump,platforms",
            "--out",
            "/tmp/duck",
        ])
        .unwrap();
        let Some(Command::Train { settings, .. }) = parsed.command else {
            panic!("train")
        };
        let mut config = TrainerConfig::default();
        settings.apply(&mut config);
        assert_eq!(config.population, 12);
        assert_eq!(
            config.case_mix,
            [
                ClockDuckCoursePattern::TwoJump,
                ClockDuckCoursePattern::Platforms
            ]
        );
        let parsed = crate::Args::try_parse_from([
            "engine-agent",
            "edit",
            "/tmp/duck",
            "--mutation-scale",
            "0.2",
        ])
        .unwrap();
        let Some(Command::Edit { settings, .. }) = parsed.command else {
            panic!("edit")
        };
        let expected = TrainerConfig {
            mutation_scale: 0.2,
            ..config.clone()
        };
        settings.apply(&mut config);
        assert_eq!(config, expected);
    }
}
