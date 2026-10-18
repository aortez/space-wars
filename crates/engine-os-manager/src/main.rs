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
    /// Prepare or activate the permanent boot layout after staged normal health.
    BootstrapCanonical {
        #[arg(long)]
        package: String,
        #[arg(long)]
        written_root: String,
        #[arg(long)]
        trial: String,
        #[command(subcommand)]
        operation: CanonicalCommand,
    },
    /// Request one supervised tryboot of an already staged bootstrap image.
    /// Root operator only; consumes the attempt even if the reboot reply is lost.
    BootstrapActivate {
        #[arg(long)]
        package: String,
        #[arg(long)]
        written_root: String,
        #[arg(long)]
        trial: String,
        /// The four original SSH_CONNECTION fields, quoted as one argument.
        #[arg(long)]
        ssh_connection: String,
    },
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
    /// Queue one bounded OS time check; repeated requests join active work.
    SyncNow,
    /// Queue a bounded update check/download; never installs or reboots.
    CheckUpdates,
    /// Select a signed update channel; this does not change the installed image.
    UpdateChannel {
        #[arg(value_parser = ["release", "nightly"])]
        channel: String,
    },
    /// Inspect the current time provider without changing clocks or state.
    Probe,
    /// Read NetworkManager radio/checkpoint state without changing connectivity.
    NetworkProbe,
    /// Observe NetworkManager changes for a bounded interval without writing radio state.
    NetworkWatchProbe {
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..=30))]
        seconds: u16,
    },
    /// Inspect SSH/TCP protection evidence without changing radio state.
    NetworkSessionProbe,
    /// Control saved Wi-Fi policy through the independent device service.
    Network {
        #[command(subcommand)]
        operation: NetworkCommand,
    },
    /// Admit an accepted SSH connection before socket-activated sshd starts.
    #[command(hide = true)]
    SshNetworkAdmit,
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
    /// Provision public OTA root/feed trust and an explicitly confirmed data version.
    ProvisionUpdates {
        #[arg(long)]
        bundle_sha256: String,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        data_version: u32,
        /// Read the owner-provided bundle from /data/spacewars-system/ota-bootstrap.json.
        /// Without this flag, only the image's /etc/spacewars bundle is used.
        #[arg(long)]
        operator_bundle: bool,
    },
    /// Explicitly import /root/.ssh/authorized_keys into an empty persistent root key file.
    ImportRootKeys,
    /// Explicitly import /home/swadmin/.ssh/authorized_keys into an empty persistent admin role.
    ImportAdminKeys,
    /// Install a compatible development client/CLI pair under the OTA lock.
    Fast {
        #[arg(allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    /// Change the fixed experimental renderer under the OTA lock.
    Renderer { renderer: String },
}

#[derive(Subcommand)]
enum NetworkCommand {
    Status,
    /// Admit/renew/release a deployment; read its secret capability line from stdin.
    Deployment {
        #[command(subcommand)]
        operation: DeploymentCommand,
    },
    Policy {
        #[arg(long)]
        generation: u64,
        #[arg(long, action = clap::ArgAction::Set, required = true)]
        partial_offline: bool,
        #[arg(long, action = clap::ArgAction::Set, required = true)]
        stay_connected: bool,
    },
    Resume {
        #[arg(long)]
        generation: u64,
    },
}

#[derive(Subcommand)]
enum DeploymentCommand {
    Admit,
    Renew,
    Release,
}

