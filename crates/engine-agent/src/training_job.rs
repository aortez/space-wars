//! One local training worker with bounded progress snapshots and atomic saves.
//! UI navigation is independent of job commands. Recovery never starts work.

use crate::{
    evolution::{
        EvolutionConfig, EvolutionError, EvolutionRun, MAX_CHECKPOINT_BYTES, atomic_write,
        read_bounded,
    },
    genome_library::{self, Entry, Origin},
    line_diagnostic::{self, KnownPolicy, LineFitness, LineTask},
    ship_arrival::ArrivalFitness,
    ship_curriculum::{self, CurriculumFitness},
    ship_qualification, ship_training,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use spacewars_ai::model::{DenseGenome, DenseModel};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const JOB_VERSION: u32 = 1;
const MAX_JOB_BYTES: usize = MAX_CHECKPOINT_BYTES * 2 + 4096;

fn error(e: impl std::fmt::Display) -> EvolutionError {
    EvolutionError(e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recipe {
    StationaryArrival,
    MixedArrival,
    /// Exact diagnostic fixture for job tests; omitted from the normal UI.
    DiagnosticLine,
}

impl Recipe {
    pub fn cases(self) -> usize {
        match self {
            Self::StationaryArrival => 4,
            Self::MixedArrival => 12,
            Self::DiagnosticLine => LineTask::default().cases().len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpec {
    pub recipe: Recipe,
    pub seed: u64,
    pub population: usize,
    pub generations: usize,
    /// None uses the recipe defaults; explicit seeds are re-evaluated afresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_genomes: Option<Vec<DenseGenome>>,
}

impl Default for JobSpec {
    fn default() -> Self {
        Self {
            recipe: Recipe::StationaryArrival,
            seed: 1,
            population: 32,
            generations: 48,
            initial_genomes: None,
        }
    }
}

impl JobSpec {
    pub fn config(&self) -> Result<EvolutionConfig, EvolutionError> {
        if !(4..=64).contains(&self.population) || !(1..=64).contains(&self.generations) {
            return Err(error("choose 4–64 candidates and 1–64 generations"));
        }
        let mut config = match self.recipe {
            Recipe::StationaryArrival => ship_training::config(1)?,
            Recipe::MixedArrival => ship_curriculum::config(1)?,
            Recipe::DiagnosticLine => EvolutionConfig::default(),
        };
        config.seed = self.seed;
        config.population = self.population;
        config.generations = self.generations;
        if let Some(seeds) = &self.initial_genomes {
            if seeds.len() > self.population || self.recipe == Recipe::DiagnosticLine {
                return Err(error("unsupported seed population"));
            }
            for genome in seeds {
                DenseModel::new(genome.clone()).map_err(error)?;
                if !genome.same_architecture(&ship_training::template()) {
                    return Err(error(
                        "selected genome does not match this training architecture",
                    ));
                }
            }
        }
        Ok(config)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    #[default]
    Loading,
    Ready,
    Starting,
    Running,
    Saving,
    PauseRequested,
    Paused,
    ResumeRequested,
    StopRequested,
    Stopped,
    Completed,
    Failed,
    Unavailable,
}

impl JobState {
    pub fn busy(self) -> bool {
        matches!(
            self,
            Self::Loading
                | Self::Starting
                | Self::Running
                | Self::Saving
                | Self::PauseRequested
                | Self::ResumeRequested
                | Self::StopRequested
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenerationProgress {
    pub generation: usize,
    pub best_arrivals: u32,
    pub median_arrivals: u32,
    /// Ordered integer terms in this recipe's selection rule, not a universal score.
    pub best_fitness: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct JobSnapshot {
    pub state: JobState,
    pub run_id: Option<String>,
    pub spec: Option<JobSpec>,
    pub completed_generations: usize,
    pub evaluated_candidates: usize,
    pub saved_candidates: usize,
    pub evaluated_episodes: usize,
    pub evaluation_millis: u64,
    pub best_id: Option<String>,
    pub history: Vec<GenerationProgress>,
    pub can_resume: bool,
    pub error: Option<String>,
}

trait ProgressFitness: Clone + Ord + Serialize + DeserializeOwned {
    fn arrivals(&self) -> u32;
    fn terms(&self) -> Vec<u32>;
}
impl ProgressFitness for ArrivalFitness {
    fn arrivals(&self) -> u32 {
        self.completed
    }
    fn terms(&self) -> Vec<u32> {
        vec![
            self.completed,
            self.position_points,
            self.velocity_points,
            self.spare_ticks,
        ]
    }
}
impl ProgressFitness for CurriculumFitness {
    fn arrivals(&self) -> u32 {
        self.total.completed
    }
    fn terms(&self) -> Vec<u32> {
        let mut terms = vec![self.stationary_completed];
        terms.extend(self.total.terms());
        terms
    }
}
impl ProgressFitness for LineFitness {
    fn arrivals(&self) -> u32 {
        self.completed
    }
    fn terms(&self) -> Vec<u32> {
        vec![self.completed, self.proximity_points, self.spare_ticks]
    }
}

enum Run {
    Stationary(EvolutionRun<ArrivalFitness>),
    Mixed(EvolutionRun<CurriculumFitness>),
    Line(EvolutionRun<LineFitness>),
}

impl Run {
    fn new(spec: &JobSpec) -> Result<Self, EvolutionError> {
        let config = spec.config()?;
        Ok(match spec.recipe {
            Recipe::StationaryArrival => Self::Stationary(EvolutionRun::new(
                &ship_training::TrainingTask,
                ship_training::template(),
                spec.initial_genomes.clone().unwrap_or_default(),
                config,
            )?),
            Recipe::MixedArrival => Self::Mixed(EvolutionRun::new(
                &ship_curriculum::TrainingTask,
                ship_training::template(),
                match &spec.initial_genomes {
                    Some(seeds) => seeds.clone(),
                    None => vec![ship_qualification::candidate()?.genome().clone()],
                },
                config,
            )?),
            Recipe::DiagnosticLine => Self::Line(EvolutionRun::new(
                &LineTask::default(),
                line_diagnostic::known_genome(KnownPolicy::Idle),
                vec![],
                config,
            )?),
        })
    }

    fn decode(spec: &JobSpec, bytes: &[u8]) -> Result<Self, EvolutionError> {
        fn check<F: ProgressFitness>(
            run: &EvolutionRun<F>,
            spec: &JobSpec,
        ) -> Result<(), EvolutionError> {
            let expected = Run::new(spec)?;
            let (template, seeds) = match &expected {
                Run::Stationary(r) => (&r.population()[0].genome, r.seeds()),
                Run::Mixed(r) => (&r.population()[0].genome, r.seeds()),
                Run::Line(r) => (&r.population()[0].genome, r.seeds()),
            };
            if run.config() != &spec.config()?
                || run.seeds() != seeds
                || run
                    .population()
                    .iter()
                    .any(|c| !c.genome.same_architecture(template))
            {
                return Err(error("saved job configuration or seed artifacts differ"));
            }
            Ok(())
        }
        Ok(match spec.recipe {
            Recipe::StationaryArrival => {
                let run = EvolutionRun::decode(bytes, &ship_training::TrainingTask)?;
                check(&run, spec)?;
                Self::Stationary(run)
            }
            Recipe::MixedArrival => {
                let run = EvolutionRun::decode(bytes, &ship_curriculum::TrainingTask)?;
                check(&run, spec)?;
                Self::Mixed(run)
            }
            Recipe::DiagnosticLine => {
                let run = EvolutionRun::decode(bytes, &LineTask::default())?;
                check(&run, spec)?;
                Self::Line(run)
            }
        })
    }

    fn encode(&self) -> Result<Vec<u8>, EvolutionError> {
        match self {
            Self::Stationary(r) => r.encode(),
            Self::Mixed(r) => r.encode(),
            Self::Line(r) => r.encode(),
        }
    }
    fn advance(&mut self) -> Result<bool, EvolutionError> {
        match self {
            Self::Stationary(r) => r.advance(&ship_training::TrainingTask),
            Self::Mixed(r) => r.advance(&ship_curriculum::TrainingTask),
            Self::Line(r) => r.advance(&LineTask::default()),
        }
    }
    fn complete(&self) -> bool {
        match self {
            Self::Stationary(r) => r.complete(),
            Self::Mixed(r) => r.complete(),
            Self::Line(r) => r.complete(),
        }
    }
    fn best(&self) -> Option<&DenseGenome> {
        match self {
            Self::Stationary(r) => r.history().last().map(|h| &h.best_genome),
            Self::Mixed(r) => r.history().last().map(|h| &h.best_genome),
            Self::Line(r) => r.history().last().map(|h| &h.best_genome),
        }
    }
    fn progress(&self, snapshot: &mut JobSnapshot) {
        fn fill<F: ProgressFitness>(run: &EvolutionRun<F>, snapshot: &mut JobSnapshot) {
            snapshot.completed_generations = run.history().len();
            snapshot.evaluated_candidates = run.history().len() * run.config().population
                + if run.complete() {
                    0
                } else {
                    run.fitness().iter().flatten().count()
                };
            snapshot.best_id = run.history().last().map(|h| h.best_id.clone());
            snapshot.history = run
                .history()
                .iter()
                .map(|h| {
                    let mut arrivals: Vec<_> =
                        h.fitness.iter().map(ProgressFitness::arrivals).collect();
                    arrivals.sort_unstable();
                    GenerationProgress {
                        generation: h.generation + 1,
                        best_arrivals: h.best_fitness.arrivals(),
                        median_arrivals: arrivals[(arrivals.len() - 1) / 2],
                        best_fitness: h.best_fitness.terms(),
                    }
                })
                .collect();
        }
        match self {
            Self::Stationary(r) => fill(r, snapshot),
            Self::Mixed(r) => fill(r, snapshot),
            Self::Line(r) => fill(r, snapshot),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedJob {
    id: String,
    spec: JobSpec,
    evaluation_millis: u64,
    // Preserve the inner checkpoint's exact serialized bytes and checksum.
    checkpoint: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    sha256: String,
    job: SavedJob,
}

struct Job {
    id: String,
    spec: JobSpec,
    run: Run,
    evaluation_millis: u64,
    saved_candidates: usize,
}

fn valid_id(id: &str) -> bool {
    id.len() > 4
        && id.starts_with("run-")
        && id.len() < 100
        && id[4..].bytes().all(|c| c.is_ascii_digit() || c == b'-')
}

impl Job {
    fn new(spec: JobSpec) -> Result<Self, EvolutionError> {
        let run = Run::new(&spec)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_nanos();
        Ok(Self {
            id: format!("run-{stamp}-{}", std::process::id()),
            spec,
            run,
            evaluation_millis: 0,
            saved_candidates: 0,
        })
    }
    fn snapshot(&self, state: JobState) -> JobSnapshot {
        let mut snapshot = JobSnapshot {
            state,
            run_id: Some(self.id.clone()),
            spec: Some(self.spec.clone()),
            saved_candidates: self.saved_candidates,
            evaluation_millis: self.evaluation_millis,
            can_resume: !self.run.complete()
                && matches!(
                    state,
                    JobState::Paused | JobState::Stopped | JobState::Failed
                ),
            ..JobSnapshot::default()
        };
        self.run.progress(&mut snapshot);
        snapshot.evaluated_episodes = snapshot.evaluated_candidates * self.spec.recipe.cases();
        snapshot
    }
    fn save(&mut self, root: &Path) -> Result<(), EvolutionError> {
        let job = SavedJob {
            id: self.id.clone(),
            spec: self.spec.clone(),
            evaluation_millis: self.evaluation_millis,
            checkpoint: String::from_utf8(self.run.encode()?).map_err(error)?,
        };
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&job).map_err(error)?)
        );
        let bytes = serde_json::to_vec(&Envelope {
            version: JOB_VERSION,
            sha256: digest,
            job,
        })
        .map_err(error)?;
        if bytes.len() > MAX_JOB_BYTES {
            return Err(error("job save exceeds size limit"));
        }
        atomic_write(&root.join(format!("{}.json", self.id)), &bytes)?;
        self.saved_candidates = self.snapshot(JobState::Saving).evaluated_candidates;
        // This is a derived export of a completed generation, never a claim
        // that an uncheckpointed population or an unevaluated winner is saved.
        if let Some(best) = self.run.best() {
            let snapshot = self.snapshot(JobState::Saving);
            let last = snapshot
                .history
                .last()
                .expect("best from completed generation");
            genome_library::save(
                &root.join("genomes"),
                &Entry::new(
                    best.clone(),
                    Origin {
                        label: format!(
                            "{:?} · seed {} · generation {}",
                            self.spec.recipe, self.spec.seed, last.generation
                        ),
                        run_id: Some(self.id.clone()),
                        generation: Some(last.generation),
                        training_arrivals: Some(last.best_arrivals),
                        training_cases: Some(self.spec.recipe.cases()),
                        runtime: Some(crate::evolution::runtime_id()?),
                    },
                )?,
            )?;
            // Preserve original provenance before publishing the mutable export.
            atomic_write(
                &root.join(format!("{}.best.json", self.id)),
                &serde_json::to_vec_pretty(best).map_err(error)?,
            )?;
        }
        Ok(())
    }
    fn load_latest(root: &Path) -> Result<Option<Self>, EvolutionError> {
        let latest = root.join("latest");
        if !latest.try_exists().map_err(error)? {
            return Ok(None);
        }
        let id = String::from_utf8(read_bounded(&latest, 100)?).map_err(error)?;
        if !valid_id(&id) {
            return Err(error("invalid saved training job ID"));
        }
        let bytes = read_bounded(&root.join(format!("{id}.json")), MAX_JOB_BYTES)?;
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(error)?;
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&envelope.job).map_err(error)?)
        );
        if envelope.version != JOB_VERSION || envelope.sha256 != digest || envelope.job.id != id {
            return Err(error("saved training job version or checksum differs"));
        }
        let saved = envelope.job;
        let run = Run::decode(&saved.spec, saved.checkpoint.as_bytes())?;
        let mut job = Self {
            id,
            spec: saved.spec,
            run,
            evaluation_millis: saved.evaluation_millis,
            saved_candidates: 0,
        };
        job.saved_candidates = job.snapshot(JobState::Paused).evaluated_candidates;
        Ok(Some(job))
    }
    fn advance(&mut self) -> Result<(), EvolutionError> {
        let start = Instant::now();
        let result = self.run.advance();
        self.evaluation_millis = self
            .evaluation_millis
            .saturating_add(start.elapsed().as_millis() as u64);
        result.map(|_| ())
    }
}

enum Command {
    Start(JobSpec),
    Pause,
    Resume,
    Stop,
}
struct Shared {
    snapshot: JobSnapshot,
    command: Option<Command>,
    shutdown: bool,
}
struct Channel {
    shared: Mutex<Shared>,
    wake: Condvar,
}

/// A lightweight clone for UI callbacks. It owns no world or worker handle.
#[derive(Clone)]
pub struct JobControl {
    channel: Arc<Channel>,
}

impl JobControl {
    pub fn snapshot(&self) -> JobSnapshot {
        self.channel.shared.lock().unwrap().snapshot.clone()
    }
    fn request(&self, command: Command) -> Result<(), EvolutionError> {
        let mut shared = self.channel.shared.lock().unwrap();
        if shared.shutdown || shared.command.is_some() {
            return Err(error("a training command is already pending"));
        }
        let state = shared.snapshot.state;
        let next = match &command {
            Command::Start(spec) if !state.busy() && state != JobState::Unavailable => {
                spec.config()?;
                JobState::Starting
            }
            Command::Pause if matches!(state, JobState::Running | JobState::Saving) => {
                JobState::PauseRequested
            }
            Command::Resume if shared.snapshot.can_resume => JobState::ResumeRequested,
            Command::Stop
                if matches!(
                    state,
                    JobState::Running | JobState::Saving | JobState::Paused | JobState::Failed
                ) && shared.snapshot.run_id.is_some() =>
            {
                JobState::StopRequested
            }
            _ => return Err(error("training command is not available in this state")),
        };
        if let Command::Start(spec) = &command {
            shared.snapshot = JobSnapshot {
                spec: Some(spec.clone()),
                ..JobSnapshot::default()
            };
        }
        shared.snapshot.state = next;
        shared.snapshot.can_resume = false;
        shared.snapshot.error = None;
        shared.command = Some(command);
        self.channel.wake.notify_one();
        Ok(())
    }
    pub fn start(&self, spec: JobSpec) -> Result<(), EvolutionError> {
        self.request(Command::Start(spec))
    }
    pub fn pause(&self) -> Result<(), EvolutionError> {
        self.request(Command::Pause)
    }
    pub fn resume(&self) -> Result<(), EvolutionError> {
        self.request(Command::Resume)
    }
    pub fn stop(&self) -> Result<(), EvolutionError> {
        self.request(Command::Stop)
    }
}

/// Dropping the manager requests a safe-boundary save and joins its worker.
/// A process crash leaves the last complete atomic checkpoint recoverable.
pub struct TrainingManager {
    control: JobControl,
    worker: Option<JoinHandle<()>>,
}

impl TrainingManager {
    pub fn new(root: PathBuf) -> Result<Self, EvolutionError> {
        let channel = Arc::new(Channel {
            shared: Mutex::new(Shared {
                snapshot: JobSnapshot::default(),
                command: None,
                shutdown: false,
            }),
            wake: Condvar::new(),
        });
        let worker_channel = Arc::clone(&channel);
        let worker = thread::Builder::new()
            .name("training-job".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || worker(root, worker_channel))
            .map_err(error)?;
        Ok(Self {
            control: JobControl { channel },
            worker: Some(worker),
        })
    }
    pub fn control(&self) -> JobControl {
        self.control.clone()
    }
}

impl Drop for TrainingManager {
    fn drop(&mut self) {
        {
            let mut shared = self.control.channel.shared.lock().unwrap();
            shared.shutdown = true;
            self.control.channel.wake.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn publish(channel: &Channel, mut snapshot: JobSnapshot) {
    let mut shared = channel.shared.lock().unwrap();
    // A completed batch must not erase an already-visible pause/stop request.
    if shared.command.is_some() {
        snapshot.state = shared.snapshot.state;
        snapshot.can_resume = false;
    }
    shared.snapshot = snapshot;
}

fn storage_lock(root: &Path) -> Result<File, EvolutionError> {
    fs::create_dir_all(root).map_err(error)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("worker.lock"))
        .map_err(error)?;
    file.try_lock().map_err(|e| {
        error(format!(
            "training storage is unavailable or already in use: {e}"
        ))
    })?;
    Ok(file)
}

fn worker(root: PathBuf, channel: Arc<Channel>) {
    let _lock = match storage_lock(&root) {
        Ok(lock) => lock,
        Err(e) => {
            publish(
                &channel,
                JobSnapshot {
                    state: JobState::Unavailable,
                    error: Some(e.0),
                    ..JobSnapshot::default()
                },
            );
            return;
        }
    };
    let mut job = match Job::load_latest(&root) {
        Ok(job) => {
            publish(
                &channel,
                job.as_ref().map_or_else(
                    || JobSnapshot {
                        state: JobState::Ready,
                        ..JobSnapshot::default()
                    },
                    |j| {
                        j.snapshot(if j.run.complete() {
                            JobState::Completed
                        } else {
                            JobState::Paused
                        })
                    },
                ),
            );
            job
        }
        Err(e) => {
            publish(
                &channel,
                JobSnapshot {
                    state: JobState::Failed,
                    error: Some(e.0),
                    ..JobSnapshot::default()
                },
            );
            None
        }
    };
    let mut running = false;
    loop {
        let command = {
            let mut shared = channel.shared.lock().unwrap();
            while shared.command.is_none() && !shared.shutdown && !running {
                shared = channel.wake.wait(shared).unwrap();
            }
            if shared.shutdown {
                shared.command = None;
                drop(shared);
                if let Some(job) = job.as_mut() {
                    let result = job.save(&root);
                    let mut snapshot = job.snapshot(if job.run.complete() {
                        JobState::Completed
                    } else {
                        JobState::Stopped
                    });
                    if let Err(e) = result {
                        snapshot.state = JobState::Failed;
                        snapshot.error = Some(e.0);
                    }
                    publish(&channel, snapshot);
                }
                break;
            }
            shared.command.take()
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<(), EvolutionError> {
                if let Some(command) = command {
                    match command {
                        Command::Start(spec) => {
                            running = false;
                            let mut next = Job::new(spec)?;
                            // Reserve a new immutable run identity before replacing
                            // its own checkpoint; even a repeated system clock must
                            // not overwrite an older run.
                            OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(root.join(format!("{}.json", next.id)))
                                .map_err(error)?;
                            next.save(&root)?;
                            atomic_write(&root.join("latest"), next.id.as_bytes())?;
                            job = Some(next);
                            running = true;
                        }
                        Command::Pause | Command::Stop => {
                            running = false;
                            let job = job
                                .as_mut()
                                .ok_or_else(|| error("no resumable training job"))?;
                            job.save(&root)?;
                            let state = if job.run.complete() {
                                JobState::Completed
                            } else if matches!(command, Command::Pause) {
                                JobState::Paused
                            } else {
                                JobState::Stopped
                            };
                            publish(&channel, job.snapshot(state));
                        }
                        Command::Resume => {
                            let job = job
                                .as_mut()
                                .ok_or_else(|| error("no resumable training job"))?;
                            job.save(&root)?;
                            running = !job.run.complete();
                        }
                    }
                } else if running {
                    let job = job.as_mut().expect("running job");
                    job.advance()?;
                    publish(&channel, job.snapshot(JobState::Saving));
                    job.save(&root)?;
                    running = !job.run.complete();
                }
                if running {
                    publish(&channel, job.as_ref().unwrap().snapshot(JobState::Running));
                } else if let Some(job) = &job
                    && job.run.complete()
                {
                    publish(&channel, job.snapshot(JobState::Completed));
                }
                Ok(())
            },
        ));
        let failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.0),
            Err(_) => {
                // Partially unwound mutable state must never overwrite the last
                // durable checkpoint. A restart can offer that checkpoint.
                job = None;
                Some("training worker failed; reopen the app to restore the last checkpoint".into())
            }
        };
        if let Some(error) = failure {
            running = false;
            let mut snapshot = job
                .as_ref()
                .map_or_else(JobSnapshot::default, |j| j.snapshot(JobState::Failed));
            snapshot.state = JobState::Failed;
            snapshot.error = Some(error);
            publish(&channel, snapshot);
        }
    }
}

#[cfg(test)]
mod tests;
