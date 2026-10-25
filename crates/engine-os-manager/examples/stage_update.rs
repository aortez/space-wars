//! Explicit owner-run signed feed/download check using native boot and time
//! evidence. Never writes a root partition, selects a boot slot or reboots.
//! Run over the owner's maintained SSH connection; no radio policy is changed.
#[cfg(target_os = "linux")]
#[path = "stage_update/request.rs"]
mod request;

#[cfg(target_os = "linux")]
mod linux {
    use super::request::Request;
    use engine_os_manager::{
        linux,
        time::{TimeReason, TimeRecord, TimeTracker},
    };
    use engine_ota::{
        DeviceIdentity, Error, RecoveryStore, Result, TrustStore, UsableTime,
        boot_linux::BootController,
        download::{Control, DownloadSession, Progress},
        install_lock::InstallLock,
        provision_linux,
        staging_linux::StagingStore,
    };
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    struct Clock(Mutex<TimeTracker>);
    impl Control for Clock {
        fn checkpoint(&self) -> Result<Option<UsableTime>> {
            let now = linux::clock_reading().map_err(|e| Error::Boot(e.to_string()))?;
            let mut tracker = self.0.lock().map_err(|_| Error::WaitingForTime)?;
            tracker.tick(now);
            let status = tracker.status(now);
            if !status.usable {
                return Ok(None);
            }
            status
                .utc_us
                .map(UsableTime::from_unix_microseconds)
                .transpose()
        }

        fn progress(&self, progress: Progress) {
            eprintln!(
                "{}",
                serde_json::to_string(&progress).expect("progress serializes")
            );
        }
    }

    pub async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let request = Request::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
        if unsafe { libc::geteuid() } != 0 {
            return Err("staging requires separately authenticated root operator access".into());
        }
        let lock = InstallLock::acquire()?;
        engine_ota::bootstrap::require_no_active_bootstrap(&lock)?;
        let recovery = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
        let controller = BootController::open(&lock)?;
        controller.authorize_maintenance(&recovery)?;
        // This temporary verifier view uses the same bounded explicit retry as
        // the signed installer. Never lower or rewrite the durable journal.
        let history = request.history(recovery.record())?;
        let boot = controller.observe()?;
        let setup = provision_linux::inspect(&boot.layout, &boot.observation.binding)?;
        let observation = linux::observe_sync().map_err(std::io::Error::other)?;
        let mut tracker = TimeTracker::new(
            boot.observation.boot_id.clone(),
            TimeRecord::default(),
            TimeReason::WaitingForNetworkSync,
            observation.now,
            None,
        );
        if !tracker.observe(&observation) {
            return Err("a current native successful NTP receipt is required".into());
        }
        let control = Arc::new(Clock(Mutex::new(tracker)));
        let session = DownloadSession::new(control.clone())?;
        let device = DeviceIdentity {
            machine: boot.image.machine.clone(),
            architecture: boot.image.architecture.clone(),
            layout: boot.image.layout.clone(),
            boot_identity: boot.image.boot_identity,
            updater_protocol: boot.image.updater_protocol,
            recovery_protocol: boot.observation.recovery_protocol,
            inactive_slot_bytes: boot
                .layout
                .inactive_partition()
                .sectors
                .checked_mul(512)
                .ok_or("slot size overflow")?,
            persistent_schema: setup.persistent_schema,
            rollback_reads: recovery.record().good().data.reads,
        };
        let mut trust = TrustStore::open(Path::new(provision_linux::TRUST_PATH))?;
        let mut staging = StagingStore::open(&boot.layout)?;
        let before = recovery.record().clone();
        let outcome = session
            .run(&mut trust, &mut staging, request.channel, &device, &history)
            .await?;
        request.check_outcome(&outcome)?;
        controller.authorize_maintenance(&recovery)?;
        let after = controller.observe()?;
        if after.observation.boot_id != boot.observation.boot_id
            || after.observation.binding != boot.observation.binding
            || after.image != boot.image
            || after.layout != boot.layout
            || recovery.record() != &before
            || provision_linux::inspect(&after.layout, &after.observation.binding)?
                .persistent_schema
                != setup.persistent_schema
            || control.checkpoint()?.is_none()
        {
            return Err("installed state or time changed during the supervised download".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": 1, "channel": request.channel, "boot_id": boot.observation.boot_id,
                "retry_payload": request.retry_payload,
                "bundle": setup.bundle, "initial_root": setup.initial_root,
                "outcome": outcome,
                "recovery_record_sha256": engine_ota::Sha256::digest(&serde_json::to_vec(&before)?),
                "time_observation": observation,
                "scope": "Owner-run native HTTPS/TUF/staging check; no root write, trial or reboot",
                "time_source": "native successful NTP receipt validated in memory",
                "installed_time_service_replaced": false
            }))?
        );
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run().await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("stage_update requires Linux");
    std::process::exit(1);
}
