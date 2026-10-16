//! Device-owned time-check worker. The status/time observer and radio coordinator
//! remain independent; slow network/provider operations never run on their threads.
use crate::network_policy::{Clock, HoldToken};
use crate::network_service::{self, Request};
use crate::network_watch::{Snapshot, Watch};
use crate::service::SharedState;
use crate::time_query::{self, Outcome};
use crate::time_schedule::{self, Active, Completion, Connection, Evidence, Schedule, Trigger};
use crate::{Error, Result, linux};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const STATE_DIRECTORY: &str = "/data/spacewars-system/time-check";
type ReadClock = Arc<dyn Fn() -> Result<Clock> + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Starting,
    Idle,
    Admitting,
    Connecting,
    Querying,
    WaitingForSync,
    Releasing,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    StateUnavailable,
    NetworkUnavailable,
    ProviderUnavailable,
    WorkerStalled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub phase: Phase,
    pub reason: Option<Reason>,
    pub schedule: Option<time_schedule::Status>,
    pub manual_pending: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub schema: u32,
    pub received_ms: u64,
    pub coalesced: bool,
}

struct Shared {
    status: Status,
    observed: Option<Clock>,
    pending: Option<Clock>,
}

#[derive(Clone)]
pub struct Handle {
    shared: Arc<Mutex<Shared>>,
    clock: ReadClock,
    stop: Arc<AtomicBool>,
}

