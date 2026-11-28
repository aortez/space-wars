//! The native Training view observes one headless Clock job and replays its scored controls.
mod decision;
mod inspector;
mod navigation;
mod owner;
mod playback;

use crate::{DuckScoreTerm, MainWindow, TrainingPoint};
use engine_agent::{
    duck_recipe::PATTERNS,
    trainer::{LatestVisit, Replay, Trainer, TrainerConfig},
    training_workers::WorkerControl,
};
use engine_common::ClockDuckCoursePattern as Pattern;
pub(crate) use navigation::{handle_action, inventory, selected_id};
use owner::{Command, Event};
use playback::Playback;
use slint::{ModelRc, VecModel};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

pub(super) const RECIPE: i32 = 2;
const TERM_NAMES: [&str; 5] = ["Exits", "Wall tags", "Clears", "No fall", "Ticks alive"];
const FIELDS: [&str; 5] = ["population", "probability", "scale", "tournament", "mix"];

pub(super) struct Job {
    pub client: Client,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.client.sender.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub(super) struct Client {
    sender: Sender<Command>,
    view: Rc<RefCell<View>>,
}

pub(super) fn install(root: PathBuf, restore: bool) -> std::io::Result<Job> {
    let (sender, commands) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("clock-training".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || owner::serve(root.join("duck-clock"), commands, events, restore))?;
    let client = Client {
        sender,
        view: Rc::new(RefCell::new(View::new(receiver))),
    };
    Ok(Job {
        client,
        thread: Some(thread),
    })
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Starting,
    Running,
    Pausing,
    Paused,
    Stopping,
    Stopped,
    Completed,
    Watching,
    Failed,
}

impl State {
    fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Pausing => "pause_requested",
            Self::Paused => "paused",
            Self::Stopping => "stop_requested",
            Self::Stopped => "stopped",
            Self::Completed => "completed",
            Self::Watching => "watching",
            Self::Failed => "failed",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Running => "Training",
            Self::Pausing => "Pausing after this generation",
            Self::Paused => "Paused",
            Self::Stopping => "Saving after this generation",
            Self::Stopped => "Saved and stopped",
            Self::Completed => "Completed",
            Self::Watching => "Saved replay",
            Self::Failed => "Training needs attention",
        }
    }
}

struct Trend {
    generation: u32,
    best: [f64; 5],
    mean: [f64; 5],
    cases: usize,
    mix: Vec<Pattern>,
}

struct DoorComparison {
    champion: Arc<Replay>,
    return_mode: i32,
    original: [Option<Playback>; 2],
}

struct View {
    events: Receiver<Event>,
    selected: bool,
    id: String,
    directory: Option<PathBuf>,
    state: State,
    config: TrainerConfig,
    pending: Option<(TrainerConfig, u32)>,
    generation: u32,
    completed: u32,
    history: Vec<Trend>,
    best_terms: Option<[f64; 5]>,
    rate: f64,
    latest: [Option<Arc<Replay>>; 2],
    live_visits: Option<LatestVisit>,
    saved_sample_generation: Option<u32>,
    playing: [Option<Playback>; 2],
    pinned: [Option<Arc<Replay>>; 2],
    compare: Option<DoorComparison>,
    fraction: f64,
    pending_step: bool,
    resume_speed: i32,
    follow_after_inspection: bool,
    workers: Option<WorkerControl>,
    archive_doors: Option<Receiver<Result<[[u32; 2]; 2], String>>>,
    last_progress: String,
    previous: Instant,
    error: String,
}

impl Client {
    pub(super) fn watch(
        &self,
        w: &MainWindow,
        directory: PathBuf,
        record: engine_agent::trainer::Generation,
        history: Vec<engine_agent::trainer::GenerationSummary>,
    ) {
        let mut view = self.view.borrow_mut();
        view.reset(w, record.config.clone());
        if record.door_exits.is_none() {
            let (send, receive) = mpsc::channel();
            let summary = record.summary().clone();
            let dir = directory.clone();
            if thread::Builder::new()
                .name("duck-archive-scores".into())
                .spawn(move || {
                    let _ = send.send(Trainer::reconstruct_door_exits(&dir, &summary));
                })
                .is_ok()
            {
                view.archive_doors = Some(receive);
            }
        }
        view.directory = Some(directory);
        view.set_history(history);
        view.event(w, Event::Finished(Box::new(record)));
        view.state = State::Watching;
        w.set_training_preview_mode(2);
        w.set_training_visible(true);
        w.set_training_setup(false);
        w.set_training_focus(5);
        view.publish(w);
    }
    pub(super) fn poll(&self, w: &MainWindow) -> bool {
        let mut view = self.view.borrow_mut();
        while let Ok(event) = view.events.try_recv() {
            view.event(w, event);
            if view.selected {
                view.publish(w);
            }
        }
        if view.selected {
            if let Some(replay) = view.live_visits.as_ref().and_then(LatestVisit::take) {
                view.latest[0] = Some(replay);
            }
            if let Some(rx) = &view.archive_doors
                && let Ok(result) = rx.try_recv()
            {
                let caption = result
                    .map(door_caption)
                    .unwrap_or_else(|_| "Archived door scores could not be reproduced".into());
                w.set_duck_door_exits(caption.into());
                view.archive_doors = None;
            }
            view.worker_status(w);
            view.play(w);
        }
        view.selected
    }

