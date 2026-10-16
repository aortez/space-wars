//! Recovery remains on the staged image until a different canonical normal
//! boot completes its own local health window. No copy or retry starts here.
use super::canonical::CanonicalAction;
use super::{
    CanonicalDefault, CanonicalFailure, CanonicalObservation, CanonicalPhase, TrialStore, invalid,
};
use crate::Result;
use crate::recovery::{HealthSample, MAX_HEALTH_GAP_MS, TRIAL_DEADLINE_MS};
use std::time::Duration;

pub trait CanonicalBackend {
    fn observe(&mut self, store: &TrialStore) -> Result<CanonicalObservation>;
    fn health(&mut self, boot: &CanonicalObservation) -> Result<HealthSample>;
    fn restore_staged(&mut self, store: &TrialStore) -> Result<CanonicalObservation>;
    fn reboot_staged(&mut self, store: &TrialStore) -> Result<()>;
    fn wait_for_sample(&mut self) {
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalOutcome {
    AwaitingMigration,
    Confirmed,
    NeedsOperator,
    StagedRebootRequested,
}

pub fn recover_canonical(
    store: &mut TrialStore,
    backend: &mut impl CanonicalBackend,
    mut report: impl FnMut(CanonicalPhase, Option<String>),
) -> Result<CanonicalOutcome> {
    let boot = backend.observe(store)?;
    let mut action = store.advance_canonical(|c, inputs| c.reconcile(inputs, &boot))?;
    for _ in 0..5 {
        report(store.canonical()?.phase, None);
        match action {
            CanonicalAction::ObserveCanonical | CanonicalAction::ObserveStaged => {
                action = observe(store, backend, &mut report)?;
            }
            CanonicalAction::RestoreStaged => {
                let boot = backend.restore_staged(store)?;
                action = store.advance_canonical(|c, inputs| c.reconcile(inputs, &boot))?;
            }
            CanonicalAction::RebootStaged => {
                backend.reboot_staged(store)?;
                return Ok(CanonicalOutcome::StagedRebootRequested);
            }
            CanonicalAction::AwaitingMigration => return Ok(CanonicalOutcome::AwaitingMigration),
            CanonicalAction::Confirmed => return Ok(CanonicalOutcome::Confirmed),
            CanonicalAction::NeedsOperator => return Ok(CanonicalOutcome::NeedsOperator),
        }
    }
    Err(invalid("canonical recovery did not reach a stable outcome"))
}

fn observe(
    store: &mut TrialStore,
    backend: &mut impl CanonicalBackend,
    report: &mut impl FnMut(CanonicalPhase, Option<String>),
) -> Result<CanonicalAction> {
    let initial = backend.observe(store)?;
    let inputs = store.durable()?.inputs.clone();
    let mut monitor = store.canonical()?.monitor(&inputs, &initial)?;
    let reason = if initial.default == CanonicalDefault::Canonical {
        CanonicalFailure::CanonicalHealthTimeout
    } else {
        CanonicalFailure::StagedHealthTimeout
    };
    loop {
        let before = backend.observe(store)?;
        same_boot(&initial, &before)?;
        if before.boot.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance_canonical(|c, inputs| c.fail(inputs, &before, reason));
        }
        report(store.canonical()?.phase, None);
        let sample = backend.health(&before);
        let after = backend.observe(store)?;
        same_boot(&before, &after)?;
        if after.boot.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance_canonical(|c, inputs| c.fail(inputs, &after, reason));
        }
        let sample = sample.and_then(|sample| {
            if sample.uptime_ms < before.boot.uptime_ms
                || sample.uptime_ms > after.boot.uptime_ms
                || after.boot.uptime_ms - sample.uptime_ms > MAX_HEALTH_GAP_MS
            {
                return Err(invalid(
                    "canonical health sample is outside the observed interval",
                ));
            }
            Ok(sample)
        });
        let proof = match sample {
            Ok(sample) => monitor.observe(Some(sample)),
            Err(error) => {
                report(
                    store.canonical()?.phase,
                    Some(error.to_string().chars().take(512).collect()),
                );
                monitor.observe(None)
            }
        };
        match proof {
            Ok(Some(proof)) => {
                return store.advance_canonical(|c, inputs| c.healthy(inputs, &after, proof));
            }
            Ok(None) => {}
            Err(error) => {
                report(store.canonical()?.phase, Some(error.to_string()));
                monitor.observe(None)?;
            }
        }
        backend.wait_for_sample();
    }
}

fn same_boot(before: &CanonicalObservation, after: &CanonicalObservation) -> Result<()> {
    let mut after = after.clone();
    if after.boot.uptime_ms < before.boot.uptime_ms {
        return Err(invalid("canonical boot clock moved backwards"));
    }
    after.boot.uptime_ms = before.boot.uptime_ms;
    if before != &after {
        return Err(invalid(
            "canonical boot evidence changed during health observation",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_canonical_coordinator_tests.rs"]
mod tests;
