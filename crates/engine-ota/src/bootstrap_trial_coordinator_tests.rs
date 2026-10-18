use super::*;
use crate::bootstrap::trial_store::tests::{Fixture, sample};
use crate::recovery::Slot;

struct Backend {
    boot: BootObservation,
    events: Vec<&'static str>,
    samples: u64,
    bad_at: Option<u64>,
    healthy: bool,
    early: bool,
    apply_failure: bool,
    stop_failure: bool,
}

impl Backend {
    fn new(boot: BootObservation) -> Self {
        Self {
            boot,
            events: Vec::new(),
            samples: 0,
            bad_at: None,
            healthy: true,
            early: true,
            apply_failure: false,
            stop_failure: false,
        }
    }
}

impl BootstrapBackend for Backend {
    fn observe(&mut self, _: &TrialStore) -> Result<BootObservation> {
        Ok(self.boot.clone())
    }
    fn verify_early_watchdog(&mut self, _: &TrialStore) -> Result<()> {
        self.events.push("early");
        if self.early {
            Ok(())
        } else {
            Err(invalid("missing early record"))
        }
    }
    fn health(&mut self, _: &BootObservation) -> Result<HealthSample> {
        self.samples += 1;
        self.events.push("health");
        if !self.healthy || self.bad_at == Some(self.samples) {
            return Err(invalid("fixture persistence or UI failure"));
        }
        Ok(sample(&self.boot, self.samples))
    }
    fn select_default(&mut self, store: &TrialStore, staged: bool) -> Result<BootObservation> {
        store.durable()?.selection(&self.boot, staged)?;
        self.events.push(if staged {
            "staged-default"
        } else {
            "legacy-default"
        });
        self.boot.default_slot = Some(if staged { Slot::A } else { Slot::B });
        if self.apply_failure {
            return Err(invalid("selector renamed before interruption"));
        }
        Ok(self.boot.clone())
    }
    fn complete_watchdog(&mut self, store: &TrialStore) -> Result<WatchdogCompletion> {
        store.watchdog_token(&self.boot)?;
        self.events.push("stop-watchdog");
        if self.stop_failure {
            return Err(invalid("watchdog stop failed"));
        }
        Ok(WatchdogCompletion::Stopped)
    }
    fn reboot_normal(&mut self, store: &TrialStore, staged: bool) -> Result<()> {
        store.durable()?.reboot(&self.boot, staged)?;
        self.events.push(if staged {
            "reboot-staged"
        } else {
            "reboot-legacy"
        });
        Ok(())
    }
    fn wait_for_sample(&mut self) {
        self.boot.uptime_ms += 2_000;
    }
}

#[test]
fn trial_health_precedes_durable_promotion_watchdog_and_separate_normal_confirmation() {
    let fixture = Fixture::new();
    let mut store = fixture.open();
    let mut backend = Backend::new(fixture.boot());
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::StagedRebootRequested
    );
    assert_eq!(backend.samples, 31);
    assert_eq!(
        &backend.events[backend.events.len() - 3..],
        ["staged-default", "stop-watchdog", "reboot-staged"]
    );
    assert_eq!(store.phase(), TrialPhase::StagedDefault);
    drop(store);
    let mut store = fixture.open();
    backend.boot.boot_id = "bb606400-dc75-446e-b26b-385592b0f562".into();
    backend.boot.tryboot = false;
    backend.boot.trial_id = None;
    backend.boot.uptime_ms = 5_000;
    backend.events.clear();
    backend.samples = 0;
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::NormalConfirmed
    );
    assert_eq!(backend.samples, 31);
    assert!(backend.events.iter().all(|event| *event == "health"));
    assert_eq!(store.phase(), TrialPhase::NormalConfirmed);
    assert!(
        !fixture
            .temp
            .path()
            .join("data/spacewars-system/recovery")
            .exists()
    );
}