    pub(super) fn refresh(&self, w: &MainWindow) -> bool {
        let view = self.view.borrow();
        if view.selected {
            view.publish(w);
        }
        view.selected
    }

    pub(super) fn start(&self, w: &MainWindow, workers: usize) -> Result<(), String> {
        let config = TrainerConfig {
            seed: w.get_training_seed() as u64,
            generations: w.get_training_generations() as u32,
            population: w.get_training_population() as usize,
            workers,
            ..Default::default()
        };
        self.sender
            .send(Command::Start(config.clone()))
            .map_err(|e| e.to_string())?;
        let mut view = self.view.borrow_mut();
        view.reset(w, config);
        w.set_training_setup(false);
        w.set_training_focus(4);
        view.publish(w);
        Ok(())
    }

    pub(super) fn deselect(&self, w: &MainWindow) {
        if self.view.borrow().selected {
            self.view.borrow_mut().selected = false;
            w.set_duck_job(false);
            let _ = self.sender.send(Command::Deselect);
        }
    }

    pub(super) fn command(&self, w: &MainWindow, id: &str) -> Result<(), String> {
        let mut view = self.view.borrow_mut();
        if matches!(id, "training.preview.current" | "training.preview.best")
            && w.get_duck_inspect() < 0
        {
            let index = usize::from(id.ends_with(".best"));
            if w.get_training_preview_mode() != index as i32 + 1
                && w.get_training_preview_follow()
                && let (Some(shown), Some(newest)) = (&view.playing[index], &view.latest[index])
                && !shown.matches(newest)
            {
                view.playing[index] = None;
            }
            w.set_training_preview_mode(index as i32 + 1);
            w.set_training_focus(4 + index as i32);
            view.play(w);
            return Ok(());
        }
        if id.starts_with("training.duck.brain.")
            || id == "training.duck.replay.toggle"
            || matches!(id, "training.preview.current" | "training.preview.best")
        {
            view.brain_command(w, id)?;
            view.play(w);
            return Ok(());
        }
        match id {
            "training.duck.term" => w.set_duck_term((w.get_duck_term() + 1) % 5),
            "training.duck.steer" => {
                view.close_inspector(w);
                w.set_duck_steering(!w.get_duck_steering());
                w.set_training_focus(match (w.get_duck_steering(), w.get_duck_can_edit()) {
                    (true, true) => 0,
                    (true, false) => 9,
                    (false, _) => 7,
                });
            }
            "training.pause" => {
                self.sender
                    .send(Command::Pause)
                    .map_err(|e| e.to_string())?;
                view.state = State::Pausing;
            }
            "training.stop" => {
                self.sender.send(Command::Stop).map_err(|e| e.to_string())?;
                view.state = State::Stopping;
            }
            "training.resume" => {
                self.sender
                    .send(Command::Resume)
                    .map_err(|e| e.to_string())?;
                view.state = State::Starting;
            }
            _ => {
                let Some(field) = FIELDS
                    .iter()
                    .position(|field| id.starts_with(&format!("training.duck.{field}.")))
                else {
                    return Err(format!("Unknown training control {id}"));
                };
                let directory = view
                    .directory
                    .as_deref()
                    .ok_or("The run is still opening")?;
                let direction = if id.ends_with("previous") { -1 } else { 1 };
                let mut accepted = None;
                let generation = Trainer::modify_edit(directory, |config| {
                    adjust(config, field, direction);
                    accepted = Some(config.clone());
                    Ok(())
                })?;
                view.pending = accepted.map(|config| (config, generation));
                view.error.clear();
            }
        }
        view.publish(w);
        Ok(())
    }
}

