//! Bounded, authenticated network requests handled by one radio coordinator.
//! The independent time/status service does not wait for these operations.
use crate::network_access::Access;
use crate::network_controller::{Availability, Backend, Controller, Status};
use crate::network_policy::{Caller, Clock, DownloadReservation, HoldKind, HoldToken, Policy};
use crate::network_watch::Watch;
use crate::{Error, Result, kiosk_health, service};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const SOCKET_PATH: &str = "/run/spacewars-system/network.sock";
pub const STATE_DIRECTORY: &str = "/data/spacewars-system/network";
const CLIENTS: usize = 8;
const FRAME_LIMIT: usize = 1024;
const FRAME_DEADLINE: Duration = Duration::from_secs(5);
const OPERATION_DEADLINE: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_secs(2);

// Do not derive Debug: these requests contain the caller's secret capability.
#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Status {
        schema: u32,
    },
    /// Root socket-activation helper: accepted connection protection, no lease.
    ProtectSsh {
        schema: u32,
    },
    SetPolicy {
        schema: u32,
        generation: u64,
        policy: Policy,
    },
    Resume {
        schema: u32,
        generation: u64,
    },
    Admit {
        schema: u32,
        token: String,
        kind: HoldKind,
    },
    AdmitTime {
        schema: u32,
        token: String,
        deadline: Clock,
        admission_deadline_ms: u64,
        mode_generation: Option<u64>,
        #[serde(default)]
        wireless_required: bool,
    },
    AdmitDownload {
        schema: u32,
        token: String,
        deadline: Clock,
        admission_deadline_ms: u64,
        mode_generation: Option<u64>,
        time_hold: Option<String>,
        handoff_deadline_ms: Option<u64>,
    },
    Renew {
        schema: u32,
        token: String,
    },
    RequireWireless {
        schema: u32,
        token: String,
    },
    Release {
        schema: u32,
        token: String,
    },
}

impl Request {
    fn supported(&self) -> bool {
        let (Self::Status { schema }
        | Self::ProtectSsh { schema }
        | Self::SetPolicy { schema, .. }
        | Self::Resume { schema, .. }
        | Self::Admit { schema, .. }
        | Self::AdmitTime { schema, .. }
        | Self::AdmitDownload { schema, .. }
        | Self::Renew { schema, .. }
        | Self::RequireWireless { schema, .. }
        | Self::Release { schema, .. }) = self;
        *schema == 1
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rejection {
    InvalidRequest,
    Busy,
    Expired,
    OwnerGone,
    Unavailable,
    Rejected,
}

#[derive(Debug, Serialize)]
pub struct Reply {
    pub schema: u32,
    pub status: Option<Status>,
    pub availability: Option<Availability>,
    pub error: Option<Rejection>,
}

impl Reply {
    fn status(status: Status) -> Self {
        Self {
            schema: 1,
            status: Some(status),
            availability: None,
            error: None,
        }
    }
    fn reject(error: Rejection) -> Self {
        Self {
            schema: 1,
            status: None,
            availability: None,
            error: Some(error),
        }
    }
}

struct Work {
    request: Request,
    caller: Caller,
    deadline: Instant,
    reply: SyncSender<Reply>,
}

pub struct NetworkServer {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    coordinator: Option<JoinHandle<()>>,
}

impl NetworkServer {
    pub(crate) fn running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
            && self
                .accept
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
            && self
                .coordinator
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }

    /// The root-owned parent and exclusive policy store are prepared by the
    /// device service. This never unlinks an existing socket or initializes state.
    pub fn bind<B: Backend + Send + 'static>(
        path: &Path,
        kiosk_uid: u32,
        controller: Controller<B>,
    ) -> Result<Self> {
        Self::bind_watched(path, kiosk_uid, controller, Watch::default())
    }

    pub(crate) fn bind_watched<B: Backend + Send + 'static>(
        path: &Path,
        kiosk_uid: u32,
        controller: Controller<B>,
        watch: Watch,
    ) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(CLIENTS);
        let access = controller.access();
        let waiting = Arc::new(AtomicUsize::new(0));
        let stopped = stop.clone();
        watch.reset();
        let coordinator = thread::spawn(move || coordinate(controller, rx, &stopped, watch));
        let stopped = stop.clone();
        let accept = thread::spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let mut index = 0;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let _ = workers.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        if workers.len() >= CLIENTS {
                            continue;
                        }
                        let Ok(caller) = peer(&stream, kiosk_uid) else {
                            continue;
                        };
                        let tx = tx.clone();
                        let access = access.clone();
                        let waiting = waiting.clone();
                        workers.push(thread::spawn(move || {
                            let _ = handle(stream, caller, tx, access, waiting);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            stop,
            accept: Some(accept),
            coordinator: Some(coordinator),
        })
    }
}

impl Drop for NetworkServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Drop queued reply senders before joining socket workers.
        if let Some(coordinator) = self.coordinator.take() {
            let _ = coordinator.join();
        }
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

