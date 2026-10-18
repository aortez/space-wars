//! Fixed-path production adapters. No UI-supplied URL, path, image identity,
//! data version or installation command enters this worker.
use super::*;
use crate::{linux, network_service};
use engine_ota::boot_linux::{BootController, BootEvidence};
use engine_ota::install_lock::InstallLock;
use engine_ota::provision_linux::{
    self, DATA_SCHEMA_PATH as DATA_SCHEMA, RECEIPT_PATH, STAGING_PATH as STAGING,
    TRUST_PATH as TRUST,
};
use engine_ota::staging_linux::StagingStore;
use engine_ota::{DeviceIdentity, ImageHistory, RecoveryStore, Selection, TrustStore};
use std::fs;
use std::os::unix::fs::MetadataExt;

const IMAGE: &str = "/etc/spacewars/image.json";

#[derive(Clone)]
pub(super) struct Native {
    time: SharedState,
    network: Watch,
    boot_id: String,
}
impl Native {
    pub fn new(time: SharedState, network: Watch, boot_id: String) -> Self {
        Self {
            time,
            network,
            boot_id,
        }
    }
    fn operation(&self, request: network_service::Request, end: Option<u64>) -> Result<()> {
        let now = self.clock()?;
        let left = end.map_or(5000, |end| end.saturating_sub(now.ms).min(5000));
        if left == 0 {
            return Err(unavailable());
        }
        let reply = network_service::request_before(
            network_service::SOCKET_PATH.as_ref(),
            &request,
            0,
            Instant::now() + Duration::from_millis(left),
        )?;
        if end.is_some_and(|end| !self.clock().is_ok_and(|clock| clock.ms < end))
            || !reply.get("error").is_some_and(serde_json::Value::is_null)
            || !reply
                .get("status")
                .is_some_and(serde_json::Value::is_object)
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
impl Inputs for Native {
    fn clock(&self) -> Result<Clock> {
        Ok(Clock {
            boot_id: self.boot_id.clone(),
            ms: linux::clock_reading()?.boottime_us / 1000,
        })
    }
    fn network(&self, _clock: &Clock) -> Option<Snapshot> {
        self.network.read(|| self.clock().ok())
    }
    fn time(&self) -> Option<UsableTime> {
        let shared = self.time.lock().ok()?;
        if shared.boot_id != self.boot_id
            || shared.persistence_error.is_some()
            || shared.observed_at.elapsed() >= Duration::from_secs(15)
        {
            return None;
        }
        let clock = linux::clock_reading().ok()?;
        let mut tracker = shared.tracker.clone();
        tracker.tick(clock);
        let status = tracker.status(clock);
        status
            .usable
            .then_some(status.utc_us)
            .flatten()
            .and_then(|us| UsableTime::from_unix_microseconds(us).ok())
    }
}

pub(super) struct Prepared {
    runtime: tokio::runtime::Runtime,
    session: download::DownloadSession,
    context: Context,
    trust: TrustStore,
    staging: StagingStore,
    channel: Channel,
}

struct Context {
    lock: InstallLock,
    journal: RecoveryStore,
    boot: BootEvidence,
    device: DeviceIdentity,
    history: ImageHistory,
    unchanged: Vec<Stamp>,
}

impl Context {
    fn open(lock: InstallLock) -> Result<Self> {
        engine_ota::bootstrap::require_no_active_bootstrap(&lock)?;
        let controller = BootController::open(&lock)?;
        let journal = RecoveryStore::open(crate::recovery_service::RECOVERY_DIRECTORY.as_ref())?;
        controller.authorize_maintenance(&journal)?;
        let boot = controller.observe()?;
        let persistent_schema =
            provision_linux::inspect(&boot.layout, &boot.observation.binding)?.persistent_schema;
        if !boot.image.data.reads.contains(persistent_schema) {
            return Err(unavailable());
        }
        let data = fs::symlink_metadata("/data")?;
        for path in [
            "/data/spacewars-system",
            TRUST,
            STAGING,
            crate::recovery_service::RECOVERY_DIRECTORY,
        ] {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.mode() & 0o077 != 0
                || metadata.dev() != data.dev()
            {
                return Err(unavailable());
            }
        }
        let device = DeviceIdentity {
            machine: boot.image.machine.clone(),
            architecture: boot.image.architecture.clone(),
            layout: boot.image.layout.clone(),
            boot_identity: boot.image.boot_identity,
            updater_protocol: boot.image.updater_protocol,
            recovery_protocol: boot.image.recovery_protocol,
            inactive_slot_bytes: boot
                .layout
                .inactive_partition()
                .sectors
                .checked_mul(512)
                .ok_or_else(unavailable)?,
            persistent_schema,
            rollback_reads: boot.image.data.reads,
        };
        let history = journal.record().image_history();
        let unchanged = [
            PathBuf::from(IMAGE),
            PathBuf::from(DATA_SCHEMA),
            PathBuf::from(RECEIPT_PATH),
            Path::new(crate::recovery_service::RECOVERY_DIRECTORY).join("state.json"),
        ]
        .into_iter()
        .map(Stamp::capture)
        .collect::<Result<_>>()?;
        Ok(Self {
            lock,
            journal,
            boot,
            device,
            history,
            unchanged,
        })
    }

    fn check(&self) -> Result<()> {
        engine_ota::bootstrap::require_no_active_bootstrap(&self.lock)?;
        if !self.unchanged.iter().all(Stamp::matches) {
            return Err(unavailable());
        }
        let controller = BootController::open(&self.lock)?;
        controller.authorize_maintenance(&self.journal)?;
        let now = controller.observe()?;
        if now.image != self.boot.image
            || now.layout != self.boot.layout
            || now.physical != self.boot.physical
            || now.hardware_profile != self.boot.hardware_profile
            || now.kernel_build_id != self.boot.kernel_build_id
            || now.observation.boot_id != self.boot.observation.boot_id
            || now.observation.binding != self.boot.observation.binding
            || now.observation.uptime_ms < self.boot.observation.uptime_ms
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

/// A read-only readiness snapshot, not installation authority. Atomic changes
/// to trust, history, data identity or staged bytes invalidate the UI snapshot.
pub(super) struct ReadyProof(Vec<Stamp>);
struct Stamp {
    path: PathBuf,
    owner: u32,
    identity: [u64; 11],
}
impl Stamp {
    fn capture(path: PathBuf) -> Result<Self> {
        Self::capture_owned(path, 0)
    }
    fn capture_owned(path: PathBuf, owner: u32) -> Result<Self> {
        let m = fs::symlink_metadata(&path)?;
        if !m.is_file() || m.uid() != owner || m.mode() & 0o022 != 0 || m.nlink() != 1 {
            return Err(unavailable());
        }
        Ok(Self {
            path,
            owner,
            identity: [
                m.dev(),
                m.ino(),
                m.len(),
                m.uid() as u64,
                m.gid() as u64,
                m.mode() as u64,
                m.nlink(),
                m.mtime() as u64,
                m.mtime_nsec() as u64,
                m.ctime() as u64,
                m.ctime_nsec() as u64,
            ],
        })
    }
    fn matches(&self) -> bool {
        Self::capture_owned(self.path.clone(), self.owner)
            .is_ok_and(|s| s.identity == self.identity)
    }
}

impl Backend for Native {
    type Prepared = Prepared;
    type Ready = ReadyProof;
    fn prepare(
        &mut self,
        active: &Active,
        channel: Channel,
        control: Arc<Control>,
    ) -> std::result::Result<Prepared, PreparationError> {
        lower_priority().map_err(|e| PreparationError::failed("priority", e))?;
        if !control.active() {
            return Err(PreparationError::failed("cancelled", unavailable()));
        }
        let lock = InstallLock::acquire().map_err(prepare_lock_error)?;
        let context =
            Context::open(lock).map_err(|e| PreparationError::failed("boot-context", e))?;
        let trust =
            TrustStore::open(TRUST.as_ref()).map_err(|e| PreparationError::failed("trust", e))?;
        let staging = StagingStore::open(&context.boot.layout)
            .map_err(|e| PreparationError::failed("staging", e))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PreparationError::failed("runtime", e))?;
        let session = {
            let _entered = runtime.enter();
            download::DownloadSession::new_at(control.clone(), active.started_ms)
                .map_err(|e| PreparationError::failed("http-session", e))?
        };
        if !control.active()
            || self
                .clock()
                .map_err(|e| PreparationError::failed("clock", e))?
                .ms
                >= active.admission_deadline_ms
        {
            return Err(PreparationError::failed(
                "admission-deadline",
                unavailable(),
            ));
        }
        Ok(Prepared {
            runtime,
            session,
            context,
            trust,
            staging,
            channel,
        })
    }
    fn run(
        mut prepared: Prepared,
        control: Arc<Control>,
    ) -> engine_ota::Result<Finished<ReadyProof>> {
        lower_priority().map_err(|_| interrupted())?;
        control.wait_ready()?;
        prepared.context.check().map_err(|_| interrupted())?;
        let mut outcome = prepared.runtime.block_on(prepared.session.run(
            &mut prepared.trust,
            &mut prepared.staging,
            prepared.channel,
            &prepared.context.device,
            &prepared.context.history,
        ))?;
        prepared.context.check().map_err(|_| interrupted())?;
        use download::Control as _;
        let time = control
            .checkpoint()?
            .ok_or(engine_ota::Error::WaitingForTime)?;
        prepared.trust.admit_time(time)?;
        if let download::Outcome::Ready { candidate } = &outcome {
            let id = candidate.id();
            let refreshed = prepared.runtime.block_on(prepared.trust.cached(
                Some(time),
                prepared.channel,
                &prepared.context.device,
                &prepared.context.history,
            ))?;
            let Selection::Available { candidate } = refreshed else {
                return Err(engine_ota::Error::CandidateChanged);
            };
            if candidate.id() != id {
                return Err(engine_ota::Error::CandidateChanged);
            }
            outcome = download::Outcome::Ready { candidate };
        }
        let time = control
            .checkpoint()?
            .ok_or(engine_ota::Error::WaitingForTime)?;
        prepared.trust.admit_time(time)?;
        let proof = if let download::Outcome::Ready { candidate } = &outcome {
            if time.0 >= candidate.expires() {
                return Err(engine_ota::Error::Download(
                    "metadata expired after verification",
                ));
            }
            let paths = [
                PathBuf::from(IMAGE),
                PathBuf::from(DATA_SCHEMA),
                PathBuf::from(RECEIPT_PATH),
                Path::new(crate::recovery_service::RECOVERY_DIRECTORY).join("state.json"),
                Path::new(TRUST).join("state.json"),
                Path::new(STAGING).join("candidate.json"),
                Path::new(STAGING)
                    .join(format!("{}.ext4.zst", candidate.image().compressed.sha256)),
            ];
            let stamps = paths
                .into_iter()
                .map(Stamp::capture)
                .collect::<Result<Vec<_>>>()
                .map_err(|_| interrupted())?;
            Some(ReadyProof(stamps))
        } else {
            None
        };
        // Check cancellation and the original connection/deadline bounds after
        // persistence without observing another, unretained wall-clock reading.
        if !control.route()? {
            return Err(interrupted());
        }
        Ok(Finished {
            outcome,
            proof,
            time,
        })
    }
    fn ready_valid(&mut self, proof: &ReadyProof) -> bool {
        proof.0.iter().all(Stamp::matches)
    }
    fn admit(&mut self, token: &HoldToken, request: &DownloadReservation) -> Result<()> {
        self.operation(
            network_service::Request::AdmitDownload {
                schema: 1,
                token: token.expose().into(),
                deadline: request.deadline.clone(),
                admission_deadline_ms: request.admission_deadline_ms,
                mode_generation: request.mode_generation,
                time_hold: request.time_hold.as_ref().map(|t| t.expose().into()),
                handoff_deadline_ms: request.handoff_deadline_ms,
            },
            Some(
                request
                    .admission_deadline_ms
                    .min(request.handoff_deadline_ms.unwrap_or(u64::MAX)),
            ),
        )
    }
    fn renew(&mut self, token: &HoldToken, deadline_ms: u64) -> Result<()> {
        self.operation(
            network_service::Request::Renew {
                schema: 1,
                token: token.expose().into(),
            },
            Some(deadline_ms),
        )
    }
    fn release(&mut self, token: &HoldToken) -> Result<()> {
        self.operation(
            network_service::Request::Release {
                schema: 1,
                token: token.expose().into(),
            },
            None,
        )
    }
}

fn prepare_lock_error(error: engine_ota::Error) -> PreparationError {
    match error {
        engine_ota::Error::Busy => PreparationError::MaintenanceBusy,
        error => PreparationError::failed("install-lock", error),
    }
}

fn lower_priority() -> Result<()> {
    // Linux nice/IO priorities are per-thread. These only lower our own worker's
    // priority; radio/time observation and the separate client remain unaffected.
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let result = unsafe { libc::syscall(libc::SYS_ioprio_set, 1, 0, 3 << 13) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // Explicit owner diagnostic for an idle, already provisioned device. This
    // opens existing stores but never runs HTTP, admits a hold, installs or
    // reboots. Do not include it in unattended root/namespace test selections.
    #[test]
    #[ignore = "root owner qualification on an idle, provisioned OTA device"]
    fn installed_preparation_recognizes_maintenance_and_reopens_after_release() {
        use crate::service::ObservedState;
        use crate::time::{TimeReason, TimeRecord, TimeTracker};
        let boot_id = linux::boot_id().unwrap();
        let reading = linux::clock_reading().unwrap();
        let mut native = Native::new(
            Arc::new(Mutex::new(ObservedState {
                tracker: TimeTracker::new(
                    boot_id.clone(),
                    TimeRecord::default(),
                    TimeReason::WaitingForNetworkSync,
                    reading,
                    None,
                ),
                boot_id: boot_id.clone(),
                provider_error: None,
                persistence_error: None,
                rtc_error: None,
                observed_at: Instant::now(),
            })),
            Watch::default(),
            boot_id,
        );
        let now = native.clock().unwrap();
        let active = Active {
            id: 1,
            trigger: schedule::Trigger::Manual,
            started_ms: now.ms,
            admission_deadline_ms: now.ms + schedule::ADMISSION_MS,
            preparation_deadline_ms: now.ms + schedule::PREPARATION_MS,
            deadline_ms: now.ms + schedule::SESSION_MS,
            admitted: false,
            payload_started: false,
        };
        let control = Arc::new(Control::new(
            Arc::new(native.clone()),
            now,
            active.deadline_ms,
            Arc::new(AtomicBool::new(false)),
        ));
        let held = InstallLock::acquire().expect("owner probe requires idle maintenance");
        assert!(matches!(
            native.prepare(&active, Channel::Release, control.clone()),
            Err(PreparationError::MaintenanceBusy)
        ));
        drop(held);
        let prepared = native
            .prepare(&active, Channel::Release, control)
            .expect("idle provisioned device must pass native preparation");
        prepared.context.check().unwrap();
    }

    #[test]
    fn only_install_lock_contention_is_retryable_and_diagnostics_are_redacted() {
        assert!(matches!(
            prepare_lock_error(engine_ota::Error::Busy),
            PreparationError::MaintenanceBusy
        ));
        assert!(matches!(
            PreparationError::failed("trust", engine_ota::Error::Busy),
            PreparationError::Failed {
                stage: "trust",
                category: "busy",
                ..
            }
        ));
        assert!(matches!(
            prepare_lock_error(std::io::Error::from_raw_os_error(libc::EACCES).into()),
            PreparationError::Failed {
                stage: "install-lock",
                category: "io",
                os_error: Some(libc::EACCES)
            }
        ));
        let error = PreparationError::failed(
            "boot-context",
            engine_ota::Error::Store("private contents".into()),
        );
        assert!(matches!(
            error,
            PreparationError::Failed {
                stage: "boot-context",
                category: "store",
                os_error: None
            }
        ));
        assert!(!format!("{error:?}").contains("private contents"));
    }

    #[test]
    fn ready_snapshot_detects_replacement_edits_permissions_and_links() {
        let temp = tempfile::tempdir().unwrap();
        let owner = unsafe { libc::geteuid() };
        for fault in ["replace", "edit", "permissions", "link", "delete"] {
            let path = temp.path().join(fault);
            fs::write(&path, b"retained evidence").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let stamp = Stamp::capture_owned(path.clone(), owner).unwrap();
            assert!(stamp.matches());
            match fault {
                "replace" => {
                    let new = temp.path().join("new");
                    fs::write(&new, b"retained evidence").unwrap();
                    fs::rename(new, &path).unwrap();
                }
                "edit" => fs::write(&path, b"new content").unwrap(),
                "permissions" => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o622)).unwrap()
                }
                "link" => fs::hard_link(&path, temp.path().join("linked")).unwrap(),
                _ => fs::remove_file(&path).unwrap(),
            }
            assert!(!stamp.matches(), "{fault}");
        }
    }
}
