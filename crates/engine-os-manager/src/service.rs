use crate::time::{TimeStatus, TimeTracker};
use crate::{Error, Result, linux};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_CLIENTS: usize = 8;
const MAX_REQUEST: usize = 1024;
const MAX_RESPONSE: usize = 16 * 1024;
const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceStatus {
    pub schema: u32,
    pub boot_id: String,
    pub service_healthy: bool,
    pub provider_error: Option<String>,
    pub persistence_error: Option<String>,
    pub rtc_error: Option<String>,
    pub time: TimeStatus,
    #[serde(default)]
    pub recovery: crate::recovery_status::RecoveryStatus,
    #[serde(default)]
    pub time_check: Option<crate::time_runtime::Status>,
    #[serde(default)]
    pub update: Option<crate::update_runtime::Status>,
    /// Absent while the saved preference is unreadable; installs then wait
    /// for confirmation.
    #[serde(default)]
    pub install_policy: Option<crate::install_policy::Status>,
    /// Whether an automatic attempt could start now. Nothing acts on it yet.
    #[serde(default)]
    pub install_admission: Option<crate::install_admission::Status>,
}

impl DeviceStatus {
    /// The reading other device decisions may use: usable time from a live
    /// observation whose time metadata is intact.
    pub fn usable_time(&self) -> Option<engine_ota::UsableTime> {
        (self.service_healthy && self.time.usable)
            .then_some(self.time.utc_us)
            .flatten()
            .and_then(|us| engine_ota::UsableTime::from_unix_microseconds(us).ok())
    }
}

pub struct ObservedState {
    pub tracker: TimeTracker,
    pub boot_id: String,
    pub provider_error: Option<String>,
    pub persistence_error: Option<String>,
    pub rtc_error: Option<String>,
    pub observed_at: Instant,
}

impl ObservedState {
    pub fn status(&self) -> Result<DeviceStatus> {
        let mut tracker = self.tracker.clone();
        let now = linux::clock_reading()?;
        tracker.tick(now);
        Ok(DeviceStatus {
            schema: 1,
            boot_id: self.boot_id.clone(),
            service_healthy: self.observed_at.elapsed() < Duration::from_secs(15)
                && self.persistence_error.is_none(),
            provider_error: self.provider_error.clone(),
            persistence_error: self.persistence_error.clone(),
            rtc_error: self.rtc_error.clone(),
            time: tracker.status(now),
            recovery: crate::recovery_status::read(&self.boot_id, now.boottime_us / 1000),
            time_check: None,
            update: None,
            install_policy: None,
            install_admission: None,
        })
    }
}

pub type SharedState = Arc<Mutex<ObservedState>>;
pub type StatusReader = Arc<dyn Fn() -> Result<DeviceStatus> + Send + Sync>;
pub type SyncNow = Arc<dyn Fn() -> Result<crate::time_runtime::Accepted> + Send + Sync>;
#[derive(Clone, Copy)]
pub enum UpdateAction {
    Check,
    Select(engine_ota::Channel),
}
pub type Update =
    Arc<dyn Fn(UpdateAction) -> Result<crate::update_runtime::Accepted> + Send + Sync>;
pub type InstallPolicy = Arc<
    dyn Fn(crate::install_policy::Change) -> Result<crate::install_policy::Status> + Send + Sync,
>;

/// The preference as saved, returned only after it is durable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPolicySaved {
    pub schema: u32,
    pub install_policy: crate::install_policy::Status,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Status {
        schema: u32,
    },
    Subscribe {
        schema: u32,
        seconds: u16,
    },
    SyncNow {
        schema: u32,
    },
    CheckUpdates {
        schema: u32,
    },
    UpdateChannel {
        schema: u32,
        channel: engine_ota::Channel,
    },
    /// An absent setting keeps its saved value.
    InstallPolicy {
        schema: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<crate::install_policy::Mode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<crate::install_policy::Window>,
    },
}

pub struct StatusServer {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl StatusServer {
    /// The parent directory is created by systemd with root:spacewars ownership.
    /// Callers must already own the independent service's exclusive state lock.
    pub fn bind(path: &Path, kiosk_uid: u32, read: StatusReader) -> Result<Self> {
        Self::bind_with_sync(path, kiosk_uid, read, None)
    }

    pub fn bind_with_sync(
        path: &Path,
        kiosk_uid: u32,
        read: StatusReader,
        sync: Option<SyncNow>,
    ) -> Result<Self> {
        Self::bind_with_actions(path, kiosk_uid, read, sync, None, None)
    }

