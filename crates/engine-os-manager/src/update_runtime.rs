//! Device-owned download coordinator. Filesystem/HTTP work and radio ownership
//! stay outside status handling and outside the client/render process.
use crate::network_policy::{Clock, DownloadReservation, HoldToken};
use crate::network_watch::{Snapshot, Watch};
use crate::service::SharedState;
use crate::update_schedule::{self as schedule, Active, Completion, Opportunity, Schedule};
use crate::{Error, Result};
use engine_ota::download::{self, Progress};
use engine_ota::{Channel, ImageDescriptor, UsableTime, VerifiedCandidate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "update_native.rs"]
mod native;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Starting,
    Idle,
    Preparing,
    WaitingForNetwork,
    WaitingForTime,
    Checking,
    Downloading,
    Verifying,
    Ready,
    Stopping,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    StateUnavailable,
    PreparationUnavailable,
    MaintenanceBusy,
    NetworkUnavailable,
    DownloadFailed,
    TimeUnavailable,
    Interrupted,
    StaleReady,
    WorkerStalled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub phase: Phase,
    pub reason: Option<Reason>,
    pub schedule: Option<schedule::Status>,
    pub progress: Option<Progress>,
    pub ready: Option<ImageDescriptor>,
    pub manual_pending: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub schema: u32,
    pub received_ms: u64,
    pub coalesced: bool,
}

trait Inputs: Send + Sync {
    fn clock(&self) -> Result<Clock>;
    fn network(&self, clock: &Clock) -> Option<Snapshot>;
    fn time(&self) -> Option<UsableTime>;
}

struct Shared {
    status: Status,
    observed: Clock,
    manual_pending: Option<Clock>,
    selections_pending: usize,
    control: Weak<Control>,
    ready: Option<(VerifiedCandidate, UsableTime)>,
    installation: Option<Arc<()>>,
    selecting: bool,
}

// Admission cannot race a scheduler decision whose result is not published yet.
// Only this flag is held during work; status and cancellation keep their lock.
struct Selecting(Arc<Mutex<Shared>>);
impl Drop for Selecting {
    fn drop(&mut self) {
        if let Ok(mut shared) = self.0.lock() {
            shared.selecting = false;
        }
    }
}

enum Command {
    Check { received: Clock, selection: u64 },
    Select(Channel),
    Offer(Offer),
}
struct Offer {
    parent: HoldToken,
    until: Clock,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<bool>,
}
impl Offer {
    fn valid(&self, now: &Clock) -> bool {
        !self.cancelled.load(Ordering::Relaxed)
            && now.boot_id == self.until.boot_id
            && now.ms < self.until.ms
    }
}
impl Drop for Offer {
    fn drop(&mut self) {
        let _ = self.reply.try_send(false);
    }
}

#[derive(Clone)]
pub struct Handle {
    shared: Arc<Mutex<Shared>>,
    inputs: Arc<dyn Inputs>,
    requests: mpsc::SyncSender<Command>,
    stop: Arc<AtomicBool>,
}
impl Handle {
    /// Read-only verifier output. Socket callers cannot manufacture this value.
    pub(crate) fn install_candidate(&self) -> Result<InstallCandidate> {
        let shared = self.shared.lock().map_err(|_| unavailable())?;
        if shared.installation.is_some()
            || shared.selecting
            || shared.control.strong_count() != 0
            || shared.manual_pending.is_some()
            || shared.selections_pending != 0
            || shared.status.phase != Phase::Ready
            || shared
                .status
                .schedule
                .as_ref()
                .is_none_or(|s| s.active.is_some())
        {
            return Err(unavailable());
        }
        self.install_snapshot(&shared)
    }

    fn install_snapshot(&self, shared: &Shared) -> Result<InstallCandidate> {
        let now = self.inputs.clock()?;
        let time = self.inputs.time().ok_or_else(unavailable)?;
        let (candidate, floor) = shared.ready.as_ref().ok_or_else(unavailable)?;
        let selected = shared.status.schedule.as_ref().ok_or_else(unavailable)?;
        if self.stop.load(Ordering::Relaxed)
            || now.boot_id != shared.observed.boot_id
            || now
                .ms
                .checked_sub(shared.observed.ms)
                .is_none_or(|age| age > 15_000)
            || time.0 < floor.0
            || time.0 >= candidate.expires()
            || candidate.image().channel != selected.channel
            || shared.selections_pending != 0
        {
            return Err(unavailable());
        }
        Ok(InstallCandidate {
            candidate: candidate.clone(),
            generation: selected.selection_generation,
        })
    }

