//! Rootfs trial transitions. The device service must durably commit a plan's
//! record before performing its action. This module never touches boot files,
//! opens a watchdog, writes a partition, or authorizes installation by itself.
use crate::{DataCompatibility, ImageDescriptor, ImageHistory, Sha256, VerifiedCandidate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RECOVERY_PROTOCOL: u32 = 1;
pub const TRIAL_DEADLINE_MS: u64 = 180_000;
pub const HEALTH_WINDOW_MS: u64 = 60_000;
pub const MAX_HEALTH_GAP_MS: u64 = 5_000;
pub const MAX_ATTEMPTS_PER_IMAGE: u8 = 2;
const MAX_FAILURES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Binds the journal to one provisioned device and layout. The Linux adapter
/// derives this from actual block devices and immutable boot/profile evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootBinding {
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub boot_identity: Sha256,
    pub hardware_profile: Sha256,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotImage {
    pub slot: Slot,
    pub build_id: String,
    pub generation: u64,
    pub payload: Sha256,
    pub expanded: Sha256,
    pub data: DataCompatibility,
}

impl SlotImage {
    fn from_descriptor(slot: Slot, image: &ImageDescriptor) -> Self {
        Self {
            slot,
            build_id: image.build_id.clone(),
            generation: image.generation,
            payload: image.compressed.sha256,
            expanded: image.expanded.sha256,
            data: image.data,
        }
    }

    fn valid(&self) -> bool {
        self.generation > 0
            && identifier(&self.build_id)
            && self.data.reads.min <= self.data.reads.max
            && self.data.reads.contains(self.data.writes)
    }
}

/// Read-only facts from this boot, never a UI-supplied slot or build string.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BootObservation {
    pub binding: BootBinding,
    pub boot_id: String,
    pub uptime_ms: u64,
    pub actual_slot: Option<Slot>,
    pub actual_build_id: Option<String>,
    pub recovery_protocol: u32,
    pub default_slot: Option<Slot>,
    pub tryboot: bool,
    pub trial_id: Option<Sha256>,
}

impl BootObservation {
    fn runs(&self, image: &SlotImage) -> bool {
        self.actual_slot == Some(image.slot)
            && self.actual_build_id.as_deref() == Some(&image.build_id)
            && self.recovery_protocol >= RECOVERY_PROTOCOL
    }

    fn normal(&self, image: &SlotImage) -> bool {
        self.runs(image)
            && self.default_slot == Some(image.slot)
            && !self.tryboot
            && self.trial_id.is_none()
    }
}

/// Qualification is tied to exact boot assets, firmware, kernel and media by
/// the service. Mere presence of /dev/watchdog or two root partitions is not
/// evidence. An unqualified device cannot start a transaction.
#[derive(Clone, Debug)]
pub struct RecoveryCapability {
    pub binding: BootBinding,
    pub tryboot_qualified: bool,
    pub watchdog_qualified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Writing,
    Prepared,
    TrialRequested,
    Observing,
    Promoting,
    RollbackPending,
    RolledBack,
    Committed,
}