    pub fn bind_with_actions(
        path: &Path,
        kiosk_uid: u32,
        read: StatusReader,
        sync: Option<SyncNow>,
        update: Option<Update>,
        policy: Option<InstallPolicy>,
    ) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
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
                        if workers.len() >= MAX_CLIENTS
                            || !peer_uid(&stream).is_ok_and(|uid| authorized(uid, kiosk_uid))
                        {
                            // No work queue or waiting thread for an unauthorized
                            // or excess peer. A retry must make a new connection.
                            continue;
                        }
                        let read = read.clone();
                        let sync = sync.clone();
                        let update = update.clone();
                        let policy = policy.clone();
                        let stop = stopped.clone();
                        workers.push(thread::spawn(move || {
                            let _ = handle(
                                stream,
                                &read,
                                sync.as_ref(),
                                update.as_ref(),
                                policy.as_ref(),
                                &stop,
                            );
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
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
        })
    }
}

impl Drop for StatusServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

pub fn authorized(uid: u32, kiosk_uid: u32) -> bool {
    uid == 0 || uid == kiosk_uid
}

fn peer_uid(stream: &UnixStream) -> Result<u32> {
    Ok(peer_credentials(stream)?.uid)
}

pub(crate) fn peer_credentials(stream: &UnixStream) -> Result<libc::ucred> {
    // SAFETY: SO_PEERCRED fills the initialized Linux ucred and its size.
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 || length as usize != std::mem::size_of_val(&credentials) {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(credentials)
}

fn handle(
    mut stream: UnixStream,
    read: &StatusReader,
    sync: Option<&SyncNow>,
    update: Option<&Update>,
    policy: Option<&InstallPolicy>,
    stop: &AtomicBool,
) -> Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = match read_frame(&mut stream, MAX_REQUEST, REQUEST_DEADLINE)
        .and_then(|frame| serde_json::from_slice::<Request>(&frame).map_err(Error::from))
    {
        Ok(request) => request,
        Err(_) => {
            return write_json(
                &mut stream,
                &serde_json::json!({"schema":1,"error":"invalid-request"}),
            );
        }
    };
    let seconds = match request {
        Request::CheckUpdates { schema: 1 } | Request::UpdateChannel { schema: 1, .. } => {
            let action = match request {
                Request::UpdateChannel { channel, .. } => UpdateAction::Select(channel),
                _ => UpdateAction::Check,
            };
            return match update
                .filter(|_| !stop.load(Ordering::Relaxed))
                .and_then(|update| update(action).ok())
            {
                Some(accepted) => write_json(&mut stream, &accepted),
                None => write_json(
                    &mut stream,
                    &serde_json::json!({"schema":1,"error":"unavailable"}),
                ),
            };
        }
        Request::InstallPolicy {
            schema: 1,
            mode,
            window,
        } => {
            let change = crate::install_policy::Change { mode, window };
            if !change.valid() {
                return write_json(
                    &mut stream,
                    &serde_json::json!({"schema":1,"error":"invalid-request"}),
                );
            }
            return match policy
                .filter(|_| !stop.load(Ordering::Relaxed))
                .and_then(|policy| policy(change).ok())
            {
                Some(install_policy) => write_json(
                    &mut stream,
                    &InstallPolicySaved {
                        schema: 1,
                        install_policy,
                    },
                ),
                None => write_json(
                    &mut stream,
                    &serde_json::json!({"schema":1,"error":"unavailable"}),
                ),
            };
        }
        Request::SyncNow { schema: 1 } => {
            return match sync
                .filter(|_| !stop.load(Ordering::Relaxed))
                .and_then(|sync| sync().ok())
            {
                Some(accepted) => write_json(&mut stream, &accepted),
                None => write_json(
                    &mut stream,
                    &serde_json::json!({"schema":1,"error":"unavailable"}),
                ),
            };
        }
        Request::Status { schema: 1 } => 0,
        Request::Subscribe { schema: 1, seconds } if (1..=60).contains(&seconds) => seconds,
        _ => {
            return write_json(
                &mut stream,
                &serde_json::json!({"schema":1,"error":"unsupported-request"}),
            );
        }
    };
    let deadline = Instant::now() + Duration::from_secs(seconds.into());
    loop {
        write_json(&mut stream, &read()?)?;
        if seconds == 0
            || stop.load(Ordering::Relaxed)
            || Instant::now() + Duration::from_secs(1) >= deadline
        {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
}

pub(crate) fn read_frame(
    stream: &mut UnixStream,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::with_capacity(256);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| Error::Invalid("socket request deadline exceeded".into()))?;
        stream.set_read_timeout(Some(remaining))?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return Ok(bytes);
        }
        if bytes.len() == limit {
            return Err(Error::Invalid("socket frame too large".into()));
        }
        bytes.push(byte[0]);
    }
}