#[derive(Subcommand)]
enum CanonicalCommand {
    Prepare,
    Activate,
    Finish,
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
        Command::CheckUpdates => println!(
            "{}",
            serde_json::to_string(&service::update_request(
                SOCKET_PATH.as_ref(),
                service::UpdateAction::Check
            )?)?
        ),
        Command::UpdateChannel { channel } => {
            let channel = match channel.as_str() {
                "release" => engine_ota::Channel::Release,
                "nightly" => engine_ota::Channel::Nightly,
                _ => {
                    return Err(engine_os_manager::Error::Invalid(
                        "unknown update channel".into(),
                    ));
                }
            };
            println!(
                "{}",
                serde_json::to_string(&service::update_request(
                    SOCKET_PATH.as_ref(),
                    service::UpdateAction::Select(channel)
                )?)?
            );
        }
        Command::SyncNow => println!(
            "{}",
            serde_json::to_string(&service::sync_now(SOCKET_PATH.as_ref())?)?
        ),
        Command::BootstrapCanonical {
            package,
            written_root,
            trial,
            operation,
        } => {
            daemon::require_root()?;
            let digest = |value: String| {
                engine_ota::Sha256::try_from(value)
                    .map_err(|cause| engine_os_manager::Error::Invalid(cause.into()))
            };
            let (package, written_root, trial) =
                (digest(package)?, digest(written_root)?, digest(trial)?);
            let report = match operation {
                CanonicalCommand::Prepare => {
                    engine_os_manager::bootstrap_canonical::prepare(package, written_root, trial)?
                }
                CanonicalCommand::Activate => {
                    engine_os_manager::bootstrap_canonical::activate(package, written_root, trial)?
                }
                CanonicalCommand::Finish => {
                    engine_os_manager::bootstrap_canonical::finish(package, written_root, trial)?
                }
            };
            println!("{}", serde_json::to_string(&report)?);
        }
        Command::BootstrapActivate {
            package,
            written_root,
            trial,
            ssh_connection,
        } => {
            daemon::require_root()?;
            let digest = |value: String| {
                engine_ota::Sha256::try_from(value)
                    .map_err(|cause| engine_os_manager::Error::Invalid(cause.into()))
            };
            let report = engine_os_manager::bootstrap_activation::activate(
                digest(package)?,
                digest(written_root)?,
                digest(trial)?,
                &ssh_connection,
            )?;
            println!("{}", serde_json::to_string(&report)?);
        }
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
        Command::NetworkProbe => println!(
            "{}",
            serde_json::to_string_pretty(&engine_os_manager::network_observation::observe()?)?
        ),
        Command::NetworkWatchProbe { seconds } => {
            use std::time::{Duration, Instant};
            let mut monitor = engine_os_manager::network_monitor::Monitor::system()?;
            let deadline = Instant::now() + Duration::from_secs(seconds.into());
            let mut samples = 0_u32;
            let (actual, change) = loop {
                let observation = monitor.snapshot()?;
                samples += 1;
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break observation;
                };
                std::thread::sleep(remaining.min(Duration::from_millis(250)));
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema":1, "samples":samples, "actual":actual, "change":change,
                }))?
            );
        }
        Command::NetworkSessionProbe => println!(
            "{}",
            serde_json::to_string_pretty(&engine_os_manager::network_sessions::observe()?)?
        ),
        Command::Network { operation } => {
            use engine_os_manager::{
                network_policy::Policy,
                network_service::{self, Request},
            };
            let request = match operation {
                NetworkCommand::Status => Request::Status { schema: 1 },
                NetworkCommand::Deployment { operation } => {
                    use engine_os_manager::network_deployment::{self, Operation};
                    network_deployment::request(match operation {
                        DeploymentCommand::Admit => Operation::Admit,
                        DeploymentCommand::Renew => Operation::Renew,
                        DeploymentCommand::Release => Operation::Release,
                    })?
                }
                NetworkCommand::Policy {
                    generation,
                    partial_offline,
                    stay_connected,
                } => Request::SetPolicy {
                    schema: 1,
                    generation,
                    policy: Policy {
                        partial_offline,
                        stay_connected,
                    },
                },
                NetworkCommand::Resume { generation } => Request::Resume {
                    schema: 1,
                    generation,
                },
            };
            let reply = network_service::request(network_service::SOCKET_PATH.as_ref(), &request)?;
            println!("{}", serde_json::to_string_pretty(&reply)?);
            if !reply.get("error").is_some_and(serde_json::Value::is_null) {
                return Err(engine_os_manager::Error::Invalid(
                    "network request was not accepted".into(),
                ));
            }
        }
        Command::SshNetworkAdmit => {
            daemon::require_root()?;
            engine_os_manager::network_ssh::admit()?;
        }
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
                MaintenanceCommand::ProvisionUpdates {
                    bundle_sha256,
                    data_version,
                    operator_bundle,
                } => {
                    let digest = engine_ota::Sha256::try_from(bundle_sha256)
                        .map_err(|error| engine_os_manager::Error::Invalid(error.into()))?;
                    let status = if operator_bundle {
                        engine_ota::provision_linux::provision_operator(digest, data_version)?
                    } else {
                        engine_ota::provision_linux::provision_installed(digest, data_version)?
                    };
                    println!("{}", serde_json::to_string(&status)?);
                }
                MaintenanceCommand::ImportRootKeys => println!(
                    "{}",
                    serde_json::to_string(&engine_os_manager::maintenance::import_root_keys()?)?
                ),
                MaintenanceCommand::ImportAdminKeys => println!(
                    "{}",
                    serde_json::to_string(&engine_os_manager::maintenance::import_admin_keys()?)?
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
    use engine_os_manager::service::{self, ObservedState, SharedState, StatusServer};
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
        // Separate workers own radio and time-check IO. Neither blocks the
        // accepted-time observer or the local read-only status response.
        let network = engine_os_manager::network_runtime::spawn();
        let updates = engine_os_manager::update_runtime::spawn(
            shared.clone(),
            network.watch(),
            boot_id.clone(),
        )?;
        let update_status = updates.handle();
        let update_action = update_status.clone();
        let time_checks = engine_os_manager::time_runtime::spawn(
            shared.clone(),
            network.watch(),
            boot_id.clone(),
            updates.handle(),
        );
        let checks = time_checks.handle();
        let sync = checks.clone();
        let reader = shared.clone();
        let _server = StatusServer::bind_with_actions(
            SOCKET_PATH.as_ref(),
            1000,
            Arc::new(move || {
                let mut status = reader
                    .lock()
                    .map_err(|_| Error::Invalid("device observation unavailable".into()))?
                    .status()?;
                status.time_check = Some(checks.status()?);
                status.update = Some(update_status.status()?);
                Ok(status)
            }),
            Some(Arc::new(move || sync.sync_now())),
            Some(Arc::new(move |action| match action {
                service::UpdateAction::Check => update_action.check_now(),
                service::UpdateAction::Select(channel) => update_action.select_channel(channel),
            })),
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
