//! Separate root service: disk and health work cannot stall time/status queries.
use crate::kiosk_health::{KioskExpectation, KioskObserver};
use crate::recovery_status::Reporter;
use crate::{Error, Result, linux};
use engine_ota::boot_linux::BootController;
use engine_ota::coordinator::{self, RecoveryBackend, RecoveryProgress, RecoveryStage};
use engine_ota::install_lock::InstallLock;
use engine_ota::recovery::{Action, AppInstance, BootObservation, HealthSample};
use engine_ota::{RecoveryStore, Sha256, WatchdogCompletion};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;

pub const RECOVERY_DIRECTORY: &str = "/data/spacewars-system/recovery";

pub fn run() -> Result<()> {
    let reporter = Reporter::open(linux::boot_id()?)?;
    reporter.publish(RecoveryProgress::new(RecoveryStage::Starting))?;
    let result = run_with(&reporter);
    if let Err(error) = &result {
        let mut progress = RecoveryProgress::new(RecoveryStage::Blocked);
        progress.detail = Some(error.to_string().chars().take(512).collect());
        let _ = reporter.publish(progress);
    }
    result
}

fn run_with(reporter: &Reporter) -> Result<()> {
    let parent = fs::symlink_metadata("/data/spacewars-system")?;
    if !parent.is_dir() || parent.uid() != 0 || parent.mode() & 0o077 != 0 {
        return Err(Error::Invalid(
            "recovery requires private root-owned system data".into(),
        ));
    }
    match fs::symlink_metadata(RECOVERY_DIRECTORY) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            reporter.publish(RecoveryProgress::new(RecoveryStage::Unprovisioned))?;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    // Missing or damaged journal contents are an error, never bootstrap input.
    let lock = InstallLock::acquire()?;
    let boot = BootController::open(&lock)?;
    let image = boot.observe()?.image;
    let mut store = RecoveryStore::open(Path::new(RECOVERY_DIRECTORY))?;
    let mut backend = LinuxBackend {
        boot,
        expected: KioskExpectation::from_image(&image)?,
        observer: KioskObserver::default(),
    };
    coordinator::recover(&mut store, &mut backend, |progress| {
        if let Err(error) = reporter.publish(progress) {
            // Reporting cannot undo a durable transition or prevent rollback.
            tracing::warn!(%error, "could not publish recovery progress");
        }
    })?;
    Ok(())
}

struct LinuxBackend<'a> {
    boot: BootController<'a>,
    expected: KioskExpectation,
    observer: KioskObserver,
}

impl RecoveryBackend for LinuxBackend<'_> {
    fn observe(&mut self) -> engine_ota::Result<BootObservation> {
        Ok(self.boot.observe()?.observation)
    }
    fn verify_early_watchdog(&mut self) -> engine_ota::Result<()> {
        self.boot.trial_watchdog().map(|_| ())
    }
    fn health(&mut self, boot: &BootObservation) -> engine_ota::Result<HealthSample> {
        let sample = self.observer.observe(&self.expected).map_err(ota_error)?;
        if sample.boot_id != boot.boot_id || Some(&sample.build_id) != boot.actual_build_id.as_ref()
        {
            return Err(ota_error("health worker described another boot or image"));
        }
        Ok(HealthSample {
            uptime_ms: sample.uptime_ms,
            instance: AppInstance {
                pid: sample.process_id,
                start_ticks: sample.start_ticks,
                invocation_id: sample.invocation_id,
            },
            build_id: sample.build_id,
            executable_verified: true,
            state_writable: true,
            control_response: sample.control_response,
            presented_frames: sample.presented_frames,
        })
    }
    fn apply(
        &mut self,
        store: &RecoveryStore,
        action: &Action,
    ) -> engine_ota::Result<BootObservation> {
        Ok(self.boot.apply(store, action)?.observation)
    }
    fn complete_watchdog(
        &mut self,
        store: &RecoveryStore,
    ) -> engine_ota::Result<WatchdogCompletion> {
        self.boot.complete_watchdog(store)
    }
    fn reboot_previous_good(
        &mut self,
        store: &RecoveryStore,
        transaction: Sha256,
    ) -> engine_ota::Result<()> {
        self.boot.authorize_rollback_reboot(store, transaction)?;
        futures_lite::future::block_on(futures_lite::future::or(
            normal_reboot(|| self.boot.authorize_rollback_reboot(store, transaction)),
            async {
                async_io::Timer::after(Duration::from_secs(5)).await;
                Err(ota_error(
                    "normal reboot request timed out; early watchdog remains active",
                ))
            },
        ))
    }
}

async fn normal_reboot(
    authorize: impl FnOnce() -> engine_ota::Result<()>,
) -> engine_ota::Result<()> {
    let connection = zbus::connection::Builder::system()
        .map_err(ota_error)?
        .method_timeout(Duration::from_secs(2))
        .build()
        .await
        .map_err(ota_error)?;
    normal_reboot_on(&connection, authorize).await
}

async fn normal_reboot_on(
    connection: &zbus::Connection,
    authorize: impl FnOnce() -> engine_ota::Result<()>,
) -> engine_ota::Result<()> {
    let login = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(ota_error)?;
    // systemd retains reboot-param in /run. Explicitly clear a stale tryboot
    // argument through logind, then read it back before requesting normal boot.
    login
        .call::<_, _, ()>("SetRebootParameter", &("",))
        .await
        .map_err(ota_error)?;
    let properties = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(ota_error)?;
    let value: zbus::zvariant::OwnedValue = properties
        .call(
            "Get",
            &("org.freedesktop.login1.Manager", "RebootParameter"),
        )
        .await
        .map_err(ota_error)?;
    let parameter: String = value.try_into().map_err(ota_error)?;
    if !parameter.is_empty() {
        return Err(ota_error("normal reboot parameter did not clear"));
    }
    authorize()?;
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(ota_error)?;
    let _: zbus::zvariant::OwnedObjectPath = manager
        .call("StartUnit", &("reboot.target", "replace-irreversibly"))
        .await
        .map_err(ota_error)?;
    Ok(())
}

fn ota_error(error: impl std::fmt::Display) -> engine_ota::Error {
    engine_ota::Error::Boot(error.to_string())
}

#[cfg(test)]
#[path = "recovery_service_tests.rs"]
mod tests;
