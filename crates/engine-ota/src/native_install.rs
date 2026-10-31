//! Ordinary fixed-path install session. The caller owns manual consent, its
//! durable association and the maintenance hold. This module never uses the
//! supervised admission bypass or starts a transaction on the caller's behalf.
use crate::boot_linux::{BootController, BootEvidence};
use crate::install_linux::{StagedPayload, WriteProgress, write_inactive};
use crate::install_lock::InstallLock;
use crate::provision_linux::{self, Status};
use crate::recovery::{FailureReason, Phase, Plan};
use crate::{
    DeviceIdentity, Error, ImageHistory, RecoveryStore, Result, Selection, Sha256, TrustStore,
    UsableTime, VerifiedCandidate,
};
use std::path::Path;

pub struct Session<'a> {
    boot: BootController<'a>,
    recovery: RecoveryStore,
    trust: TrustStore,
    payload: StagedPayload,
    candidate: VerifiedCandidate,
    original: BootEvidence,
    setup: Status,
    device: DeviceIdentity,
    history: ImageHistory,
}
impl<'a> Session<'a> {
    pub async fn open(
        lock: &'a InstallLock,
        expected: &VerifiedCandidate,
        time: &mut impl FnMut() -> Result<UsableTime>,
    ) -> Result<Self> {
        crate::bootstrap::require_no_active_bootstrap(lock)?;
        let boot = BootController::open(lock)?;
        let recovery = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
        boot.authorize_maintenance(&recovery)?;
        let original = boot.observe()?;
        let setup = provision_linux::inspect(&original.layout, &original.observation.binding)?;
        let device = DeviceIdentity {
            machine: original.image.machine.clone(),
            architecture: original.image.architecture.clone(),
            layout: original.image.layout.clone(),
            boot_identity: original.image.boot_identity,
            updater_protocol: original.image.updater_protocol,
            recovery_protocol: original.observation.recovery_protocol,
            inactive_slot_bytes: original
                .layout
                .inactive_partition()
                .sectors
                .checked_mul(512)
                .ok_or_else(|| invalid("inactive capacity overflow"))?,
            persistent_schema: setup.persistent_schema,
            rollback_reads: recovery.record().good().data.reads,
        };
        let history = recovery.record().image_history();
        let trust = TrustStore::open(Path::new(provision_linux::TRUST_PATH))?;
        let payload = StagedPayload::open(expected, &original.layout)?;
        let mut session = Self {
            boot,
            recovery,
            trust,
            payload,
            candidate: expected.clone(),
            original,
            setup,
            device,
            history,
        };
        session
            .trust
            .revalidate_image(
                expected.id(),
                session.payload.reader()?,
                Some(time()?),
                expected.image().channel,
                &session.device,
                &session.history,
            )
            .await?;
        session.plan(time).await?; // Qualify before asking the app to acknowledge.
        Ok(session)
    }

    pub fn recovery(&self) -> &RecoveryStore {
        &self.recovery
    }
    pub fn recovery_mut(&mut self) -> &mut RecoveryStore {
        &mut self.recovery
    }

    fn check_boot(&self) -> Result<()> {
        let current = self.boot.observe()?;
        let setup = provision_linux::inspect(&current.layout, &current.observation.binding)?;
        if current.observation.boot_id != self.original.observation.boot_id
            || current.observation.binding != self.original.observation.binding
            || current.image != self.original.image
            || current.layout != self.original.layout
            || setup.bundle != self.setup.bundle
            || setup.initial_root != self.setup.initial_root
            || setup.persistent_schema != self.setup.persistent_schema
        {
            return Err(invalid("installation boot or provisioned data changed"));
        }
        self.payload.check()
    }

    async fn metadata(&mut self, time: &mut impl FnMut() -> Result<UsableTime>) -> Result<()> {
        match self
            .trust
            .cached(
                Some(time()?),
                self.candidate.image().channel,
                &self.device,
                &self.history,
            )
            .await?
        {
            Selection::Available { candidate } if candidate.id() == self.candidate.id() => Ok(()),
            _ => Err(Error::CandidateChanged),
        }
    }

    /// Recheck after the app acknowledgement. Opened payload identity protects
    /// the earlier full hash; the writer also hashes again before opening a root.
    pub async fn plan(&mut self, time: &mut impl FnMut() -> Result<UsableTime>) -> Result<Plan> {
        self.check_boot()?;
        let plan = self.boot.plan_install(&self.recovery, &self.candidate)?;
        self.metadata(time).await?;
        self.payload.check()?;
        Ok(plan)
    }

