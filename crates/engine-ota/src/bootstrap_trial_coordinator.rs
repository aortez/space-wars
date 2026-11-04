//! Runtime sequencing for the first supervised trial and its next normal boot.
//! No installation, automatic retry or production journal provisioning occurs.
use super::trial::TrialAction;
use super::{TrialFailure, TrialPhase, TrialStore, invalid};
use crate::recovery::{BootObservation, HealthSample, MAX_HEALTH_GAP_MS, TRIAL_DEADLINE_MS};
use crate::{Result, WatchdogCompletion};
use std::time::Duration;

pub trait BootstrapBackend {
    fn observe(&mut self, store: &TrialStore) -> Result<BootObservation>;
    fn verify_early_watchdog(&mut self, store: &TrialStore) -> Result<()>;
    fn health(&mut self, boot: &BootObservation) -> Result<HealthSample>;
    fn select_default(&mut self, store: &TrialStore, staged: bool) -> Result<BootObservation>;
    fn complete_watchdog(&mut self, store: &TrialStore) -> Result<WatchdogCompletion>;
    fn reboot_normal(&mut self, store: &TrialStore, staged: bool) -> Result<()>;
    fn wait_for_sample(&mut self) {
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapOutcome {
    StagedRebootRequested,
    LegacyRebootRequested,
    NormalConfirmed,
}

/// A restarted observer needs a new continuous window in the same boot. The
/// absolute 180-second deadline and immutable early watchdog never restart.
pub fn recover_bootstrap(
    store: &mut TrialStore,
    backend: &mut impl BootstrapBackend,
    mut report: impl FnMut(TrialPhase, Option<String>),
) -> Result<BootstrapOutcome> {
    let boot = backend.observe(store)?;
    let mut action = store.advance(|record| record.reconcile(&boot))?;
    for _ in 0..8 {
        report(store.phase(), None);
        match action {
            TrialAction::Observe => {
                action = observe(store, backend, &mut report)?;
            }
            TrialAction::SelectStaged | TrialAction::RestoreLegacy => {
                let boot = backend.select_default(store, action == TrialAction::SelectStaged)?;
                action = store.advance(|record| record.reconcile(&boot))?;
            }
            TrialAction::CompleteWatchdog => {
                backend.complete_watchdog(store)?;
                action = TrialAction::RebootStaged;
            }
            TrialAction::RebootStaged | TrialAction::RebootLegacy => {
                let staged = action == TrialAction::RebootStaged;
                backend.reboot_normal(store, staged)?;
                return Ok(if staged {
                    BootstrapOutcome::StagedRebootRequested
                } else {
                    BootstrapOutcome::LegacyRebootRequested
                });
            }
            TrialAction::NormalConfirmed => return Ok(BootstrapOutcome::NormalConfirmed),
        }
    }
    Err(invalid(
        "bootstrap recovery action sequence did not terminate",
    ))
}

fn observe(
    store: &mut TrialStore,
    backend: &mut impl BootstrapBackend,
    report: &mut impl FnMut(TrialPhase, Option<String>),
) -> Result<TrialAction> {
    let initial = backend.observe(store)?;
    if initial.tryboot
        && let Err(error) = backend.verify_early_watchdog(store)
    {
        report(
            store.phase(),
            Some(error.to_string().chars().take(512).collect()),
        );
        return store.advance(|record| record.fail(TrialFailure::WatchdogUnavailable, &initial));
    }
    let mut monitor = store.durable()?.monitor(&initial)?;
    loop {
        let before = backend.observe(store)?;
        same_boot(&initial, &before)?;
        if before.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance(|record| record.fail(TrialFailure::HealthTimeout, &before));
        }
        report(store.phase(), None);
        let sample = backend.health(&before);
        let after = backend.observe(store)?;
        same_boot(&before, &after)?;
        if after.uptime_ms >= TRIAL_DEADLINE_MS {
            return store.advance(|record| record.fail(TrialFailure::HealthTimeout, &after));
        }
        let sample = sample.and_then(|sample| {
            if sample.uptime_ms < before.uptime_ms
                || sample.uptime_ms > after.uptime_ms
                || after.uptime_ms - sample.uptime_ms > MAX_HEALTH_GAP_MS
            {
                return Err(invalid(
                    "bootstrap health sample is outside the observed interval",
                ));
            }
            Ok(sample)
        });
        let proof = match sample {
            Ok(sample) => monitor.observe(Some(sample)),
            Err(error) => {
                report(
                    store.phase(),
                    Some(error.to_string().chars().take(512).collect()),
                );
                monitor.observe(None)
            }
        };
        match proof {
            Ok(Some(proof)) => return store.advance(|record| record.healthy(proof, &after)),
            Ok(None) => {}
            Err(error) => {
                report(store.phase(), Some(error.to_string()));
                monitor.observe(None)?;
            }
        }
        backend.wait_for_sample();
    }
}

fn same_boot(before: &BootObservation, after: &BootObservation) -> Result<()> {
    let mut after = after.clone();
    if after.uptime_ms < before.uptime_ms {
        return Err(invalid("bootstrap boot-relative clock went backwards"));
    }
    after.uptime_ms = before.uptime_ms;
    if before != &after {
        return Err(invalid(
            "bootstrap boot evidence changed during observation",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_trial_coordinator_tests.rs"]
mod tests;