    pub(crate) fn claim_install(&self, expected: &InstallCandidate) -> Result<InstallSelection> {
        let mut shared = self.shared.lock().map_err(|_| unavailable())?;
        let current = self.install_snapshot(&shared)?;
        if shared.installation.is_some()
            || shared.selecting
            || shared.control.strong_count() != 0
            || shared.manual_pending.is_some()
            || shared.status.phase != Phase::Ready
            || shared
                .status
                .schedule
                .as_ref()
                .is_none_or(|s| s.active.is_some())
            || current.candidate.id() != expected.candidate.id()
            || current.generation != expected.generation
        {
            return Err(unavailable());
        }
        let marker = Arc::new(());
        shared.installation = Some(marker.clone());
        Ok(InstallSelection {
            handle: self.clone(),
            marker,
            selected: current,
        })
    }

    pub(crate) fn install_time(&self) -> engine_ota::Result<UsableTime> {
        self.inputs.time().ok_or(engine_ota::Error::WaitingForTime)
    }

    pub fn status(&self) -> Result<Status> {
        let now = self.inputs.clock()?;
        let shared = self.shared.lock().map_err(|_| unavailable())?;
        let mut status = shared.status.clone();
        status.manual_pending = shared.manual_pending.is_some();
        if self.stop.load(Ordering::Relaxed)
            || shared.observed.boot_id != now.boot_id
            || now
                .ms
                .checked_sub(shared.observed.ms)
                .is_none_or(|age| age > 15_000)
        {
            status.phase = Phase::Unavailable;
            status.reason = Some(Reason::WorkerStalled);
            status.ready = None;
        } else if let Some((candidate, floor)) = &shared.ready
            && self
                .inputs
                .time()
                .is_none_or(|time| time.0 < floor.0 || time.0 >= candidate.expires())
        {
            status.phase = Phase::Idle;
            status.reason = Some(Reason::TimeUnavailable);
            status.ready = None;
        }
        Ok(status)
    }

    pub fn check_now(&self) -> Result<Accepted> {
        let now = self.inputs.clock()?;
        if self.status()?.phase == Phase::Unavailable {
            return Err(unavailable());
        }
        let mut shared = self.shared.lock().map_err(|_| unavailable())?;
        if shared.installation.is_some() {
            return Err(unavailable());
        }
        if shared.selections_pending > 0 {
            return Err(unavailable());
        }
        let status = shared.status.schedule.as_ref().ok_or_else(unavailable)?;
        if let Some(received_ms) = shared.manual_pending.as_ref().map(|r| r.ms).or_else(|| {
            status
                .active
                .as_ref()
                .filter(|a| now.ms < a.deadline_ms)
                .map(|a| a.started_ms)
        }) {
            return Ok(Accepted {
                schema: 1,
                received_ms,
                coalesced: true,
            });
        }
        self.requests
            .try_send(Command::Check {
                received: now.clone(),
                selection: status.selection_generation,
            })
            .map_err(|_| unavailable())?;
        shared.manual_pending = Some(now.clone());
        Ok(Accepted {
            schema: 1,
            received_ms: now.ms,
            coalesced: false,
        })
    }

    pub fn select_channel(&self, channel: Channel) -> Result<Accepted> {
        let now = self.inputs.clock()?;
        if self.status()?.phase == Phase::Unavailable {
            return Err(unavailable());
        }
        let mut shared = self.shared.lock().map_err(|_| unavailable())?;
        if shared.installation.is_some() {
            return Err(unavailable());
        }
        if shared.selections_pending == 0
            && shared
                .status
                .schedule
                .as_ref()
                .is_some_and(|s| s.channel == channel)
        {
            return Ok(Accepted {
                schema: 1,
                received_ms: now.ms,
                coalesced: true,
            });
        }
        self.requests
            .try_send(Command::Select(channel))
            .map_err(|_| unavailable())?;
        shared.selections_pending += 1;
        if let Some(control) = shared.control.upgrade() {
            control.cancel();
        }
        shared.ready = None;
        shared.status.ready = None;
        shared.status.phase = if shared.control.strong_count() > 0 {
            Phase::Stopping
        } else {
            Phase::Idle
        };
        Ok(Accepted {
            schema: 1,
            received_ms: now.ms,
            coalesced: false,
        })
    }

