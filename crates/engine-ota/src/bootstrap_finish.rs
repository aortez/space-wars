//! Retirement of bootstrap-only files and explicit initial recovery handoff.
//! Entering this state ends automatic staged fallback; every retired byte is
//! archived on /data before any boot file can be removed.
use super::canonical::CanonicalPlan;
use super::trial::TrialInputs;
use super::{invalid, trial_files};
use crate::recovery::RecoveryRecord;
use crate::{Blob, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FinishPhase {
    Archiving,
    Removing,
    Ready,
    Handoff,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FinishState {
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub files: BTreeMap<String, Blob>,
    pub phase: FinishPhase,
    pub removed: BTreeSet<String>,
    pub pending: Option<String>,
    pub baseline: Option<RecoveryRecord>,
}

impl FinishState {
    pub fn new(
        files: BTreeMap<String, Blob>,
        inputs: &TrialInputs,
        plan: &CanonicalPlan,
    ) -> Result<Self> {
        let state = Self {
            files,
            phase: FinishPhase::Archiving,
            removed: BTreeSet::new(),
            pending: None,
            baseline: None,
        };
        state.validate(inputs, plan)?;
        Ok(state)
    }

    pub fn validate(&self, inputs: &TrialInputs, plan: &CanonicalPlan) -> Result<()> {
        let mut expected = trial_files::expected(inputs, true);
        expected.retain(|p, _| !plan.files.contains_key(p) && p != "hostname.txt");
        let mut declared = self.files.clone();
        let temporary: Vec<_> = declared
            .keys()
            .filter(|p| trial_files::temporary(p))
            .cloned()
            .collect();
        if temporary.len() > 16 {
            return Err(invalid("too many bootstrap retirement temporaries"));
        }
        for path in temporary {
            if declared.remove(&path).unwrap().bytes > 16 * 1024 {
                return Err(invalid("oversized bootstrap retirement temporary"));
            }
        }
        let paths: BTreeSet<_> = self.files.keys().cloned().collect();
        if declared != expected
            || self.files.len() > 1024
            || !self.removed.is_subset(&paths)
            || self.pending.as_ref().is_some_and(|p| {
                self.phase != FinishPhase::Removing
                    || !paths.contains(p)
                    || self.removed.contains(p)
            })
            || (self.phase == FinishPhase::Archiving
                && (!self.removed.is_empty() || self.pending.is_some()))
            || (self.phase >= FinishPhase::Ready
                && (self.removed != paths || self.pending.is_some()))
            || (self.phase >= FinishPhase::Handoff) != self.baseline.is_some()
        {
            return Err(invalid("invalid bootstrap retirement state"));
        }
        if let Some(baseline) = &self.baseline {
            baseline.validate()?;
            let good = baseline.good();
            if good.slot != inputs.plan.target
                || good.build_id != inputs.image.build_id
                || good.generation != 0
                || good.data != inputs.image.data
                || baseline.retained_good().is_some()
                || baseline.transaction().is_some()
                || baseline.image_history().highest_attempted_generation != 0
                || !baseline.image_history().failed_images.is_empty()
            {
                return Err(invalid(
                    "bootstrap baseline differs from the confirmed unpublished image",
                ));
            }
        }
        Ok(())
    }

    pub fn check_successor(&self, previous: &Self) -> Result<()> {
        if self.files != previous.files
            || self.phase < previous.phase
            || !previous.removed.is_subset(&self.removed)
            || previous
                .baseline
                .as_ref()
                .is_some_and(|b| self.baseline.as_ref() != Some(b))
        {
            return Err(invalid(
                "bootstrap retirement intent or completed work changed",
            ));
        }
        Ok(())
    }

    pub fn archived(&mut self) -> Result<()> {
        if self.phase != FinishPhase::Archiving {
            return Err(invalid(
                "bootstrap retirement archive was already completed",
            ));
        }
        self.phase = FinishPhase::Removing;
        Ok(())
    }

    pub fn begin_file(&mut self, path: &str) -> Result<()> {
        if self.phase != FinishPhase::Removing
            || !self.files.contains_key(path)
            || self.removed.contains(path)
            || self.pending.as_deref().is_some_and(|p| p != path)
        {
            return Err(invalid("bootstrap removal lacks matching durable intent"));
        }
        self.pending = Some(path.into());
        Ok(())
    }

    pub fn removed(&mut self, path: &str) -> Result<()> {
        if self.phase != FinishPhase::Removing || self.pending.as_deref() != Some(path) {
            return Err(invalid("bootstrap removal completion lacks durable intent"));
        }
        self.removed.insert(path.into());
        self.pending = None;
        Ok(())
    }

    pub fn ready(&mut self) -> Result<()> {
        if self.phase != FinishPhase::Removing
            || self.pending.is_some()
            || self.removed.len() != self.files.len()
        {
            return Err(invalid("bootstrap retirement is incomplete"));
        }
        self.phase = FinishPhase::Ready;
        Ok(())
    }

    pub fn handoff(&mut self, baseline: RecoveryRecord) -> Result<()> {
        if self.phase != FinishPhase::Ready {
            return Err(invalid("bootstrap handoff requires completed cleanup"));
        }
        self.baseline = Some(baseline);
        self.phase = FinishPhase::Handoff;
        Ok(())
    }

    pub fn complete(&mut self) -> Result<()> {
        if self.phase != FinishPhase::Handoff {
            return Err(invalid("bootstrap handoff was not prepared"));
        }
        self.phase = FinishPhase::Complete;
        Ok(())
    }
}
