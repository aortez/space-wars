//! Canonical boot migration after a separately confirmed staged normal boot.
//! The verified staged tree is the fallback from the first durable copy intent.
use super::invalid;
use super::trial::TrialInputs;
use crate::boot::{BootManifest, UNPREPARED_TRIAL};
use crate::recovery::{BootObservation, HealthMonitor, HealthProof};
use crate::{Blob, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CanonicalPlan {
    pub manifest: String,
    pub config: String,
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub files: BTreeMap<String, Blob>,
}

pub(super) fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}

impl CanonicalPlan {
    pub fn validate(&self, inputs: &TrialInputs) -> Result<()> {
        inputs.validate()?;
        let manifest = BootManifest::parse(self.manifest.as_bytes(), inputs.image.boot_identity)?;
        let mut expected = manifest.files().clone();
        for (name, contents) in [
            ("config.txt", self.config.clone()),
            ("cmdline.txt", inputs.plan.normal_cmdline.clone()),
            ("spacewars-trial.txt", UNPREPARED_TRIAL.into()),
            (
                "spacewars-device.txt",
                format!(
                    "[all]\ninclude spacewars-profiles/{}.txt\n[all]\n",
                    inputs.plan.profile
                ),
            ),
            (
                "spacewars-boot-id",
                format!("{}\n", inputs.image.boot_identity),
            ),
            ("spacewars-boot-manifest.json", self.manifest.clone()),
        ] {
            let value = blob(contents.as_bytes());
            if name == "config.txt" && expected.get(name) != Some(&value) {
                return Err(invalid("canonical config differs from the image manifest"));
            }
            expected.insert(name.into(), value);
        }
        if expected != self.files
            || self.files.len() > 1024
            || self.config.is_empty()
            || self.config.len() > 16 * 1024
            || self.config == inputs.plan.normal_config
            || self
                .files
                .keys()
                .any(|p| p.starts_with("sw-bootstrap/") || p.starts_with('.'))
            || inputs
                .plan
                .staged_assets
                .iter()
                .any(|(p, b)| self.files.get(p) != Some(b))
            || inputs.plan.unchanged_firmware.iter().any(|(p, b)| {
                self.files.get(p) != Some(b) || inputs.binding.original_boot_files.get(p) != Some(b)
            })
        {
            return Err(invalid(
                "canonical plan differs from the checked bootstrap image",
            ));
        }
        // FAT folds names, including directory components. Reject collisions
        // with retained paths before taking ownership of recovery or copying.
        let retained = super::trial_files::expected(inputs, true);
        let mut names = BTreeMap::new();
        for path in retained.keys().chain(self.files.keys()) {
            for end in path.match_indices('/').map(|(i, _)| i).chain([path.len()]) {
                let component = &path[..end];
                let entry = (component, end == path.len());
                if names
                    .insert(component.to_ascii_lowercase(), entry)
                    .is_some_and(|previous| previous != entry)
                {
                    return Err(invalid("canonical and retained boot paths conflict"));
                }
            }
        }
        Ok(())
    }

    fn paths(&self) -> BTreeSet<String> {
        self.files
            .keys()
            .filter(|p| p.as_str() != "config.txt")
            .cloned()
            .collect()
    }

