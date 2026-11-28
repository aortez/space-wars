//! The Clock trainer owns a deterministic population and commits one scored generation at a time.
use crate::{
    duck_recipe::{self, DuckCase, DuckControls, Score, Visit},
    evolution::runtime_id,
    training_workers::{EpisodePool, WorkerConfig, WorkerControl},
};
use engine_common::ClockDuckCoursePattern;
use serde::{Deserialize, Serialize};
use spacewars_ai::c3::{C3Genome, EvolutionDraws};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

mod store;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerConfig {
    pub population: usize,
    pub seed: u64,
    pub generations: u32,
    pub mutation_probability: f32,
    pub mutation_scale: f32,
    pub tournament: usize,
    pub case_mix: Vec<ClockDuckCoursePattern>,
    pub cases_per_generation: usize,
    pub workers: usize,
}

impl Default for TrainerConfig {
    fn default() -> Self {
        Self {
            population: 50,
            seed: 1,
            generations: 100,
            mutation_probability: 0.08,
            mutation_scale: 0.12,
            tournament: 3,
            case_mix: duck_recipe::PATTERNS.to_vec(),
            cases_per_generation: 2,
            workers: 0,
        }
    }
}

impl TrainerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=256).contains(&self.population)
            || !(1..=self.population).contains(&self.tournament)
        {
            return Err(
                "Population must be 2–256 and tournament size must fit the population".into(),
            );
        }
        if !(1..=10000).contains(&self.generations)
            || !(1..=32).contains(&self.cases_per_generation)
            || self.workers > 64
        {
            return Err("Choose 1–10000 generations, 1–32 cases and 0–64 workers".into());
        }
        if !(0.0..=1.).contains(&self.mutation_probability)
            || !(0.0..=spacewars_ai::c3::PARAMETER_LIMIT).contains(&self.mutation_scale)
        {
            return Err("Invalid mutation probability or scale".into());
        }
        duck_recipe::training_cases(self.seed, 0, &self.case_mix, 1).map_err(reason)?;
        Ok(())
    }

    fn validate_edit(&self, original: &Self) -> Result<(), String> {
        self.validate()?;
        if self.seed != original.seed || self.generations != original.generations {
            return Err("Run seed and generation limit are fixed at creation".into());
        }
        Ok(())
    }

    fn worker_config(&self) -> WorkerConfig {
        if self.workers == 0 {
            WorkerConfig::default()
        } else {
            WorkerConfig::Fixed {
                workers: self.workers,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub generation: u32,
    pub genome_id: String,
    #[serde(skip)]
    live_genome: Option<Arc<C3Genome>>,
    #[serde(flatten)]
    visit: Visit,
}

impl std::ops::Deref for Replay {
    type Target = Visit;
    fn deref(&self) -> &Visit {
        &self.visit
    }
}
impl Replay {
    /// Live scoring can publish a new offspring before its checkpoint is saved.
    pub fn live_genome(&self) -> Option<Arc<C3Genome>> {
        self.live_genome.clone()
    }
    pub fn controls_at(&self, tick: u64) -> DuckControls {
        self.visit.control_at(tick)
    }
    pub fn ticks(&self) -> u64 {
        self.visit.progress.tick
    }
}

/// One generation's most recently finished visit. A slow viewer skips visits
/// which have already been replaced. Taking a visit never interrupts scoring.
#[derive(Clone, Default)]
pub struct LatestVisit(Arc<Mutex<Completion>>);

#[derive(Default)]
struct Completion {
    index: usize,
    pending: Option<Arc<Replay>>,
}

impl LatestVisit {
    pub fn take(&self) -> Option<Arc<Replay>> {
        self.0.lock().unwrap().pending.take()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationSummary {
    pub run_id: String,
    pub generation: u32,
    pub config: TrainerConfig,
    pub best_genome: String,
    pub best_case: DuckCase,
    /// In priority order: exits, alternating tags, clears, non-falls, ticks alive.
    pub best_terms: [f64; 5],
    /// Champion training visits in entrance order (left, right), each [exits, visits].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_exits: Option<[[u32; 2]; 2]>,
    pub mean_terms: [f64; 5],
    pub spread_terms: [f64; 5],
    pub elapsed_seconds: f64,
    pub episodes_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    #[serde(flatten)]
    report: GenerationSummary,
    pub current: Replay,
    pub best: Replay,
    /// Actual worker completion order. Older records saved only the deterministic sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_finished: Option<Replay>,
}
impl std::ops::Deref for Generation {
    type Target = GenerationSummary;
    fn deref(&self) -> &GenerationSummary {
        &self.report
    }
}
impl Generation {
    pub fn summary(&self) -> &GenerationSummary {
        &self.report
    }
}

fn exits_by_entrance<'a>(visits: impl IntoIterator<Item = &'a Visit>) -> [[u32; 2]; 2] {
    let mut doors = [[0; 2]; 2];
    for visit in visits {
        let [exits, total] = &mut doors[usize::from(visit.case.entrance_right)];
        *exits += visit.score.exits as u32;
        *total += 1;
    }
    doors
}

fn reason(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub struct Trainer {
    dir: PathBuf,
    state: store::Checkpoint,
    pending: Option<store::Edit>,
    pool: EpisodePool,
    population: Vec<Arc<C3Genome>>,
    latest: Option<Generation>,
    _owner: File,
}

impl Trainer {
    pub fn new(config: TrainerConfig, dir: PathBuf) -> Result<Self, String> {
        Self::seeded(config, dir, Vec::new())
    }

    pub fn seeded(
        config: TrainerConfig,
        dir: PathBuf,
        mut seeds: Vec<C3Genome>,
    ) -> Result<Self, String> {
        config.validate()?;
        if seeds.len() > config.population {
            return Err("More starting genomes than population slots".into());
        }
        let owner = store::lock_run(&dir)?;
        if dir.join(store::MANIFEST).exists() {
            return Err("Run already exists; resume it or choose a new directory".into());
        }
        let pool = EpisodePool::new(config.worker_config()).map_err(reason)?;
        store::prepare(&dir)?;
        seeds.extend(
            (seeds.len()..config.population).map(|slot| C3Genome::random(config.seed, slot)),
        );
        let population: Vec<_> = seeds.into_iter().map(Arc::new).collect();
        let state = store::Checkpoint::new(config, &population)?;
        store::save_population(&dir, &population)?;
        state.save(&dir)?;
        Ok(Self {
            dir,
            state,
            pending: None,
            pool,
            population,
            latest: None,
            _owner: owner,
        })
    }

    pub fn resume(dir: PathBuf) -> Result<Self, String> {
        let owner = store::lock_run(&dir)?;
        let state = store::Checkpoint::load(&dir)?;
        if state.runtime != runtime_id().map_err(reason)? {
            return Err("Exact resume requires the original executable and platform".into());
        }
        let population = state
            .population
            .iter()
            .map(|id| store::load_genome(&dir, id).map(Arc::new))
            .collect::<Result<Vec<_>, _>>()?;
        let pending = state.edit(&dir)?;
        let latest = state.latest(&dir)?;
        let pool = EpisodePool::new(state.config.worker_config()).map_err(reason)?;
        store::restore_history(&dir, state.history_bytes)?;
        Ok(Self {
            dir,
            state,
            pending,
            pool,
            population,
            latest,
            _owner: owner,
        })
    }

    pub fn completed(&self) -> u32 {
        self.state.completed
    }
    pub fn worker_control(&self) -> WorkerControl {
        self.pool.control()
    }
    pub fn config(&self) -> &TrainerConfig {
        &self.state.config
    }
    pub fn pending(&self) -> Option<&TrainerConfig> {
        self.pending.as_ref().map(|e| &e.config)
    }
    pub fn pending_generation(&self) -> Option<u32> {
        self.pending.as_ref().map(|e| e.effective_generation)
    }
    pub fn latest(&self) -> Option<&Generation> {
        self.latest.as_ref()
    }
    pub fn history(&self) -> Result<Vec<GenerationSummary>, String> {
        self.state.history(&self.dir)
    }

    pub fn read_config(dir: &Path) -> Result<TrainerConfig, String> {
        let state = store::Checkpoint::load(dir)?;
        Ok(state.edit(dir)?.map_or(state.config, |e| e.config))
    }

    /// The persistent edit mailbox is also available to a separate CLI process.
    pub fn submit_edit(dir: &Path, config: TrainerConfig) -> Result<u32, String> {
        store::submit_edit(dir, config).map(|edit| edit.effective_generation)
    }
    /// Changes fields on the latest pending settings while holding the generation boundary lock.
    pub fn modify_edit(
        dir: &Path,
        modify: impl FnOnce(&mut TrainerConfig) -> Result<(), String>,
    ) -> Result<u32, String> {
        store::modify_edit(dir, modify).map(|edit| edit.effective_generation)
    }
    pub fn queue_edit(&mut self, config: TrainerConfig) -> Result<(), String> {
        self.pending = Some(store::submit_edit(&self.dir, config)?);
        Ok(())
    }

    pub fn champion(&self) -> Result<C3Genome, String> {
        if self.latest.is_none() {
            return Err("No generation has finished".into());
        }
        Ok(self.population[0].as_ref().clone())
    }

    /// Watch a scored run without acquiring ownership or advancing a checkpoint.
    pub fn read_replay(
        dir: &Path,
        generation: Option<u32>,
    ) -> Result<(Generation, Vec<GenerationSummary>), String> {
        let state = store::Checkpoint::load(dir)?;
        let generation = generation.unwrap_or(state.completed);
        let record = state.record(dir, generation)?;
        let mut history = state.history(dir)?;
        history.truncate(generation as usize);
        if history.last() != Some(record.summary()) {
            return Err("Saved replay and generation history disagree".into());
        }
        Ok((record, history))
    }

    /// Re-evaluate a historical champion on its scored cases. A different engine
    /// build can inspect the archive when every saved fitness term reproduces.
    pub fn reconstruct_door_exits(
        dir: &Path,
        summary: &GenerationSummary,
    ) -> Result<[[u32; 2]; 2], String> {
        let state = store::Checkpoint::load(dir)?;
        if summary.run_id != state.run_id || !(1..=state.completed).contains(&summary.generation) {
            return Err("Generation does not belong to this run's scored history".into());
        }
        let config = &summary.config;
        config.validate()?;
        let model = Self::read_genome(dir, &summary.best_genome)?;
        let cases = duck_recipe::training_cases(
            config.seed,
            u64::from(summary.generation),
            &config.case_mix,
            config.cases_per_generation,
        )
        .map_err(reason)?;
        let visits = cases
            .into_iter()
            .map(|case| duck_recipe::run_model(case, &model, false))
            .collect::<Result<Vec<_>, _>>()
            .map_err(reason)?;
        let total: Score = visits.iter().map(|v| v.score).sum();
        if total.terms().map(|term| term as f64) != summary.best_terms {
            return Err("Replayed champion scores differ from the recorded fitness terms".into());
        }
        Ok(exits_by_entrance(&visits))
    }

    pub fn read_genome(dir: &Path, id: &str) -> Result<C3Genome, String> {
        store::load_genome(dir, id)
    }

    pub fn read_champion(dir: &Path) -> Result<C3Genome, String> {
        let state = store::Checkpoint::load(dir)?;
        if state.completed == 0 {
            return Err("No generation has finished".into());
        }
        Self::read_genome(dir, &state.population[0])
    }

    pub fn run_generation(&mut self) -> Result<Generation, String> {
        self.score_generation(false, |_, _, _| {}, None)
    }

    pub fn run_generation_with(
        &mut self,
        on_sample: impl Fn(&Replay) + Sync,
    ) -> Result<Generation, String> {
        self.run_generation_observed(|_, _| {}, on_sample)
    }

    /// Reports the reserved settings before evaluating any candidates.
    pub fn run_generation_observed(
        &mut self,
        on_start: impl FnOnce(u32, &TrainerConfig),
        on_sample: impl Fn(&Replay) + Sync,
    ) -> Result<Generation, String> {
        self.score_generation(false, |n, config, _| on_start(n, config), Some(&on_sample))
    }

    /// Opens a fresh, bounded live feed before dispatching this generation's visits.
    pub fn run_generation_live(
        &mut self,
        on_start: impl FnOnce(u32, &TrainerConfig, LatestVisit),
    ) -> Result<Generation, String> {
        self.score_generation(true, on_start, None)
    }

    fn score_generation(
        &mut self,
        live: bool,
        on_start: impl FnOnce(u32, &TrainerConfig, LatestVisit),
        on_sample: Option<&(dyn Fn(&Replay) + Sync)>,
    ) -> Result<Generation, String> {
        let generation = self.completed() + 1;
        if generation > self.config().generations {
            return Err("Run has reached its generation limit".into());
        }
        let start = Instant::now();
        let pending = store::schedule(&self.dir, &self.state)?;
        let config = pending
            .as_ref()
            .map_or(&self.state.config, |e| &e.config)
            .clone();
        self.pool
            .control()
            .configure(config.worker_config())
            .map_err(reason)?;
        let population = self.next_population(&config, generation)?;
        let cases = duck_recipe::training_cases(
            config.seed,
            u64::from(generation),
            &config.case_mix,
            config.cases_per_generation,
        )
        .map_err(reason)?;
        let tasks: Vec<_> = (0..population.len())
            .flat_map(|slot| (0..cases.len()).map(move |case| (slot, case)))
            .collect();
        // Keep the archived sample independent of worker scheduling.
        let sample_slot = if generation == 1 {
            0
        } else {
            1 + ((generation as usize - 2) % (population.len() - 1))
        };
        let latest = LatestVisit::default();
        let results = self
            .pool
            .map_with_start(
                &tasks,
                || on_start(generation, &config, latest.clone()),
                |&(slot, case)| {
                    let visit = duck_recipe::run_model(cases[case], &population[slot], true)
                        .map_err(reason)?;
                    let replay = Arc::new(Replay {
                        generation,
                        genome_id: population[slot].id().into(),
                        live_genome: live.then(|| population[slot].clone()),
                        visit,
                    });
                    // Publish and select the archival replay at the same completion point.
                    *latest.0.lock().unwrap() = Completion {
                        index: slot * cases.len() + case,
                        pending: live.then(|| replay.clone()),
                    };
                    if slot == sample_slot
                        && case == 0
                        && let Some(on_sample) = on_sample
                    {
                        store::save_genome(&self.dir, &population[slot])?;
                        on_sample(&replay);
                    }
                    Ok::<_, String>(replay)
                },
            )
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let scores: Vec<Score> = results
            .chunks(cases.len())
            .map(|visits| visits.iter().map(|v| v.score).sum())
            .collect();
        let mut order: Vec<usize> = (0..scores.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(scores[i]));
        let winner = order[0];
        let winner_visits = &results[winner * cases.len()..(winner + 1) * cases.len()];
        let door_exits = exits_by_entrance(winner_visits.iter().map(|r| &r.visit));
        let best = winner_visits
            .iter()
            .max_by_key(|r| r.score)
            .unwrap()
            .as_ref()
            .clone();
        let current = results[sample_slot * cases.len()].as_ref().clone();
        let last_finished = results[latest.0.lock().unwrap().index].as_ref().clone();
        let mean_terms = std::array::from_fn(|i| {
            scores.iter().map(|s| s.terms()[i] as f64).sum::<f64>() / scores.len() as f64
        });
        let spread_terms = std::array::from_fn(|i| {
            (scores
                .iter()
                .map(|s| (s.terms()[i] as f64 - mean_terms[i]).powi(2))
                .sum::<f64>()
                / scores.len() as f64)
                .sqrt()
        });
        let ranked: Vec<_> = order.iter().map(|&slot| population[slot].clone()).collect();
        store::save_population(&self.dir, &ranked)?;
        let elapsed_seconds = start.elapsed().as_secs_f64();
        let record = Generation {
            report: GenerationSummary {
                run_id: self.state.run_id.clone(),
                generation,
                config: config.clone(),
                best_genome: best.genome_id.clone(),
                best_case: best.case,
                best_terms: scores[winner].terms().map(|v| v as f64),
                door_exits: Some(door_exits),
                mean_terms,
                spread_terms,
                elapsed_seconds,
                episodes_per_second: tasks.len() as f64 / elapsed_seconds,
            },
            current,
            best,
            last_finished: Some(last_finished),
        };
        let next = self
            .state
            .commit(&self.dir, &record, &ranked, pending.as_ref())?;
        self.population = ranked;
        self.state = next;
        self.latest = Some(record.clone());
        self.pending = None;
        Ok(record)
    }

    fn next_population(
        &self,
        config: &TrainerConfig,
        generation: u32,
    ) -> Result<Vec<Arc<C3Genome>>, String> {
        if self.completed() == 0 {
            let mut result = self
                .population
                .iter()
                .take(config.population)
                .cloned()
                .collect::<Vec<_>>();
            result.extend(
                (result.len()..config.population)
                    .map(|i| Arc::new(C3Genome::random(config.seed, i))),
            );
            return Ok(result);
        }
        let mut result = Vec::with_capacity(config.population);
        result.push(self.population[0].clone());
        for slot in 1..config.population {
            let mut draws =
                EvolutionDraws::new(config.seed, generation as usize, slot, b"offspring");
            let rank = (0..config.tournament)
                .map(|_| draws.index(self.population.len()))
                .min()
                .unwrap();
            result.push(Arc::new(
                self.population[rank]
                    .mutated(
                        &mut draws,
                        config.mutation_probability,
                        config.mutation_scale,
                    )
                    .map_err(reason)?,
            ));
        }
        Ok(result)
    }
}
