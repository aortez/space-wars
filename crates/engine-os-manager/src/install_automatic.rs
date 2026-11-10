//! Starts an installation by itself for a saved Automatic preference. While
//! the gate is open it watches the app. Once the app has shown only the
//! launcher or the Clock, without a person, for ten minutes, it spends the
//! window's one attempt and counts down a minute; any use of the app in that
//! minute ends the attempt until the next window. Each look during the
//! countdown also tells the app what to show. Otherwise the attempt takes
//! the confirmed path from its own confirmation on: the same coordinator, app
//! handoff, journals, locks and writer. Nothing here writes or restarts.
use super::service::{Backend, Work};
use super::{Context, Coordinator, HostObservation};
use crate::install_admission::{Gate, Gatekeeper};
use serde::{Deserialize, Serialize};
use spacewars_control::install::{
    Consent, Prepare, SCHEMA, transport,
    wire::{Failure, Phase as Handoff, Scene},
};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// Where this module logs, at `info`. The service records it at its default
/// level, so that an installation nobody confirmed leaves a trace.
pub const LOG_TARGET: &str = module_path!();
/// The app must have gone this long without a person.
pub const IDLE_MS: u64 = 10 * 60_000;
/// A question the worker has not picked up by then is dropped unanswered.
const ASK: Duration = Duration::from_secs(3);
/// The worker's answer may take this much longer: it observes the app.
const ANSWER: Duration = Duration::from_secs(7);

/// How often the runner looks, and how long it warns.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pace {
    /// Between looks at the gate and, while it is open, at the app.
    pub watch: Duration,
    /// Between looks at the app during the countdown.
    pub count: Duration,
    /// From the spent attempt to the installation's own confirmation.
    pub countdown: Duration,
}

impl Pace {
    pub const NATIVE: Self = Self {
        watch: Duration::from_secs(30),
        count: Duration::from_secs(1),
        countdown: Duration::from_secs(60),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// The gate is closed or no image can be offered, and no countdown has
    /// ended since the service started.
    Waiting,
    /// The gate is open; `hold` says what the attempt waits for.
    Watching,
    /// The window's attempt is spent. The installation starts at
    /// `countdown_ends_ms` unless the app is used first.
    Countdown,
    /// The last countdown ended without an installation; `hold` says why.
    Postponed,
    /// The last countdown handed its installation to the confirmed path,
    /// whose phases `install status` reports.
    Started,
}

/// Why an open gate has not started an installation, or what ended a
/// countdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hold {
    /// The app did not answer as the live kiosk, is busy with work of its own
    /// or is held by an installation.
    AppUnavailable,
    /// The app shows a game, or does not say what it shows.
    Game,
    /// A person used the app within the idle time, or the app cannot tell.
    InUse,
    /// The saved mode stopped being Automatic during the countdown.
    NotAutomatic,
    /// The device would refuse this installation now, whoever asked. The
    /// service log names the reason.
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub phase: Phase,
    pub hold: Option<Hold>,
    /// Milliseconds since boot.
    pub countdown_ends_ms: Option<u64>,
}

impl Status {
    fn new(phase: Phase, hold: Option<Hold>) -> Self {
        Self {
            phase,
            hold,
            countdown_ends_ms: None,
        }
    }
}

/// The image an attempt is for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Image {
    pub build_id: String,
    /// Shown to whoever watches the countdown.
    pub generation: u64,
}

/// The runner's latest status, for the device status.
#[derive(Clone)]
pub struct Watch(Arc<Mutex<Status>>);

impl Watch {
    pub fn read(&self) -> Option<Status> {
        self.0.lock().ok().map(|status| *status)
    }
}

/// Whether an observed app lets an attempt start or go on. Missing evidence
/// is a game in use.
pub(super) fn idle(host: &HostObservation) -> Result<(), Hold> {
    if host.fenced {
        return Err(Hold::AppUnavailable);
    }
    if !matches!(host.scene, Some(Scene::Launcher | Scene::Clock)) {
        return Err(Hold::Game);
    }
    if host.idle_ms.is_none_or(|idle| idle < IDLE_MS) {
        return Err(Hold::InUse);
    }
    Ok(())
}