impl Phase {
    fn terminal(self) -> bool {
        matches!(self, Self::RolledBack | Self::Committed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureReason {
    InterruptedBeforeTrial,
    TrialReset,
    HealthTimeout,
    WrongImage,
    PersistentStateUnavailable,
    PromotionInterrupted,
    InstallationFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedImage {
    pub generation: u64,
    pub attempts: u8,
    pub reason: FailureReason,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: Sha256,
    pub candidate_id: Sha256,
    pub previous_good: SlotImage,
    pub candidate: SlotImage,
    pub attempt: u8,
    pub origin_boot_id: String,
    pub trial_boot_id: Option<String>,
    pub phase: Phase,
    pub health: Option<HealthProof>,
    pub failure: Option<FailureReason>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRecord {
    schema: u32,
    binding: BootBinding,
    good: SlotImage,
    retained_good: Option<SlotImage>,
    highest_attempted_generation: u64,
    sequence: u64,
    failures: BTreeMap<Sha256, FailedImage>,
    transaction: Option<Transaction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    None,
    WriteInactive { slot: Slot, transaction: Sha256 },
    PrepareTrial { slot: Slot, transaction: Sha256 },
    RebootTrial { transaction: Sha256 },
    ObserveTrial { transaction: Sha256 },
    PromoteDefault { slot: Slot, transaction: Sha256 },
    RestoreDefault { slot: Slot, transaction: Sha256 },
    RebootPreviousGood { transaction: Sha256 },
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub(crate) previous: Sha256,
    pub(crate) record: RecoveryRecord,
    pub(crate) action: Action,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum RecoveryError {
    #[error("missing, inconsistent or unsupported recovery state")]
    InvalidState,
    #[error("actual root, build, boot mode or boot identity does not match recovery state")]
    BootMismatch,
    #[error("trial boot and watchdog recovery require hardware qualification")]
    Unqualified,
    #[error("another update transaction is still active")]
    Busy,
    #[error("candidate is incompatible, already installed or below the attempted generation floor")]
    Ineligible,
    #[error("candidate is quarantined or its explicit retry allowance is exhausted")]
    Quarantined,
    #[error("unexpected or stale transaction command")]
    StaleCommand,
    #[error("written bytes do not match the verified candidate")]
    WriteMismatch,
    #[error("continuous local health has not been established")]
    Unhealthy,
}

type Result<T> = std::result::Result<T, RecoveryError>;

impl RecoveryRecord {
    /// Only the supervised bootstrap creates initial good-image evidence.
    /// Missing/corrupt state is never silently reinitialized by the daemon.
    pub fn bootstrap(
        binding: BootBinding,
        good: SlotImage,
        boot: &BootObservation,
    ) -> Result<Self> {
        if boot.binding != binding || !boot.normal(&good) || !boot_id(&boot.boot_id) {
            return Err(RecoveryError::BootMismatch);
        }
        let record = Self {
            schema: 1,
            binding,
            highest_attempted_generation: good.generation,
            good,
            retained_good: None,
            sequence: 0,
            failures: BTreeMap::new(),
            transaction: None,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<()> {
        let invalid = RecoveryError::InvalidState;
        if self.schema != 1
            || !self.good.valid()
            || self.highest_attempted_generation < self.good.generation
            || self.failures.len() > MAX_FAILURES
        {
            return Err(invalid);
        }
        if let Some(previous) = &self.retained_good
            && (!previous.valid() || previous.slot == self.good.slot)
        {
            return Err(invalid);
        }
        for failed in self.failures.values() {
            if failed.generation == 0
                || failed.generation > self.highest_attempted_generation
                || !(1..=MAX_ATTEMPTS_PER_IMAGE).contains(&failed.attempts)
            {
                return Err(invalid);
            }
        }
        if let Some(tx) = &self.transaction {
            if self.sequence == 0
                || !tx.previous_good.valid()
                || !tx.candidate.valid()
                || tx.previous_good.slot == tx.candidate.slot
                || tx.candidate.generation <= tx.previous_good.generation
                || tx.candidate.generation != self.highest_attempted_generation
                || !(1..=MAX_ATTEMPTS_PER_IMAGE).contains(&tx.attempt)
                || !boot_id(&tx.origin_boot_id)
                || tx
                    .trial_boot_id
                    .as_ref()
                    .is_some_and(|id| !boot_id(id) || id == &tx.origin_boot_id)
                || !tx
                    .previous_good
                    .data
                    .reads
                    .contains(tx.candidate.data.writes)
                || !tx
                    .candidate
                    .data
                    .reads
                    .contains(tx.previous_good.data.writes)
                || tx.id
                    != transaction_id(
                        self.sequence,
                        tx.candidate_id,
                        &tx.origin_boot_id,
                        &self.binding,
                    )
            {
                return Err(invalid);
            }
            if tx.phase == Phase::Committed {
                if self.good != tx.candidate
                    || self.retained_good.as_ref() != Some(&tx.previous_good)
                {
                    return Err(invalid);
                }
            } else if self.good != tx.previous_good || self.retained_good.is_some() {
                return Err(invalid);
            }
            if matches!(
                tx.phase,
                Phase::Observing | Phase::Promoting | Phase::Committed
            ) && tx.trial_boot_id.is_none()
            {
                return Err(invalid);
            }
            if matches!(
                tx.phase,
                Phase::Writing | Phase::Prepared | Phase::TrialRequested
            ) && tx.trial_boot_id.is_some()
            {
                return Err(invalid);
            }
            if matches!(tx.phase, Phase::Promoting | Phase::Committed) {
                let health = tx.health.as_ref().ok_or(RecoveryError::InvalidState)?;
                if !health.valid(tx) {
                    return Err(invalid);
                }
            } else if tx.health.is_some() {
                return Err(invalid);
            }
            if matches!(tx.phase, Phase::RollbackPending | Phase::RolledBack) {
                let failed = self
                    .failures
                    .get(&tx.candidate.payload)
                    .ok_or(RecoveryError::InvalidState)?;
                if tx.failure != Some(failed.reason) || failed.attempts != tx.attempt {
                    return Err(invalid);
                }
            } else if tx.failure.is_some() {
                return Err(invalid);
            }
        }
        Ok(())
    }

    pub fn good(&self) -> &SlotImage {
        &self.good
    }
    pub fn retained_good(&self) -> Option<&SlotImage> {
        self.retained_good.as_ref()
    }
    pub fn transaction(&self) -> Option<&Transaction> {
        self.transaction.as_ref()
    }

    pub fn image_history(&self) -> ImageHistory {
        ImageHistory {
            installed_sha256: Some(self.good.payload),
            highest_attempted_generation: self.highest_attempted_generation,
            failed_images: self.failures.keys().copied().collect(),
        }
    }

    /// Used only for an explicit retry of the current failed payload. The
    /// verifier still performs signatures, expiration and compatibility checks.
    /// This temporary view is never persisted and never lowers the real floor.
    pub fn retry_verification_history(&self, payload: Sha256) -> Result<ImageHistory> {
        self.validate()?;
        let failed = self
            .failures
            .get(&payload)
            .ok_or(RecoveryError::Quarantined)?;
        if failed.attempts >= MAX_ATTEMPTS_PER_IMAGE
            || failed.generation != self.highest_attempted_generation
            || self
                .transaction
                .as_ref()
                .is_none_or(|tx| tx.phase != Phase::RolledBack || tx.candidate.payload != payload)
        {
            return Err(RecoveryError::Quarantined);
        }
        let mut history = self.image_history();
        history.failed_images.remove(&payload);
        history.highest_attempted_generation = failed.generation - 1;
        Ok(history)
    }

    pub fn begin(
        &self,
        candidate: &VerifiedCandidate,
        boot: &BootObservation,
        capability: &RecoveryCapability,
    ) -> Result<Plan> {
        self.begin_descriptor(candidate.image(), boot, capability, false)
    }

    pub fn begin_explicit_retry(
        &self,
        candidate: &VerifiedCandidate,
        boot: &BootObservation,
        capability: &RecoveryCapability,
    ) -> Result<Plan> {
        self.retry_verification_history(candidate.image().compressed.sha256)?;
        self.begin_descriptor(candidate.image(), boot, capability, true)
    }

    fn begin_descriptor(
        &self,
        image: &ImageDescriptor,
        boot: &BootObservation,
        capability: &RecoveryCapability,
        explicit_retry: bool,
    ) -> Result<Plan> {
        self.check_boot(boot)?;
        if !boot.normal(&self.good) {
            return Err(RecoveryError::BootMismatch);
        }
        if capability.binding != self.binding
            || !capability.tryboot_qualified
            || !capability.watchdog_qualified
        {
            return Err(RecoveryError::Unqualified);
        }
        if self
            .transaction
            .as_ref()
            .is_some_and(|tx| !tx.phase.terminal())
        {
            return Err(RecoveryError::Busy);
        }
        if image.validate().is_err()
            || image.boot_identity != self.binding.boot_identity
            || image.min_recovery_protocol > RECOVERY_PROTOCOL
            || image.compressed.sha256 == self.good.payload
            || !self.good.data.reads.contains(image.data.writes)
            || !image.data.reads.contains(self.good.data.writes)
        {
            return Err(RecoveryError::Ineligible);
        }
        let attempt = match self.failures.get(&image.compressed.sha256) {
            Some(failed) if explicit_retry && failed.generation == image.generation => {
                self.retry_verification_history(image.compressed.sha256)?;
                failed.attempts + 1
            }
            Some(_) => return Err(RecoveryError::Quarantined),
            None if explicit_retry => return Err(RecoveryError::Quarantined),
            None => 1,
        };
        if (!explicit_retry && image.generation <= self.highest_attempted_generation)
            || image.generation <= self.good.generation
        {
            return Err(RecoveryError::Ineligible);
        }
        if self.failures.len() == MAX_FAILURES
            && !self.failures.contains_key(&image.compressed.sha256)
        {
            return Err(RecoveryError::Quarantined);
        }
        let mut next = self.clone();
        next.sequence = next
            .sequence
            .checked_add(1)
            .ok_or(RecoveryError::InvalidState)?;
        let id = transaction_id(
            next.sequence,
            image.candidate_id(),
            &boot.boot_id,
            &self.binding,
        );
        // The writer will overwrite this slot. Never continue advertising its
        // former content as a retained good image after granting write access.
        next.retained_good = None;
        next.highest_attempted_generation = next.highest_attempted_generation.max(image.generation);
        next.transaction = Some(Transaction {
            id,
            candidate_id: image.candidate_id(),
            previous_good: self.good.clone(),
            candidate: SlotImage::from_descriptor(self.good.slot.other(), image),
            attempt,
            origin_boot_id: boot.boot_id.clone(),
            trial_boot_id: None,
            phase: Phase::Writing,
            health: None,
            failure: None,
        });
        next.plan(
            self,
            Action::WriteInactive {
                slot: self.good.slot.other(),
                transaction: id,
            },
        )
    }

    /// The privileged writer supplies independently checked compressed and
    /// expanded digests after flushing and rereading the inactive device.
    pub fn written(
        &self,
        id: Sha256,
        payload: Sha256,
        expanded: Sha256,
        boot: &BootObservation,
    ) -> Result<Plan> {
        self.check_boot(boot)?;
        let tx = self.command(id, Phase::Writing)?;
        if !boot.normal(&self.good) || boot.boot_id != tx.origin_boot_id {
            return Err(RecoveryError::BootMismatch);
        }
        if payload != tx.candidate.payload || expanded != tx.candidate.expanded {
            return Err(RecoveryError::WriteMismatch);
        }
        let mut next = self.clone();
        next.transaction.as_mut().unwrap().phase = Phase::Prepared;
        next.plan(
            self,
            Action::PrepareTrial {
                slot: tx.candidate.slot,
                transaction: id,
            },
        )
    }

    /// Called after the trial selector has been flushed and read back. The
    /// default must still select the old good root. Persist before rebooting;
    /// replaying this command must never request another trial boot.
    pub fn trial_prepared(&self, id: Sha256, boot: &BootObservation) -> Result<Plan> {
        self.check_boot(boot)?;
        let tx = self.command(id, Phase::Prepared)?;
        if !boot.normal(&self.good) || boot.boot_id != tx.origin_boot_id {
            return Err(RecoveryError::BootMismatch);
        }
        let mut next = self.clone();
        next.transaction.as_mut().unwrap().phase = Phase::TrialRequested;
        next.plan(self, Action::RebootTrial { transaction: id })
    }

    /// Reconcile after every service start. A fresh observer must establish a
    /// new continuous health window; the deadline remains measured from boot.
    pub fn reconcile(&self, boot: &BootObservation) -> Result<Plan> {
        self.check_boot(boot)?;
        let Some(tx) = &self.transaction else {
            return if boot.normal(&self.good) {
                self.clone().plan(self, Action::None)
            } else {
                Err(RecoveryError::BootMismatch)
            };
        };
        if tx.phase.terminal() {
            let just_committed = tx.phase == Phase::Committed
                && tx.trial_boot_id.as_ref() == Some(&boot.boot_id)
                && self.is_trial(tx, boot)
                && boot.runs(&self.good)
                && boot.default_slot == Some(self.good.slot);
            return if boot.normal(&self.good) || just_committed {
                self.clone().plan(self, Action::None)
            } else {
                Err(RecoveryError::BootMismatch)
            };
        }
        if boot.normal(&tx.previous_good) {
            let reason = tx.failure.unwrap_or(match tx.phase {
                Phase::Writing | Phase::Prepared => FailureReason::InterruptedBeforeTrial,
                Phase::TrialRequested if boot.boot_id == tx.origin_boot_id => {
                    FailureReason::InterruptedBeforeTrial
                }
                Phase::Promoting => FailureReason::PromotionInterrupted,
                _ => FailureReason::TrialReset,
            });
            return self.fail(reason, boot);
        }
        if tx.phase == Phase::Promoting
            && boot.runs(&tx.candidate)
            && boot.default_slot == Some(tx.candidate.slot)
            && ((!boot.tryboot && boot.trial_id.is_none()) || self.is_trial(tx, boot))
        {
            return self.finish_promotion(boot);
        }
        if !self.is_trial(tx, boot) {
            return Err(RecoveryError::BootMismatch);
        }
        if tx.phase == Phase::RollbackPending {
            return self.fail(tx.failure.ok_or(RecoveryError::InvalidState)?, boot);
        }
        if !matches!(
            tx.phase,
            Phase::TrialRequested | Phase::Observing | Phase::Promoting
        ) || boot.default_slot != Some(tx.previous_good.slot)
            || boot.boot_id == tx.origin_boot_id
            || tx
                .trial_boot_id
                .as_ref()
                .is_some_and(|id| id != &boot.boot_id)
        {
            return Err(RecoveryError::BootMismatch);
        }
        if !boot.runs(&tx.candidate) {
            return self.fail(FailureReason::WrongImage, boot);
        }
        if boot.uptime_ms >= TRIAL_DEADLINE_MS {
            return self.fail(FailureReason::HealthTimeout, boot);
        }
        let mut next = self.clone();
        let active = next.transaction.as_mut().unwrap();
        active.phase = Phase::Observing;
        active.trial_boot_id = Some(boot.boot_id.clone());
        active.health = None;
        next.plan(self, Action::ObserveTrial { transaction: tx.id })
    }

    pub fn health_confirmed(&self, proof: HealthProof, boot: &BootObservation) -> Result<Plan> {
        self.check_boot(boot)?;
        let tx = self.command(proof.transaction, Phase::Observing)?;
        if !self.is_trial(tx, boot)
            || !boot.runs(&tx.candidate)
            || boot.default_slot != Some(tx.previous_good.slot)
            || tx.trial_boot_id.as_ref() != Some(&boot.boot_id)
        {
            return Err(RecoveryError::BootMismatch);
        }
        if !proof.valid(tx)
            || proof.finished_ms > boot.uptime_ms
            || boot.uptime_ms - proof.finished_ms > MAX_HEALTH_GAP_MS
            || boot.uptime_ms >= TRIAL_DEADLINE_MS
        {
            return Err(RecoveryError::Unhealthy);
        }
        let mut next = self.clone();
        let active = next.transaction.as_mut().unwrap();
        active.phase = Phase::Promoting;
        active.health = Some(proof);
        next.plan(
            self,
            Action::PromoteDefault {
                slot: tx.candidate.slot,
                transaction: tx.id,
            },
        )
    }

    /// The selector change and record commit are separate durable operations.
    /// Promoting records retain proof so either bootable selector choice can be
    /// reconciled if power is lost between them.
    pub fn finish_promotion(&self, boot: &BootObservation) -> Result<Plan> {
        self.check_boot(boot)?;
        let tx = self
            .transaction
            .as_ref()
            .ok_or(RecoveryError::StaleCommand)?;
        if tx.phase != Phase::Promoting {
            return Err(RecoveryError::StaleCommand);
        }
        if !boot.runs(&tx.candidate)
            || boot.default_slot != Some(tx.candidate.slot)
            || (boot.tryboot && !self.is_trial(tx, boot))
            || (!boot.tryboot && boot.trial_id.is_some())
        {
            return Err(RecoveryError::BootMismatch);
        }
        let mut next = self.clone();
        next.retained_good = Some(tx.previous_good.clone());
        next.good = tx.candidate.clone();
        next.transaction.as_mut().unwrap().phase = Phase::Committed;
        next.plan(self, Action::None)
    }

    pub fn fail(&self, reason: FailureReason, boot: &BootObservation) -> Result<Plan> {
        self.check_boot(boot)?;
        let tx = self
            .transaction
            .as_ref()
            .ok_or(RecoveryError::StaleCommand)?;
        if tx.phase.terminal() {
            return Err(RecoveryError::StaleCommand);
        }
        let on_good = boot.normal(&tx.previous_good);
        if !on_good
            && (!self.is_trial(tx, boot)
                || !matches!(boot.default_slot, Some(slot) if slot == tx.previous_good.slot || slot == tx.candidate.slot))
        {
            return Err(RecoveryError::BootMismatch);
        }
        let reason = tx.failure.unwrap_or(reason);
        let mut next = self.clone();
        next.failures.insert(
            tx.candidate.payload,
            FailedImage {
                generation: tx.candidate.generation,
                attempts: tx.attempt,
                reason,
            },
        );
        let active = next.transaction.as_mut().unwrap();
        active.phase = if on_good {
            Phase::RolledBack
        } else {
            Phase::RollbackPending
        };
        active.failure = Some(reason);
        active.health = None;
        let action = if on_good {
            Action::None
        } else if boot.default_slot == Some(tx.candidate.slot) {
            Action::RestoreDefault {
                slot: tx.previous_good.slot,
                transaction: tx.id,
            }
        } else {
            Action::RebootPreviousGood { transaction: tx.id }
        };
        next.plan(self, action)
    }

    pub(crate) fn fingerprint(&self) -> Sha256 {
        Sha256::digest(&serde_json::to_vec(self).expect("recovery state serializes"))
    }

    /// The Linux adapter may change only the selector corresponding to a
    /// durably stored phase. Replaying a completed promotion is reconciled by
    /// finish_promotion(), rather than writing the default a second time.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn selection_change(
        &self,
        action: &Action,
        boot: &BootObservation,
    ) -> Result<(Slot, Option<Sha256>)> {
        self.check_boot(boot)?;
        match *action {
            Action::PrepareTrial { slot, transaction } => {
                let tx = self.command(transaction, Phase::Prepared)?;
                if slot != tx.candidate.slot
                    || !boot.normal(&tx.previous_good)
                    || boot.boot_id != tx.origin_boot_id
                {
                    return Err(RecoveryError::BootMismatch);
                }
                Ok((slot, Some(transaction)))
            }
            Action::PromoteDefault { slot, transaction } => {
                let tx = self.command(transaction, Phase::Promoting)?;
                if slot != tx.candidate.slot
                    || !self.is_trial(tx, boot)
                    || !boot.runs(&tx.candidate)
                    || tx.trial_boot_id.as_ref() != Some(&boot.boot_id)
                    || boot.default_slot != Some(tx.previous_good.slot)
                {
                    return Err(RecoveryError::BootMismatch);
                }
                let proof = tx.health.as_ref().ok_or(RecoveryError::Unhealthy)?;
                if proof.finished_ms > boot.uptime_ms
                    || boot.uptime_ms - proof.finished_ms > MAX_HEALTH_GAP_MS
                    || boot.uptime_ms >= TRIAL_DEADLINE_MS
                {
                    return Err(RecoveryError::Unhealthy);
                }
                Ok((slot, None))
            }
            Action::RestoreDefault { slot, transaction } => {
                let tx = self.command(transaction, Phase::RollbackPending)?;
                if slot != tx.previous_good.slot
                    || !self.is_trial(tx, boot)
                    || boot.boot_id == tx.origin_boot_id
                    || !matches!(boot.default_slot, Some(value) if value == tx.previous_good.slot || value == tx.candidate.slot)
                {
                    return Err(RecoveryError::BootMismatch);
                }
                Ok((slot, None))
            }
            _ => Err(RecoveryError::StaleCommand),
        }
    }

    fn check_boot(&self, boot: &BootObservation) -> Result<()> {
        self.validate()?;
        if boot.binding != self.binding
            || !boot_id(&boot.boot_id)
            || boot.actual_slot.is_none()
            || boot.default_slot.is_none()
        {
            return Err(RecoveryError::BootMismatch);
        }
        Ok(())
    }

    fn is_trial(&self, tx: &Transaction, boot: &BootObservation) -> bool {
        boot.tryboot && boot.trial_id == Some(tx.id) && boot.actual_slot == Some(tx.candidate.slot)
    }

    fn command(&self, id: Sha256, phase: Phase) -> Result<&Transaction> {
        self.transaction
            .as_ref()
            .filter(|tx| tx.id == id && tx.phase == phase)
            .ok_or(RecoveryError::StaleCommand)
    }

    fn plan(self, previous: &Self, action: Action) -> Result<Plan> {
        self.validate()?;
        Ok(Plan {
            previous: previous.fingerprint(),
            record: self,
            action,
        })
    }
}

/// A fresh control response is tied to the actual systemd process instance and
/// executable. The adapter checks the peer PID/start time and executable hash.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInstance {
    pub pid: u32,
    pub start_ticks: u64,
    pub invocation_id: String,
}

#[derive(Clone, Debug)]
pub struct HealthSample {
    pub uptime_ms: u64,
    pub instance: AppInstance,
    pub build_id: String,
    pub executable_verified: bool,
    pub state_writable: bool,
    pub control_response: u64,
    pub presented_frames: u64,
}

/// Only HealthMonitor constructs proofs. Persistent deserialization is for the
/// private, checksummed root-owned journal, never a socket request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthProof {
    transaction: Sha256,
    boot_id: String,
    started_ms: u64,
    finished_ms: u64,
    instance: AppInstance,
}

impl HealthProof {
    fn valid(&self, tx: &Transaction) -> bool {
        self.transaction == tx.id
            && tx.trial_boot_id.as_ref() == Some(&self.boot_id)
            && self.finished_ms < TRIAL_DEADLINE_MS
            && self
                .finished_ms
                .checked_sub(self.started_ms)
                .is_some_and(|duration| duration >= HEALTH_WINDOW_MS)
            && self.instance.pid > 1
            && self.instance.start_ticks > 0
            && invocation_id(&self.instance.invocation_id)
    }
}

pub struct HealthMonitor {
    transaction: Sha256,
    boot_id: String,
    build_id: String,
    started_ms: Option<u64>,
    last: Option<HealthSample>,
}

impl HealthMonitor {
    pub fn new(record: &RecoveryRecord, boot: &BootObservation) -> Result<Self> {
        record.check_boot(boot)?;
        let tx = record
            .transaction
            .as_ref()
            .ok_or(RecoveryError::StaleCommand)?;
        if tx.phase != Phase::Observing
            || !record.is_trial(tx, boot)
            || !boot.runs(&tx.candidate)
            || tx.trial_boot_id.as_ref() != Some(&boot.boot_id)
        {
            return Err(RecoveryError::BootMismatch);
        }
        Ok(Self {
            transaction: tx.id,
            boot_id: boot.boot_id.clone(),
            build_id: tx.candidate.build_id.clone(),
            started_ms: None,
            last: None,
        })
    }

    /// A failed request, stale frame count, observation gap or process restart
    /// breaks continuity. Wall time and Internet state play no role.
    pub fn observe(&mut self, sample: Option<HealthSample>) -> Result<Option<HealthProof>> {
        let Some(sample) = sample else {
            self.started_ms = None;
            self.last = None;
            return Ok(None);
        };
        if sample.uptime_ms >= TRIAL_DEADLINE_MS {
            self.started_ms = None;
            self.last = None;
            return Err(RecoveryError::Unhealthy);
        }
        if sample.build_id != self.build_id
            || !sample.executable_verified
            || !sample.state_writable
            || sample.instance.pid <= 1
            || sample.instance.start_ticks == 0
            || !invocation_id(&sample.instance.invocation_id)
        {
            self.started_ms = None;
            self.last = None;
            return Err(RecoveryError::Unhealthy);
        }
        let continuous = self.last.as_ref().is_some_and(|last| {
            last.instance == sample.instance
                && sample
                    .uptime_ms
                    .checked_sub(last.uptime_ms)
                    .is_some_and(|gap| gap > 0 && gap <= MAX_HEALTH_GAP_MS)
                && sample.control_response > last.control_response
                && sample.presented_frames > last.presented_frames
        });
        if !continuous {
            self.started_ms = Some(sample.uptime_ms);
        }
        let started = self.started_ms.unwrap_or(sample.uptime_ms);
        let proof =
            (sample.uptime_ms.saturating_sub(started) >= HEALTH_WINDOW_MS).then(|| HealthProof {
                transaction: self.transaction,
                boot_id: self.boot_id.clone(),
                started_ms: started,
                finished_ms: sample.uptime_ms,
                instance: sample.instance.clone(),
            });
        self.last = Some(sample);
        Ok(proof)
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn transaction_id(sequence: u64, candidate: Sha256, origin: &str, binding: &BootBinding) -> Sha256 {
    Sha256::digest(
        &serde_json::to_vec(&(sequence, candidate, origin, binding))
            .expect("transaction identity serializes"),
    )
}

fn invocation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn boot_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
