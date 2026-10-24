//! Durable handoff-to-recovery association. No state here authorizes a write,
//! resumes an installation, or supplies boot/watchdog qualification.
//!
//! The native worker retains InstallLock across prepare, begin, the writer and
//! the acknowledged reboot handoff. Restart reconciliation takes the same lock, so a
//! surviving worker cannot race an abort decision. The startup worker reconciles
//! this journal and renews its durable deployment hold until the host is released.
use super::{HOST_FRESH_MS, HostObservation, ManualAuthorization};
use crate::schedule_store::{StateRecord, Store};
use crate::{Error, Result};
use engine_ota::install_lock::InstallLock;
use engine_ota::recovery::{Action, Phase, Plan, Transaction};
use engine_ota::{RecoveryStore, Sha256};
use serde::{Deserialize, Serialize};
use spacewars_control::install::{Aborted, Message, Prepare, SCHEMA, Ticket};
use std::path::Path;

const DIRECTORY: &str = "/data/spacewars-system/install-handoff";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Committed,
    RolledBack,
}

/// Only a fresh reconciliation under the install lock can produce these
/// decisions. InProgress is not success, and an unknown association is an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    Clear,
    Aborted(Aborted),
    InProgress {
        ticket: Ticket,
        transaction: Sha256,
        phase: Phase,
    },
    Finished {
        ticket: Ticket,
        transaction: Sha256,
        outcome: Outcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case", deny_unknown_fields)]
enum State {
    Preparing,
    Armed {
        transaction: Sha256,
    },
    Aborted,
    Finished {
        transaction: Sha256,
        outcome: Outcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    prepare: Prepare,
    boot_id: String,
    selection_generation: u64,
    recovery_before: Sha256,
    state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deployment: Option<crate::network_policy::HoldToken>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    handoff: Option<Handoff>,
}

impl StateRecord for Record {
    fn initial() -> Self {
        Self {
            schema: 1,
            generation: 0,
            handoff: None,
        }
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1 || (self.generation == 0 && self.handoff.is_some()) {
            return Err(invalid("invalid handoff record"));
        }
        if let Some(handoff) = &self.handoff {
            handoff
                .prepare
                .validate()
                .map_err(|_| invalid("invalid saved preparation"))?;
            if !boot_id(&handoff.boot_id) {
                return Err(invalid("invalid handoff boot identity"));
            }
        }
        Ok(())
    }
}

// Not decoded from requests: sampled from an exclusively owned, non-poisoned
// RecoveryStore while the caller holds the cross-tool installation lock.
struct RecoveryView {
    fingerprint: Sha256,
    transaction: Option<Transaction>,
}
impl RecoveryView {
    fn read(recovery: &RecoveryStore) -> Result<Self> {
        let record = recovery.durable_record()?;
        Ok(Self {
            fingerprint: record.fingerprint(),
            transaction: record.transaction().cloned(),
        })
    }
    fn active(&self) -> bool {
        self.transaction
            .as_ref()
            .is_some_and(|tx| !matches!(tx.phase, Phase::Committed | Phase::RolledBack))
    }
}

pub struct Journal {
    store: Store<Record>,
    record: Record,
}

impl Journal {
    /// Production callers cannot select a different journal path. Missing state
    /// inside an existing directory is corruption, never a fresh installation.
    pub fn open_native() -> Result<Self> {
        Self::check_parent()?;
        Self::open(Path::new(DIRECTORY))
    }

    pub fn existing_native() -> Result<Option<Self>> {
        Self::check_parent()?;
        Ok(Store::existing(Path::new(DIRECTORY))?.map(|(store, record)| Self { store, record }))
    }

    fn check_parent() -> Result<()> {
        use std::os::unix::fs::MetadataExt;
        let parent = std::fs::symlink_metadata("/data/spacewars-system")?;
        if unsafe { libc::geteuid() } != 0
            || !parent.is_dir()
            || parent.uid() != 0
            || parent.mode() & 0o077 != 0
        {
            return Err(invalid("unsafe handoff parent directory"));
        }
        Ok(())
    }

    fn open(path: &Path) -> Result<Self> {
        let (store, record) = Store::open(path)?;
        Ok(Self { store, record })
    }

    /// Persist before sending Prepare to the host. The record binds the exact
    /// host/settings revisions and the recovery baseline preceding any write.
    pub fn prepare(
        &mut self,
        lock: &InstallLock,
        prepare: &Prepare,
        boot_id: &str,
        selection_generation: u64,
        recovery: &RecoveryStore,
    ) -> Result<()> {
        lock.check()?;
        self.prepare_inner(
            prepare,
            boot_id,
            selection_generation,
            &RecoveryView::read(recovery)?,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_with_hold(
        &mut self,
        lock: &InstallLock,
        prepare: &Prepare,
        boot_id: &str,
        generation: u64,
        recovery: &RecoveryStore,
        token: crate::network_policy::HoldToken,
    ) -> Result<()> {
        lock.check()?;
        self.prepare_inner(
            prepare,
            boot_id,
            generation,
            &RecoveryView::read(recovery)?,
            Some(token),
        )
    }

    /// Read-only maintenance renewal also runs while independent recovery holds
    /// InstallLock. It never releases a hold or changes the handoff decision.
    pub(crate) fn deployment(&self) -> Result<Option<crate::network_policy::HoldToken>> {
        self.store.check()?;
        Ok(self
            .record
            .handoff
            .as_ref()
            .and_then(|h| h.deployment.clone()))
    }

    fn prepare_inner(
        &mut self,
        prepare: &Prepare,
        boot_id: &str,
        selection_generation: u64,
        recovery: &RecoveryView,
        deployment: Option<crate::network_policy::HoldToken>,
    ) -> Result<()> {
        self.store.check()?;
        if self.record.handoff.is_some() || recovery.active() {
            return Err(invalid("reconcile and release the previous handoff first"));
        }
        self.save(Some(Handoff {
            prepare: prepare.clone(),
            boot_id: boot_id.into(),
            selection_generation,
            recovery_before: recovery.fingerprint,
            state: State::Preparing,
            deployment,
        }))
    }

    /// Consume the local one-use consent and persist the exact planned recovery
    /// transaction BEFORE advancing the recovery journal. A crash in between can
    /// then prove no write began by comparing the unchanged baseline. Only the
    /// subsequent successful RecoveryStore::advance returns the write action.
    pub fn begin(
        &mut self,
        lock: &InstallLock,
        recovery: &mut RecoveryStore,
        authorization: ManualAuthorization,
        plan: Plan,
    ) -> Result<Action> {
        lock.check()?;
        self.begin_inner(recovery, authorization, plan)
    }

    fn begin_inner(
        &mut self,
        recovery: &mut RecoveryStore,
        authorization: ManualAuthorization,
        plan: Plan,
    ) -> Result<Action> {
        let (before, transaction) = plan
            .installation()
            .ok_or_else(|| invalid("not an installation plan"))?;
        if recovery.durable_record()?.fingerprint() != before {
            return Err(invalid("recovery changed before installation admission"));
        }
        self.arm_inner(authorization, before, transaction)?;
        Ok(recovery.advance(|_| Ok(plan))?)
    }

    fn arm_inner(
        &mut self,
        authorization: ManualAuthorization,
        before: Sha256,
        transaction: &Transaction,
    ) -> Result<()> {
        self.store.check()?;
        let mut handoff = self
            .record
            .handoff
            .clone()
            .ok_or_else(|| invalid("no durable preparation"))?;
        if handoff.state != State::Preparing
            || authorization.ticket() != &handoff.prepare.ticket
            || authorization.selection_generation() != handoff.selection_generation
            || before != handoff.recovery_before
            || transaction.candidate_id.to_string() != handoff.prepare.ticket.candidate.as_str()
            || transaction.origin_boot_id != handoff.boot_id
            || transaction.phase != Phase::Writing
        {
            return Err(invalid("installation does not match durable preparation"));
        }
        handoff.state = State::Armed {
            transaction: transaction.id,
        };
        self.save(Some(handoff))
    }

    /// Reopen both journals after any uncertain persistence result. Never call
    /// this in parallel with a live worker: the shared install lock is required.
    /// The existing recovery worker, not this journal, finishes or fails a tx.
    pub fn reconcile(
        &mut self,
        lock: &InstallLock,
        recovery: &RecoveryStore,
    ) -> Result<Resolution> {
        lock.check()?;
        self.reconcile_inner(&RecoveryView::read(recovery)?)
    }

    fn reconcile_inner(&mut self, recovery: &RecoveryView) -> Result<Resolution> {
        self.store.check()?;
        let Some(mut handoff) = self.record.handoff.clone() else {
            return if recovery.active() {
                Err(invalid(
                    "active recovery transaction has no handoff association",
                ))
            } else {
                Ok(Resolution::Clear)
            };
        };
        let ticket = handoff.prepare.ticket.clone();
        match handoff.state {
            State::Preparing | State::Armed { .. }
                if recovery.fingerprint == handoff.recovery_before =>
            {
                handoff.state = State::Aborted;
                self.save(Some(handoff))?;
                Ok(Resolution::Aborted(Aborted {
                    schema: SCHEMA,
                    ticket,
                }))
            }
            State::Aborted if recovery.fingerprint == handoff.recovery_before => {
                Ok(Resolution::Aborted(Aborted {
                    schema: SCHEMA,
                    ticket,
                }))
            }
            State::Armed { transaction } | State::Finished { transaction, .. } => {
                let tx = recovery
                    .transaction
                    .as_ref()
                    .filter(|tx| {
                        tx.id == transaction
                            && tx.candidate_id.to_string() == ticket.candidate.as_str()
                            && tx.origin_boot_id == handoff.boot_id
                    })
                    .ok_or_else(|| {
                        invalid("recovery transaction does not match durable handoff")
                    })?;
                let outcome = match tx.phase {
                    Phase::Committed => Some(Outcome::Committed),
                    Phase::RolledBack => Some(Outcome::RolledBack),
                    _ => None,
                };
                match (&handoff.state, outcome) {
                    (State::Finished { outcome: saved, .. }, Some(actual)) if *saved == actual => {}
                    (State::Finished { .. }, _) => {
                        return Err(invalid("finished recovery outcome changed"));
                    }
                    (_, Some(outcome)) => {
                        handoff.state = State::Finished {
                            transaction,
                            outcome,
                        };
                        self.save(Some(handoff))?;
                    }
                    (_, None) => {}
                }
                Ok(match outcome {
                    Some(outcome) => Resolution::Finished {
                        ticket,
                        transaction,
                        outcome,
                    },
                    None => Resolution::InProgress {
                        ticket,
                        transaction,
                        phase: tx.phase,
                    },
                })
            }
            _ => Err(invalid(
                "recovery changed without a matching installation intent",
            )),
        }
    }

    /// Retain a terminal decision until an authenticated observation made AFTER
    /// that decision proves the fence is gone. The callback must query the active
    /// systemd kiosk instance, never return cached evidence or request JSON.
    pub fn release(
        &mut self,
        lock: &InstallLock,
        recovery: &RecoveryStore,
        ticket: &Ticket,
        observe: impl FnOnce() -> Result<HostObservation>,
    ) -> Result<()> {
        lock.check()?;
        self.check_ticket(ticket)?;
        let recovery = RecoveryView::read(recovery)?;
        match self.reconcile_inner(&recovery)? {
            Resolution::Aborted(_) | Resolution::Finished { .. } => {}
            _ => return Err(invalid("handoff is not terminal")),
        }
        let after_ms = spacewars_control::install::transport::boot_ms()?;
        let host = observe()?;
        let now_ms = spacewars_control::install::transport::boot_ms()?;
        lock.check()?;
        self.release_inner(&recovery, ticket, &host, after_ms, now_ms)
    }

    fn release_inner(
        &mut self,
        recovery: &RecoveryView,
        ticket: &Ticket,
        host: &HostObservation,
        after_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        self.check_ticket(ticket)?;
        host.instance
            .validate()
            .map_err(|_| invalid("invalid release host"))?;
        if host.fenced
            || host.observed_ms < after_ms
            || now_ms
                .checked_sub(host.observed_ms)
                .is_none_or(|age| age > HOST_FRESH_MS)
        {
            return Err(invalid("host has not released the handoff fence"));
        }
        match self.reconcile_inner(recovery)? {
            Resolution::Aborted(aborted) if &aborted.ticket == ticket => {}
            Resolution::Finished { ticket: saved, .. } if &saved == ticket => {}
            _ => return Err(invalid("handoff is not terminal or ticket is stale")),
        }
        self.save(None)
    }

    fn check_ticket(&self, ticket: &Ticket) -> Result<()> {
        self.store.check()?;
        if self
            .record
            .handoff
            .as_ref()
            .map(|handoff| &handoff.prepare.ticket)
            != Some(ticket)
        {
            return Err(invalid("handoff ticket is stale"));
        }
        Ok(())
    }

    fn save(&mut self, handoff: Option<Handoff>) -> Result<()> {
        let record = Record {
            schema: 1,
            generation: self
                .record
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("handoff generation exhausted"))?,
            handoff,
        };
        self.store.save(&record)?;
        self.record = record;
        Ok(())
    }
}

fn boot_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, byte)| {
            if [8, 13, 18, 23].contains(&i) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "install_journal_tests.rs"]
mod tests;