impl Handle {
    fn new(clock: ReadClock) -> Self {
        let observed = clock().ok();
        Self {
            shared: Arc::new(Mutex::new(Shared {
                status: Status {
                    phase: Phase::Starting,
                    reason: None,
                    schedule: None,
                    manual_pending: false,
                },
                observed,
                pending: None,
            })),
            clock,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> Result<Status> {
        let now = (self.clock)()?;
        let shared = self.shared.lock().map_err(|_| unavailable())?;
        let mut status = shared.status.clone();
        status.manual_pending = shared.pending.is_some();
        if self.stop.load(Ordering::Relaxed)
            || shared.observed.as_ref().is_some_and(|last| {
                last.boot_id != now.boot_id
                    || now.ms.checked_sub(last.ms).is_none_or(|age| age > 15_000)
            })
        {
            status.phase = Phase::Unavailable;
            status.reason = Some(Reason::WorkerStalled);
        }
        Ok(status)
    }

    /// The authenticated socket calls this only for an explicit Sync now.
    /// One pending timestamp is shared by all callers; reading status never
    /// reaches this path. Accepted means queued/coalesced, not synchronized.
    pub fn sync_now(&self) -> Result<Accepted> {
        let now = (self.clock)()?;
        if self.status()?.phase == Phase::Unavailable {
            return Err(unavailable());
        }
        let mut shared = self.shared.lock().map_err(|_| unavailable())?;
        if self.stop.load(Ordering::Relaxed) {
            return Err(unavailable());
        }
        if let Some(active) = shared
            .status
            .schedule
            .as_ref()
            .and_then(|s| s.active.as_ref())
            .filter(|a| now.ms < a.deadline_ms)
        {
            return Ok(Accepted {
                schema: 1,
                received_ms: active.started_ms,
                coalesced: true,
            });
        }
        if let Some(pending) = &shared.pending {
            return Ok(Accepted {
                schema: 1,
                received_ms: pending.ms,
                coalesced: true,
            });
        }
        shared.pending = Some(now.clone());
        Ok(Accepted {
            schema: 1,
            received_ms: now.ms,
            coalesced: false,
        })
    }

    fn take_request(&self) -> Option<Clock> {
        self.shared.lock().ok()?.pending.take()
    }

    fn publish(
        &self,
        clock: &Clock,
        schedule: Option<time_schedule::Status>,
        phase: Phase,
        reason: Option<Reason>,
    ) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.observed = Some(clock.clone());
            shared.status = Status {
                phase,
                reason,
                schedule,
                manual_pending: shared.pending.is_some(),
            };
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
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn spawn(time: SharedState, network: Watch, boot_id: String) -> Runtime {
    let clock: ReadClock = Arc::new(move || {
        Ok(Clock {
            boot_id: boot_id.clone(),
            ms: linux::clock_reading()?.boottime_us / 1000,
        })
    });
    spawn_with(
        PathBuf::from(STATE_DIRECTORY),
        Native {
            time,
            network,
            clock: clock.clone(),
            socket: network_service::SOCKET_PATH.into(),
        },
        clock,
    )
}

fn spawn_with<B: Backend + Clone + Send + 'static>(
    directory: PathBuf,
    backend: B,
    clock: ReadClock,
) -> Runtime {
    let handle = Handle::new(clock);
    let worker_handle = handle.clone();
    let worker = thread::spawn(move || {
        let mut runner: Option<Runner<B>> = None;
        let mut retry = Instant::now();
        while !worker_handle.stop.load(Ordering::Relaxed) {
            let Ok(now) = (worker_handle.clock)() else {
                break;
            };
            if runner.is_none() && Instant::now() >= retry {
                match Runner::open(&directory, backend.clone(), &now) {
                    Ok(started) => runner = Some(started),
                    Err(_) => {
                        worker_handle.publish(
                            &now,
                            None,
                            Phase::Unavailable,
                            Some(Reason::StateUnavailable),
                        );
                        retry = Instant::now() + Duration::from_secs(5);
                    }
                }
            }
            if let Some(active) = runner.as_mut() {
                let pending = worker_handle.take_request();
                if active.step(pending.as_ref(), &worker_handle).is_err() {
                    worker_handle.publish(
                        &now,
                        None,
                        Phase::Unavailable,
                        Some(Reason::StateUnavailable),
                    );
                    runner = None; // Drop releases only this worker's hold.
                    retry = Instant::now() + Duration::from_secs(5);
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        drop(runner);
        worker_handle.stop.store(true, Ordering::Relaxed);
    });
    Runtime {
        handle,
        worker: Some(worker),
    }
}

#[derive(Clone)]
struct Context {
    active: Active,
    mode_generation: u64,
    epoch: u64,
}
impl Context {
    fn valid(&self, clock: &Clock, network: &Snapshot) -> bool {
        clock.ms < self.active.deadline_ms
            && network.epoch == self.epoch
            && (self.active.trigger == Trigger::Manual
                || (network.status.policy.policy.partial_offline
                    && network.status.policy.mode_generation == self.mode_generation))
            && network.status.pending.is_none()
            && network.status.suspended.is_none()
            && network.status.deferred.is_none()
            && network
                .status
                .actual
                .as_ref()
                .is_some_and(|radio| radio.networking_enabled && !radio.startup)
    }
}

trait Backend {
    fn clock(&self) -> Result<Clock>;
    fn network(&self, now: &Clock) -> Option<Snapshot>;
    fn evidence(&self, now: &Clock) -> Option<Evidence>;
    fn admit(&mut self, token: &HoldToken, active: &Active, mode: u64) -> Result<()>;
    fn fallback(&mut self, token: &HoldToken, active: &Active, mode: u64) -> Result<()>;
    fn release(&mut self, token: &HoldToken) -> Result<()>;
    fn query(
        &mut self,
        schedule: &mut Schedule,
        context: &Context,
        stop: Arc<AtomicBool>,
    ) -> Outcome;
}

struct Held {
    token: HoldToken,
    id: u64,
    epoch: Option<u64>,
    escalated: bool,
    retry_ms: u64,
}
struct Runner<B: Backend> {
    schedule: Schedule,
    backend: B,
    held: Option<Held>,
}
impl<B: Backend> Drop for Runner<B> {
    fn drop(&mut self) {
        if let Some(held) = self.held.take() {
            let _ = self.backend.release(&held.token);
        }
    }
}

impl<B: Backend> Runner<B> {
    fn open(directory: &Path, backend: B, clock: &Clock) -> Result<Self> {
        Ok(Self {
            schedule: Schedule::open(directory, clock)?,
            backend,
            held: None,
        })
    }

    fn observe(
        &mut self,
        pending: Option<&Clock>,
    ) -> Result<(Clock, time_schedule::Status, Option<Snapshot>)> {
        let now = self.backend.clock()?;
        let network = self.backend.network(&now);
        let evidence = self.backend.evidence(&now);
        let now = self.backend.clock()?;
        if let Some(received) = pending {
            self.schedule
                .sync_received(&now, received, evidence.as_ref())?;
        }
        let last = self.schedule.status(&now)?;
        let (enabled, mode) = network
            .as_ref()
            .map_or((last.enabled, last.mode_generation), |n| {
                (
                    n.status.policy.policy.partial_offline,
                    n.status.policy.mode_generation,
                )
            });
        let status = self
            .schedule
            .observe_policy(&now, enabled, mode, evidence.as_ref())?;
        Ok((now, status, network))
    }

    fn release_finished(&mut self, now: &Clock, status: &time_schedule::Status, handle: &Handle) {
        if self
            .held
            .as_ref()
            .is_some_and(|h| status.active.as_ref().is_none_or(|a| a.id != h.id))
        {
            handle.publish(now, Some(status.clone()), Phase::Releasing, None);
            if let Some(held) = self.held.take() {
                let _ = self.backend.release(&held.token);
            }
        }
    }

    fn step(&mut self, pending: Option<&Clock>, handle: &Handle) -> Result<()> {
        let (mut now, mut status, mut network) = self.observe(pending)?;
        self.release_finished(&now, &status, handle);
        let Some(mut active) = status.active.clone() else {
            handle.publish(&now, Some(status), Phase::Idle, None);
            return Ok(());
        };
        let Some(initial) = network.as_ref() else {
            handle.publish(
                &now,
                Some(status),
                Phase::Admitting,
                Some(Reason::NetworkUnavailable),
            );
            return Ok(());
        };
        if self.held.is_none() {
            self.held = Some(Held {
                token: HoldToken::generate()?,
                id: active.id,
                epoch: None,
                escalated: false,
                retry_ms: 0,
            });
        }
        let held = self.held.as_mut().expect("allocated hold");
        if held.epoch != Some(initial.epoch) && now.ms >= held.retry_ms {
            handle.publish(&now, Some(status.clone()), Phase::Admitting, None);
            held.retry_ms = now.ms.saturating_add(1000);
            if self
                .backend
                .admit(&held.token, &active, status.mode_generation)
                .is_ok()
            {
                let current = self.backend.clock()?;
                if self.schedule.admitted(&current, active.id)? {
                    held.epoch = Some(initial.epoch);
                    held.escalated = false;
                }
            }
            (now, status, network) = self.observe(None)?;
            self.release_finished(&now, &status, handle);
            let Some(current) = status.active.as_ref().filter(|a| a.id == active.id) else {
                handle.publish(&now, Some(status), Phase::Idle, None);
                return Ok(());
            };
            active = current.clone();
        }
        let Some(view) = network else {
            handle.publish(
                &now,
                Some(status),
                Phase::Admitting,
                Some(Reason::NetworkUnavailable),
            );
            return Ok(());
        };
        let Some(held) = self.held.as_mut() else {
            return Ok(());
        };
        if held.epoch != Some(view.epoch) {
            handle.publish(
                &now,
                Some(status),
                Phase::Admitting,
                Some(Reason::NetworkUnavailable),
            );
            return Ok(());
        }
        let context = Context {
            active: active.clone(),
            mode_generation: status.mode_generation,
            epoch: view.epoch,
        };
        if !context.valid(&now, &view) {
            handle.publish(
                &now,
                Some(status),
                Phase::Connecting,
                Some(Reason::NetworkUnavailable),
            );
            return Ok(());
        }
        if active.wireless_required && !held.escalated && now.ms >= held.retry_ms {
            held.retry_ms = now.ms.saturating_add(1000);
            held.escalated = self
                .backend
                .fallback(&held.token, &active, status.mode_generation)
                .is_ok();
            // Refresh policy, identity and accepted time before any subsequent
            // operation; a radio reply does not prove the attempt is still live.
            let (now, status, _) = self.observe(None)?;
            self.release_finished(&now, &status, handle);
            let phase = if status.active.is_some() {
                Phase::Connecting
            } else {
                Phase::Idle
            };
            handle.publish(&now, Some(status), phase, None);
            return Ok(());
        }
        let radio = view.status.actual.as_ref().expect("guard requires radio");
        let connection = if radio.wireless_available {
            Some(Connection::Wireless)
        } else if radio.ethernet_available {
            Some(Connection::Ethernet)
        } else {
            None
        };
        let Some(connection) = connection else {
            handle.publish(&now, Some(status), Phase::Connecting, None);
            return Ok(());
        };
        self.schedule.connected(&now, active.id, connection)?;
        status = self.schedule.status(&now)?;
        active = status.active.clone().expect("connected live attempt");
        if !active.query_requested {
            handle.publish(&now, Some(status), Phase::Querying, None);
            let context = Context { active, ..context };
            let outcome = self
                .backend
                .query(&mut self.schedule, &context, handle.stop.clone());
            (now, status, network) = self.observe(None)?;
            if outcome == Outcome::Unavailable
                && !handle.stop.load(Ordering::Relaxed)
                && network.as_ref().is_some_and(|n| context.valid(&now, n))
                && status
                    .active
                    .as_ref()
                    .is_some_and(|a| a.id == context.active.id)
            {
                self.schedule
                    .fail(&now, context.active.id, Completion::ProviderUnavailable)?;
                status = self.schedule.status(&now)?;
            }
            self.release_finished(&now, &status, handle);
        }
        let phase = match status.active.as_ref() {
            Some(a) if a.query_requested => Phase::WaitingForSync,
            Some(_) => Phase::Admitting,
            None => Phase::Idle,
        };
        handle.publish(&now, Some(status), phase, None);
        Ok(())
    }
}

#[derive(Clone)]
struct Native {
    time: SharedState,
    network: Watch,
    clock: ReadClock,
    socket: PathBuf,
}
impl Native {
    fn operation(&self, request: Request, deadline_ms: Option<u64>) -> Result<()> {
        let now = self.clock()?;
        let ms = deadline_ms.map_or(5000, |end| end.saturating_sub(now.ms).min(5000));
        if ms == 0 {
            return Err(unavailable());
        }
        let reply = network_service::request_before(
            &self.socket,
            &request,
            0,
            Instant::now() + Duration::from_millis(ms),
        )?;
        let after = self.clock()?;
        if !reply.get("error").is_some_and(serde_json::Value::is_null)
            || !reply
                .get("status")
                .is_some_and(serde_json::Value::is_object)
            || deadline_ms.is_some_and(|end| after.ms >= end)
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
impl Backend for Native {
    fn clock(&self) -> Result<Clock> {
        (self.clock)()
    }
    fn network(&self, now: &Clock) -> Option<Snapshot> {
        self.network.read(now)
    }
    fn evidence(&self, now: &Clock) -> Option<Evidence> {
        let shared = self.time.lock().ok()?;
        if shared.boot_id != now.boot_id
            || shared.persistence_error.is_some()
            || shared.observed_at.elapsed() >= Duration::from_secs(15)
        {
            return None;
        }
        Evidence::accepted(&shared.tracker, linux::clock_reading().ok()?)
    }
    fn admit(&mut self, token: &HoldToken, active: &Active, mode: u64) -> Result<()> {
        let clock = self.clock()?;
        let admission = if !active.admitted {
            active.admission_deadline_ms
        } else {
            active.deadline_ms
        };
        self.operation(
            Request::AdmitTime {
                schema: 1,
                token: token.expose().into(),
                deadline: Clock {
                    boot_id: clock.boot_id,
                    ms: active.deadline_ms,
                },
                admission_deadline_ms: admission,
                mode_generation: (active.trigger == Trigger::Automatic).then_some(mode),
                wireless_required: false,
            },
            Some(admission),
        )
    }
    fn fallback(&mut self, token: &HoldToken, active: &Active, mode: u64) -> Result<()> {
        let clock = self.clock()?;
        self.operation(
            Request::AdmitTime {
                schema: 1,
                token: token.expose().into(),
                deadline: Clock {
                    boot_id: clock.boot_id,
                    ms: active.deadline_ms,
                },
                admission_deadline_ms: active.deadline_ms,
                mode_generation: (active.trigger == Trigger::Automatic).then_some(mode),
                wireless_required: true,
            },
            Some(active.deadline_ms),
        )
    }
    fn release(&mut self, token: &HoldToken) -> Result<()> {
        self.operation(
            Request::Release {
                schema: 1,
                token: token.expose().into(),
            },
            None,
        )
    }
    fn query(
        &mut self,
        schedule: &mut Schedule,
        context: &Context,
        stop: Arc<AtomicBool>,
    ) -> Outcome {
        time_query::request(schedule, context.active.id, || {
            let now = self.clock()?;
            let network = self.network(&now).ok_or_else(unavailable)?;
            if stop.load(Ordering::Relaxed) || !context.valid(&now, &network) {
                return Err(unavailable());
            }
            if !network
                .status
                .actual
                .as_ref()
                .is_some_and(|radio| radio.ethernet_available || radio.wireless_available)
            {
                return Err(unavailable());
            }
            Ok(now)
        })
    }
}

fn unavailable() -> Error {
    Error::Invalid("time-check worker unavailable".into())
}

#[cfg(test)]
#[path = "time_runtime_tests.rs"]
mod tests;