impl View {
    fn new(events: Receiver<Event>) -> Self {
        Self {
            events,
            selected: false,
            id: String::new(),
            directory: None,
            state: State::Starting,
            config: TrainerConfig::default(),
            pending: None,
            generation: 0,
            completed: 0,
            history: vec![],
            best_terms: None,
            rate: 0.0,
            latest: Default::default(),
            live_visits: None,
            saved_sample_generation: None,
            playing: Default::default(),
            pinned: Default::default(),
            compare: None,
            fraction: 0.,
            pending_step: false,
            resume_speed: 0,
            follow_after_inspection: true,
            workers: None,
            archive_doors: None,
            last_progress: String::new(),
            previous: Instant::now(),
            error: String::new(),
        }
    }
    fn reset(&mut self, w: &MainWindow, config: TrainerConfig) {
        self.selected = true;
        self.id.clear();
        self.directory = None;
        self.state = State::Starting;
        self.config = config;
        self.pending = None;
        self.generation = 0;
        self.completed = 0;
        self.history.clear();
        self.rate = 0.0;
        self.best_terms = None;
        self.error.clear();
        self.latest = Default::default();
        self.live_visits = None;
        self.saved_sample_generation = None;
        self.playing = Default::default();
        self.pinned = Default::default();
        self.compare = None;
        self.pending_step = false;
        self.fraction = 0.;
        self.workers = None;
        self.archive_doors = None;
        self.last_progress.clear();
        w.set_duck_review_playing(false);
        w.set_duck_archived_sample(false);
        w.set_duck_compare(false);
        w.set_duck_inspect(-1);
        w.set_duck_action(1);
        w.set_duck_current_outputs(Default::default());
        w.set_duck_best_outputs(Default::default());
        w.set_duck_door_exits("Awaiting a scored generation".into());
        w.set_duck_job(true);
        w.set_duck_steering(false);
        w.set_training_preview_mode(1);
        w.set_training_preview_follow(true);
        w.set_duck_current_frame(Default::default());
        w.set_duck_best_frame(Default::default());
        w.set_duck_current_id("Awaiting first visit".into());
        w.set_duck_best_id("Awaiting champion".into());
        w.set_duck_current_details("First scored case pending".into());
        w.set_duck_best_details("First completed generation pending".into());
    }

    fn set_history(&mut self, history: Vec<engine_agent::trainer::GenerationSummary>) {
        self.history = history
            .into_iter()
            .map(|r| Trend {
                generation: r.generation,
                best: r.best_terms,
                mean: r.mean_terms,
                cases: r.config.cases_per_generation,
                mix: r.config.case_mix,
            })
            .collect();
    }

    fn event(&mut self, w: &MainWindow, event: Event) {
        match event {
            Event::Opened {
                path,
                config,
                pending,
                completed,
                history,
            } => {
                if !w.get_training_has_job() && w.get_training_visible() {
                    w.set_training_setup(false);
                }
                self.reset(w, config);
                self.directory = Some(path);
                self.completed = completed;
                self.generation = completed;
                self.pending = pending;
                self.set_history(history);
            }
            Event::Scoring(generation, config, workers, visits) => {
                self.live_visits = Some(visits);
                self.workers = Some(workers);
                self.last_progress.clear();
                if !matches!(self.state, State::Pausing | State::Stopping) {
                    self.state = State::Running;
                }
                self.generation = generation;
                if self.pending.as_ref().is_some_and(|p| p.1 <= generation) {
                    self.pending = None;
                }
                self.config = config;
            }
            Event::Finished(generation) => {
                let record = generation.summary();
                self.workers = None;
                self.live_visits = None;
                w.set_duck_door_exits(
                    record
                        .door_exits
                        .map(door_caption)
                        .unwrap_or_else(|| {
                            if self.archive_doors.is_some() {
                                "Replaying archived cases for exit counts".into()
                            } else {
                                "Exit split unavailable in this archive".into()
                            }
                        })
                        .into(),
                );
                self.id = record.run_id.clone();
                self.completed = record.generation;
                self.generation = record.generation;
                self.rate = record.episodes_per_second;
                if self
                    .history
                    .last()
                    .is_none_or(|g| g.generation < record.generation)
                {
                    self.history.push(Trend {
                        generation: record.generation,
                        best: record.best_terms,
                        mean: record.mean_terms,
                        cases: record.config.cases_per_generation,
                        mix: record.config.case_mix.clone(),
                    });
                }
                self.config = record.config.clone();
                self.best_terms = Some(record.best_terms);
                if generation.last_finished.is_none() {
                    self.saved_sample_generation = Some(record.generation);
                    w.set_duck_archived_sample(true);
                }
                self.latest = [
                    Some(Arc::new(
                        generation.last_finished.unwrap_or(generation.current),
                    )),
                    Some(Arc::new(generation.best)),
                ];
            }
            Event::RecoveryFailed(id, error) => {
                self.reset(w, TrainerConfig::default());
                self.id = id;
                self.state = State::Failed;
                self.error = error;
            }
            Event::Paused => self.state = State::Paused,
            Event::Stopped | Event::Complete => {
                self.state = if matches!(event, Event::Complete) {
                    State::Completed
                } else {
                    State::Stopped
                };
                self.pending_step = false;
                w.set_duck_review_playing(false);
            }
            Event::Failed(error) => {
                self.state = State::Failed;
                self.error = error;
            }
        }
    }

