use super::*;
use crate::recovery::{AppInstance, RecoveryError, Slot};

struct Backend {
    boot: BootObservation,
    events: Vec<&'static str>,
    healthy: bool,
    early: bool,
    fail_apply: bool,
    fail_stop: bool,
    sample_count: u64,
    bad_sample_at: Option<u64>,
}

impl RecoveryBackend for Backend {
    fn observe(&mut self) -> Result<BootObservation> {
        Ok(self.boot.clone())
    }
    fn verify_early_watchdog(&mut self) -> Result<()> {
        self.events.push("read-early");
        self.early
            .then_some(())
            .ok_or_else(|| invalid("missing early record"))
    }
    fn health(&mut self, _: &BootObservation) -> Result<HealthSample> {
        self.events.push("health");
        self.sample_count += 1;
        if !self.healthy || self.bad_sample_at == Some(self.sample_count) {
            return Err(invalid("fixture app is not responsive"));
        }
        Ok(HealthSample {
            uptime_ms: self.boot.uptime_ms,
            instance: AppInstance {
                pid: 123,
                start_ticks: 1,
                invocation_id: "a".repeat(32),
            },
            build_id: self.boot.actual_build_id.clone().unwrap(),
            executable_verified: true,
            state_writable: true,
            control_response: self.sample_count,
            presented_frames: self.sample_count * 10,
        })
    }
    fn apply(&mut self, store: &RecoveryStore, action: &Action) -> Result<BootObservation> {
        let (slot, trial) = store
            .durable_record()?
            .selection_change(action, &self.boot)?;
        assert!(trial.is_none());
        self.events.push("selector");
        self.boot.default_slot = Some(slot);
        if self.fail_apply {
            return Err(invalid(
                "fixture selector readback interrupted after rename",
            ));
        }
        Ok(self.boot.clone())
    }
    fn complete_watchdog(&mut self, store: &RecoveryStore) -> Result<WatchdogCompletion> {
        store.durable_record()?.watchdog_completion(&self.boot)?;
        self.events.push("stop-watchdog");
        if self.fail_stop {
            return Err(invalid("fixture disable failed"));
        }
        Ok(WatchdogCompletion::Stopped)
    }
    fn reboot_previous_good(&mut self, store: &RecoveryStore, id: Sha256) -> Result<()> {
        store.durable_record()?.rollback_reboot(id, &self.boot)?;
        self.events.push("normal-reboot");
        Ok(())
    }
    fn wait_for_sample(&mut self) {
        self.boot.uptime_ms += 2_000;
    }
}

fn setup() -> (tempfile::TempDir, RecoveryStore, Backend) {
    let directory = tempfile::tempdir().unwrap();
    let (record, boot) = crate::recovery::tests::trial();
    let store = RecoveryStore::initialize(&directory.path().join("recovery"), record).unwrap();
    let backend = Backend {
        boot,
        events: Vec::new(),
        healthy: true,
        early: true,
        fail_apply: false,
        fail_stop: false,
        sample_count: 0,
        bad_sample_at: None,
    };
    (directory, store, backend)
}

#[test]
fn actual_journal_orders_health_selector_commit_and_watchdog_completion() {
    let (directory, mut store, mut backend) = setup();
    let mut progress = Vec::new();
    assert_eq!(
        recover(&mut store, &mut backend, |p| progress.push(p.stage)).unwrap(),
        RecoveryOutcome::Healthy
    );
    assert_eq!(backend.boot.uptime_ms, 80_000);
    assert_eq!(backend.sample_count, 31);
    assert_eq!(
        &backend.events[backend.events.len() - 2..],
        &["selector", "stop-watchdog"]
    );
    assert_eq!(progress.last(), Some(&RecoveryStage::Healthy));
    assert!(!backend.events.contains(&"normal-reboot"));
    drop(store);
    let mut store = RecoveryStore::open(&directory.path().join("recovery")).unwrap();
    assert_eq!(
        store.record().transaction().unwrap().phase,
        Phase::Committed
    );
    backend.events.clear();
    backend.boot.uptime_ms += 86_400_000;
    recover(&mut store, &mut backend, |_| {}).unwrap();
    assert_eq!(backend.events, ["stop-watchdog"]);
}