pub(super) enum Step {
    /// Before the countdown: an offer would succeed now and the app is idle.
    /// The worker then spends the window's attempt.
    Admit,
    /// After the countdown: the same again, then the offer and its own
    /// confirmation.
    Install,
}

/// A question for the install worker, which alone evaluates a context.
pub(super) struct Ask {
    pub step: Step,
    pub image: String,
    pub until: Instant,
    pub reply: mpsc::SyncSender<Result<(), Hold>>,
}

/// What both steps require of the worker's fresh context.
fn admissible<'a>(
    state: &Coordinator,
    context: &'a Context,
    image: &str,
) -> Result<&'a HostObservation, Hold> {
    let host = context.host.as_ref().ok_or(Hold::AppUnavailable)?;
    idle(host)?;
    let offered = context
        .ready
        .as_ref()
        .is_some_and(|ready| ready.candidate.build_id == image);
    // A standing offer is no obstacle: the attempt confirms that same offer.
    let free = matches!(
        state.status().phase,
        Handoff::Idle | Handoff::Offered | Handoff::Aborted
    );
    if !offered || !free {
        return Err(Hold::Refused);
    }
    Ok(host)
}

/// The worker's context refused an attempt. Logged when the reason changes,
/// because the watcher asks again every time it looks.
pub(super) fn refused(last: &mut Option<Failure>, failure: Failure) -> Hold {
    if last.replace(failure) != Some(failure) {
        tracing::info!(?failure, "automatic installation refused.");
    }
    Hold::Refused
}

pub(super) fn admit(state: &Coordinator, context: &Context, image: &str) -> Result<(), Hold> {
    admissible(state, context, image).map(|_| ())
}

/// Spends the open window's attempt. Called without the coordinator's lock,
/// because the save waits on storage.
pub(super) fn begin(gate: &Gatekeeper, image: &str) -> Result<(), Hold> {
    match gate.begin(image) {
        Ok(Gate::Open) => Ok(()),
        Ok(gate) => {
            tracing::info!(
                ?gate,
                "automatic installation gate closed before its countdown."
            );
            Err(Hold::Refused)
        }
        Err(error) => {
            tracing::warn!(%error, "automatic installation attempt could not be saved.");
            Err(Hold::Refused)
        }
    }
}

/// The attempt's own offer and confirmation. It ends the Clock and nothing
/// else: a game was refused above, and without `end_game` the coordinator
/// refuses one again.
pub(super) fn confirm(
    state: &mut Coordinator,
    context: &Context,
    image: &str,
    now_ms: u64,
) -> Result<Prepare, Hold> {
    let host = admissible(state, context, image)?;
    let refused = |error| {
        tracing::info!(%error, "automatic installation refused at its confirmation.");
        Hold::Refused
    };
    let offer = state.offer(context, now_ms).map_err(refused)?;
    let consent = Consent {
        schema: SCHEMA,
        ticket: offer.ticket,
        end_game: host.scene == Some(Scene::Clock),
    };
    state.confirm(&consent, context, now_ms).map_err(refused)
}

struct Counting {
    image: Image,
    ends_ms: u64,
}

struct Runner {
    backend: Arc<dyn Backend>,
    gate: Arc<Gatekeeper>,
    pace: Pace,
    work: mpsc::SyncSender<Work>,
    status: Watch,
}

impl Runner {
    fn publish(&self, status: Status) {
        if let Ok(mut current) = self.status.0.lock() {
            *current = status;
        }
    }

    fn app(&self) -> Result<(), Hold> {
        let host = self.backend.observe().map_err(|_| Hold::AppUnavailable)?;
        idle(&host)
    }

    /// What a running countdown still needs each time it looks. The look is
    /// also what keeps the countdown on the app's screen.
    fn counting(&self, counting: &Counting) -> Result<(), Hold> {
        if !self.gate.automatic() {
            return Err(Hold::NotAutomatic);
        }
        let host = self
            .backend
            .countdown(counting.image.generation, counting.ends_ms)
            .map_err(|_| Hold::AppUnavailable)?;
        idle(&host)
    }

