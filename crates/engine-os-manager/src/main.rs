use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(about = "Space-Wars independent device service")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the root-owned device service (normally started by systemd).
    Serve,
    /// Reconcile an existing recovery journal and observe a trial boot.
    /// Normally invoked by the separate spacewars-recovery service.
    Recover,
    /// Prepare persistent SSH login keys before sshd, without changing boot state.
    #[command(hide = true)]
    InitializeAccess,
    /// Preserve seeded network profiles once, before the persistent bind mount.
    #[command(hide = true)]
    InitializeNetwork,
    /// Separately authenticated root operator tools; not available on the device socket.
    Maintenance {
        #[command(subcommand)]
        operation: MaintenanceCommand,
    },
    /// Read a bounded JSON status snapshot independently of the game client.
    Status,
    /// Inspect the current time provider without changing clocks or state.
    Probe,
    /// Capture the actual RTC before the time provider starts.
    #[command(hide = true)]
    CaptureRtc,
    /// Isolated, bounded RTC operation used only by the device service.
    #[command(hide = true)]
    RtcWriteWorker { receipt: String },
    /// Isolated health observation, invoked only by the device coordinator.
    #[command(hide = true)]
    KioskHealthWorker { expectation: String },
    /// Fixed temporary write using the kiosk account's permissions.
    #[command(hide = true)]
    KioskStateWorker,
}