    fn context(&self, inputs: &TrialInputs) -> Sha256 {
        Sha256::digest(
            &serde_json::to_vec(&("canonical-bootstrap-v1", inputs, self))
                .expect("context serializes"),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanonicalPhase {
    Preparing,
    Prepared,
    Selecting,
    Selected,
    Observing,
    Confirmed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanonicalFailure {
    StagedHealthTimeout,
    CanonicalHealthTimeout,
    RebootRequestFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalDefault {
    Staged,
    Canonical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalObservation {
    pub boot: BootObservation,
    pub default: CanonicalDefault,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CanonicalState {
    pub plan: CanonicalPlan,
    pub phase: CanonicalPhase,
    pub completed: BTreeSet<String>,
    pub pending: Option<String>,
    staged_boot_id: String,
    staged_health: Option<HealthProof>,
    selection_boot_id: Option<String>,
    reboot_requested: bool,
    canonical_boot_id: Option<String>,
    canonical_health: Option<HealthProof>,
    return_boot_id: Option<String>,
    pub failure: Option<CanonicalFailure>,
    pub finish: Option<super::finish::FinishState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CanonicalAction {
    ObserveStaged,
    ObserveCanonical,
    AwaitingMigration,
    RestoreStaged,
    RebootStaged,
    Confirmed,
    NeedsOperator,
}

impl CanonicalState {
    pub fn new(
        plan: CanonicalPlan,
        inputs: &TrialInputs,
        boot_id: String,
        health: HealthProof,
    ) -> Result<Self> {
        let state = Self {
            plan,
            phase: CanonicalPhase::Preparing,
            completed: BTreeSet::new(),
            pending: None,
            staged_boot_id: boot_id,
            staged_health: Some(health),
            selection_boot_id: None,
            reboot_requested: false,
            canonical_boot_id: None,
            canonical_health: None,
            return_boot_id: None,
            failure: None,
            finish: None,
        };
        state.validate(inputs)?;
        Ok(state)
    }

    pub fn validate(&self, inputs: &TrialInputs) -> Result<()> {
        self.plan.validate(inputs)?;
        let paths = self.plan.paths();
        let valid_id = |id: &str| crate::recovery::boot_id(id) && id != inputs.origin_boot_id;
        let selected = matches!(
            self.phase,
            CanonicalPhase::Selecting
                | CanonicalPhase::Selected
                | CanonicalPhase::Observing
                | CanonicalPhase::Confirmed
        );
        let preparing = matches!(
            self.phase,
            CanonicalPhase::Preparing | CanonicalPhase::Prepared
        );
        if !valid_id(&self.staged_boot_id)
            || (preparing
                && (self.selection_boot_id.is_some()
                    || self.reboot_requested
                    || self.canonical_boot_id.is_some()))
            || (matches!(
                self.phase,
                CanonicalPhase::Selecting | CanonicalPhase::Selected
            ) && self.canonical_boot_id.is_some())
            || self
                .selection_boot_id
                .as_ref()
                .is_some_and(|id| !valid_id(id))
            || self.canonical_boot_id.as_ref().is_some_and(|id| {
                !valid_id(id)
                    || Some(id) == self.selection_boot_id.as_ref()
                    || id == &self.staged_boot_id
            })
            || self.return_boot_id.as_ref().is_some_and(|id| !valid_id(id))
            || !self.completed.is_subset(&paths)
            || self.pending.as_ref().is_some_and(|p| {
                !matches!(
                    self.phase,
                    CanonicalPhase::Preparing | CanonicalPhase::Failed
                ) || !paths.contains(p)
                    || self.completed.contains(p)
            })
            || (self.phase != CanonicalPhase::Preparing
                && self.phase != CanonicalPhase::Failed
                && (self.completed != paths || self.pending.is_some()))
            || (selected && self.selection_boot_id.is_none())
            || (self.reboot_requested && self.selection_boot_id.is_none())
            || (self.phase == CanonicalPhase::Failed) != self.failure.is_some()
            || (self.return_boot_id.is_some() && self.phase != CanonicalPhase::Failed)
            || self
                .staged_health
                .as_ref()
                .is_some_and(|p| !p.matches_bootstrap(inputs.context(true), &self.staged_boot_id))
            || self.canonical_health.as_ref().is_some_and(|p| {
                self.canonical_boot_id
                    .as_ref()
                    .is_none_or(|id| !p.matches_bootstrap(self.plan.context(inputs), id))
            })
            || (self.phase == CanonicalPhase::Confirmed) != self.canonical_health.is_some()
            || (matches!(
                self.phase,
                CanonicalPhase::Observing | CanonicalPhase::Confirmed
            ) && self.canonical_boot_id.is_none())
        {
            return Err(invalid("invalid canonical migration state"));
        }
        if let Some(finish) = &self.finish {
            if !matches!(
                self.phase,
                CanonicalPhase::Observing | CanonicalPhase::Confirmed | CanonicalPhase::Failed
            ) {
                return Err(invalid(
                    "bootstrap retirement requires canonical normal confirmation",
                ));
            }
            finish.validate(inputs, &self.plan)?;
        }
        Ok(())
    }

    pub fn check_successor(&self, previous: &Self) -> Result<()> {
        if self.plan != previous.plan
            || !previous.completed.is_subset(&self.completed)
            || (previous.reboot_requested && !self.reboot_requested)
            || previous
                .failure
                .is_some_and(|reason| self.failure != Some(reason))
        {
            return Err(invalid(
                "canonical migration changed immutable intent or restored consumed authority",
            ));
        }
        if let Some(previous) = &previous.finish {
            self.finish
                .as_ref()
                .ok_or_else(|| invalid("bootstrap retirement cannot restore staged fallback"))?
                .check_successor(previous)?;
        }
        Ok(())
    }

    pub fn authorize_finish(
        &self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
    ) -> Result<()> {
        self.check_boot(inputs, boot)?;
        if self.phase != CanonicalPhase::Confirmed
            || self.canonical_boot_id.as_ref() != Some(&boot.boot.boot_id)
            || self.canonical_health.is_none()
            || boot.default != CanonicalDefault::Canonical
        {
            return Err(invalid(
                "bootstrap retirement requires this confirmed canonical normal boot",
            ));
        }
        Ok(())
    }

    pub fn begin_finish(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
        files: BTreeMap<String, Blob>,
    ) -> Result<()> {
        self.authorize_finish(inputs, boot)?;
        if self.finish.is_some() {
            return Err(invalid("bootstrap retirement was already initialized"));
        }
        self.finish = Some(super::finish::FinishState::new(files, inputs, &self.plan)?);
        Ok(())
    }

    pub fn finish(&self) -> Result<&super::finish::FinishState> {
        self.finish
            .as_ref()
            .ok_or_else(|| invalid("bootstrap retirement is not initialized"))
    }

    pub fn check_boot(&self, inputs: &TrialInputs, boot: &CanonicalObservation) -> Result<()> {
        self.validate(inputs)?;
        let actual = &boot.boot;
        if actual.binding != inputs.boot_binding()
            || !crate::recovery::boot_id(&actual.boot_id)
            || actual.boot_id == inputs.origin_boot_id
            || actual.actual_slot != Some(inputs.plan.target)
            || actual.default_slot != Some(inputs.plan.target)
            || actual.actual_build_id.as_deref() != Some(inputs.image.build_id.as_str())
            || actual.recovery_protocol != inputs.image.recovery_protocol
            || actual.tryboot
            || actual.trial_id.is_some()
        {
            return Err(invalid(
                "canonical migration does not match the running normal boot",
            ));
        }
        Ok(())
    }

    pub fn authorize_copy(&self, inputs: &TrialInputs, boot: &CanonicalObservation) -> Result<()> {
        self.check_boot(inputs, boot)?;
        if !matches!(
            self.phase,
            CanonicalPhase::Preparing | CanonicalPhase::Prepared
        ) || boot.default != CanonicalDefault::Staged
            || self.staged_boot_id != boot.boot.boot_id
            || self.staged_health.is_none()
        {
            return Err(invalid(
                "canonical copy requires this confirmed staged normal boot",
            ));
        }
        Ok(())
    }

    pub fn begin_file(&mut self, path: &str) -> Result<()> {
        if self.phase != CanonicalPhase::Preparing
            || !self.plan.paths().contains(path)
            || self.completed.contains(path)
            || self.pending.as_deref().is_some_and(|p| p != path)
        {
            return Err(invalid("canonical file is not the recorded next copy"));
        }
        self.pending = Some(path.into());
        Ok(())
    }

    pub fn finish_file(&mut self, path: &str) -> Result<()> {
        if self.phase != CanonicalPhase::Preparing || self.pending.as_deref() != Some(path) {
            return Err(invalid("canonical file completion lacks durable intent"));
        }
        self.completed.insert(path.into());
        self.pending = None;
        Ok(())
    }

    pub fn prepared(&mut self) -> Result<()> {
        if !matches!(
            self.phase,
            CanonicalPhase::Preparing | CanonicalPhase::Prepared
        ) || self.pending.is_some()
            || self.completed != self.plan.paths()
        {
            return Err(invalid("canonical boot files are incomplete"));
        }
        self.phase = CanonicalPhase::Prepared;
        Ok(())
    }

    pub fn begin_selection(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
    ) -> Result<()> {
        self.check_boot(inputs, boot)?;
        if !matches!(
            self.phase,
            CanonicalPhase::Prepared | CanonicalPhase::Selecting
        ) || self.reboot_requested
            || self.staged_boot_id != boot.boot.boot_id
            || self.staged_health.is_none()
        {
            return Err(invalid(
                "canonical selection requires the confirmed staged boot",
            ));
        }
        self.phase = CanonicalPhase::Selecting;
        self.selection_boot_id = Some(boot.boot.boot_id.clone());
        Ok(())
    }

    pub fn selected(&mut self, inputs: &TrialInputs, boot: &CanonicalObservation) -> Result<()> {
        self.check_boot(inputs, boot)?;
        if self.phase != CanonicalPhase::Selecting
            || boot.default != CanonicalDefault::Canonical
            || self.selection_boot_id.as_ref() != Some(&boot.boot.boot_id)
        {
            return Err(invalid(
                "canonical selector publication does not match its intent",
            ));
        }
        self.phase = CanonicalPhase::Selected;
        Ok(())
    }

    pub fn request_reboot(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
    ) -> Result<()> {
        self.check_boot(inputs, boot)?;
        if self.phase != CanonicalPhase::Selected
            || self.reboot_requested
            || self.selection_boot_id.as_ref() != Some(&boot.boot.boot_id)
            || boot.default != CanonicalDefault::Canonical
        {
            return Err(invalid("canonical reboot requires an unused selected boot"));
        }
        self.reboot_requested = true;
        Ok(())
    }

    pub fn authorize_reboot(
        &self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
        fallback: bool,
    ) -> Result<()> {
        self.check_boot(inputs, boot)?;
        let allowed = if fallback {
            self.phase == CanonicalPhase::Failed
                && boot.default == CanonicalDefault::Staged
                && self.return_boot_id.as_ref() == Some(&boot.boot.boot_id)
        } else {
            self.phase == CanonicalPhase::Selected
                && self.reboot_requested
                && boot.default == CanonicalDefault::Canonical
                && self.selection_boot_id.as_ref() == Some(&boot.boot.boot_id)
        };
        if !allowed {
            return Err(invalid(
                "canonical migration does not authorize this normal reboot",
            ));
        }
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
    ) -> Result<CanonicalAction> {
        self.check_boot(inputs, boot)?;
        if self.finish.is_some() {
            if boot.default != CanonicalDefault::Canonical {
                return Err(invalid(
                    "bootstrap retirement cannot return to a staged default",
                ));
            }
            if self.phase == CanonicalPhase::Failed {
                return Ok(CanonicalAction::NeedsOperator);
            }
        }
        if self.phase == CanonicalPhase::Failed {
            if boot.default == CanonicalDefault::Canonical {
                if self.selection_boot_id.as_ref() != Some(&boot.boot.boot_id) {
                    self.return_boot_id = Some(boot.boot.boot_id.clone());
                }
                return Ok(CanonicalAction::RestoreStaged);
            }
            return Ok(
                if self.return_boot_id.as_ref() == Some(&boot.boot.boot_id) {
                    CanonicalAction::RebootStaged
                } else {
                    CanonicalAction::NeedsOperator
                },
            );
        }
        if boot.default == CanonicalDefault::Staged {
            if !matches!(
                self.phase,
                CanonicalPhase::Preparing | CanonicalPhase::Prepared | CanonicalPhase::Selecting
            ) {
                return Err(invalid("canonical phase disagrees with the staged default"));
            }
            if self.staged_boot_id == boot.boot.boot_id && self.staged_health.is_some() {
                return Ok(CanonicalAction::AwaitingMigration);
            }
            self.staged_boot_id.clone_from(&boot.boot.boot_id);
            self.staged_health = None;
            return Ok(CanonicalAction::ObserveStaged);
        }
        if !matches!(
            self.phase,
            CanonicalPhase::Selecting
                | CanonicalPhase::Selected
                | CanonicalPhase::Observing
                | CanonicalPhase::Confirmed
        ) || self.selection_boot_id.as_ref() == Some(&boot.boot.boot_id)
        {
            return Err(invalid(
                "canonical confirmation requires a separate selected normal boot",
            ));
        }
        if self.phase == CanonicalPhase::Confirmed
            && self.canonical_boot_id.as_ref() == Some(&boot.boot.boot_id)
        {
            return Ok(CanonicalAction::Confirmed);
        }
        self.phase = CanonicalPhase::Observing;
        self.canonical_boot_id = Some(boot.boot.boot_id.clone());
        self.canonical_health = None;
        Ok(CanonicalAction::ObserveCanonical)
    }

    pub fn monitor(
        &self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
    ) -> Result<HealthMonitor> {
        self.check_boot(inputs, boot)?;
        let canonical = boot.default == CanonicalDefault::Canonical;
        if if canonical {
            self.phase != CanonicalPhase::Observing
                || self.canonical_boot_id.as_ref() != Some(&boot.boot.boot_id)
        } else {
            !matches!(
                self.phase,
                CanonicalPhase::Preparing | CanonicalPhase::Prepared | CanonicalPhase::Selecting
            ) || self.staged_boot_id != boot.boot.boot_id
                || self.staged_health.is_some()
        } {
            return Err(invalid("canonical health is not observing this boot"));
        }
        Ok(HealthMonitor::bootstrap(
            if canonical {
                self.plan.context(inputs)
            } else {
                inputs.context(true)
            },
            &boot.boot,
        )?)
    }

    pub fn healthy(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
        proof: HealthProof,
    ) -> Result<CanonicalAction> {
        self.monitor(inputs, boot)?;
        let canonical = boot.default == CanonicalDefault::Canonical;
        let context = if canonical {
            self.plan.context(inputs)
        } else {
            inputs.context(true)
        };
        if !proof.matches_bootstrap(context, &boot.boot.boot_id)
            || !proof.fresh_at(boot.boot.uptime_ms)
        {
            return Err(invalid(
                "canonical health proof is stale or belongs to another boot",
            ));
        }
        if canonical {
            self.canonical_health = Some(proof);
            self.phase = CanonicalPhase::Confirmed;
            Ok(CanonicalAction::Confirmed)
        } else {
            self.staged_health = Some(proof);
            Ok(CanonicalAction::AwaitingMigration)
        }
    }

    pub fn fail(
        &mut self,
        inputs: &TrialInputs,
        boot: &CanonicalObservation,
        reason: CanonicalFailure,
    ) -> Result<CanonicalAction> {
        self.check_boot(inputs, boot)?;
        if self.phase == CanonicalPhase::Confirmed {
            return Err(invalid(
                "confirmed canonical boot cannot be failed implicitly",
            ));
        }
        self.phase = CanonicalPhase::Failed;
        self.failure.get_or_insert(reason);
        self.canonical_health = None;
        self.reconcile(inputs, boot)
    }
}
