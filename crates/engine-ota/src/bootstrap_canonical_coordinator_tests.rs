use super::*;
use crate::bootstrap::canonical_files::tests::Migration;
use crate::bootstrap::trial_store::tests::sample;

struct Backend {
    boot: CanonicalObservation,
    samples: u64,
    healthy: bool,
    events: Vec<&'static str>,
}
impl CanonicalBackend for Backend {
    fn observe(&mut self, _: &TrialStore) -> Result<CanonicalObservation> {
        Ok(self.boot.clone())
    }
    fn health(&mut self, _: &CanonicalObservation) -> Result<HealthSample> {
        self.samples += 1;
        if self.healthy {
            Ok(sample(&self.boot.boot, self.samples))
        } else {
            Err(invalid("app unresponsive"))
        }
    }
    fn restore_staged(&mut self, store: &TrialStore) -> Result<CanonicalObservation> {
        assert_eq!(store.canonical()?.phase, CanonicalPhase::Failed);
        self.events.push("staged-default");
        self.boot.default = CanonicalDefault::Staged;
        Ok(self.boot.clone())
    }
    fn reboot_staged(&mut self, store: &TrialStore) -> Result<()> {
        store
            .canonical()?
            .authorize_reboot(&store.durable()?.inputs, &self.boot, true)?;
        self.events.push("normal-reboot");
        Ok(())
    }
    fn wait_for_sample(&mut self) {
        self.boot.boot.uptime_ms += 2_000;
    }
}

#[test]
fn canonical_health_requires_a_distinct_boot_and_never_reuses_staged_health() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    let selected = fixture.selected(&mut store);
    let mut backend = Backend {
        boot: selected,
        samples: 0,
        healthy: true,
        events: vec![],
    };
    assert!(recover_canonical(&mut store, &mut backend, |_, _| {}).is_err());
    backend.boot.boot.boot_id = "cc606400-dc75-446e-b26b-385592b0f562".into();
    backend.boot.boot.uptime_ms = 5_000;
    assert_eq!(
        recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
        CanonicalOutcome::Confirmed
    );
    assert_eq!(backend.samples, 31);
    assert!(backend.events.is_empty());
    assert_eq!(store.canonical_phase(), Some(CanonicalPhase::Confirmed));
    assert_eq!(
        recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
        CanonicalOutcome::Confirmed
    );
    assert_eq!(backend.samples, 31);
    backend.boot.boot.boot_id = "dd606400-dc75-446e-b26b-385592b0f562".into();
    backend.boot.boot.uptime_ms = 5_000;
    assert_eq!(
        recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
        CanonicalOutcome::Confirmed
    );
    assert_eq!(backend.samples, 62);
}

#[test]
fn unhealthy_or_late_canonical_observer_returns_to_stage_without_a_retry_loop() {
    for healthy in [true, false] {
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        let mut selected = fixture.selected(&mut store);
        selected.boot.boot_id = "cc606400-dc75-446e-b26b-385592b0f562".into();
        selected.boot.uptime_ms = if healthy { 150_000 } else { 5_000 };
        let mut backend = Backend {
            boot: selected,
            samples: 0,
            healthy,
            events: vec![],
        };
        assert_eq!(
            recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
            CanonicalOutcome::StagedRebootRequested
        );
        assert_eq!(backend.events, ["staged-default", "normal-reboot"]);
        assert_eq!(
            store.canonical().unwrap().failure,
            Some(CanonicalFailure::CanonicalHealthTimeout)
        );
        backend.boot.boot.boot_id = "dd606400-dc75-446e-b26b-385592b0f562".into();
        backend.boot.boot.uptime_ms = 5_000;
        assert_eq!(
            recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
            CanonicalOutcome::NeedsOperator
        );
        assert_eq!(backend.events.len(), 2);
    }
}

#[test]
fn interrupted_copy_boot_rechecks_stage_health_without_resetting_copy_progress() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    assert!(
        fixture
            .prepare(&mut store, |p| if p == "canonical-file-completed" {
                Err(invalid("stop"))
            } else {
                Ok(())
            })
            .is_err()
    );
    let completed = store.canonical().unwrap().completed.clone();
    let mut boot = fixture.normal.clone();
    boot.boot.boot_id = "dd606400-dc75-446e-b26b-385592b0f562".into();
    boot.boot.uptime_ms = 5_000;
    assert!(
        store
            .canonical()
            .unwrap()
            .authorize_copy(&fixture.fixture.inputs, &boot)
            .is_err()
    );
    let mut backend = Backend {
        boot,
        samples: 0,
        healthy: true,
        events: vec![],
    };
    assert_eq!(
        recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
        CanonicalOutcome::AwaitingMigration
    );
    assert_eq!(backend.samples, 31);
    assert!(backend.events.is_empty());
    assert_eq!(store.canonical().unwrap().completed, completed);
    store
        .canonical()
        .unwrap()
        .authorize_copy(&fixture.fixture.inputs, &backend.boot)
        .unwrap();
}

#[test]
fn lost_canonical_reboot_reply_consumes_request_and_late_boot_returns_to_stage() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    let selected = fixture.selected(&mut store);
    store
        .advance_canonical(|c, i| c.request_reboot(i, &selected))
        .unwrap();
    store
        .advance_canonical(|c, i| c.fail(i, &selected, CanonicalFailure::RebootRequestFailed))
        .unwrap();
    assert!(
        store
            .advance_canonical(|c, i| c.request_reboot(i, &selected))
            .is_err()
    );
    let mut boot = selected;
    boot.boot.boot_id = "cc606400-dc75-446e-b26b-385592b0f562".into();
    boot.boot.uptime_ms = 5_000;
    let mut backend = Backend {
        boot,
        samples: 0,
        healthy: true,
        events: vec![],
    };
    assert_eq!(
        recover_canonical(&mut store, &mut backend, |_, _| {}).unwrap(),
        CanonicalOutcome::StagedRebootRequested
    );
    assert_eq!(backend.samples, 0);
}
