//! Supervised canonical migration and its independent normal-boot health.
use crate::kiosk_health::{KioskExpectation, KioskObserver};
use crate::recovery_service::{kiosk_health, normal_reboot};
use crate::recovery_status::Reporter;
use engine_ota::bootstrap::{
    self, CanonicalBackend, CanonicalBoot, CanonicalObservation, CanonicalOutcome, CanonicalPhase,
    CanonicalReport, TrialStore,
};
use engine_ota::coordinator::{RecoveryProgress, RecoveryStage};
use engine_ota::install_lock::InstallLock;
use engine_ota::recovery::HealthSample;
use engine_ota::{Result, Sha256};
use std::time::Duration;

pub fn prepare(package: Sha256, written_root: Sha256, trial: Sha256) -> Result<CanonicalReport> {
    bootstrap::prepare_canonical_boot(package, written_root, trial, |point| eprintln!("{point}"))
}

pub fn activate(package: Sha256, written_root: Sha256, trial: Sha256) -> Result<CanonicalReport> {
    bootstrap::activate_canonical_boot(
        package,
        written_root,
        trial,
        |point| eprintln!("{point}"),
        |permit| {
            futures_lite::future::block_on(futures_lite::future::or(
                crate::recovery_service::canonical_reboot(|| permit.authorize_reboot()),
                async {
                    async_io::Timer::after(Duration::from_secs(5)).await;
                    Err(engine_ota::Error::Boot(
                        "canonical reboot request timed out".into(),
                    ))
                },
            ))?;
            permit.reboot_queued()
        },
    )
}

pub(crate) fn recover(
    lock: &InstallLock,
    store: &mut TrialStore,
    reporter: &Reporter,
) -> crate::Result<()> {
    let mut backend = LinuxBackend {
        boot: CanonicalBoot::open(lock, store)?,
        expected: KioskExpectation::from_image(store.image())?,
        observer: KioskObserver::default(),
    };
    let build = store.image().build_id.clone();
    let trial = store.trial_id();
    let publish = |stage, detail: String| {
        let mut progress = RecoveryProgress::new(stage);
        progress.candidate_build_id = Some(build.clone());
        progress.transaction = Some(trial);
        progress.detail = Some(detail.chars().take(512).collect());
        if let Err(error) = reporter.publish(progress) {
            tracing::warn!(%error, "could not publish canonical migration progress");
        }
    };
    let outcome = bootstrap::recover_canonical(store, &mut backend, |phase, detail| {
        publish(
            if phase == CanonicalPhase::Failed {
                RecoveryStage::RollingBack
            } else {
                RecoveryStage::Observing
            },
            format!(
                "supervised canonical migration {phase:?}{}",
                detail.map(|s| format!(": {s}")).unwrap_or_default()
            ),
        );
    })?;
    let (stage, detail) = match outcome {
        CanonicalOutcome::AwaitingMigration => (
            RecoveryStage::BootstrapConfirmed,
            "Staged normal boot remains healthy; root operator canonical migration is pending",
        ),
        CanonicalOutcome::Confirmed => (
            RecoveryStage::BootstrapConfirmed,
            "Separate canonical normal boot confirmed; staged-tree cleanup and OTA provisioning remain pending",
        ),
        CanonicalOutcome::NeedsOperator => (
            RecoveryStage::Blocked,
            "Canonical migration failed; staged default retained and operator reconciliation required",
        ),
        CanonicalOutcome::StagedRebootRequested => (
            RecoveryStage::RebootRequested,
            "Canonical migration requested return to the retained staged normal boot",
        ),
    };
    publish(stage, detail.into());
    Ok(())
}

struct LinuxBackend<'a> {
    boot: CanonicalBoot<'a>,
    expected: KioskExpectation,
    observer: KioskObserver,
}
impl CanonicalBackend for LinuxBackend<'_> {
    fn observe(&mut self, store: &TrialStore) -> Result<CanonicalObservation> {
        self.boot.observe(store)
    }
    fn health(&mut self, boot: &CanonicalObservation) -> Result<HealthSample> {
        kiosk_health(&mut self.observer, &self.expected, &boot.boot)
    }
    fn restore_staged(&mut self, store: &TrialStore) -> Result<CanonicalObservation> {
        self.boot.select(store, false)
    }
    fn reboot_staged(&mut self, store: &TrialStore) -> Result<()> {
        self.boot.authorize_reboot(store, true)?;
        futures_lite::future::block_on(futures_lite::future::or(
            normal_reboot(|| self.boot.authorize_reboot(store, true)),
            async {
                async_io::Timer::after(Duration::from_secs(5)).await;
                Err(engine_ota::Error::Boot(
                    "canonical fallback reboot timed out".into(),
                ))
            },
        ))
    }
}
