//! Explicit owner-selected signed installation for physical qualification.
//! Excluded from default builds. Uses the ordinary journal/writer/recovery; it
//! cannot grant automatic capability, choose paths or reset attempt history.
use crate::boot_linux::{BootController, BootEvidence};
use crate::install_linux::{StagedPayload, WriteProgress, write_inactive};
use crate::install_lock::InstallLock;
pub use crate::native_install::TrialPermit;
use crate::provision_linux::{self, Status};
use crate::recovery::{Action, BootBinding, FailureReason, RecoveryRecord};
use crate::{
    Channel, DeviceIdentity, Error, ImageDescriptor, ImageHistory, ImageIdentity, RecoveryStore,
    Result, Selection as FeedSelection, Sha256, TrustStore, UsableTime, VerifiedCandidate,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_SELECTION_BYTES: u64 = 64 * 1024;
const SELECTION_MS: u64 = 15 * 60_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    schema: u32,
    purpose: String,
    boot_id: String,
    binding: BootBinding,
    installed_image: ImageIdentity,
    recovery: Sha256,
    bundle: Sha256,
    initial_root: Sha256,
    persistent_schema: u32,
    candidate: ImageDescriptor,
    retry_payload: Option<Sha256>,
    selected_ms: u64,
    expires_ms: u64,
}

impl Selection {
    pub fn fingerprint(&self) -> Sha256 {
        Sha256::digest(&serde_json::to_vec(self).expect("selection serializes"))
    }

    pub fn parse(bytes: &[u8], expected: Sha256) -> Result<Self> {
        if bytes.len() as u64 > MAX_SELECTION_BYTES {
            return Err(invalid("selection exceeds its size bound"));
        }
        let selected: Self = serde_json::from_slice(bytes)?;
        if selected.fingerprint() != expected {
            return Err(invalid("selection differs from the owner-selected digest"));
        }
        Ok(selected)
    }

    fn new(
        boot: &BootEvidence,
        record: &RecoveryRecord,
        setup: &Status,
        candidate: &VerifiedCandidate,
        retry_payload: Option<Sha256>,
    ) -> Result<Self> {
        let selected_ms = boot.observation.uptime_ms;
        Ok(Self {
            schema: 1,
            purpose: "supervised-signed-trial".into(),
            boot_id: boot.observation.boot_id.clone(),
            binding: boot.observation.binding.clone(),
            installed_image: boot.image.clone(),
            recovery: record.fingerprint(),
            bundle: setup.bundle,
            initial_root: setup.initial_root,
            persistent_schema: setup.persistent_schema,
            candidate: candidate.image().clone(),
            retry_payload,
            selected_ms,
            expires_ms: selected_ms
                .checked_add(SELECTION_MS)
                .ok_or_else(|| invalid("selection time overflow"))?,
        })
    }

    fn check(
        &self,
        boot: &BootEvidence,
        record: &RecoveryRecord,
        setup: &Status,
        candidate: &VerifiedCandidate,
    ) -> Result<()> {
        let now = boot.observation.uptime_ms;
        if self.schema != 1
            || self.purpose != "supervised-signed-trial"
            || self.selected_ms.checked_add(SELECTION_MS) != Some(self.expires_ms)
            || now < self.selected_ms
            || now >= self.expires_ms
            || self.boot_id != boot.observation.boot_id
            || self.binding != boot.observation.binding
            || boot.observation.tryboot
            || self.installed_image != boot.image
            || self.recovery != record.fingerprint()
            || self.bundle != setup.bundle
            || self.initial_root != setup.initial_root
            || self.persistent_schema != setup.persistent_schema
            || &self.candidate != candidate.image()
            || self
                .retry_payload
                .is_some_and(|payload| payload != self.candidate.compressed.sha256)
        {
            return Err(invalid(
                "selected boot, history, candidate, data or deadline changed",
            ));
        }
        record.maintenance(&boot.observation)?;
        Ok(())
    }
}

struct Context<'a> {
    boot: BootController<'a>,
    recovery: RecoveryStore,
    trust: TrustStore,
    evidence: BootEvidence,
    setup: Status,
    device: DeviceIdentity,
}

impl<'a> Context<'a> {
    fn open(lock: &'a InstallLock) -> Result<Self> {
        crate::bootstrap::require_no_active_bootstrap(lock)?;
        let recovery = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
        let boot = BootController::open(lock)?;
        boot.authorize_maintenance(&recovery)?;
        let evidence = boot.observe()?;
        if evidence.observation.tryboot {
            return Err(invalid(
                "supervised installation requires a confirmed normal boot",
            ));
        }
        let setup = provision_linux::inspect(&evidence.layout, &evidence.observation.binding)?;
        let device = DeviceIdentity {
            machine: evidence.image.machine.clone(),
            architecture: evidence.image.architecture.clone(),
            layout: evidence.image.layout.clone(),
            boot_identity: evidence.image.boot_identity,
            updater_protocol: evidence.image.updater_protocol,
            recovery_protocol: evidence.observation.recovery_protocol,
            inactive_slot_bytes: evidence
                .layout
                .inactive_partition()
                .sectors
                .checked_mul(512)
                .ok_or_else(|| invalid("inactive capacity overflow"))?,
            persistent_schema: setup.persistent_schema,
            rollback_reads: recovery.record().good().data.reads,
        };
        let trust = TrustStore::open(Path::new(provision_linux::TRUST_PATH))?;
        Ok(Self {
            boot,
            recovery,
            trust,
            evidence,
            setup,
            device,
        })
    }