fn peer(stream: &UnixStream, kiosk_uid: u32) -> Result<Caller> {
    let credentials = service::peer_credentials(stream)?;
    if !service::authorized(credentials.uid, kiosk_uid) || credentials.pid <= 0 {
        return Err(Error::Invalid("unauthorized network caller".into()));
    }
    let pid = credentials.pid as u32;
    Ok(Caller {
        uid: credentials.uid,
        pid,
        process_start_ticks: kiosk_health::process(pid, credentials.uid)?,
    })
}

struct CoordinatorGuard(Access);
impl Drop for CoordinatorGuard {
    fn drop(&mut self) {
        self.0.stop();
    }
}

fn coordinate<B: Backend>(
    mut controller: Controller<B>,
    rx: Receiver<Work>,
    stop: &AtomicBool,
    watch: Watch,
) {
    let access = controller.access();
    let _owner = CoordinatorGuard(access.clone());
    let mut next_tick = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_tick {
            // A failed observation is retried; the controller never treats it
            // as idle. Storage uncertainty remains poisoned inside the store.
            access.begin();
            let status = controller.tick().ok();
            access.finish(status.clone(), controller.ssh_blocked());
            watch.publish(status);
            next_tick = Instant::now() + TICK;
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(work) => {
                access.begin();
                let reply = if stop.load(Ordering::Relaxed) {
                    Reply::reject(Rejection::Unavailable)
                } else if Instant::now() >= work.deadline {
                    Reply::reject(Rejection::Expired)
                } else if !kiosk_health::process(work.caller.pid, work.caller.uid)
                    .is_ok_and(|start| start == work.caller.process_start_ticks)
                {
                    Reply::reject(Rejection::OwnerGone)
                } else {
                    if matches!(
                        work.request,
                        Request::SetPolicy { .. } | Request::Resume { .. }
                    ) {
                        // An old snapshot cannot authorize work while a policy
                        // mutation is in flight, including off/on toggle races.
                        watch.publish(None);
                    }
                    perform(&mut controller, work.caller, work.request)
                };
                access.finish(reply.status.clone(), controller.ssh_blocked());
                if let Some(status) = &reply.status {
                    watch.publish(Some(status.clone()));
                }
                let _ = work.reply.try_send(reply);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    watch.reset();
}

fn perform<B: Backend>(controller: &mut Controller<B>, caller: Caller, request: Request) -> Reply {
    let result = (|| -> Result<Reply> {
        Ok(match request {
            Request::ProtectSsh { .. } => Reply::reject(Rejection::InvalidRequest),
            Request::Status { .. } => controller
                .status()
                .map(Reply::status)
                .unwrap_or_else(|_| Reply::reject(Rejection::Unavailable)),
            Request::SetPolicy {
                generation, policy, ..
            } => Reply::status(controller.set_policy(generation, policy)?),
            Request::Resume { generation, .. } => Reply::status(controller.resume(generation)?),
            Request::Admit { token, kind, .. } => {
                let admission = controller.admit(&HoldToken::parse(token)?, caller, kind)?;
                Reply {
                    schema: 1,
                    status: Some(admission.status),
                    availability: Some(admission.availability),
                    error: None,
                }
            }
            Request::AdmitTime {
                token,
                deadline,
                admission_deadline_ms,
                mode_generation,
                wireless_required,
                ..
            } => {
                let admission = controller.admit_time(
                    &HoldToken::parse(token)?,
                    caller,
                    &deadline,
                    admission_deadline_ms,
                    mode_generation,
                    wireless_required,
                )?;
                Reply {
                    schema: 1,
                    status: Some(admission.status),
                    availability: Some(admission.availability),
                    error: None,
                }
            }
            Request::AdmitDownload {
                token,
                deadline,
                admission_deadline_ms,
                mode_generation,
                time_hold,
                handoff_deadline_ms,
                ..
            } => {
                let admission = controller.admit_download(
                    &HoldToken::parse(token)?,
                    caller,
                    &DownloadReservation {
                        deadline,
                        admission_deadline_ms,
                        mode_generation,
                        time_hold: time_hold.map(HoldToken::parse).transpose()?,
                        handoff_deadline_ms,
                    },
                )?;
                Reply {
                    schema: 1,
                    status: Some(admission.status),
                    availability: Some(admission.availability),
                    error: None,
                }
            }
            Request::Renew { token, .. } => {
                let admission = controller.renew(&HoldToken::parse(token)?, caller)?;
                Reply {
                    schema: 1,
                    status: Some(admission.status),
                    availability: Some(admission.availability),
                    error: None,
                }
            }
            Request::RequireWireless { token, .. } => {
                let admission = controller.require_wireless(&HoldToken::parse(token)?, caller)?;
                Reply {
                    schema: 1,
                    status: Some(admission.status),
                    availability: Some(admission.availability),
                    error: None,
                }
            }
            Request::Release { token, .. } => {
                Reply::status(controller.release(&HoldToken::parse(token)?, caller)?)
            }
        })
    })();
    // Do not echo file names, arbitrary peer errors or capability-bearing input.
    match result {
        Ok(reply) => reply,
        Err(Error::Invalid(_)) => Reply::reject(Rejection::Rejected),
        Err(_) => Reply::reject(Rejection::Unavailable),
    }
}

// Leave socket-worker capacity for diagnostics and admission while durable
// operations wait. Dropping a connection does not cancel an accepted mutation.
struct Waiting(Arc<AtomicUsize>);
impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn handle(
    mut stream: UnixStream,
    caller: Caller,
    tx: SyncSender<Work>,
    access: Access,
    waiting: Arc<AtomicUsize>,
) -> Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = service::read_frame(&mut stream, FRAME_LIMIT, FRAME_DEADLINE)
        .and_then(|bytes| serde_json::from_slice::<Request>(&bytes).map_err(Error::from));
    let request = match request {
        Ok(request) if request.supported() => request,
        _ => return service::write_json(&mut stream, &Reply::reject(Rejection::InvalidRequest)),
    };
    if matches!(request, Request::ProtectSsh { .. }) {
        let reply = if caller.uid != 0 {
            Reply::reject(Rejection::Rejected)
        } else if access.protect_ssh() {
            Reply {
                schema: 1,
                status: access.status(),
                availability: Some(Availability::SessionProtected),
                error: None,
            }
        } else {
            Reply::reject(Rejection::Unavailable)
        };
        return service::write_json(&mut stream, &reply);
    }
    let diagnostic = matches!(request, Request::Status { .. });
    if diagnostic
        && access.busy()
        && let Some(status) = access.status()
    {
        return service::write_json(&mut stream, &Reply::status(status));
    }
    if waiting
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < CLIENTS / 2).then_some(count + 1)
        })
        .is_err()
    {
        return service::write_json(&mut stream, &Reply::reject(Rejection::Busy));
    }
    let _waiting = Waiting(waiting);
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    let deadline = Instant::now() + OPERATION_DEADLINE;
    match tx.try_send(Work {
        request,
        caller,
        deadline,
        reply: reply_tx,
    }) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => {
            return service::write_json(&mut stream, &Reply::reject(Rejection::Busy));
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            return service::write_json(&mut stream, &Reply::reject(Rejection::Unavailable));
        }
    }
    loop {
        let reply = match reply_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(reply) => reply,
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {
                if diagnostic && access.busy() {
                    if let Some(status) = access.status() {
                        return service::write_json(&mut stream, &Reply::status(status));
                    }
                    if deadline.saturating_duration_since(Instant::now())
                        < OPERATION_DEADLINE - Duration::from_millis(250)
                    {
                        return service::write_json(
                            &mut stream,
                            &Reply::reject(Rejection::Unavailable),
                        );
                    }
                }
                continue;
            }
            Err(_) => Reply::reject(Rejection::Unavailable),
        };
        return service::write_json(&mut stream, &reply);
    }
}

