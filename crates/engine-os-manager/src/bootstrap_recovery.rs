//! First-image recovery shares kiosk observations and fixed normal reboot work,
//! but has no production OTA journal or qualified previous-good image.
use crate::Result;
use crate::kiosk_health::{KioskExpectation, KioskObserver};
use crate::recovery_service::{kiosk_health, normal_reboot};
use crate::recovery_status::Reporter;
use engine_ota::WatchdogCompletion;
use engine_ota::bootstrap::{
    self, BootstrapBackend, BootstrapBoot, BootstrapOutcome, TrialPhase, TrialStore,
};
use engine_ota::coordinator::{RecoveryProgress, RecoveryStage};
use engine_ota::install_lock::InstallLock;
use engine_ota::recovery::{BootObservation, HealthSample};
use std::time::Duration;

pub(crate) fn run(lock: &InstallLock, store: &mut TrialStore, reporter: &Reporter) -> Result<()> {
    if store.canonical_phase().is_some() {
        return crate::bootstrap_canonical::recover(lock, store, reporter);
    }
    let mut backend = LinuxBackend {
        boot: BootstrapBoot::open(lock, store)?,
        expected: KioskExpectation::from_image(store.image())?,
        observer: KioskObserver::default(),
    };
    let build = store.image().build_id.clone();
    let trial = store.trial_id();
    let publish = |stage, detail| {
        let mut progress = RecoveryProgress::new(stage);
        progress.candidate_build_id = Some(build.clone());
        progress.transaction = Some(trial);
        progress.detail = Some(detail);
        if let Err(error) = reporter.publish(progress) {
            tracing::warn!(%error, "could not publish supervised bootstrap progress");
        }
    };
    let result = bootstrap::recover_bootstrap(store, &mut backend, |phase, detail| {
        let stage = match phase {
            TrialPhase::Staging | TrialPhase::TrialReady | TrialPhase::TrialRequested => {
                RecoveryStage::Starting
            }
            TrialPhase::ObservingTrial | TrialPhase::ObservingNormal => RecoveryStage::Observing,
            TrialPhase::PromotingStaged | TrialPhase::StagedDefault => RecoveryStage::Promoting,
            TrialPhase::NormalConfirmed => RecoveryStage::BootstrapConfirmed,
            TrialPhase::Failed => RecoveryStage::RollingBack,
        };
        let detail = match detail {
            Some(error) => format!("supervised bootstrap {phase:?}: {error}"),
            None => format!("supervised bootstrap {phase:?}"),
        };
        publish(stage, detail.chars().take(512).collect());
    })?;
    match result {
        BootstrapOutcome::NormalConfirmed => publish(
            RecoveryStage::BootstrapConfirmed,
            "Staged normal boot confirmed; canonical migration and OTA provisioning remain pending"
                .into(),
        ),
        BootstrapOutcome::StagedRebootRequested => publish(
            RecoveryStage::RebootRequested,
            "Supervised bootstrap requested a staged normal boot".into(),
        ),
        BootstrapOutcome::LegacyRebootRequested => publish(
            RecoveryStage::RebootRequested,
            "Supervised bootstrap requested the retained legacy default".into(),
        ),
    }
    Ok(())
}

struct LinuxBackend<'a> {
    boot: BootstrapBoot<'a>,
    expected: KioskExpectation,
    observer: KioskObserver,
}

impl BootstrapBackend for LinuxBackend<'_> {
    fn observe(&mut self, store: &TrialStore) -> engine_ota::Result<BootObservation> {
        self.boot.observe(store)
    }
    fn verify_early_watchdog(&mut self, store: &TrialStore) -> engine_ota::Result<()> {
        self.boot.trial_watchdog(store).map(|_| ())
    }
    fn health(&mut self, boot: &BootObservation) -> engine_ota::Result<HealthSample> {
        kiosk_health(&mut self.observer, &self.expected, boot)
    }
    fn select_default(
        &mut self,
        store: &TrialStore,
        staged: bool,
    ) -> engine_ota::Result<BootObservation> {
        self.boot.select_default(store, staged)
    }
    fn complete_watchdog(&mut self, store: &TrialStore) -> engine_ota::Result<WatchdogCompletion> {
        self.boot.complete_watchdog(store)
    }
    fn reboot_normal(&mut self, store: &TrialStore, staged: bool) -> engine_ota::Result<()> {
        self.boot.authorize_reboot(store, staged)?;
        futures_lite::future::block_on(futures_lite::future::or(
            normal_reboot(|| self.boot.authorize_reboot(store, staged)),
            async {
                async_io::Timer::after(Duration::from_secs(5)).await;
                Err(engine_ota::Error::Boot(
                    "bootstrap normal reboot request timed out".into(),
                ))
            },
        ))
    }
}