    fn publish(&self, w: &MainWindow) {
        w.set_training_has_job(true);
        w.set_training_run_id(self.id.as_str().into());
        let busy = matches!(
            self.state,
            State::Starting | State::Running | State::Pausing | State::Stopping
        );
        w.set_training_active(busy);
        w.set_training_status(self.state.label().into());
        w.set_training_status_code(self.state.code().into());
        w.set_training_can_start(!busy);
        w.set_training_can_pause(self.state == State::Running);
        w.set_duck_can_edit(
            matches!(self.state, State::Running | State::Paused)
                && self.generation < self.config.generations,
        );
        w.set_training_can_resume(
            matches!(self.state, State::Paused | State::Stopped)
                && self.completed < self.config.generations,
        );
        w.set_training_can_stop(matches!(self.state, State::Running | State::Paused));
        w.set_training_error(self.error.as_str().into());
        w.set_training_launcher_label(
            if busy {
                "Training · Active"
            } else {
                "Training · Saved run"
            }
            .into(),
        );
        w.set_training_progress(self.completed as f32 / self.config.generations.max(1) as f32);
        w.set_training_progress_label(
            format!(
                "Generation {} / {}   ·   Population {}   ·   Seed {}",
                self.generation, self.config.generations, self.config.population, self.config.seed
            )
            .into(),
        );
        if self.workers.is_none() {
            w.set_training_worker_status(
                if matches!(self.state, State::Completed | State::Stopped) {
                    format!(
                        "0 active workers · Last generation {:.1} visits/s",
                        self.rate
                    )
                } else if self.rate > 0.0 {
                    format!("{:.1} episodes/s", self.rate)
                } else {
                    "Evaluating cases".into()
                }
                .into(),
            );
        }
        let config = self.pending.as_ref().map(|p| &p.0).unwrap_or(&self.config);
        w.set_duck_edit_status(
            match &self.pending {
                Some((_, g)) => format!("Settings accepted for generation {g}"),
                None if self.state == State::Watching => {
                    format!("Recorded settings · Generation {}", self.generation)
                }
                None if self.completed >= self.config.generations => {
                    "Run complete · Checkpoint saved".into()
                }
                None if self.generation >= self.config.generations => {
                    "Scoring the final generation".into()
                }
                None if self.state == State::Stopped => {
                    "Checkpoint saved · Resume to edit the run".into()
                }
                None => format!(
                    "Edits take effect in generation {}",
                    self.generation.max(self.completed) + 1
                ),
            }
            .into(),
        );
        w.set_duck_settings(ModelRc::new(VecModel::from(vec![
            config.population.to_string().into(),
            format!("{:.1}%", 100. * config.mutation_probability).into(),
            format!("{:.3}", config.mutation_scale).into(),
            config.tournament.to_string().into(),
            mix_label(&config.case_mix).into(),
        ])));
        w.set_training_snapshot(serde_json::json!({
            "state": self.state.code(), "run_id": self.id, "run_dir": self.directory, "generation": self.generation,
            "completed_generations": self.completed, "settings": self.config, "pending": self.pending,
            "best_terms": self.best_terms, "episodes_per_second": self.rate,
        }).to_string().into());
        self.chart(w);
    }