    fn ask(&self, step: Step, image: &str) -> Result<(), Hold> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.work
            .try_send(Work::Automatic(Ask {
                step,
                image: image.into(),
                until: Instant::now() + ASK,
                reply,
            }))
            .map_err(|_| Hold::Refused)?;
        answer
            .recv_timeout(ASK + ANSWER)
            .unwrap_or(Err(Hold::Refused))
    }

    fn step(&self, counting: Option<Counting>) -> Option<Counting> {
        let Some(counting) = counting else {
            return self.watch();
        };
        let ended = match self.counting(&counting) {
            Err(hold) => Err(hold),
            Ok(()) => match transport::boot_ms() {
                Ok(now) if now < counting.ends_ms => return Some(counting),
                Ok(_) => self.ask(Step::Install, &counting.image.build_id),
                Err(_) => Err(Hold::Refused),
            },
        };
        let image = counting.image.build_id;
        match ended {
            Ok(()) => {
                tracing::info!(image, "automatic installation started after its countdown.");
                self.publish(Status::new(Phase::Started, None));
            }
            Err(hold) => {
                tracing::info!(
                    image,
                    ?hold,
                    "automatic installation postponed to the next window."
                );
                self.publish(Status::new(Phase::Postponed, Some(hold)));
            }
        }
        None
    }

    fn watch(&self) -> Option<Counting> {
        let image = self.backend.ready();
        let build_id = image.as_ref().map(|image| image.build_id.as_str());
        let (Gate::Open, Some(image)) = (self.gate.status(build_id).gate, image) else {
            // How the last countdown ended stays readable until the gate opens.
            if self
                .status
                .read()
                .is_some_and(|status| !matches!(status.phase, Phase::Postponed | Phase::Started))
            {
                self.publish(Status::new(Phase::Waiting, None));
            }
            return None;
        };
        let admitted = self
            .app()
            .and_then(|()| self.ask(Step::Admit, &image.build_id))
            .and_then(|()| transport::boot_ms().map_err(|_| Hold::Refused));
        match admitted {
            Ok(now) => {
                let ends_ms = now.saturating_add(self.pace.countdown.as_millis() as u64);
                tracing::info!(
                    image = image.build_id,
                    seconds = self.pace.countdown.as_secs(),
                    "automatic installation countdown started."
                );
                self.publish(Status {
                    phase: Phase::Countdown,
                    hold: None,
                    countdown_ends_ms: Some(ends_ms),
                });
                Some(Counting { image, ends_ms })
            }
            Err(hold) => {
                let waiting = Status::new(Phase::Watching, Some(hold));
                if self.status.read() != Some(waiting) {
                    tracing::info!(
                        image = image.build_id,
                        ?hold,
                        "automatic installation is waiting."
                    );
                    self.publish(waiting);
                }
                None
            }
        }
    }
}

/// The running watcher. Dropping it stops the thread and waits for it.
pub(super) struct Handle {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
    status: Watch,
}

impl Handle {
    pub fn watch(&self) -> Watch {
        self.status.clone()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn spawn(
    backend: Arc<dyn Backend>,
    gate: Arc<Gatekeeper>,
    pace: Pace,
    work: mpsc::SyncSender<Work>,
) -> Handle {
    let status = Watch(Arc::new(Mutex::new(Status::new(Phase::Waiting, None))));
    let runner = Runner {
        backend,
        gate,
        pace,
        work,
        status: status.clone(),
    };
    let (stop, stopped) = mpsc::channel::<()>();
    let thread = thread::spawn(move || {
        let mut counting = None;
        loop {
            let pause = if counting.is_some() {
                runner.pace.count
            } else {
                runner.pace.watch
            };
            if !matches!(
                stopped.recv_timeout(pause),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                return;
            }
            counting = runner.step(counting);
        }
    });
    Handle {
        stop: Some(stop),
        thread: Some(thread),
        status,
    }
}

#[cfg(test)]
#[path = "install_automatic_tests.rs"]
mod tests;