#[derive(Subcommand)]
enum MaintenanceCommand {
    /// Explicitly import /root/.ssh/authorized_keys into an empty persistent root key file.
    ImportRootKeys,
    /// Install a compatible development client/CLI pair under the OTA lock.
    Fast {
        #[arg(allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    /// Change the fixed experimental renderer under the OTA lock.
    Renderer { renderer: String },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_args: Args) -> engine_os_manager::Result<()> {
    Err(engine_os_manager::Error::Invalid(
        "the device service requires Linux; desktop time remains OS-managed".into(),
    ))
}

#[cfg(target_os = "linux")]
fn run(args: Args) -> engine_os_manager::Result<()> {
    use engine_os_manager::{RTC_BOOT_PATH, SOCKET_PATH, linux, service};
    match args.command {
        Command::Status => println!(
            "{}",
            serde_json::to_string_pretty(&service::read_status(SOCKET_PATH.as_ref())?)?
        ),
        Command::Probe => {
            let observation = linux::observe_sync();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema": 1, "boot_id": linux::boot_id()?,
                    "observation": observation.as_ref().ok(),
                    "provider_error": observation.as_ref().err(),
                }))?
            );
        }
        Command::Serve => daemon::serve()?,
        Command::Recover => {
            daemon::require_root()?;
            engine_os_manager::recovery_service::run()?;
        }
        Command::InitializeAccess => {
            daemon::require_root()?;
            println!(
                "{}",
                serde_json::to_string(&engine_ota::access_linux::initialize()?)?
            );
        }
        Command::InitializeNetwork => {
            daemon::require_root()?;
            println!(
                "{}",
                serde_json::to_string(&engine_ota::network_linux::initialize()?)?
            );
        }
        Command::Maintenance { operation } => {
            daemon::require_root()?;
            use engine_os_manager::maintenance::{Request, run};
            match operation {
                MaintenanceCommand::ImportRootKeys => println!(
                    "{}",
                    serde_json::to_string(&engine_os_manager::maintenance::import_root_keys()?)?
                ),
                MaintenanceCommand::Fast { arguments } => run(Request::fast(arguments)?)?,
                MaintenanceCommand::Renderer { renderer } => run(Request::renderer(renderer)?)?,
            }
        }
        Command::CaptureRtc => {
            daemon::require_root()?;
            let path = std::path::Path::new(RTC_BOOT_PATH);
            // Never recapture later in the boot after software may have set it.
            if !path.exists() {
                let reading = linux::capture_rtc()?;
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?;
                file.write_all(&serde_json::to_vec(&reading)?)?;
                file.sync_all()?;
            }
        }
        Command::RtcWriteWorker { receipt } => {
            daemon::require_root()?;
            let receipt = serde_json::from_str(&receipt)?;
            println!("{}", serde_json::to_string(&linux::write_rtc(receipt)?)?);
        }
        Command::KioskHealthWorker { expectation } => {
            daemon::require_root()?;
            let expected = serde_json::from_str(&expectation)?;
            let result = engine_os_manager::kiosk_health::observe_worker(&expected)
                .map_err(|error| error.to_string().chars().take(1024).collect::<String>());
            println!("{}", serde_json::to_string(&result)?);
        }
        Command::KioskStateWorker => engine_os_manager::kiosk_health::state_worker()?,
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod daemon {
    use engine_os_manager::service::{ObservedState, SharedState, StatusServer};
    use engine_os_manager::store::TimeStore;
    use engine_os_manager::time::{RtcBootReading, TimeReason, TimeTracker};
    use engine_os_manager::{Error, RTC_BOOT_PATH, Result, SOCKET_PATH, STATE_DIRECTORY, linux};
    use std::fs::{self, OpenOptions};
    use std::io::Read;
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub fn require_root() -> Result<()> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(Error::Invalid(
                "device mutations require the root-owned service".into(),
            ));
        }
        Ok(())
    }

    fn read_rtc_boot() -> Option<RtcBootReading> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(RTC_BOOT_PATH)
            .ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != 0
            || metadata.mode() & 0o077 != 0
            || metadata.len() > 4096
        {
            return None;
        }
        serde_json::from_reader(file.take(4097)).ok()
    }

    struct RtcWorker {
        child: Arc<Mutex<Child>>,
        generation: u64,
        timed_out: Arc<AtomicBool>,
    }

    impl RtcWorker {
        fn new(child: Child, generation: u64) -> Self {
            let child = Arc::new(Mutex::new(child));
            let timed_out = Arc::new(AtomicBool::new(false));
            let watched = child.clone();
            let expired = timed_out.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                if let Ok(mut child) = watched.lock()
                    && !matches!(child.try_wait(), Ok(Some(_)))
                {
                    // Holding the Child lock prevents reaping/PID reuse
                    // between this check and kill, even during a DBus stall.
                    expired.store(true, Ordering::Relaxed);
                    let _ = child.kill();
                }
            });
            Self {
                child,
                generation,
                timed_out,
            }
        }
    }

    pub fn serve() -> Result<()> {
        require_root()?;
        match fs::DirBuilder::new()
            .mode(0o700)
            .create("/data/spacewars-system")
        {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let parent = fs::symlink_metadata("/data/spacewars-system")?;
        if !parent.is_dir() || parent.uid() != 0 || parent.mode() & 0o077 != 0 {
            return Err(Error::Invalid(
                "system data must be a private root-owned directory".into(),
            ));
        }
        let store = TimeStore::open(STATE_DIRECTORY.as_ref())?;
        let (record, reason) = store.load();
        let mut saved = record.clone();
        let boot_id = linux::boot_id()?;
        let mut tracker = TimeTracker::new(
            boot_id.clone(),
            record,
            reason.clone().unwrap_or(TimeReason::WaitingForNetworkSync),
            linux::clock_reading()?,
            read_rtc_boot().as_ref(),
        );
        let mut provider_error = Some("awaiting provider observation".into());
        let mut persistence_error = reason
            .filter(|reason| *reason == TimeReason::CorruptState)
            .map(|_| "corrupt time metadata; waiting for verified recovery".into());
        let mut rtc_error = None;
        let shared: SharedState = Arc::new(Mutex::new(ObservedState {
            tracker: tracker.clone(),
            boot_id: boot_id.clone(),
            provider_error: provider_error.clone(),
            persistence_error: persistence_error.clone(),
            rtc_error: None,
            observed_at: Instant::now(),
        }));
        // The exclusive store lock prevents another service unlinking our socket.
        match fs::symlink_metadata(SOCKET_PATH) {
            Ok(metadata) if metadata.file_type().is_socket() && metadata.uid() == 0 => {
                fs::remove_file(SOCKET_PATH)?
            }
            Ok(_) => return Err(Error::Invalid("unsafe device socket path".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let reader = shared.clone();
        let _server = StatusServer::bind(
            SOCKET_PATH.as_ref(),
            1000,
            Arc::new(move || {
                reader
                    .lock()
                    .map_err(|_| Error::Invalid("device observation unavailable".into()))?
                    .status()
            }),
        )?;
        let mut rtc_worker: Option<RtcWorker> = None;
        let mut save_retry = Instant::now();
        let mut save_delay = Duration::from_secs(15);
        loop {
            let new_sync = match linux::observe_sync() {
                Ok(observation) => {
                    provider_error = None;
                    tracker.observe(&observation)
                }
                Err(error) => {
                    provider_error = Some(bounded(&error));
                    tracker.tick(linux::clock_reading()?);
                    false
                }
            };
            if new_sync {
                // Persist invalidation before touching the RTC so an interrupted
                // write cannot leave the next boot trusting an old association.
                tracker.invalidate_rtc();
            }
            let mut rtc_finished = false;
            if let Some(worker) = rtc_worker.as_mut() {
                let mut child = worker
                    .child
                    .lock()
                    .map_err(|_| Error::Invalid("RTC worker lock poisoned".into()))?;
                match child.try_wait() {
                    Ok(Some(status)) => {
                        if status.success()
                            && !worker.timed_out.load(Ordering::Relaxed)
                            && worker.generation == tracker.record().success_generation
                        {
                            let result = child
                                .stdout
                                .take()
                                .ok_or_else(|| Error::Invalid("missing RTC result".into()))
                                .and_then(|stdout| {
                                    serde_json::from_reader::<_, linux::RtcWriteResult>(
                                        stdout.take(4097),
                                    )
                                    .map_err(Error::from)
                                });
                            match result {
                                Ok(result) => {
                                    tracker.record_rtc_write(result.driver, result.verified_utc_us);
                                    rtc_error = None;
                                }
                                Err(error) => rtc_error = Some(bounded(&error.to_string())),
                            }
                        } else {
                            rtc_error = Some("RTC write/readback did not complete for the current synchronization".into());
                        }
                        rtc_finished = true;
                    }
                    Ok(None) => {
                        if worker.timed_out.load(Ordering::Relaxed) {
                            rtc_error = Some(
                                "RTC worker exceeded five seconds; network time remains usable"
                                    .into(),
                            );
                        }
                        // Retain the handle until reaped; never accumulate hung workers.
                    }
                    Err(error) => {
                        rtc_error = Some(bounded(&error.to_string()));
                        let _ = child.kill();
                        worker.timed_out.store(true, Ordering::Relaxed);
                    }
                }
            }
            if rtc_finished {
                rtc_worker = None;
            }
            if tracker.record() != &saved && Instant::now() >= save_retry {
                match store.save(tracker.record()) {
                    Ok(()) => {
                        saved = tracker.record().clone();
                        persistence_error = None;
                        save_delay = Duration::from_secs(15);
                    }
                    Err(error) => {
                        persistence_error = Some(bounded(&error.to_string()));
                        save_retry = Instant::now() + save_delay;
                        save_delay = (save_delay * 2).min(Duration::from_secs(15 * 60));
                    }
                }
            }
            if new_sync
                && persistence_error.is_none()
                && tracker.record() == &saved
                && rtc_worker.is_none()
            {
                match Command::new(std::env::current_exe()?)
                    .arg("rtc-write-worker")
                    .arg(serde_json::to_string(
                        &tracker
                            .record()
                            .last_sync
                            .as_ref()
                            .ok_or_else(|| {
                                Error::Invalid("missing accepted RTC synchronization".into())
                            })?
                            .receipt,
                    )?)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(child) => {
                        rtc_worker =
                            Some(RtcWorker::new(child, tracker.record().success_generation))
                    }
                    Err(error) => rtc_error = Some(bounded(&error.to_string())),
                }
            }
            *shared
                .lock()
                .map_err(|_| Error::Invalid("device status lock poisoned".into()))? =
                ObservedState {
                    tracker: tracker.clone(),
                    boot_id: boot_id.clone(),
                    provider_error: provider_error.clone(),
                    persistence_error: persistence_error.clone(),
                    rtc_error: rtc_error.clone(),
                    observed_at: Instant::now(),
                };
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn bounded(message: &str) -> String {
        message.chars().take(256).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::process::ExitStatusExt;

        #[test]
        fn rtc_deadline_kills_its_worker_even_when_observation_is_busy() {
            let child = Command::new("/bin/sh")
                .args(["-c", "exec sleep 30"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let worker = RtcWorker::new(child, 1);
            let started = Instant::now();
            loop {
                if let Some(status) = worker.child.lock().unwrap().try_wait().unwrap() {
                    assert_eq!(status.signal(), Some(libc::SIGKILL));
                    assert!(worker.timed_out.load(Ordering::Relaxed));
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(8));
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
