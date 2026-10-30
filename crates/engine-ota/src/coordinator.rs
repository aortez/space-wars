//! Startup reconciliation and trial observation, isolated from time/status work.
//! The backend supplies fixed OS operations; policy and durable ordering remain
//! here and in the journal. This entry point cannot begin an installation.
use crate::recovery::{
    Action, BootObservation, FailureReason, HealthMonitor, HealthSample, MAX_HEALTH_GAP_MS, Phase,
    TRIAL_DEADLINE_MS,
};
use crate::{Error, RecoveryStore, Result, Sha256, WatchdogCompletion};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryStage {
    Starting,
    Unprovisioned,
    Observing,
    Promoting,
    RollingBack,
    Healthy,
    BootstrapConfirmed,
    RebootRequested,
    Blocked,
}

impl RecoveryStage {
    pub fn finished(self) -> bool {
        matches!(
            self,
            Self::Unprovisioned
                | Self::Healthy
                | Self::BootstrapConfirmed
                | Self::RebootRequested
                | Self::Blocked
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryProgress {
    pub stage: RecoveryStage,
    pub phase: Option<Phase>,
    pub transaction: Option<Sha256>,
    pub good_build_id: Option<String>,
    pub candidate_build_id: Option<String>,
    /// Generation 0 is unpublished: a development image or bootstrap baseline.
    pub good_generation: Option<u64>,
    pub candidate_generation: Option<u64>,
    pub detail: Option<String>,
}

impl RecoveryProgress {
    pub fn new(stage: RecoveryStage) -> Self {
        Self {
            stage,
            phase: None,
            transaction: None,
            good_build_id: None,
            candidate_build_id: None,
            good_generation: None,
            candidate_generation: None,
            detail: None,
        }
    }

    fn for_store(stage: RecoveryStage, store: &RecoveryStore, detail: Option<String>) -> Self {
        let tx = store.record().transaction();
        Self {
            stage,
            phase: tx.map(|tx| tx.phase),
            transaction: tx.map(|tx| tx.id),
            good_build_id: Some(store.record().good().build_id.clone()),
            candidate_build_id: tx.map(|tx| tx.candidate.build_id.clone()),
            good_generation: Some(store.record().good().generation),
            candidate_generation: tx.map(|tx| tx.candidate.generation),
            detail: detail.map(|text| text.chars().take(512).collect()),
        }
    }
}

/// Implementations must use real boot evidence and independently checked app
/// samples. No values here originate in an unprivileged request.
pub trait RecoveryBackend {
    fn observe(&mut self) -> Result<BootObservation>;
    fn verify_early_watchdog(&mut self) -> Result<()>;
    fn health(&mut self, boot: &BootObservation) -> Result<HealthSample>;
    fn apply(&mut self, store: &RecoveryStore, action: &Action) -> Result<BootObservation>;
    fn complete_watchdog(&mut self, store: &RecoveryStore) -> Result<WatchdogCompletion>;
    fn reboot_previous_good(&mut self, store: &RecoveryStore, transaction: Sha256) -> Result<()>;
    fn wait_for_sample(&mut self) {
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryOutcome {
    Healthy,
    RebootRequested,
}

/// Reconcile exactly once on startup. A restarted observer gets a fresh health
/// window but inherits the original boot deadline and untouched early watchdog.
pub fn recover(
    store: &mut RecoveryStore,
    backend: &mut impl RecoveryBackend,
    mut report: impl FnMut(RecoveryProgress),
) -> Result<RecoveryOutcome> {
    let boot = backend.observe()?;
    let mut action = store.advance(|record| record.reconcile(&boot))?;
    // A valid startup path needs at most observe -> promote -> done, or
    // observe -> restore -> reboot. Bound accidental adapter/action loops.
    for _ in 0..6 {
        match action {
            Action::None => {
                if backend.observe()?.tryboot {
                    backend.complete_watchdog(store)?;
                }
                report(RecoveryProgress::for_store(
                    RecoveryStage::Healthy,
                    store,
                    None,
                ));
                return Ok(RecoveryOutcome::Healthy);
            }
            Action::ObserveTrial { .. } => {
                action = observe_trial(store, backend, &mut report)?;
            }
            Action::PromoteDefault { .. } => {
                report(RecoveryProgress::for_store(
                    RecoveryStage::Promoting,
                    store,
                    None,
                ));
                let boot = backend.apply(store, &action)?;
                action = store.advance(|record| record.finish_promotion(&boot))?;
            }
            Action::RestoreDefault { .. } => {
                report(RecoveryProgress::for_store(
                    RecoveryStage::RollingBack,
                    store,
                    None,
                ));
                let boot = backend.apply(store, &action)?;
                let reason = store
                    .record()
                    .transaction()
                    .and_then(|tx| tx.failure)
                    .ok_or_else(|| invalid("rollback has no retained failure"))?;
                action = store.advance(|record| record.fail(reason, &boot))?;
            }
            Action::RebootPreviousGood { transaction } => {
                report(RecoveryProgress::for_store(
                    RecoveryStage::RollingBack,
                    store,
                    None,
                ));
                backend.reboot_previous_good(store, transaction)?;
                report(RecoveryProgress::for_store(
                    RecoveryStage::RebootRequested,
                    store,
                    None,
                ));
                return Ok(RecoveryOutcome::RebootRequested);
            }
            _ => {
                return Err(invalid(
                    "startup reconciliation returned an installation action",
                ));
            }
        }
    }
    Err(invalid("recovery action sequence did not terminate"))
}

fn observe_trial(
    store: &mut RecoveryStore,
    backend: &mut impl RecoveryBackend,
    report: &mut impl FnMut(RecoveryProgress),
) -> Result<Action> {
    let initial = backend.observe()?;
    if let Err(error) = backend.verify_early_watchdog() {
        report(RecoveryProgress::for_store(
            RecoveryStage::RollingBack,
            store,
            Some(error.to_string()),
        ));
        return store.advance(|record| record.fail(FailureReason::WatchdogUnavailable, &initial));
    }
    let mut monitor = HealthMonitor::new(store.record(), &initial)?;
    loop {
        let before = backend.observe()?;
        same_trial(&initial, &before)?;
        if before.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance(|record| record.fail(FailureReason::HealthTimeout, &before));
        }
        report(RecoveryProgress::for_store(
            RecoveryStage::Observing,
            store,
            None,
        ));
        let sample = backend.health(&before);
        let after = backend.observe()?;
        same_trial(&before, &after)?;
        if after.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance(|record| record.fail(FailureReason::HealthTimeout, &after));
        }
        let sample = sample.and_then(|sample| {
            if sample.uptime_ms < before.uptime_ms
                || sample.uptime_ms > after.uptime_ms
                || after.uptime_ms - sample.uptime_ms > MAX_HEALTH_GAP_MS
            {
                return Err(invalid(
                    "health observation is outside the current sample interval",
                ));
            }
            Ok(sample)
        });
        let proof = match sample {
            Ok(sample) => monitor.observe(Some(sample)),
            Err(error) => {
                report(RecoveryProgress::for_store(
                    RecoveryStage::Observing,
                    store,
                    Some(error.to_string()),
                ));
                monitor.observe(None)
            }
        };
        match proof {
            Ok(Some(proof)) => {
                return store.advance(|record| record.health_confirmed(proof, &after));
            }
            Ok(None) => {}
            Err(error) => {
                report(RecoveryProgress::for_store(
                    RecoveryStage::Observing,
                    store,
                    Some(error.to_string()),
                ));
                monitor.observe(None)?;
            }
        }
        backend.wait_for_sample();
    }
}

fn same_trial(before: &BootObservation, after: &BootObservation) -> Result<()> {
    let mut expected = before.clone();
    expected.uptime_ms = after.uptime_ms;
    if &expected != after || after.uptime_ms < before.uptime_ms {
        return Err(invalid("boot evidence changed during health observation"));
    }
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::Boot(message.into())
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
