//! Native adapters and startup for the independent network coordinator.
use crate::network_controller::{Backend, Controller, Snapshot, WriteResult};
use crate::network_monitor::Monitor;
use crate::network_observation::Observation;
use crate::network_policy::{Activity, Clock};
use crate::network_service::{NetworkServer, SOCKET_PATH, STATE_DIRECTORY};
use crate::{Error, Result, linux, network_radio, network_sessions};
use engine_ota::install_lock::InstallLock;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Runtime {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Initialization and retries also stay off the time/status thread. In
/// particular, a short recovery-helper lock must not disable this boot's API.
pub fn spawn() -> Runtime {
    spawn_with(start)
}

pub(crate) fn spawn_with(
    mut start: impl FnMut() -> Result<NetworkServer> + Send + 'static,
) -> Runtime {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let worker = thread::spawn(move || {
        let mut server: Option<NetworkServer> = None;
        let mut retry_at = Instant::now();
        let mut delay = Duration::from_millis(250);
        let mut last_error = None;
        let mut started_at = None;
        while !stopped.load(Ordering::Relaxed) {
            if server.as_ref().is_some_and(|server| !server.running()) {
                server = None;
                started_at = None;
                retry_at = Instant::now() + delay;
                delay = (delay * 2).min(Duration::from_secs(5));
            } else if started_at
                .is_some_and(|start: Instant| start.elapsed() >= Duration::from_secs(10))
            {
                delay = Duration::from_millis(250);
                last_error = None;
            }
            if server.is_none() && Instant::now() >= retry_at {
                match start() {
                    Ok(started) => {
                        server = Some(started);
                        started_at = Some(Instant::now());
                    }
                    Err(error) => {
                        let message: String = error.to_string().chars().take(1024).collect();
                        if last_error.as_ref() != Some(&message) {
                            tracing::error!(error = %message, "network coordinator unavailable; retrying");
                            last_error = Some(message);
                        }
                        retry_at = Instant::now() + delay;
                        delay = (delay * 2).min(Duration::from_secs(5));
                    }
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    Runtime {
        stop,
        worker: Some(worker),
    }
}

#[derive(Default)]
pub struct Native {
    monitor: Option<Monitor>,
    guard: Weak<InstallLock>,
}

impl Native {
    fn monitor(&mut self) -> Result<&mut Monitor> {
        if self.monitor.is_none() {
            self.monitor = Some(Monitor::system()?);
        }
        Ok(self.monitor.as_mut().expect("initialized monitor"))
    }

    fn install_activity(&self) -> Option<bool> {
        if let Some(guard) = self.guard.upgrade() {
            return guard.check().ok().map(|()| false);
        }
        match InstallLock::acquire() {
            Ok(guard) => guard.check().ok().map(|()| false),
            Err(engine_ota::Error::Busy) => Some(true),
            Err(_) => None,
        }
    }
}

impl Backend for Native {
    type ShutdownGuard = Arc<InstallLock>;

    fn clock(&mut self) -> Result<Clock> {
        Ok(Clock {
            boot_id: linux::boot_id()?,
            ms: linux::clock_reading()?.boottime_us / 1000,
        })
    }

    fn snapshot(&mut self) -> Result<Snapshot> {
        let ssh = network_sessions::observe()
            .ok()
            .and_then(|report| report.ssh_activity);
        let (radio, change) = self.monitor()?.snapshot()?;
        Ok(Snapshot {
            clock: self.clock()?,
            radio,
            activity: Activity {
                ssh_sessions: ssh,
                checkpoints: None, // Filled from the same radio observation by Controller.
                install_lock: self.install_activity(),
                // All coordinator operations are serialized here. External
                // install, SSH and NM work have their own observations above;
                // admitted setup/download/deployment work retains policy holds.
                operations: Some(false),
            },
            change,
        })
    }

    fn shutdown_guard(&mut self) -> Result<Option<Self::ShutdownGuard>> {
        if self.guard.upgrade().is_some() {
            return Err(Error::Invalid(
                "network shutdown guard is already held".into(),
            ));
        }
        match InstallLock::acquire() {
            Ok(lock) => {
                let guard = Arc::new(lock);
                self.guard = Arc::downgrade(&guard);
                Ok(Some(guard))
            }
            Err(engine_ota::Error::Busy) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn set_wireless(&mut self, expected: &Observation, enabled: bool) -> Result<WriteResult> {
        let guard = self.guard.upgrade();
        if !enabled && guard.as_ref().is_none_or(|guard| guard.check().is_err()) {
            return Ok(WriteResult::NotSubmitted);
        }
        if !self
            .monitor()
            .and_then(|monitor| monitor.prepare_write(expected, enabled))
            .is_ok_and(|ready| ready)
        {
            return Ok(WriteResult::NotSubmitted);
        }
        let result = network_radio::set_wireless(expected, enabled);
        self.monitor
            .as_mut()
            .expect("prepared monitor")
            .finish_write(&result);
        result
    }

    fn reconcile(&mut self) -> Result<()> {
        // Failed replacement leaves the previous evidence in place. Only an
        // explicit resume calls this, never a periodic observation retry.
        self.monitor = Some(Monitor::system()?);
        Ok(())
    }
}

/// Called after the service has validated the private parent directory. The
/// policy store lock is acquired before removing a stale socket inode.
fn start() -> Result<NetworkServer> {
    start_with(
        Path::new(STATE_DIRECTORY),
        Path::new(SOCKET_PATH),
        1000,
        Native::default(),
    )
}

pub(crate) fn start_with<B: Backend + Send + 'static>(
    directory: &Path,
    socket: &Path,
    kiosk_uid: u32,
    backend: B,
) -> Result<NetworkServer> {
    let controller = Controller::open(directory, backend)?;
    // SAFETY: geteuid has no preconditions. The production entry point is root.
    let owner = unsafe { libc::geteuid() };
    match fs::symlink_metadata(socket) {
        Ok(meta) if meta.uid() == owner && meta.file_type().is_socket() => fs::remove_file(socket)?,
        Ok(_) => return Err(Error::Invalid("unsafe network socket path".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    NetworkServer::bind(socket, kiosk_uid, controller)
}
