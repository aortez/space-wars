//! Pizza normal play on a dedicated simulation thread.
//!
//! The worker owns the Pizza state and runs the host's fixed updates with the
//! host's catch-up rule: owed updates run back to back, at most
//! `MAX_FIXED_STEPS_PER_TICK` per wake, and any remaining backlog is dropped.
//! The UI thread queues input, reads the latest frame and status, and presents
//! each newly published frame, so long updates no longer delay presentation.

use std::any::Any;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use engine_common::{Action, RenderFrame, Scenario, StepResult, TickModel};
use scenario_pizza::{PizzaScenario, PizzaState};

use super::pizza::{REGISTRATION, status_of, step_with_lab};
use super::{ClientScenario, RenderBackend, ScenarioRegistration};
use crate::host::{MAX_FIXED_STEPS_PER_TICK, PizzaStatus, fixed_step_duration};
use crate::input::ClientInput;
use crate::pizza_lab::Recorder;
use crate::render::{FrameLayout, Viewport};

/// Pizza normal play whose updates run on a `PizzaWorker`.
pub(crate) struct PizzaWorkerClientScenario {
    worker: PizzaWorker,
    aspect_ratio: f32,
}

impl PizzaWorkerClientScenario {
    pub(crate) fn spawn(
        state: PizzaState,
        lab: Option<Recorder>,
        aspect_ratio: f32,
    ) -> io::Result<Self> {
        let dt =
            fixed_step_duration(PizzaScenario::tick_model()).expect("Pizza runs fixed updates");
        Ok(Self {
            worker: PizzaWorker::spawn(Simulation { state, lab }, dt)?,
            aspect_ratio,
        })
    }

    #[cfg(test)]
    pub(crate) fn worker(&self) -> &PizzaWorker {
        &self.worker
    }
}

impl ClientScenario for PizzaWorkerClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn pizza_status(&self) -> Option<PizzaStatus> {
        Some(self.worker.status())
    }

    fn tick_model(&self) -> TickModel {
        PizzaScenario::tick_model()
    }

    /// The host only steps worker adapters without time, to cancel pointer
    /// input or trigger an event; these apply before it continues.
    fn step(&mut self, actions: &[Action], _dt: Duration) -> StepResult {
        if !actions.is_empty() {
            self.worker.submit(Command::Actions(actions.to_vec()), true);
        }
        StepResult::default()
    }

    fn map_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        Vec::new()
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![RenderFrame::clone(&self.worker.frame())]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn publish_realtime_actions(&self, actions: &[Action], _observed_at: Instant) {
        self.worker
            .submit(Command::Actions(actions.to_vec()), false);
    }

    fn has_simulation_worker(&self) -> bool {
        true
    }

    fn simulation_worker_updates(&self) -> Option<u64> {
        Some(self.worker.updates())
    }

    fn simulation_worker_frame(&self) -> Option<u64> {
        Some(self.worker.generation())
    }

    fn set_realtime_paused(&self, paused: bool) {
        self.worker.set_paused(paused);
    }

    fn shutdown_realtime(&mut self) {
        self.worker.stop();
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        let aspect_ratio = viewport.aspect_ratio();
        if aspect_ratio != self.aspect_ratio {
            self.aspect_ratio = aspect_ratio;
            // Waiting keeps the next presented frame inside the new walls.
            self.worker.submit(Command::AspectRatio(aspect_ratio), true);
        }
    }

    fn runtime_error(&self) -> Option<String> {
        self.worker.error()
    }

    fn runtime_diagnostics(&self) -> String {
        format!(
            "pizza_simulation_worker=1\npizza_worker_updates={}\npizza_worker_dropped_backlogs={}",
            self.worker.updates(),
            self.worker.dropped_backlogs()
        )
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The state a worker owns.
pub(crate) struct Simulation {
    pub state: PizzaState,
    pub lab: Option<Recorder>,
}

impl Simulation {
    fn step(&mut self, actions: &[Action], dt: Duration) {
        step_with_lab(&mut self.state, &mut self.lab, actions, dt);
    }

    fn apply(&mut self, command: Command) {
        match command {
            // Pizza applies actions before advancing, so applying them without
            // time now equals applying them in the next update.
            Command::Actions(actions) => self.step(&actions, Duration::ZERO),
            Command::AspectRatio(aspect_ratio) => self.state.set_aspect_ratio(aspect_ratio),
            #[cfg(test)]
            Command::Panic => panic!("requested by test"),
        }
    }
}

/// UI requests, applied in order at update boundaries without advancing time.
enum Command {
    Actions(Vec<Action>),
    AspectRatio(f32),
    #[cfg(test)]
    Panic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Paused,
    Running,
    Stopped,
}

struct Control {
    mode: Mode,
    commands: Vec<Command>,
    /// Commands accepted and applied; synchronous requests wait on these.
    queued: u64,
    applied: u64,
    /// An update is advancing time. Pausing waits for its boundary.
    updating: bool,
    error: Option<String>,
    /// Test pacing: updates run only when requested, never by the clock.
    #[cfg(test)]
    manual_updates: Option<u64>,
}

struct Published {
    frame: Arc<RenderFrame>,
    status: PizzaStatus,
    /// Counts publications, so the host presents each frame once.
    generation: u64,
}

struct Shared {
    control: Mutex<Control>,
    changed: Condvar,
    published: Mutex<Published>,
    updates: AtomicU64,
    dropped_backlogs: AtomicU64,
}

pub(crate) struct PizzaWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<Option<Simulation>>>,
}

