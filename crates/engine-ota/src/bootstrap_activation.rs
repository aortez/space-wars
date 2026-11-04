//! A consumed, root-operator-only handoff. The device wrapper retains its
//! installation lock, journal lock and inactive-root claim for this lifetime.
use super::{TrialPhase, TrialStore, invalid};
use crate::{Result, Sha256};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BootActivationReport {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub root_install_sha256: Sha256,
    pub trial_id: Sha256,
    pub request_boot_id: String,
    pub reboot_job_accepted: bool,
    pub automatic_updates_enabled: bool,
}

/// Only the checked device wrapper can construct this permission. Neither a
/// serialized record nor a user-supplied trial token can construct one.
pub struct TrialActivation<'a> {
    store: &'a mut TrialStore,
    boot_id: &'a str,
    guard: &'a mut dyn FnMut() -> Result<()>,
    began: bool,
    authorized: bool,
    queued: bool,
}

impl TrialActivation<'_> {
    /// Persist the consumed attempt before any reboot parameter or job request.
    pub fn begin(&mut self) -> Result<()> {
        if self.began || self.queued {
            return Err(invalid("bootstrap activation has already begun"));
        }
        (self.guard)()?;
        self.store.advance(|record| record.request(self.boot_id))?;
        self.began = true;
        Ok(())
    }

    /// Recheck held device context immediately before each reboot mutation.
    pub fn authorize_reboot(&mut self) -> Result<()> {
        self.authorized = false;
        if !self.began || self.queued {
            return Err(invalid("bootstrap activation is not requesting a reboot"));
        }
        (self.guard)()?;
        self.store.durable()?.authorize_request(self.boot_id)?;
        self.authorized = true;
        Ok(())
    }

    /// Call only after PID 1 has accepted the fixed reboot job. No further
    /// storage access: orderly shutdown may already be stopping this process.
    pub fn reboot_queued(&mut self) -> Result<()> {
        if !self.began || !self.authorized || self.queued {
            return Err(invalid("bootstrap reboot was not authorized"));
        }
        self.queued = true;
        Ok(())
    }

    /// Persist failure before best-effort reboot-parameter cleanup. This never
    /// restores TrialReady or permits automatic retry, even after a lost reply.
    pub fn fail_request(&mut self) -> Result<()> {
        if self.queued {
            return Err(invalid(
                "an accepted reboot requires operator reconciliation",
            ));
        }
        self.authorized = false;
        if self.began {
            self.store
                .advance(|record| record.fail_request(self.boot_id))?;
        }
        Ok(())
    }
}

pub(super) fn run(
    store: &mut TrialStore,
    boot_id: &str,
    guard: &mut dyn FnMut() -> Result<()>,
    request: impl FnOnce(&mut TrialActivation<'_>) -> Result<()>,
) -> Result<BootActivationReport> {
    let record = store.durable()?;
    if record.phase != TrialPhase::TrialReady || !crate::recovery::boot_id(boot_id) {
        return Err(invalid(
            "bootstrap activation requires an unused ready attempt",
        ));
    }
    let report = BootActivationReport {
        schema: 1,
        scope: "one-shot reboot job accepted; trial boot, health and normal confirmation remain unobserved",
        package_sha256: record.inputs.binding.package_sha256,
        root_install_sha256: record.inputs.root_install_sha256,
        trial_id: record.inputs.plan.trial_id,
        request_boot_id: boot_id.into(),
        reboot_job_accepted: true,
        automatic_updates_enabled: false,
    };
    let mut permit = TrialActivation {
        store,
        boot_id,
        guard,
        began: false,
        authorized: false,
        queued: false,
    };
    let outcome = request(&mut permit).and_then(|()| {
        if permit.queued {
            Ok(())
        } else {
            Err(invalid("bootstrap reboot job was not accepted"))
        }
    });
    if let Err(error) = outcome {
        if let Err(failure) = permit.fail_request() {
            return Err(invalid(&format!(
                "{error}; could not record activation failure: {failure}; operator reconciliation required"
            )));
        }
        return Err(error);
    }
    Ok(report)
}

#[cfg(test)]
#[path = "bootstrap_activation_tests.rs"]
mod tests;
