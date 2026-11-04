//! Publish one exact initial recovery journal, then archive the bootstrap attempt.
//! A crash while both journals exist still leaves the active reservation blocking
//! installation; it never grants permission to recreate missing production state.
use super::finish::FinishState;
use super::finish_files::{absent, flush, rename, same_mount};
use super::{FinishPhase, TrialStore, backup, invalid, state};
use crate::owned_fs::{open_at, owned, read_file, unchanged};
use crate::recovery::RecoveryRecord;
use crate::{Result, recovery_store};
use std::fs::File;
use std::io::{Seek, Write};

const PENDING: &str = ".bootstrap-recovery";
const PENDING_STATE: &str = ".state.pending";
const RECOVERY: &str = "recovery";

pub(super) fn check_coexistence(
    system: &File,
    owner: u32,
    finish: Option<&FinishState>,
    completed: bool,
) -> Result<()> {
    let pending = !absent(system, PENDING)?;
    let published = !absent(system, RECOVERY)?;
    if !pending
        && !published
        && !completed
        && finish.is_none_or(|f| f.phase != FinishPhase::Complete)
    {
        return Ok(());
    }
    let finish = finish
        .filter(|f| f.phase >= FinishPhase::Handoff)
        .ok_or_else(|| invalid("bootstrap conflicts with production recovery state"))?;
    if (pending && published)
        || (completed && finish.phase != FinishPhase::Complete)
        || (finish.phase == FinishPhase::Complete && !published)
    {
        return Err(invalid(
            "bootstrap handoff directories disagree with durable intent",
        ));
    }
    let baseline = finish
        .baseline
        .as_ref()
        .ok_or_else(|| invalid("missing initial recovery plan"))?;
    if pending {
        let directory = state::optional_directory(system, PENDING, owner)?
            .ok_or_else(|| invalid("missing recovery staging directory"))?;
        same_mount(&directory, system)?;
        verify(&directory, owner, baseline, false)?;
    }
    if published {
        verify_published(system, owner, baseline)?;
    }
    Ok(())
}

fn verify(directory: &File, owner: u32, baseline: &RecoveryRecord, complete: bool) -> Result<()> {
    backup::inventory(directory, &["state.json", "lock", PENDING_STATE])?;
    let expected = recovery_store::initial_bytes(baseline)?;
    for component in ["state.json", "lock", PENDING_STATE] {
        if absent(directory, component)? {
            if complete && component != PENDING_STATE {
                return Err(invalid("published initial recovery journal is incomplete"));
            }
            continue;
        }
        if complete && component == PENDING_STATE {
            return Err(invalid(
                "published recovery journal retained incomplete state",
            ));
        }
        let file = open_at(directory, component, libc::O_RDONLY, 0)?;
        owned(&file.metadata()?, false, owner, true)?;
        same_mount(&file, directory)?;
        let bytes = read_file(file, owner, recovery_store::MAX_BYTES)?;
        if (component == "state.json" && bytes != expected)
            || (component == "lock" && !bytes.is_empty())
        {
            return Err(invalid(
                "initial recovery journal differs from its handoff plan",
            ));
        }
    }
    if !absent(directory, "state.json")? && !absent(directory, PENDING_STATE)? {
        return Err(invalid(
            "published and pending initial recovery records conflict",
        ));
    }
    Ok(())
}

pub(super) fn verify_published(system: &File, owner: u32, baseline: &RecoveryRecord) -> Result<()> {
    let directory = state::optional_directory(system, RECOVERY, owner)?
        .ok_or_else(|| invalid("production recovery journal disappeared"))?;
    same_mount(&directory, system)?;
    verify(&directory, owner, baseline, true)
}

pub(super) fn publish(
    store: &mut TrialStore,
    owner: u32,
    mut guard: impl FnMut(&TrialStore) -> Result<()>,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    guard(store)?;
    let finish = store.canonical()?.finish()?;
    if finish.phase < FinishPhase::Handoff {
        return Err(invalid(
            "initial recovery publication lacks durable handoff intent",
        ));
    }
    let baseline = finish
        .baseline
        .clone()
        .ok_or_else(|| invalid("missing initial recovery plan"))?;
    let data = store.data_directory()?.try_clone()?;
    let system = backup::directory(&data, "spacewars-system", owner)?;
    same_mount(&system, &data)?;
    check_coexistence(&system, owner, Some(finish), store.is_completed())?;
    if absent(&system, RECOVERY)? {
        let directory = backup::directory(&system, PENDING, owner)?;
        verify(&directory, owner, &baseline, false)?;
        let lock = backup::writable(&directory, "lock", owner)?;
        if lock.metadata()?.len() != 0 {
            return Err(invalid("nonempty initial recovery lock"));
        }
        lock.sync_all()?;
        checkpoint("handoff-directory-ready")?;
        if absent(&directory, "state.json")? {
            let bytes = recovery_store::initial_bytes(&baseline)?;
            let mut pending = backup::writable(&directory, PENDING_STATE, owner)?;
            pending.set_len(0)?;
            pending.rewind()?;
            pending.write_all(&bytes)?;
            checkpoint("handoff-record-written")?;
            pending.sync_all()?;
            checkpoint("handoff-record-synced")?;
            guard(store)?;
            let named = open_at(&directory, PENDING_STATE, libc::O_RDONLY, 0)?;
            if !unchanged(&pending.metadata()?, &named.metadata()?) {
                return Err(invalid("initial recovery temporary changed"));
            }
            rename(&directory, PENDING_STATE, &directory, "state.json")?;
            checkpoint("handoff-record-renamed")?;
        }
        verify(&directory, owner, &baseline, true)?;
        open_at(&directory, "state.json", libc::O_RDONLY, 0)?.sync_all()?;
        directory.sync_all()?;
        system.sync_all()?;
        flush(&data)?;
        checkpoint("handoff-directory-synced")?;
        guard(store)?;
        backup::check_named_directory(&data, "spacewars-system", &system, owner)?;
        backup::check_named_directory(&system, PENDING, &directory, owner)?;
        rename(&system, PENDING, &system, RECOVERY)?;
        checkpoint("handoff-recovery-published")?;
    }
    verify_published(&system, owner, &baseline)?;
    system.sync_all()?;
    data.sync_all()?;
    flush(&data)?;
    checkpoint("handoff-recovery-synced")?;
    guard(store)?;
    if store.finish_phase() == Some(FinishPhase::Handoff) {
        store.advance_finish(FinishState::complete)?;
        checkpoint("handoff-complete-recorded")?;
    }
    guard(store)?;
    store.archive_completed(checkpoint)
}

#[cfg(test)]
#[path = "bootstrap_handoff_tests.rs"]
mod tests;
