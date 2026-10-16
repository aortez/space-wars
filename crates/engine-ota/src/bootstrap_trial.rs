//! First-image health and staged-normal confirmation. The legacy root is only
//! a supervised fallback; this record never creates a qualified OTA baseline.
use super::{BootPlan, BootstrapBinding, invalid};
use crate::recovery::{BootBinding, BootObservation, HealthMonitor, HealthProof};
use crate::{ImageIdentity, Result, Sha256};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrialInputs {
    pub binding: BootstrapBinding,
    pub root_install_sha256: Sha256,
    pub image: ImageIdentity,
    pub kernel_build_id: String,
    pub command_line_template: String,
    pub legacy_config: String,
    pub plan: BootPlan,
    pub origin_boot_id: String,
}

impl TrialInputs {
    pub(super) fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        self.image.validate()?;
        crate::boot::BootCommandLine::parse(self.command_line_template.as_bytes())?;
        if self.image.source.dirty
            || self.image.machine != "raspberrypi-spacewars"
            || self.image.architecture != "aarch64"
            || self.image.updater_protocol != crate::UPDATER_PROTOCOL
            || self.image.recovery_protocol != crate::recovery::RECOVERY_PROTOCOL
            || self.plan.schema != 1
            || self.plan.target != self.binding.target
            || self.plan.profile != self.binding.hardware_profile
            || self.plan.build_id != self.image.build_id
            || self.plan.boot_identity != self.image.boot_identity
            || self.plan.trial_id.as_bytes() == &[0; 32]
            || self.root_install_sha256.as_bytes() == &[0; 32]
            || !crate::recovery::boot_id(&self.origin_boot_id)
            || self.kernel_build_id.len() != 40
            || !self
                .kernel_build_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.plan.staged_assets.is_empty()
            || self
                .plan
                .staged_assets
                .keys()
                .any(|path| !crate::boot::asset_path(path))
            || self.plan.trial_config.len() > 16 * 1024
            || self.plan.normal_config.len() > 16 * 1024
            || self.plan.trial_config.is_empty()
            || self.plan.normal_config.is_empty()
            || self.plan.trial_config == self.plan.normal_config
            || self.legacy_config.is_empty()
            || self.legacy_config.len() > 16 * 1024
            || self.binding.original_boot_files.get("config.txt")
                != Some(&crate::Blob {
                    bytes: self.legacy_config.len() as u64,
                    sha256: Sha256::digest(self.legacy_config.as_bytes()),
                })
        {
            return Err(invalid("invalid bootstrap trial context"));
        }
        Ok(())
    }

    pub(super) fn boot_binding(&self) -> BootBinding {
        BootBinding {
            device_id: self.binding.device_id,
            layout_id: self.binding.layout_id,
            boot_identity: self.image.boot_identity,
            hardware_profile: Sha256::digest(self.plan.normal_config.as_bytes()),
        }
    }

    fn context(&self, normal: bool) -> Sha256 {
        Sha256::digest(&serde_json::to_vec(&(self, normal)).expect("bootstrap context serializes"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrialPhase {
    Staging,
    TrialReady,
    TrialRequested,
    ObservingTrial,
    PromotingStaged,
    StagedDefault,
    ObservingNormal,
    NormalConfirmed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrialFailure {
    ActivationRequestFailed,
    HealthTimeout,
    WatchdogUnavailable,
    TrialReset,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrialRecord {
    schema: u32,
    pub inputs: TrialInputs,
    pub phase: TrialPhase,
    request_boot_id: Option<String>,
    trial_boot_id: Option<String>,
    trial_health: Option<HealthProof>,
    normal_boot_id: Option<String>,
    normal_health: Option<HealthProof>,
    pub failure: Option<TrialFailure>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TrialAction {
    Observe,
    SelectStaged,
    RestoreLegacy,
    CompleteWatchdog,
    RebootStaged,
    RebootLegacy,
    NormalConfirmed,
}

impl TrialRecord {
    pub(super) fn staging(inputs: TrialInputs) -> Result<Self> {
        let record = Self {
            schema: 2,
            inputs,
            phase: TrialPhase::Staging,
            request_boot_id: None,
            trial_boot_id: None,
            trial_health: None,
            normal_boot_id: None,
            normal_health: None,
            failure: None,
        };
        record.validate()?;
        Ok(record)
    }

    #[cfg(test)]
    pub(super) fn requested(inputs: TrialInputs) -> Result<Self> {
        let mut record = Self::staging(inputs)?;
        record.phase = TrialPhase::TrialRequested;
        record.request_boot_id = Some(record.inputs.origin_boot_id.clone());
        record.validate()?;
        Ok(record)
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.inputs.validate()?;
        let valid_boot = |id: &Option<String>| {
            id.as_ref().is_none_or(|id| {
                crate::recovery::boot_id(id)
                    && id != &self.inputs.origin_boot_id
                    && Some(id) != self.request_boot_id.as_ref()
            })
        };
        let valid_health = |normal, id: &Option<String>, proof: &Option<HealthProof>| {
            proof.as_ref().is_none_or(|proof| {
                id.as_ref()
                    .is_some_and(|id| proof.matches_bootstrap(self.inputs.context(normal), id))
            })
        };
        let started = self.trial_boot_id.is_some();
        let healthy = self.trial_health.is_some();
        let normal = self.normal_boot_id.is_some();
        let confirmed = self.normal_health.is_some();
        let requested = self.request_boot_id.is_some();
        let shape = match self.phase {
            TrialPhase::Staging | TrialPhase::TrialReady => {
                !requested && !started && !healthy && !normal && !confirmed
            }
            TrialPhase::TrialRequested => !started && !healthy && !normal && !confirmed,
            TrialPhase::ObservingTrial => started && !healthy && !normal && !confirmed,
            TrialPhase::PromotingStaged | TrialPhase::StagedDefault => {
                started && healthy && !normal && !confirmed
            }
            TrialPhase::ObservingNormal => started && healthy && normal && !confirmed,
            TrialPhase::NormalConfirmed => started && healthy && normal && confirmed,
            TrialPhase::Failed => !confirmed,
        };
        if self.schema != 2
            || !shape
            || (!matches!(self.phase, TrialPhase::Staging | TrialPhase::TrialReady) && !requested)
            || self
                .request_boot_id
                .as_ref()
                .is_some_and(|id| !crate::recovery::boot_id(id))
            || (self.failure == Some(TrialFailure::ActivationRequestFailed)
                && (started || healthy || normal || confirmed))
            || (self.phase == TrialPhase::Failed) != self.failure.is_some()
            || !valid_boot(&self.trial_boot_id)
            || !valid_boot(&self.normal_boot_id)
            || (normal && self.normal_boot_id == self.trial_boot_id)
            || !valid_health(false, &self.trial_boot_id, &self.trial_health)
            || !valid_health(true, &self.normal_boot_id, &self.normal_health)
        {
            return Err(invalid("invalid bootstrap trial state"));
        }
        Ok(())
    }

    pub(super) fn request(&mut self, boot_id: &str) -> Result<()> {
        self.validate()?;
        if self.phase != TrialPhase::TrialReady || !crate::recovery::boot_id(boot_id) {
            return Err(invalid(
                "bootstrap activation requires an unused ready attempt",
            ));
        }
        self.request_boot_id = Some(boot_id.into());
        self.phase = TrialPhase::TrialRequested;
        self.validate()
    }

    pub(super) fn authorize_request(&self, boot_id: &str) -> Result<()> {
        self.validate()?;
        if self.phase != TrialPhase::TrialRequested
            || self.request_boot_id.as_deref() != Some(boot_id)
        {
            return Err(invalid(
                "bootstrap reboot request is not authorized for this boot",
            ));
        }
        Ok(())
    }

    pub(super) fn fail_request(&mut self, boot_id: &str) -> Result<()> {
        if self.phase == TrialPhase::Failed
            && self.failure == Some(TrialFailure::ActivationRequestFailed)
            && self.request_boot_id.as_deref() == Some(boot_id)
        {
            return self.validate();
        }
        self.authorize_request(boot_id)?;
        self.phase = TrialPhase::Failed;
        self.failure = Some(TrialFailure::ActivationRequestFailed);
        self.validate()
    }

    pub(super) fn check_boot(&self, boot: &BootObservation) -> Result<()> {
        self.validate()?;
        if boot.binding != self.inputs.boot_binding()
            || !crate::recovery::boot_id(&boot.boot_id)
            || boot.boot_id == self.inputs.origin_boot_id
            || self.request_boot_id.as_ref() == Some(&boot.boot_id)
            || boot.actual_slot != Some(self.inputs.plan.target)
            || boot.actual_build_id.as_deref() != Some(self.inputs.image.build_id.as_str())
            || boot.recovery_protocol != self.inputs.image.recovery_protocol
            || !matches!(boot.default_slot, Some(slot) if slot == self.inputs.binding.original_slot || slot == self.inputs.plan.target)
            || if boot.tryboot {
                boot.trial_id != Some(self.inputs.plan.trial_id)
            } else {
                boot.trial_id.is_some()
                    || (boot.default_slot != Some(self.inputs.plan.target)
                        && self.phase != TrialPhase::Failed)
            }
        {
            return Err(invalid(
                "running boot does not match the supervised bootstrap",
            ));
        }
        Ok(())
    }

    pub(super) fn reconcile(&mut self, boot: &BootObservation) -> Result<TrialAction> {
        self.check_boot(boot)?;
        let legacy = boot.default_slot == Some(self.inputs.binding.original_slot);
        if self.phase == TrialPhase::Failed {
            return Ok(if legacy {
                TrialAction::RebootLegacy
            } else {
                TrialAction::RestoreLegacy
            });
        }
        if boot.tryboot {
            if self
                .trial_boot_id
                .as_ref()
                .is_some_and(|id| id != &boot.boot_id)
            {
                return self.fail(TrialFailure::TrialReset, boot);
            }
            match self.phase {
                TrialPhase::TrialRequested if legacy => {
                    self.phase = TrialPhase::ObservingTrial;
                    self.trial_boot_id = Some(boot.boot_id.clone());
                    Ok(TrialAction::Observe)
                }
                TrialPhase::ObservingTrial if legacy => Ok(TrialAction::Observe),
                TrialPhase::PromotingStaged if legacy => {
                    if !self.trial_health.as_ref().unwrap().fresh_at(boot.uptime_ms) {
                        return self.fail(TrialFailure::HealthTimeout, boot);
                    }
                    Ok(TrialAction::SelectStaged)
                }
                TrialPhase::PromotingStaged | TrialPhase::StagedDefault if !legacy => {
                    self.phase = TrialPhase::StagedDefault;
                    Ok(TrialAction::CompleteWatchdog)
                }
                _ => Err(invalid(
                    "bootstrap trial phase and normal selection disagree",
                )),
            }
        } else if !legacy
            && self.trial_health.is_some()
            && matches!(
                self.phase,
                TrialPhase::PromotingStaged
                    | TrialPhase::StagedDefault
                    | TrialPhase::ObservingNormal
                    | TrialPhase::NormalConfirmed
            )
        {
            if self.trial_boot_id.as_ref() == Some(&boot.boot_id) {
                return Err(invalid(
                    "normal confirmation requires a separate normal boot",
                ));
            }
            if self.phase == TrialPhase::NormalConfirmed
                && self.normal_boot_id.as_ref() == Some(&boot.boot_id)
            {
                return Ok(TrialAction::NormalConfirmed);
            }
            self.phase = TrialPhase::ObservingNormal;
            self.normal_boot_id = Some(boot.boot_id.clone());
            self.normal_health = None;
            Ok(TrialAction::Observe)
        } else {
            Err(invalid("normal bootstrap boot has no durable trial health"))
        }
    }

    pub(super) fn monitor(&self, boot: &BootObservation) -> Result<HealthMonitor> {
        self.check_boot(boot)?;
        let normal = self.phase == TrialPhase::ObservingNormal;
        if (normal && (boot.tryboot || self.normal_boot_id.as_ref() != Some(&boot.boot_id)))
            || (!normal
                && (self.phase != TrialPhase::ObservingTrial
                    || !boot.tryboot
                    || self.trial_boot_id.as_ref() != Some(&boot.boot_id)))
        {
            return Err(invalid("bootstrap health is not observing this boot"));
        }
        Ok(HealthMonitor::bootstrap(self.inputs.context(normal), boot)?)
    }

    pub(super) fn healthy(
        &mut self,
        proof: HealthProof,
        boot: &BootObservation,
    ) -> Result<TrialAction> {
        self.monitor(boot)?;
        let normal = !boot.tryboot;
        if !proof.matches_bootstrap(self.inputs.context(normal), &boot.boot_id)
            || !proof.fresh_at(boot.uptime_ms)
        {
            return Err(invalid(
                "bootstrap health proof is stale or belongs to another stage",
            ));
        }
        if normal {
            self.normal_health = Some(proof);
            self.phase = TrialPhase::NormalConfirmed;
            Ok(TrialAction::NormalConfirmed)
        } else {
            if boot.default_slot != Some(self.inputs.binding.original_slot) {
                return Err(invalid("trial default changed before health was confirmed"));
            }
            self.trial_health = Some(proof);
            self.phase = TrialPhase::PromotingStaged;
            Ok(TrialAction::SelectStaged)
        }
    }

    pub(super) fn fail(
        &mut self,
        reason: TrialFailure,
        boot: &BootObservation,
    ) -> Result<TrialAction> {
        self.check_boot(boot)?;
        // No late failure may write an unproven selector or reclassify an
        // already promoted normal boot. Ambiguous states require the operator.
        if self.phase == TrialPhase::NormalConfirmed {
            return Err(invalid("confirmed bootstrap cannot be failed implicitly"));
        }
        self.phase = TrialPhase::Failed;
        self.failure.get_or_insert(reason);
        self.normal_health = None;
        Ok(
            if boot.default_slot == Some(self.inputs.binding.original_slot) {
                TrialAction::RebootLegacy
            } else {
                TrialAction::RestoreLegacy
            },
        )
    }

    pub(super) fn selection(&self, boot: &BootObservation, staged: bool) -> Result<&str> {
        self.check_boot(boot)?;
        if !staged {
            if self.phase != TrialPhase::Failed
                || boot.default_slot != Some(self.inputs.plan.target)
            {
                return Err(invalid(
                    "bootstrap does not authorize restoring the legacy default",
                ));
            }
            return Ok(&self.inputs.legacy_config);
        }
        if self.phase != TrialPhase::PromotingStaged
            || !boot.tryboot
            || self.trial_boot_id.as_ref() != Some(&boot.boot_id)
            || boot.default_slot != Some(self.inputs.binding.original_slot)
            || !self
                .trial_health
                .as_ref()
                .is_some_and(|proof| proof.fresh_at(boot.uptime_ms))
        {
            return Err(invalid("bootstrap does not authorize the staged default"));
        }
        Ok(&self.inputs.plan.normal_config)
    }

    pub(super) fn watchdog_completion(&self, boot: &BootObservation) -> Result<Sha256> {
        self.check_boot(boot)?;
        if self.phase != TrialPhase::StagedDefault
            || !boot.tryboot
            || self.trial_boot_id.as_ref() != Some(&boot.boot_id)
            || boot.default_slot != Some(self.inputs.plan.target)
            || self.trial_health.is_none()
        {
            return Err(invalid(
                "bootstrap has no durable promoted trial for watchdog completion",
            ));
        }
        Ok(self.inputs.plan.trial_id)
    }

    pub(super) fn reboot(&self, boot: &BootObservation, staged: bool) -> Result<()> {
        self.check_boot(boot)?;
        let allowed = if staged {
            self.watchdog_completion(boot).is_ok()
        } else {
            self.phase == TrialPhase::Failed
                && boot.default_slot == Some(self.inputs.binding.original_slot)
        };
        if !allowed {
            return Err(invalid("bootstrap does not authorize this normal reboot"));
        }
        Ok(())
    }
}
