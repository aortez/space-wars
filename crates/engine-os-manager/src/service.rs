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
        })
    }
}

pub type SharedState = Arc<Mutex<ObservedState>>;
pub type StatusReader = Arc<dyn Fn() -> Result<DeviceStatus> + Send + Sync>;
pub type SyncNow = Arc<dyn Fn() -> Result<crate::time_runtime::Accepted> + Send + Sync>;

#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Status { schema: u32 },
    Subscribe { schema: u32, seconds: u16 },
    SyncNow { schema: u32 },
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
                        let stop = stopped.clone();
                        workers.push(thread::spawn(move || {
                            let _ = handle(stream, &read, sync.as_ref(), &stop);
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