    /// Requires the caller to have committed this session's exact plan through
    /// both journals. Errors retain/quarantine its transaction; never retry here.
    pub async fn write_and_reboot(
        &mut self,
        time: &mut impl FnMut() -> Result<UsableTime>,
        report: impl FnMut(WriteProgress),
        reboot: impl FnOnce(&mut TrialPermit<'_, '_>) -> Result<()>,
    ) -> Result<Sha256> {
        let transaction = self
            .recovery
            .durable_record()?
            .transaction()
            .filter(|tx| tx.phase == Phase::Writing && tx.candidate_id == self.candidate.id())
            .ok_or_else(|| invalid("exact durable installation admission required"))?
            .id;
        let result = async {
            self.check_boot()?;
            self.metadata(time).await?;
            let action = write_inactive(
                &self.boot,
                &mut self.recovery,
                &self.candidate,
                &mut self.payload,
                report,
            )?;
            self.check_boot()?;
            self.metadata(time).await?;
            self.boot.apply(&self.recovery, &action)?;
            let mut permit = TrialPermit::new(
                &self.boot,
                &mut self.recovery,
                &mut self.trust,
                time,
                self.candidate.expires(),
                transaction,
            );
            reboot(&mut permit)?;
            if !permit.is_queued() {
                return Err(invalid("trial reboot was not acknowledged"));
            }
            Ok(transaction)
        }
        .await;
        if result.is_err()
            && self
                .recovery
                .durable_record()?
                .transaction()
                .is_some_and(|tx| {
                    tx.id == transaction
                        && !matches!(tx.phase, Phase::Committed | Phase::RolledBack)
                })
        {
            let boot = self.boot.observe()?;
            self.recovery
                .advance(|r| r.fail(FailureReason::InstallationFailed, &boot.observation))?;
        }
        result
    }
}

/// Only a live admitted session can create a permit. Keep its install lock and
/// maintenance hold alive until reboot; an uncertain reply is never replayed.
pub struct TrialPermit<'a, 'b> {
    boot: &'a BootController<'b>,
    recovery: &'a mut RecoveryStore,
    /// Signed candidates recheck trusted time and metadata expiry before each
    /// reboot step. A development image has no metadata to expire.
    signed: Option<SignedTime<'a>>,
    transaction: Sha256,
    consumed: bool,
    queued: bool,
}
struct SignedTime<'a> {
    trust: &'a mut TrustStore,
    time: &'a mut dyn FnMut() -> Result<UsableTime>,
    expires: jiff::Timestamp,
}
impl<'a, 'b> TrialPermit<'a, 'b> {
    pub(crate) fn new(
        boot: &'a BootController<'b>,
        recovery: &'a mut RecoveryStore,
        trust: &'a mut TrustStore,
        time: &'a mut dyn FnMut() -> Result<UsableTime>,
        expires: jiff::Timestamp,
        transaction: Sha256,
    ) -> Self {
        Self {
            boot,
            recovery,
            signed: Some(SignedTime {
                trust,
                time,
                expires,
            }),
            transaction,
            consumed: false,
            queued: false,
        }
    }
    pub(crate) fn development(
        boot: &'a BootController<'b>,
        recovery: &'a mut RecoveryStore,
        transaction: Sha256,
    ) -> Self {
        Self {
            boot,
            recovery,
            signed: None,
            transaction,
            consumed: false,
            queued: false,
        }
    }
    fn time(&mut self) -> Result<()> {
        match &mut self.signed {
            Some(signed) => admit_reboot_time(signed.trust, (signed.time)(), signed.expires),
            None => Ok(()),
        }
    }
    pub fn begin(&mut self) -> Result<()> {
        if self.consumed {
            return Err(invalid("trial request already consumed"));
        }
        self.time()?;
        let boot = self
            .boot
            .authorize_trial_reboot(self.recovery, self.transaction, false)?;
        self.recovery
            .advance(|r| r.trial_prepared(self.transaction, &boot))?;
        self.consumed = true;
        Ok(())
    }
    pub fn authorize(&mut self) -> Result<()> {
        if !self.consumed || self.queued {
            return Err(invalid("trial request is not pending"));
        }
        self.time()?;
        self.boot
            .authorize_trial_reboot(self.recovery, self.transaction, true)?;
        Ok(())
    }
    pub fn queued(&mut self) -> Result<()> {
        self.authorize()?;
        self.queued = true;
        Ok(())
    }
    pub(crate) fn is_queued(&self) -> bool {
        self.queued
    }
    pub fn fail(&mut self) -> Result<()> {
        let boot = self.boot.observe()?;
        self.recovery
            .advance(|r| r.fail(FailureReason::InstallationFailed, &boot.observation))?;
        Ok(())
    }
}
pub(crate) fn admit_reboot_time(
    trust: &mut TrustStore,
    time: Result<UsableTime>,
    expires: jiff::Timestamp,
) -> Result<()> {
    let time = time?;
    trust.admit_time(time)?;
    if time.0 >= expires {
        return Err(invalid("metadata expired before trial reboot"));
    }
    Ok(())
}
fn invalid(detail: &str) -> Error {
    Error::Install(detail.into())
}