/// A lost response does not cancel a reservation. Retry Admit with the same
/// capability or inspect status; do not invent a new token for every retry.
pub fn request(path: &Path, request: &Request) -> Result<serde_json::Value> {
    request_from(path, request, 0)
}

fn request_from(path: &Path, request: &Request, server_uid: u32) -> Result<serde_json::Value> {
    request_before(
        path,
        request,
        server_uid,
        Instant::now() + OPERATION_DEADLINE + 2 * FRAME_DEADLINE + Duration::from_secs(2),
    )
}

pub(crate) fn request_before(
    path: &Path,
    request: &Request,
    server_uid: u32,
    deadline: Instant,
) -> Result<serde_json::Value> {
    let remaining = |limit: Duration| {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .map(|time| time.min(limit))
            .ok_or_else(|| Error::Invalid("network request deadline exceeded".into()))
    };
    let mut bytes = serde_json::to_vec(request)?;
    if !request.supported() || bytes.len() > FRAME_LIMIT {
        return Err(Error::Invalid("invalid network request".into()));
    }
    let connection_deadline = remaining(FRAME_DEADLINE)?;
    let mut stream = futures_lite::future::block_on(futures_lite::future::or(
        async { async_io::Async::<UnixStream>::connect(path).await },
        async {
            async_io::Timer::after(connection_deadline).await;
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "network socket connection timed out",
            ))
        },
    ))?
    .into_inner()?;
    stream.set_nonblocking(false)?;
    if service::peer_credentials(&stream)?.uid != server_uid {
        return Err(Error::Invalid("unexpected network service owner".into()));
    }
    stream.set_write_timeout(Some(remaining(Duration::from_secs(2))?))?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    let reply = service::read_frame(
        &mut stream,
        16 * 1024,
        remaining(OPERATION_DEADLINE + FRAME_DEADLINE)?,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&reply)?;
    if value.get("schema") != Some(&serde_json::json!(1)) {
        return Err(Error::Invalid("unsupported network reply".into()));
    }
    Ok(value)
}

#[cfg(test)]
#[path = "network_service_tests.rs"]
mod tests;