#[test]
fn local_failures_reset_continuity_without_extending_deadline_or_opening_watchdog() {
    let fixture = Fixture::new();
    let mut store = fixture.open();
    let mut backend = Backend::new(fixture.boot());
    backend.bad_at = Some(30);
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::StagedRebootRequested
    );
    assert_eq!(backend.samples, 61);
    for early in [false, true] {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let mut backend = Backend::new(fixture.boot());
        backend.early = early;
        backend.healthy = false;
        assert_eq!(
            recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
            BootstrapOutcome::LegacyRebootRequested
        );
        assert_eq!(store.phase(), TrialPhase::Failed);
        assert!(!backend.events.contains(&"stop-watchdog"));
        assert!(!backend.events.contains(&"staged-default"));
        assert_eq!(backend.samples == 0, !early);
        backend.events.clear();
        backend.healthy = true;
        assert_eq!(
            recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
            BootstrapOutcome::LegacyRebootRequested
        );
        assert_eq!(backend.events, ["reboot-legacy"]);
    }
}

#[test]
fn failed_staged_normal_boot_restores_legacy_and_never_retries_or_claims_ota_health() {
    let fixture = Fixture::new();
    let (mut store, mut boot) = fixture.promoted();
    boot.tryboot = false;
    boot.trial_id = None;
    boot.boot_id = "bb606400-dc75-446e-b26b-385592b0f562".into();
    boot.uptime_ms = 140_000;
    let mut backend = Backend::new(boot);
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::LegacyRebootRequested
    );
    assert_eq!(store.failure(), Some(TrialFailure::HealthTimeout));
    assert_eq!(
        &backend.events[backend.events.len() - 2..],
        ["legacy-default", "reboot-legacy"]
    );
    assert!(!backend.events.contains(&"early"));
    assert!(!backend.events.contains(&"stop-watchdog"));
}

#[test]
fn crash_after_selector_publication_reconciles_and_failed_watchdog_stop_cannot_reboot() {
    let fixture = Fixture::new();
    let mut store = fixture.open();
    let mut backend = Backend::new(fixture.boot());
    backend.apply_failure = true;
    assert!(recover_bootstrap(&mut store, &mut backend, |_, _| {}).is_err());
    assert_eq!(store.phase(), TrialPhase::PromotingStaged);
    assert!(!backend.events.contains(&"stop-watchdog"));
    drop(store);
    let mut store = fixture.open();
    backend.apply_failure = false;
    backend.stop_failure = true;
    backend.events.clear();
    assert!(recover_bootstrap(&mut store, &mut backend, |_, _| {}).is_err());
    assert_eq!(store.phase(), TrialPhase::StagedDefault);
    assert_eq!(backend.events, ["stop-watchdog"]);
    backend.stop_failure = false;
    backend.events.clear();
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::StagedRebootRequested
    );
    assert_eq!(backend.events, ["stop-watchdog", "reboot-staged"]);
}

#[test]
fn startup_restart_does_not_reuse_partial_health_or_accept_another_boot_or_image() {
    let fixture = Fixture::new();
    let mut store = fixture.open();
    let boot = fixture.boot();
    store.advance(|record| record.reconcile(&boot)).unwrap();
    drop(store);
    let mut store = fixture.open();
    let mut backend = Backend::new(boot);
    backend.boot.uptime_ms = 140_000;
    assert_eq!(
        recover_bootstrap(&mut store, &mut backend, |_, _| {}).unwrap(),
        BootstrapOutcome::LegacyRebootRequested
    );
    for case in ["origin", "device", "build", "trial", "slot", "normal"] {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let mut backend = Backend::new(fixture.boot());
        match case {
            "origin" => backend.boot.boot_id = fixture.inputs.origin_boot_id.clone(),
            "device" => backend.boot.binding.device_id = crate::Sha256::digest(b"different device"),
            "build" => backend.boot.actual_build_id = Some("different".into()),
            "trial" => backend.boot.trial_id = Some(crate::Sha256::digest(b"different trial")),
            "slot" => backend.boot.actual_slot = Some(Slot::B),
            "normal" => {
                backend.boot.tryboot = false;
                backend.boot.trial_id = None;
                backend.boot.default_slot = Some(Slot::A);
            }
            _ => unreachable!(),
        }
        assert!(
            recover_bootstrap(&mut store, &mut backend, |_, _| {}).is_err(),
            "{case}"
        );
        assert!(backend.events.is_empty(), "{case}");
    }
}