#[test]
fn failure_and_missing_watchdog_reboot_without_refresh_or_promotion() {
    for early in [true, false] {
        let (_directory, mut store, mut backend) = setup();
        backend.healthy = false;
        backend.early = early;
        assert_eq!(
            recover(&mut store, &mut backend, |_| {}).unwrap(),
            RecoveryOutcome::RebootRequested
        );
        let tx = store.record().transaction().unwrap();
        assert_eq!(tx.phase, Phase::RollbackPending);
        assert_eq!(
            tx.failure,
            Some(if early {
                FailureReason::HealthTimeout
            } else {
                FailureReason::WatchdogUnavailable
            })
        );
        assert_eq!(backend.boot.default_slot, Some(Slot::A));
        assert_eq!(backend.events.last(), Some(&"normal-reboot"));
        assert!(!backend.events.contains(&"selector"));
        assert!(!backend.events.contains(&"stop-watchdog"));
        if early {
            assert_eq!(backend.boot.uptime_ms, TRIAL_DEADLINE_MS);
        } else {
            assert!(!backend.events.contains(&"health"));
        }
    }
}

#[test]
fn failed_sample_restarts_window_and_late_service_start_cannot_extend_deadline() {
    let (_directory, mut store, mut backend) = setup();
    backend.bad_sample_at = Some(30);
    recover(&mut store, &mut backend, |_| {}).unwrap();
    assert_eq!(backend.boot.uptime_ms, 140_000);
    let (_directory, mut store, mut backend) = setup();
    backend.boot.uptime_ms = 140_000;
    assert_eq!(
        recover(&mut store, &mut backend, |_| {}).unwrap(),
        RecoveryOutcome::RebootRequested
    );
    assert_eq!(backend.boot.uptime_ms, TRIAL_DEADLINE_MS);
    assert!(!backend.events.contains(&"stop-watchdog"));
}

#[test]
fn selector_error_leaves_durable_proof_and_restart_reconciles_actual_choice() {
    let (directory, mut store, mut backend) = setup();
    backend.fail_apply = true;
    assert!(recover(&mut store, &mut backend, |_| {}).is_err());
    assert_eq!(
        store.record().transaction().unwrap().phase,
        Phase::Promoting
    );
    assert!(!backend.events.contains(&"stop-watchdog"));
    drop(store);
    let mut store = RecoveryStore::open(&directory.path().join("recovery")).unwrap();
    backend.events.clear();
    recover(&mut store, &mut backend, |_| {}).unwrap();
    assert_eq!(backend.events, ["stop-watchdog"]);
    assert_eq!(
        store.record().transaction().unwrap().phase,
        Phase::Committed
    );
}

#[test]
fn stop_failure_does_not_report_success_or_undo_the_committed_good_root() {
    let (_directory, mut store, mut backend) = setup();
    backend.fail_stop = true;
    let mut progress = Vec::new();
    assert!(recover(&mut store, &mut backend, |p| progress.push(p.stage)).is_err());
    assert!(!progress.contains(&RecoveryStage::Healthy));
    assert_eq!(
        store.record().transaction().unwrap().phase,
        Phase::Committed
    );
    assert_eq!(backend.boot.default_slot, Some(Slot::B));
    assert!(!backend.events.contains(&"normal-reboot"));
}

#[test]
fn rollback_reboot_requires_recorded_failure_and_restored_previous_default() {
    let (_directory, mut store, mut backend) = setup();
    let tx = store.record().transaction().unwrap().id;
    assert_eq!(
        store.record().rollback_reboot(tx, &backend.boot),
        Err(RecoveryError::StaleCommand)
    );
    backend.boot.default_slot = Some(Slot::B);
    let action = store
        .advance(|r| r.fail(FailureReason::HealthTimeout, &backend.boot))
        .unwrap();
    assert!(store.record().rollback_reboot(tx, &backend.boot).is_err());
    backend.apply(&store, &action).unwrap();
    store.record().rollback_reboot(tx, &backend.boot).unwrap();
    backend.boot.tryboot = false;
    assert!(store.record().rollback_reboot(tx, &backend.boot).is_err());
}