    fn history(&self, retry: Option<Sha256>) -> Result<ImageHistory> {
        Ok(match retry {
            Some(payload) => self.recovery.record().retry_verification_history(payload)?,
            None => self.recovery.record().image_history(),
        })
    }

    async fn payload(
        &mut self,
        channel: Channel,
        history: &ImageHistory,
        time: &mut impl FnMut() -> Result<UsableTime>,
    ) -> Result<(VerifiedCandidate, StagedPayload)> {
        let candidate = match self
            .trust
            .cached(Some(time()?), channel, &self.device, history)
            .await?
        {
            FeedSelection::Available { candidate } => candidate,
            _ => return Err(Error::CandidateChanged),
        };
        let mut payload = StagedPayload::open(&candidate, &self.evidence.layout)?;
        self.reverify(&candidate, &mut payload, history, time)
            .await?;
        Ok((candidate, payload))
    }

    async fn reverify(
        &mut self,
        candidate: &VerifiedCandidate,
        payload: &mut StagedPayload,
        history: &ImageHistory,
        time: &mut impl FnMut() -> Result<UsableTime>,
    ) -> Result<()> {
        self.trust
            .revalidate_image(
                candidate.id(),
                payload.reader()?,
                Some(time()?),
                candidate.image().channel,
                &self.device,
                history,
            )
            .await?;
        // Hashing can take time. Recheck signatures/expiry and retain the final
        // accepted time before allowing a journal transition or reboot request.
        match self
            .trust
            .cached(
                Some(time()?),
                candidate.image().channel,
                &self.device,
                history,
            )
            .await?
        {
            FeedSelection::Available { candidate: current } if current.id() == candidate.id() => {
                Ok(())
            }
            _ => Err(Error::CandidateChanged),
        }
    }

    fn check(&self, selected: &Selection, candidate: &VerifiedCandidate) -> Result<()> {
        let boot = self.boot.observe()?;
        let setup = provision_linux::inspect(&boot.layout, &boot.observation.binding)?;
        selected.check(&boot, self.recovery.durable_record()?, &setup, candidate)
    }
}

/// Cached metadata and staged bytes only. Updates trusted-time floors but does
/// not write roots, selectors, recovery state or qualification evidence.
pub async fn inspect(
    channel: Channel,
    retry: Option<Sha256>,
    mut time: impl FnMut() -> Result<UsableTime>,
) -> Result<Selection> {
    let lock = InstallLock::acquire()?;
    let mut context = Context::open(&lock)?;
    let history = context.history(retry)?;
    let (candidate, _payload) = context.payload(channel, &history, &mut time).await?;
    let selected = Selection::new(
        &context.boot.observe()?,
        context.recovery.record(),
        &context.setup,
        &candidate,
        retry,
    )?;
    context.check(&selected, &candidate)?;
    Ok(selected)
}

/// One explicit install-and-restart. The same journal consumes generation,
/// retry and reboot allowances used by ordinary recovery. Never resumes a write
/// or repeats an ambiguous reboot; inspect/reconcile the retained transaction.
pub async fn install(
    selected: Selection,
    mut time: impl FnMut() -> Result<UsableTime>,
    report: impl FnMut(WriteProgress),
    reboot: impl FnOnce(&mut TrialPermit<'_, '_>) -> Result<()>,
) -> Result<Sha256> {
    let lock = InstallLock::acquire()?;
    let mut context = Context::open(&lock)?;
    let history = context.history(selected.retry_payload)?;
    let (candidate, mut payload) = context
        .payload(selected.candidate.channel, &history, &mut time)
        .await?;
    context.check(&selected, &candidate)?;
    let boot = context.boot.observe()?;
    let action = context.recovery.advance(|record| {
        record.begin_supervised(
            &candidate,
            &boot.observation,
            selected.recovery,
            selected.retry_payload.is_some(),
        )
    })?;
    let Action::WriteInactive { transaction, .. } = action else {
        return Err(invalid("supervised admission did not authorize its writer"));
    };
    // From here, every failure belongs to this durable transaction. The prior
    // good/default stays intact, and an interrupted process cannot start again.
    let outcome = async {
        let action = write_inactive(
            &context.boot,
            &mut context.recovery,
            &candidate,
            &mut payload,
            report,
        )?;
        context
            .reverify(&candidate, &mut payload, &history, &mut time)
            .await?;
        let current = context.boot.observe()?;
        let setup = provision_linux::inspect(&current.layout, &current.observation.binding)?;
        if setup.bundle != selected.bundle
            || setup.initial_root != selected.initial_root
            || setup.persistent_schema != selected.persistent_schema
        {
            return Err(invalid("provisioned data changed during installation"));
        }
        context.boot.apply(&context.recovery, &action)?;
        let mut permit = TrialPermit::new(
            &context.boot,
            &mut context.recovery,
            &mut context.trust,
            &mut time,
            candidate.expires(),
            transaction,
        );
        reboot(&mut permit)?;
        if !permit.is_queued() {
            return Err(invalid("supervised trial reboot was not acknowledged"));
        }
        Ok(transaction)
    }
    .await;
    if outcome.is_err()
        && context.recovery.record().transaction().is_some_and(|tx| {
            tx.id == transaction
                && !matches!(
                    tx.phase,
                    crate::recovery::Phase::RolledBack | crate::recovery::Phase::Committed
                )
        })
    {
        let boot = context.boot.observe()?;
        context
            .recovery
            .advance(|record| record.fail(FailureReason::InstallationFailed, &boot.observation))?;
    }
    outcome
}

fn invalid(message: &str) -> Error {
    Error::Install(message.into())
}

#[cfg(test)]
#[path = "supervised_install_tests.rs"]
mod tests;