    /// Called after time admission and before its provider query or release.
    /// Waiting is bounded by both five seconds and the original NTP deadline.
    pub(crate) fn offer_time(&self, parent: &HoldToken, deadline: &Clock) {
        let Ok(now) = self.inputs.clock() else {
            return;
        };
        if now.boot_id != deadline.boot_id || now.ms >= deadline.ms {
            return;
        }
        let until = Clock {
            boot_id: now.boot_id,
            ms: deadline.ms.min(now.ms.saturating_add(5000)),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(1);
        if self
            .requests
            .try_send(Command::Offer(Offer {
                parent: parent.clone(),
                until: until.clone(),
                cancelled: cancelled.clone(),
                reply: tx,
            }))
            .is_err()
        {
            return;
        }
        let wall_end = Instant::now() + Duration::from_secs(5);
        loop {
            if self.stop.load(Ordering::Relaxed)
                || Instant::now() >= wall_end
                || !self
                    .inputs
                    .clock()
                    .is_ok_and(|now| now.boot_id == until.boot_id && now.ms < until.ms)
            {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct InstallCandidate {
    pub candidate: VerifiedCandidate,
    pub generation: u64,
}
pub(crate) struct InstallSelection {
    handle: Handle,
    marker: Arc<()>,
    selected: InstallCandidate,
}
impl InstallSelection {
    pub fn check(&self) -> Result<()> {
        let shared = self.handle.shared.lock().map_err(|_| unavailable())?;
        let current = self.handle.install_snapshot(&shared)?;
        if !shared
            .installation
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(m, &self.marker))
            || current.candidate.id() != self.selected.candidate.id()
            || current.generation != self.selected.generation
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
impl Drop for InstallSelection {
    fn drop(&mut self) {
        if let Ok(mut shared) = self.handle.shared.lock()
            && shared
                .installation
                .as_ref()
                .is_some_and(|m| Arc::ptr_eq(m, &self.marker))
        {
            shared.installation = None;
        }
    }
}

pub struct Runtime {
    handle: Handle,
    worker: Option<JoinHandle<()>>,
}
impl Runtime {
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.handle.stop.store(true, Ordering::Relaxed);
        if let Ok(shared) = self.handle.shared.lock()
            && let Some(control) = shared.control.upgrade()
        {
            control.cancel();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn spawn(time: SharedState, network: Watch, boot_id: String) -> Result<Runtime> {
    spawn_with(
        PathBuf::from(schedule::STATE_DIRECTORY),
        native::Native::new(time, network, boot_id),
    )
}

fn spawn_with<B: Backend>(path: PathBuf, backend: B) -> Result<Runtime> {
    let now = backend.clock()?;
    let inputs: Arc<dyn Inputs> = Arc::new(backend.clone());
    let (tx, rx) = mpsc::sync_channel(8);
    let handle = Handle {
        shared: Arc::new(Mutex::new(Shared {
            status: Status {
                phase: Phase::Starting,
                reason: None,
                schedule: None,
                progress: None,
                ready: None,
                manual_pending: false,
            },
            observed: now,
            manual_pending: None,
            selections_pending: 0,
            control: Weak::new(),
            ready: None,
            installation: None,
            selecting: false,
        })),
        inputs,
        requests: tx,
        stop: Arc::new(AtomicBool::new(false)),
    };
    let worker_handle = handle.clone();
    let worker = thread::spawn(move || {
        let mut runner: Option<Runner<B>> = None;
        let mut retry = Instant::now();
        while !worker_handle.stop.load(Ordering::Relaxed) {
            if runner.is_none() && Instant::now() >= retry {
                runner = backend
                    .clock()
                    .and_then(|clock| Runner::open(&path, backend.clone(), &clock))
                    .ok();
                retry = Instant::now() + Duration::from_secs(5);
            }
            let result = runner
                .as_mut()
                .map_or(Err(unavailable()), |r| r.step(&rx, &worker_handle));
            if result.is_err() {
                runner = None;
                if let Ok(now) = backend.clock() {
                    publish(
                        &worker_handle,
                        &now,
                        None,
                        Phase::Unavailable,
                        Some(Reason::StateUnavailable),
                        None,
                        None,
                    );
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        drop(runner);
        worker_handle.stop.store(true, Ordering::Relaxed);
    });
    Ok(Runtime {
        handle,
        worker: Some(worker),
    })
}

#[derive(Clone)]
struct Lease {
    epoch: u64,
    mode: Option<u64>,
    until_ms: u64,
}
#[derive(Default)]
struct Live {
    lease: Option<Lease>,
    progress: Option<Progress>,
    payload_at: Option<Clock>,
}
struct Control {
    inputs: Arc<dyn Inputs>,
    start: Clock,
    deadline_ms: u64,
    stop: Arc<AtomicBool>,
    cancelled: AtomicBool,
    live: Mutex<Live>,
}
impl Control {
    fn new(inputs: Arc<dyn Inputs>, start: Clock, deadline_ms: u64, stop: Arc<AtomicBool>) -> Self {
        Self {
            inputs,
            start,
            deadline_ms,
            stop,
            cancelled: AtomicBool::new(false),
            live: Mutex::new(Live::default()),
        }
    }
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Relaxed) && !self.stop.load(Ordering::Relaxed)
    }
    fn route(&self) -> engine_ota::Result<bool> {
        let now = self.inputs.clock().map_err(|_| interrupted())?;
        if !self.active()
            || now.boot_id != self.start.boot_id
            || now.ms < self.start.ms
            || now.ms >= self.deadline_ms
        {
            return Err(interrupted());
        }
        let lease = self
            .live
            .lock()
            .map_err(|_| interrupted())?
            .lease
            .clone()
            .ok_or_else(interrupted)?;
        let network = self.inputs.network(&now).ok_or_else(interrupted)?;
        if now.ms >= lease.until_ms
            || network.epoch != lease.epoch
            || !available_policy(&network)
            || lease
                .mode
                .is_some_and(|m| network.status.policy.mode_generation != m)
        {
            return Err(interrupted());
        }
        Ok(network
            .status
            .actual
            .as_ref()
            .is_some_and(|r| r.ethernet_available || r.wireless_available))
    }
    fn wait_ready(&self) -> engine_ota::Result<()> {
        loop {
            let now = self.inputs.clock().map_err(|_| interrupted())?;
            if now.ms >= self.start.ms.saturating_add(schedule::PREPARATION_MS) {
                return Err(interrupted());
            }
            if self.route()? && self.inputs.time().is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}
impl download::Control for Control {
    fn checkpoint(&self) -> engine_ota::Result<Option<UsableTime>> {
        if !self.route()? {
            return Err(interrupted());
        }
        Ok(self.inputs.time())
    }
    fn progress(&self, progress: Progress) {
        if let (Ok(now), Ok(mut live)) = (self.inputs.clock(), self.live.lock()) {
            if progress.phase == download::Phase::Downloading && live.payload_at.is_none() {
                live.payload_at = Some(now);
            }
            live.progress = Some(progress);
        }
    }
}

struct Finished<P> {
    outcome: download::Outcome,
    proof: Option<P>,
    time: UsableTime,
}
// Only contention on the shared installation lock is retryable. A busy trust
// store, invalid boot evidence or another preparation error still fails closed.
#[derive(Debug)]
enum PreparationError {
    MaintenanceBusy,
    Failed {
        stage: &'static str,
        category: &'static str,
        os_error: Option<i32>,
    },
}
impl PreparationError {
    fn failed(stage: &'static str, error: impl Into<Error>) -> Self {
        // Never format arbitrary error strings: parser/transport errors may
        // contain feed URLs or private file contents. Keep diagnostics bounded.
        let (category, os_error) = match error.into() {
            Error::Io(error) | Error::Ota(engine_ota::Error::Io(error)) => {
                ("io", error.raw_os_error())
            }
            Error::Json(_) | Error::Ota(engine_ota::Error::Json(_)) => ("json", None),
            Error::Invalid(_) => ("invalid-state", None),
            Error::Ota(error) => {
                use engine_ota::Error::*;
                let category = match error {
                    Busy => "busy",
                    Recovery(_) => "recovery",
                    WaitingForTime | TimeMovedBackwards => "time",
                    Timeout => "timeout",
                    Descriptor(_) | ImageIdentity(_) | ImageMismatch => "image",
                    Boot(_) => "boot",
                    Access(_) => "access",
                    Network(_) => "network",
                    Persistence(_) => "persistence",
                    Install(_) => "install",
                    Feed(_) => "feed",
                    Store(_) => "store",
                    RefreshRequired | CandidateChanged => "candidate",
                    Staging(_) | StagingSpace => "staging",
                    DownloadResponse | Download(_) => "download",
                    Tuf(_) => "verification",
                    Io(_) | Json(_) => unreachable!("handled above"),
                };
                (category, None)
            }
        };
        Self::Failed {
            stage,
            category,
            os_error,
        }
    }
}

trait Backend: Inputs + Clone + Send + Sync + 'static {
    type Prepared: Send + 'static;
    type Ready: Send + 'static;
    fn prepare(
        &mut self,
        active: &Active,
        channel: Channel,
        control: Arc<Control>,
    ) -> std::result::Result<Self::Prepared, PreparationError>;
    fn run(
        prepared: Self::Prepared,
        control: Arc<Control>,
    ) -> engine_ota::Result<Finished<Self::Ready>>;
    fn ready_valid(&mut self, proof: &Self::Ready) -> bool;
    fn admit(&mut self, token: &HoldToken, request: &DownloadReservation) -> Result<()>;
    fn renew(&mut self, token: &HoldToken, deadline_ms: u64) -> Result<()>;
    fn release(&mut self, token: &HoldToken) -> Result<()>;
}
struct Job<P> {
    token: HoldToken,
    active: Active,
    control: Arc<Control>,
    worker: Option<JoinHandle<engine_ota::Result<Finished<P>>>>,
    renew_ms: u64,
}
impl<P> Drop for Job<P> {
    fn drop(&mut self) {
        self.control.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Ready<P> {
    candidate: VerifiedCandidate,
    proof: P,
    time: UsableTime,
}
struct PreparationRetry {
    attempt_id: u64,
    after_ms: u64,
    epoch: u64,
    mode: Option<u64>,
}
struct Runner<B: Backend> {
    schedule: Schedule,
    backend: B,
    job: Option<Job<B::Ready>>,
    ready: Option<Ready<B::Ready>>,
    reason: Option<Reason>,
    admitting: Option<HoldToken>,
    preparation_retry: Option<PreparationRetry>,
}
impl<B: Backend> Drop for Runner<B> {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let token = job.token.clone();
            drop(job); // Cancel and await IO before releasing connectivity.
            let _ = self.backend.release(&token);
        }
        if let Some(token) = self.admitting.take() {
            let _ = self.backend.release(&token);
        }
    }
}
impl<B: Backend> Runner<B> {
    fn open(path: &Path, backend: B, clock: &Clock) -> Result<Self> {
        Ok(Self {
            schedule: Schedule::open(path, clock)?,
            backend,
            job: None,
            ready: None,
            reason: None,
            admitting: None,
            preparation_retry: None,
        })
    }
    fn step(&mut self, requests: &mpsc::Receiver<Command>, handle: &Handle) -> Result<()> {
        {
            let mut shared = handle.shared.lock().map_err(|_| unavailable())?;
            if shared.installation.is_some() {
                // Preserve the claimed snapshot while continuing the heartbeat.
                // Expiry and trusted time are checked by the install session.
                shared.observed = self.backend.clock()?;
                return Ok(());
            }
            shared.selecting = true;
        }
        let _selection = Selecting(handle.shared.clone());
        let mut offer = None;
        let mut manual = None;
        for _ in 0..8 {
            match requests.try_recv() {
                Ok(Command::Check {
                    received,
                    selection,
                }) => {
                    handle
                        .shared
                        .lock()
                        .map_err(|_| unavailable())?
                        .manual_pending = None;
                    manual = Some((received, selection));
                }
                Ok(Command::Select(channel)) => {
                    let result = self
                        .backend
                        .clock()
                        .and_then(|now| self.schedule.select(&now, channel));
                    let mut shared = handle.shared.lock().map_err(|_| unavailable())?;
                    shared.selections_pending = shared.selections_pending.saturating_sub(1);
                    shared.status.schedule = Some(result?);
                    if let Some(job) = &self.job {
                        job.control.cancel();
                    }
                    self.ready = None;
                    self.preparation_retry = None;
                    self.reason = None;
                }
                Ok(Command::Offer(request)) => {
                    offer = Some(request);
                }
                Err(_) => break,
            }
        }
        let mut now = self.backend.clock()?;
        // Consume timestamped payload progress before checking preparation expiry.
        if let Some(job) = &self.job {
            let payload = job
                .control
                .live
                .lock()
                .map_err(|_| unavailable())?
                .payload_at
                .clone();
            if let Some(payload) = payload {
                self.schedule
                    .payload_started(&now, job.active.id, &payload)?;
            }
        }
        self.finish_job(&now)?;
        if let Some(job) = &self.job {
            if let Some(offer) = offer.take() {
                let _ = offer.reply.try_send(job.control.route().is_ok());
            }
            if !job.control.active() {
                self.reason = Some(Reason::Interrupted);
            }
        }
        if self.job.is_none() {
            if let Some((received, selection)) = manual {
                // A changed channel may legitimately invalidate a queued request.
                if selection == self.schedule.status(&now)?.selection_generation
                    && received.boot_id == now.boot_id
                    && received.ms <= now.ms
                {
                    self.schedule.check_now(&now, &received, selection)?;
                }
            }
            let network = self.backend.network(&now);
            let opportunity = if offer.as_ref().is_some_and(|o| o.valid(&now)) {
                Opportunity::TimeCheck
            } else if network.as_ref().is_some_and(connected) {
                Opportunity::Connected
            } else {
                Opportunity::None
            };
            let partial = network
                .as_ref()
                .is_none_or(|n| n.status.policy.policy.partial_offline);
            let status = self.schedule.automatic(&now, partial, opportunity)?;
            if self.preparation_retry.as_ref().is_some_and(|retry| {
                status
                    .active
                    .as_ref()
                    .is_none_or(|a| a.id != retry.attempt_id)
            }) {
                self.preparation_retry = None;
            }
            if let Some(active) = status.active.clone() {
                self.ready = None;
                if self.preparation_retry.is_none() {
                    self.reason = None;
                }
                if let Some(network) = network {
                    let changed = self.preparation_retry.as_ref().is_some_and(|retry| {
                        network.epoch != retry.epoch
                            || retry
                                .mode
                                .is_some_and(|m| status_of(&network).mode_generation != m)
                    });
                    if changed {
                        self.schedule
                            .finish(&now, active.id, Completion::Cancelled)?;
                        self.preparation_retry = None;
                        self.reason = Some(Reason::Interrupted);
                    } else if self
                        .preparation_retry
                        .as_ref()
                        .is_none_or(|r| now.ms >= r.after_ms)
                        && network
                            .status
                            .actual
                            .as_ref()
                            .is_some_and(|radio| !radio.startup)
                        && (active.trigger == schedule::Trigger::Manual
                            || !partial
                            || opportunity != Opportunity::None)
                    {
                        // Policy can be known before NM supplies its first
                        // usable radio observation. Wait within this attempt's
                        // original deadline before preparing or requesting a hold.
                        // A previous time offer is not authority for a later
                        // partial-offline wake. Require a fresh opportunity.
                        self.start(active, &status, &network, offer.as_ref(), handle)?;
                    }
                }
            }
        }
        now = self.backend.clock()?;
        let status = self.schedule.automatic(&now, true, Opportunity::None)?;
        if status.active.is_none() {
            self.preparation_retry = None;
        }
        if let Some(job) = self.job.as_mut() {
            if status.active.as_ref().is_none_or(|a| a.id != job.active.id)
                || job.control.route().is_err()
            {
                job.control.cancel();
            }
            if job.control.active() && now.ms >= job.renew_ms {
                job.renew_ms = now.ms.saturating_add(30_000);
                if self
                    .backend
                    .renew(&job.token, job.active.deadline_ms)
                    .is_ok()
                {
                    if let Ok(mut live) = job.control.live.lock()
                        && let Some(lease) = live.lease.as_mut()
                    {
                        lease.until_ms = now.ms.saturating_add(120_000).min(job.active.deadline_ms);
                    }
                } else {
                    job.control.cancel();
                }
            }
        }
        if let Some(ready) = &self.ready
            && !self.backend.ready_valid(&ready.proof)
        {
            self.ready = None;
            self.reason = Some(Reason::StaleReady);
        }
        self.publish(handle)?;
        Ok(())
    }

    fn start(
        &mut self,
        active: Active,
        status: &schedule::Status,
        network: &Snapshot,
        offer: Option<&Offer>,
        handle: &Handle,
    ) -> Result<()> {
        let started = Clock {
            boot_id: self.backend.clock()?.boot_id,
            ms: active.started_ms,
        };
        let control = Arc::new(Control::new(
            handle.inputs.clone(),
            started,
            active.deadline_ms,
            handle.stop.clone(),
        ));
        handle.shared.lock().map_err(|_| unavailable())?.control = Arc::downgrade(&control);
        if handle
            .shared
            .lock()
            .map_err(|_| unavailable())?
            .selections_pending
            > 0
        {
            self.schedule
                .finish(&self.backend.clock()?, active.id, Completion::Cancelled)?;
            return Ok(());
        }
        publish(
            handle,
            &self.backend.clock()?,
            Some(status.clone()),
            Phase::Preparing,
            None,
            None,
            None,
        );
        let prepared = match self
            .backend
            .prepare(&active, status.channel, control.clone())
        {
            Ok(prepared) => prepared,
            Err(PreparationError::MaintenanceBusy) => {
                if self.preparation_retry.is_none() {
                    tracing::info!(
                        attempt_id = active.id,
                        stage = "install-lock",
                        "update preparation waiting for maintenance"
                    );
                }
                self.preparation_retry = Some(PreparationRetry {
                    attempt_id: active.id,
                    after_ms: self.backend.clock()?.ms.saturating_add(1000),
                    epoch: network.epoch,
                    mode: (active.trigger == schedule::Trigger::Automatic)
                        .then_some(status_of(network).mode_generation),
                });
                self.reason = Some(Reason::MaintenanceBusy);
                // Keep the persisted attempt and all original deadlines. No
                // download hold or HTTP session exists while maintenance owns
                // the lock; any offered NTP handoff can finish independently.
                return Ok(());
            }
            Err(PreparationError::Failed {
                stage,
                category,
                os_error,
            }) => {
                tracing::warn!(
                    attempt_id = active.id,
                    stage,
                    category,
                    os_error,
                    "update preparation failed"
                );
                self.preparation_retry = None;
                self.schedule
                    .finish(&self.backend.clock()?, active.id, Completion::Failed)?;
                self.reason = Some(Reason::PreparationUnavailable);
                return Ok(());
            }
        };
        if self.preparation_retry.take().is_some() {
            tracing::info!(
                attempt_id = active.id,
                "update preparation resumed after maintenance"
            );
        }
        self.reason = None;
        let before = self.backend.clock()?;
        if !control.active() || offer.is_some_and(|o| !o.valid(&before)) {
            self.schedule
                .finish(&before, active.id, Completion::Cancelled)?;
            return Ok(());
        }
        let token = HoldToken::generate()?;
        self.admitting = Some(token.clone());
        let automatic = active.trigger == schedule::Trigger::Automatic;
        let request = DownloadReservation {
            deadline: Clock {
                boot_id: before.boot_id.clone(),
                ms: active.deadline_ms,
            },
            admission_deadline_ms: active.admission_deadline_ms,
            mode_generation: automatic.then_some(status_of(network).mode_generation),
            time_hold: if automatic {
                offer.map(|o| o.parent.clone())
            } else {
                None
            },
            handoff_deadline_ms: if automatic {
                offer.map(|o| o.until.ms)
            } else {
                None
            },
        };
        let admitted = self.backend.admit(&token, &request);
        let now = self.backend.clock()?;
        let latest = self.backend.network(&now);
        if admitted.is_err()
            || now.ms >= active.admission_deadline_ms
            || !control.active()
            || offer.is_some_and(|o| !o.valid(&now))
            || latest.as_ref().is_none_or(|n| {
                n.epoch != network.epoch
                    || !available_policy(n)
                    || request
                        .mode_generation
                        .is_some_and(|m| status_of(n).mode_generation != m)
            })
        {
            let _ = self.backend.release(&token);
            self.admitting = None;
            self.schedule.finish(&now, active.id, Completion::Failed)?;
            self.reason = Some(Reason::NetworkUnavailable);
            return Ok(());
        }
        if !self.schedule.admitted(&now, active.id)? {
            let _ = self.backend.release(&token);
            self.admitting = None;
            return Ok(());
        }
        control.live.lock().map_err(|_| unavailable())?.lease = Some(Lease {
            epoch: network.epoch,
            mode: request.mode_generation,
            until_ms: before.ms.saturating_add(120_000).min(active.deadline_ms),
        });
        let work = control.clone();
        let worker = thread::spawn(move || B::run(prepared, work));
        self.job = Some(Job {
            token,
            active,
            control,
            worker: Some(worker),
            renew_ms: now.ms.saturating_add(30_000),
        });
        self.admitting = None;
        if let Some(offer) = offer {
            let _ = offer.reply.try_send(true);
        }
        Ok(())
    }

    fn finish_job(&mut self, now: &Clock) -> Result<()> {
        if !self
            .job
            .as_ref()
            .is_some_and(|j| j.worker.as_ref().is_some_and(|w| w.is_finished()))
        {
            return Ok(());
        }
        let mut job = self.job.take().expect("finished job");
        let finished = job
            .worker
            .take()
            .expect("worker")
            .join()
            .unwrap_or_else(|_| Err(interrupted()));
        let cancelled = !job.control.active();
        let completion = if cancelled {
            Completion::Cancelled
        } else {
            match &finished {
                Ok(Finished {
                    outcome: download::Outcome::Ready { .. },
                    ..
                }) => Completion::Ready,
                Ok(Finished {
                    outcome: download::Outcome::NoCandidate,
                    ..
                }) => Completion::NoCandidate,
                Ok(Finished {
                    outcome: download::Outcome::Ineligible { .. },
                    ..
                }) => Completion::Ineligible,
                Err(_) => Completion::Failed,
            }
        };
        let accepted = self.schedule.finish(now, job.active.id, completion);
        let _ = self.backend.release(&job.token);
        if accepted? && !cancelled {
            match finished {
                Ok(Finished {
                    outcome: download::Outcome::Ready { candidate },
                    proof: Some(proof),
                    time,
                }) => {
                    self.ready = Some(Ready {
                        candidate,
                        proof,
                        time,
                    });
                }
                Err(_) => self.reason = Some(Reason::DownloadFailed),
                _ => {}
            }
        }
        Ok(())
    }

    fn publish(&mut self, handle: &Handle) -> Result<()> {
        let now = self.backend.clock()?;
        let status = self.schedule.status(&now)?;
        let (phase, progress) = if let Some(job) = &self.job {
            let progress = job.control.live.lock().map_err(|_| unavailable())?.progress;
            let phase = if !job.control.active() {
                Phase::Stopping
            } else if let Some(p) = progress {
                match p.phase {
                    download::Phase::Checking => Phase::Checking,
                    download::Phase::Downloading => Phase::Downloading,
                    download::Phase::Verifying | download::Phase::Ready => Phase::Verifying,
                }
            } else if !job.control.route().unwrap_or(false) {
                Phase::WaitingForNetwork
            } else if self.backend.time().is_none() {
                Phase::WaitingForTime
            } else {
                Phase::Preparing
            };
            (phase, progress)
        } else if self.ready.is_some() {
            (Phase::Ready, None)
        } else if status.active.is_some() {
            (
                if self.preparation_retry.is_some() {
                    Phase::Preparing
                } else {
                    Phase::WaitingForNetwork
                },
                None,
            )
        } else {
            (Phase::Idle, None)
        };
        publish(
            handle,
            &now,
            Some(status),
            phase,
            self.reason,
            progress,
            self.ready.as_ref().map(|r| (r.candidate.clone(), r.time)),
        );
        Ok(())
    }
}

fn status_of(network: &Snapshot) -> &crate::network_policy::Status {
    &network.status.policy
}
fn available_policy(network: &Snapshot) -> bool {
    network.status.pending.is_none()
        && network.status.suspended.is_none()
        && network.status.deferred.is_none()
        && network
            .status
            .actual
            .as_ref()
            .is_some_and(|r| r.networking_enabled && !r.startup)
}
fn connected(network: &Snapshot) -> bool {
    available_policy(network)
        && network
            .status
            .actual
            .as_ref()
            .is_some_and(|r| r.ethernet_available || r.wireless_available)
}
fn publish(
    handle: &Handle,
    now: &Clock,
    schedule: Option<schedule::Status>,
    phase: Phase,
    reason: Option<Reason>,
    progress: Option<Progress>,
    ready: Option<(VerifiedCandidate, UsableTime)>,
) {
    if let Ok(mut shared) = handle.shared.lock() {
        shared.observed = now.clone();
        shared.status = Status {
            phase,
            reason,
            schedule,
            progress,
            ready: ready.as_ref().map(|r| r.0.image().clone()),
            manual_pending: shared.manual_pending.is_some(),
        };
        shared.ready = ready;
    }
}
fn unavailable() -> Error {
    Error::Invalid("update worker unavailable".into())
}
fn interrupted() -> engine_ota::Error {
    engine_ota::Error::Download("download session interrupted")
}

#[cfg(test)]
#[path = "update_runtime_tests.rs"]
mod tests;