pub(crate) fn write_json(stream: &mut UnixStream, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_RESPONSE {
        return Err(Error::Invalid("socket response exceeds limit".into()));
    }
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    Ok(())
}

/// This diagnostic has no dependency on the UI process or selected scenario.
pub fn read_status(path: &Path) -> Result<DeviceStatus> {
    let mut stream = connect(path)?;
    stream.write_all(b"{\"schema\":1,\"command\":\"status\"}\n")?;
    let response = read_frame(&mut stream, MAX_RESPONSE, REQUEST_DEADLINE)?;
    Ok(serde_json::from_slice(&response)?)
}

/// Explicit, bounded request to the root-owned service. Acknowledgement means
/// queued/coalesced; inspect status for a confirmed synchronization afterwards.
pub fn sync_now(path: &Path) -> Result<crate::time_runtime::Accepted> {
    let mut stream = connect(path)?;
    if peer_credentials(&stream)?.uid != 0 {
        return Err(Error::Invalid("unexpected device service owner".into()));
    }
    stream.write_all(b"{\"schema\":1,\"command\":\"sync-now\"}\n")?;
    let response = read_frame(&mut stream, MAX_RESPONSE, REQUEST_DEADLINE)?;
    let accepted: crate::time_runtime::Accepted = serde_json::from_slice(&response)
        .map_err(|_| Error::Invalid("time-check worker unavailable".into()))?;
    if accepted.schema != 1 {
        return Err(Error::Invalid("unsupported time-check reply".into()));
    }
    Ok(accepted)
}

pub fn update_request(
    path: &Path,
    action: UpdateAction,
) -> Result<crate::update_runtime::Accepted> {
    let mut stream = connect(path)?;
    if peer_credentials(&stream)?.uid != 0 {
        return Err(Error::Invalid("unexpected device service owner".into()));
    }
    let request = match action {
        UpdateAction::Check => Request::CheckUpdates { schema: 1 },
        UpdateAction::Select(channel) => Request::UpdateChannel { schema: 1, channel },
    };
    write_json(&mut stream, &request)?;
    let response = read_frame(&mut stream, MAX_RESPONSE, REQUEST_DEADLINE)?;
    let accepted: crate::update_runtime::Accepted = serde_json::from_slice(&response)
        .map_err(|_| Error::Invalid("update worker unavailable".into()))?;
    if accepted.schema != 1 {
        return Err(Error::Invalid("unsupported update reply".into()));
    }
    Ok(accepted)
}

/// Saves one or both settings through the root-owned service. A refusal or a
/// lost reply leaves the outcome unconfirmed; read status before retrying.
pub fn install_policy(
    path: &Path,
    change: crate::install_policy::Change,
) -> Result<crate::install_policy::Status> {
    let mut stream = connect(path)?;
    if peer_credentials(&stream)?.uid != 0 {
        return Err(Error::Invalid("unexpected device service owner".into()));
    }
    write_json(
        &mut stream,
        &Request::InstallPolicy {
            schema: 1,
            mode: change.mode,
            window: change.window,
        },
    )?;
    let response = read_frame(&mut stream, MAX_RESPONSE, REQUEST_DEADLINE)?;
    let saved: InstallPolicySaved = serde_json::from_slice(&response).map_err(|_| {
        Error::Invalid("install policy change was not confirmed; read status".into())
    })?;
    if saved.schema != 1 {
        return Err(Error::Invalid("unsupported install policy reply".into()));
    }
    Ok(saved.install_policy)
}

fn connect(path: &Path) -> Result<UnixStream> {
    let stream = futures_lite::future::block_on(futures_lite::future::or(
        async { async_io::Async::<UnixStream>::connect(path).await },
        async {
            async_io::Timer::after(REQUEST_DEADLINE).await;
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "device socket connection timed out",
            ))
        },
    ))?
    .into_inner()?;
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_policy::{Mode, Window};

    #[test]
    fn install_policy_request_names_only_the_settings_it_changes() {
        let wire = |mode, window| {
            serde_json::to_string(&Request::InstallPolicy {
                schema: 1,
                mode,
                window,
            })
            .unwrap()
        };
        assert_eq!(
            wire(Some(Mode::Automatic), None),
            r#"{"command":"install-policy","schema":1,"mode":"automatic"}"#
        );
        assert_eq!(
            wire(None, Some(Window::default())),
            r#"{"command":"install-policy","schema":1,"window":{"start_minute":180,"end_minute":300}}"#
        );
    }
}