    fn chart(&self, w: &MainWindow) {
        let index = w.get_duck_term().clamp(0, 4) as usize;
        let recent = &self.history[self.history.len().saturating_sub(100)..];
        let per_case = |value: f64, cases: usize| value / cases.max(1) as f64;
        let maximum = recent
            .iter()
            .map(|g| per_case(g.best[index].max(g.mean[index]), g.cases))
            .fold(0., f64::max);
        let ceiling = maximum.max(1.);
        let points: Vec<_> = recent
            .iter()
            .map(|g| TrainingPoint {
                best: (per_case(g.best[index], g.cases) / ceiling) as f32,
                median: (per_case(g.mean[index], g.cases) / ceiling) as f32,
            })
            .collect();
        w.set_training_points(ModelRc::new(VecModel::from(points)));
        let mix_changes: Vec<_> = recent
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0].mix != pair[1].mix)
            .map(|(i, _)| (i + 1) as i32)
            .collect();
        w.set_duck_mix_changes(ModelRc::new(VecModel::from(mix_changes)));
        w.set_duck_chart_zero(!recent.is_empty() && maximum == 0.);
        let generations = recent.first().map_or(String::new(), |first| {
            format!("G{}–{} · ", first.generation, self.completed)
        });
        w.set_duck_chart_range(
            format!(
                "{generations}Gold best · Blue mean · 0–{}/case",
                number(ceiling)
            )
            .into(),
        );
        let scored_cases = self
            .history
            .last()
            .map_or(self.config.cases_per_generation, |g| g.cases);
        w.set_duck_score_label(
            format!(
                "Best score by term · {scored_cases} {}",
                if scored_cases == 1 { "case" } else { "cases" }
            )
            .into(),
        );
        let terms: Vec<_> = TERM_NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let maximum = self
                    .history
                    .iter()
                    .map(|g| g.best[i].max(g.mean[i]))
                    .fold(1., f64::max);
                DuckScoreTerm {
                    name: (*name).into(),
                    value: self
                        .best_terms
                        .map_or_else(|| "—".into(), |b| number(b[i]))
                        .into(),
                    fraction: self
                        .best_terms
                        .map_or(0., |b| (b[i] / maximum).clamp(0., 1.) as f32),
                }
            })
            .collect();
        w.set_duck_terms(ModelRc::new(VecModel::from(terms)));
    }

    fn worker_status(&mut self, w: &MainWindow) {
        if let Some(workers) = &self.workers {
            let s = workers.snapshot();
            let progress = format!(
                "{}/{} visits · {} active workers · {:.1} episodes/s",
                s.batch_completed,
                self.config.population * self.config.cases_per_generation,
                s.active,
                s.episodes_per_second
            );
            if self.last_progress != progress {
                w.set_training_worker_status(progress.as_str().into());
                self.last_progress = progress;
            }
        }
    }
}

fn door_caption([left, right]: [[u32; 2]; 2]) -> String {
    format!(
        "Exits by door   L {}/{}   ·   R {}/{}",
        left[0], left[1], right[0], right[1]
    )
}

fn number(x: f64) -> String {
    if (x - x.round()).abs() < 0.005 {
        format!("{x:.0}")
    } else {
        format!("{x:.2}")
    }
}

fn mix_label(patterns: &[Pattern]) -> String {
    if patterns.len() == 4 {
        "All four courses".into()
    } else {
        patterns
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn adjust(config: &mut TrainerConfig, field: usize, direction: i32) {
    fn next<T: Copy + PartialOrd>(old: T, choices: &[T], dir: i32) -> T {
        if dir > 0 {
            *choices.iter().find(|v| **v > old).unwrap_or(&choices[0])
        } else {
            *choices
                .iter()
                .rev()
                .find(|v| **v < old)
                .unwrap_or(choices.last().unwrap())
        }
    }
    match field {
        0 => config.population = next(config.population, &[4, 8, 16, 24, 32, 50, 64], direction),
        1 => {
            config.mutation_probability = next(
                config.mutation_probability,
                &[0.001, 0.005, 0.01, 0.025, 0.05, 0.10, 0.25, 0.5, 1.],
                direction,
            )
        }
        2 => {
            config.mutation_scale = next(
                config.mutation_scale,
                &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.],
                direction,
            )
        }
        3 => config.tournament = next(config.tournament, &[1, 2, 3, 4, 6, 8], direction),
        _ => {
            let i = if config.case_mix.len() == 1 {
                1 + PATTERNS
                    .iter()
                    .position(|p| Some(p) == config.case_mix.first())
                    .unwrap_or(0) as i32
            } else {
                0
            };
            let new = (i + direction).rem_euclid(5);
            config.case_mix = if new == 0 {
                PATTERNS.to_vec()
            } else {
                vec![PATTERNS[(new - 1) as usize]]
            };
        }
    }
}