impl PizzaWorker {
    /// Starts paused; the host resumes it on its first tick.
    fn spawn(simulation: Simulation, dt: Duration) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                mode: Mode::Paused,
                commands: Vec::new(),
                queued: 0,
                applied: 0,
                updating: false,
                error: None,
                #[cfg(test)]
                manual_updates: None,
            }),
            changed: Condvar::new(),
            published: Mutex::new(Published {
                frame: Arc::new(PizzaScenario::render_frame(&simulation.state)),
                status: status_of(&simulation.state),
                generation: 1,
            }),
            updates: AtomicU64::new(0),
            dropped_backlogs: AtomicU64::new(0),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("pizza-simulation".into())
            .spawn(move || {
                let mut simulation = simulation;
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    run(&mut simulation, &worker_shared, dt);
                }));
                match result {
                    Ok(()) => Some(simulation),
                    Err(payload) => {
                        let mut control = lock(&worker_shared.control);
                        control.error = Some(format!(
                            "Pizza simulation stopped: {}",
                            panic_message(&*payload)
                        ));
                        control.mode = Mode::Stopped;
                        control.updating = false;
                        worker_shared.changed.notify_all();
                        None
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    /// Queues a command; with `wait`, returns once it has been applied.
    fn submit(&self, command: Command, wait: bool) {
        let mut control = lock(&self.shared.control);
        if control.mode == Mode::Stopped {
            return;
        }
        control.commands.push(command);
        control.queued += 1;
        let ticket = control.queued;
        self.shared.changed.notify_all();
        while wait && control.applied < ticket && control.mode != Mode::Stopped {
            control = wait_on(&self.shared.changed, control);
        }
    }

    /// Once pausing returns, no update is in progress or can start.
    fn set_paused(&self, paused: bool) {
        let mut control = lock(&self.shared.control);
        if control.mode == Mode::Stopped {
            return;
        }
        let mode = if paused { Mode::Paused } else { Mode::Running };
        if control.mode != mode {
            control.mode = mode;
            self.shared.changed.notify_all();
        }
        while paused && control.updating {
            control = wait_on(&self.shared.changed, control);
        }
    }

    fn frame(&self) -> Arc<RenderFrame> {
        Arc::clone(&lock(&self.shared.published).frame)
    }

    fn status(&self) -> PizzaStatus {
        lock(&self.shared.published).status.clone()
    }

    fn generation(&self) -> u64 {
        lock(&self.shared.published).generation
    }

    fn updates(&self) -> u64 {
        self.shared.updates.load(Ordering::Relaxed)
    }

    fn dropped_backlogs(&self) -> u64 {
        self.shared.dropped_backlogs.load(Ordering::Relaxed)
    }

    fn error(&self) -> Option<String> {
        lock(&self.shared.control).error.clone()
    }

    /// Stops after any update in progress and returns the simulation, unless
    /// it failed.
    fn stop(&mut self) -> Option<Simulation> {
        {
            let mut control = lock(&self.shared.control);
            control.mode = Mode::Stopped;
            self.shared.changed.notify_all();
        }
        self.thread.take()?.join().ok().flatten()
    }

    #[cfg(test)]
    pub(crate) fn use_manual_pacing(&self) {
        lock(&self.shared.control).manual_updates = Some(0);
    }

    /// Runs `updates` more updates under manual pacing and waits for them.
    #[cfg(test)]
    pub(crate) fn advance(&self, updates: u64) {
        let mut control = lock(&self.shared.control);
        *control
            .manual_updates
            .as_mut()
            .expect("advance requires manual pacing") += updates;
        self.shared.changed.notify_all();
        while control.mode == Mode::Running
            && (control.manual_updates != Some(0) || control.updating)
        {
            control = wait_on(&self.shared.changed, control);
        }
    }
}

impl Drop for PizzaWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

enum Next {
    Commands,
    Update { paced: bool },
}

fn run(simulation: &mut Simulation, shared: &Shared, dt: Duration) {
    let mut pacer = Pacer::new(dt, Instant::now());
    let mut running = false;
    // Updates left in the current wake's batch, and that batch's size.
    let mut remaining = 0;
    let mut batch = 0;
    loop {
        let mut control = lock(&shared.control);
        let next = loop {
            if control.mode == Mode::Stopped {
                return;
            }
            if !control.commands.is_empty() {
                break Next::Commands;
            }
            if control.mode == Mode::Paused {
                running = false;
                control = wait_on(&shared.changed, control);
                continue;
            }
            #[cfg(test)]
            if let Some(requested) = control.manual_updates {
                if requested == 0 {
                    control = wait_on(&shared.changed, control);
                    continue;
                }
                control.manual_updates = Some(requested - 1);
                break Next::Update { paced: false };
            }
            let now = Instant::now();
            if !running {
                // Like the host, a resumed run owes nothing for the pause.
                running = true;
                pacer.restart(now);
                remaining = 0;
            }
            if remaining == 0 {
                let due = pacer.next_due();
                if now < due {
                    control = wait_timeout_on(&shared.changed, control, due - now);
                    continue;
                }
                batch = pacer.take_due(now);
                remaining = batch;
                if remaining == 0 {
                    continue;
                }
            }
            break Next::Update { paced: true };
        };

        match next {
            Next::Commands => {
                let commands = std::mem::take(&mut control.commands);
                drop(control);
                let count = commands.len() as u64;
                for command in commands {
                    simulation.apply(command);
                }
                publish(simulation, shared);
                let mut control = lock(&shared.control);
                control.applied += count;
                shared.changed.notify_all();
            }
            Next::Update { paced } => {
                control.updating = true;
                drop(control);
                simulation.step(&[], dt);
                shared.updates.fetch_add(1, Ordering::Relaxed);
                publish(simulation, shared);
                if paced {
                    pacer.complete();
                    remaining -= 1;
                    if remaining == 0 && pacer.finish(batch) {
                        shared.dropped_backlogs.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let mut control = lock(&shared.control);
                control.updating = false;
                shared.changed.notify_all();
            }
        }
    }
}

fn publish(simulation: &Simulation, shared: &Shared) {
    let frame = Arc::new(PizzaScenario::render_frame(&simulation.state));
    let status = status_of(&simulation.state);
    let mut published = lock(&shared.published);
    published.frame = frame;
    published.status = status;
    published.generation += 1;
}

/// The host's fixed-step accounting, applied on the worker's own wakes.
#[derive(Debug)]
struct Pacer {
    dt: Duration,
    owed: Duration,
    last: Instant,
}

impl Pacer {
    fn new(dt: Duration, now: Instant) -> Self {
        Self {
            dt,
            owed: Duration::ZERO,
            last: now,
        }
    }

    fn restart(&mut self, now: Instant) {
        self.owed = Duration::ZERO;
        self.last = now;
    }

    fn next_due(&self) -> Instant {
        self.last + self.dt.saturating_sub(self.owed)
    }

    /// Adds the time since the previous wake and returns the updates to run.
    fn take_due(&mut self, now: Instant) -> usize {
        self.owed += now.saturating_duration_since(self.last);
        self.last = now;
        let due = self.owed.as_nanos() / self.dt.as_nanos();
        due.min(MAX_FIXED_STEPS_PER_TICK as u128) as usize
    }

    fn complete(&mut self) {
        self.owed = self.owed.saturating_sub(self.dt);
    }

    /// Ends a batch. After a full batch the host forgets the remaining
    /// backlog; returns whether that dropped at least one update.
    fn finish(&mut self, batch: usize) -> bool {
        if batch < MAX_FIXED_STEPS_PER_TICK {
            return false;
        }
        let dropped = self.owed >= self.dt;
        self.owed = Duration::ZERO;
        dropped
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn wait_on<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|error| error.into_inner())
}

fn wait_timeout_on<'a, T>(
    condvar: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
) -> MutexGuard<'a, T> {
    condvar
        .wait_timeout(guard, timeout)
        .unwrap_or_else(|error| error.into_inner())
        .0
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{PointerAction, PointerPhase, RenderPoint};
    use scenario_pizza::profile::Sample;
    use scenario_pizza::{PizzaAction, PizzaBounds, PizzaEventKind, capacity_lab};

    const DT: Duration = Duration::from_nanos(16_666_667);

    fn fixture() -> PizzaState {
        capacity_lab::mixed_size_fixture(500, 500, PizzaBounds::default(), 7).unwrap()
    }

    fn manual_worker(state: PizzaState) -> PizzaWorker {
        let worker = PizzaWorker::spawn(Simulation { state, lab: None }, DT).unwrap();
        worker.use_manual_pacing();
        worker.set_paused(false);
        worker
    }

    fn pointer(phase: PointerPhase) -> Action {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(0.5, 0.28125),
            phase,
        })
    }

    #[test]
    fn the_host_and_worker_share_one_update_length() {
        assert_eq!(fixed_step_duration(PizzaScenario::tick_model()), Some(DT));
    }

    #[test]
    fn worker_updates_and_queued_input_match_direct_stepping() {
        let trigger = PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode();
        let mut direct = fixture();
        for tick in 0..240 {
            let actions = match tick {
                30 => vec![pointer(PointerPhase::Press)],
                45 => vec![pointer(PointerPhase::Release)],
                120 => vec![trigger.clone()],
                _ => Vec::new(),
            };
            PizzaScenario::step(&mut direct, &actions, DT);
        }

        let mut worker = manual_worker(fixture());
        worker.advance(30);
        worker.submit(Command::Actions(vec![pointer(PointerPhase::Press)]), false);
        worker.advance(15);
        worker.submit(
            Command::Actions(vec![pointer(PointerPhase::Release)]),
            false,
        );
        worker.advance(75);
        worker.submit(Command::Actions(vec![trigger]), true);
        worker.advance(120);
        assert_eq!(worker.updates(), 240);
        assert_eq!(worker.status().observation.tick, 240);
        let simulation = worker.stop().unwrap();
        assert_eq!(
            Sample::capture(&simulation.state).fingerprint,
            Sample::capture(&direct).fingerprint
        );
        assert_eq!(simulation.state.population(), direct.population());
    }

    #[test]
    fn waiting_commands_apply_while_paused_without_advancing_time() {
        let worker = manual_worker(fixture());
        worker.advance(3);
        worker.set_paused(true);
        let trigger = PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode();
        worker.submit(Command::Actions(vec![trigger]), true);
        let status = worker.status();
        assert_eq!(status.observation.tick, 3);
        assert!(matches!(
            status.observation.event,
            scenario_pizza::PizzaEventState::Idle {
                pending_manual: Some(PizzaEventKind::DirectionalGravity),
                ..
            }
        ));
        worker.submit(Command::AspectRatio(0.6), true);
        assert_eq!(worker.status().observation.tick, 3);
        assert_eq!(worker.updates(), 3);
    }

    #[test]
    fn every_update_and_command_publishes_a_new_frame() {
        let worker = manual_worker(fixture());
        let first = worker.generation();
        worker.advance(3);
        assert_eq!(worker.generation(), first + 3);
        worker.set_paused(true);
        assert_eq!(worker.generation(), first + 3);
        worker.submit(Command::AspectRatio(0.6), true);
        assert_eq!(worker.generation(), first + 4);
    }

    #[test]
    fn paused_real_time_worker_stops_at_an_update_boundary() {
        let worker = PizzaWorker::spawn(
            Simulation {
                state: fixture(),
                lab: None,
            },
            DT,
        )
        .unwrap();
        worker.set_paused(false);
        let started = Instant::now();
        while worker.updates() < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "worker did not run"
            );
            thread::sleep(Duration::from_millis(5));
        }
        worker.set_paused(true);
        let paused = worker.updates();
        assert_eq!(worker.status().observation.tick, paused);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(worker.updates(), paused);
        worker.set_paused(false);
        while worker.updates() == paused {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "worker did not resume"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn catch_up_runs_at_most_five_updates_and_then_drops_the_backlog() {
        let start = Instant::now();
        let dt = Duration::from_millis(10);
        let mut pacer = Pacer::new(dt, start);
        assert_eq!(pacer.next_due(), start + dt);
        let wake = start + Duration::from_millis(25);
        assert_eq!(pacer.take_due(wake), 2);
        pacer.complete();
        pacer.complete();
        assert!(!pacer.finish(2));
        assert_eq!(pacer.next_due(), wake + Duration::from_millis(5));

        let late = wake + Duration::from_millis(100);
        assert_eq!(pacer.take_due(late), MAX_FIXED_STEPS_PER_TICK);
        for _ in 0..MAX_FIXED_STEPS_PER_TICK {
            pacer.complete();
        }
        assert!(pacer.finish(MAX_FIXED_STEPS_PER_TICK));
        assert_eq!(pacer.next_due(), late + dt);

        pacer.restart(late);
        assert_eq!(pacer.take_due(late + Duration::from_millis(50)), 5);
        for _ in 0..5 {
            pacer.complete();
        }
        // Exactly five owed updates leave no backlog to drop.
        assert!(!pacer.finish(5));
    }

    #[test]
    fn a_failed_update_is_reported_and_releases_waiting_callers() {
        let mut worker = manual_worker(fixture());
        worker.submit(Command::Panic, true);
        let error = worker.error().unwrap();
        assert!(error.contains("requested by test"), "{error}");
        worker.set_paused(false);
        worker.submit(Command::AspectRatio(0.6), true);
        assert!(worker.stop().is_none());
    }
}
