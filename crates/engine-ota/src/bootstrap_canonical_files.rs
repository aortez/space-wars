//! One flushed canonical replacement at a time, authorized by durable intent.
//! The complete staged tree and normal config survive every preparation step.
use super::canonical::{CanonicalState, blob};
use super::trial::TrialInputs;
use super::{CanonicalDefault, CanonicalPhase, TrialStore, invalid, stage_files, trial_files};
use crate::owned_fs::{io_result, name, open_at, owned, read_file, unchanged};
use crate::{Blob, Error, Result};
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

const PENDING: &str = ".spacewars-canonical.pending";
const RESERVE: u64 = 4 * 1024 * 1024 + 64 * 1024;

pub(super) struct CanonicalFiles {
    pub directory: File,
    pub inputs: TrialInputs,
    owner: u32,
}

impl CanonicalFiles {
    pub fn open(
        directory: File,
        owner: u32,
        inputs: TrialInputs,
        state: &CanonicalState,
    ) -> Result<Self> {
        owned(&directory.metadata()?, true, owner, false)?;
        let files = Self {
            directory,
            inputs,
            owner,
        };
        files.verify(state)?;
        Ok(files)
    }

    pub fn selected(&self, state: &CanonicalState) -> Result<CanonicalDefault> {
        let config = read_file(
            open_at(&self.directory, "config.txt", libc::O_RDONLY, 0)?,
            self.owner,
            16 * 1024,
        )?;
        if config == self.inputs.plan.normal_config.as_bytes() {
            return Ok(CanonicalDefault::Staged);
        }
        if config == state.plan.config.as_bytes() {
            return Ok(CanonicalDefault::Canonical);
        }
        Err(invalid(
            "canonical migration default is neither staged nor canonical",
        ))
    }

    pub fn verify(&self, state: &CanonicalState) -> Result<()> {
        state.validate(&self.inputs)?;
        let mut expected = trial_files::expected(&self.inputs, true);
        let mut actual = stage_files::inventory(&self.directory, self.owner)?;
        let mut bounds = expected.clone();
        for (path, value) in &state.plan.files {
            bounds
                .entry(path.clone())
                .and_modify(|b| b.bytes = b.bytes.max(value.bytes))
                .or_insert_with(|| value.clone());
        }
        let selected = self.selected(state)?;
        if selected == CanonicalDefault::Canonical {
            if !matches!(
                state.phase,
                CanonicalPhase::Selecting
                    | CanonicalPhase::Selected
                    | CanonicalPhase::Observing
                    | CanonicalPhase::Confirmed
                    | CanonicalPhase::Failed
            ) || state.pending.is_some()
                || state.completed.len() + 1 != state.plan.files.len()
            {
                return Err(invalid(
                    "canonical default appeared before its complete copy intent",
                ));
            }
            expected.insert("config.txt".into(), blob(state.plan.config.as_bytes()));
        } else if matches!(
            state.phase,
            CanonicalPhase::Selected | CanonicalPhase::Observing | CanonicalPhase::Confirmed
        ) {
            return Err(invalid("selected canonical default changed"));
        }
        for path in &state.completed {
            expected.insert(path.clone(), state.plan.files[path].clone());
        }
        if let Some(path) = &state.pending
            && actual.get(path) == state.plan.files.get(path)
        {
            expected.insert(path.clone(), state.plan.files[path].clone());
        }
        if let Some(pending) = actual.remove(PENDING) {
            let limit = self.pending_limit(state)?;
            if pending.bytes > limit {
                return Err(invalid("oversized canonical temporary"));
            }
            bounds.insert(
                PENDING.into(),
                Blob {
                    bytes: limit,
                    sha256: pending.sha256,
                },
            );
        }
        let temporaries: Vec<_> = actual
            .keys()
            .filter(|p| trial_files::temporary(p))
            .cloned()
            .collect();
        if temporaries.len() > 16 {
            return Err(invalid("too many retained selector temporaries"));
        }
        for path in temporaries {
            let contents = actual.remove(&path).unwrap();
            if contents.bytes > 16 * 1024 {
                return Err(invalid("oversized retained selector temporary"));
            }
            bounds.insert(path, contents);
        }
        if actual != expected {
            return Err(invalid(
                "canonical, staged or retained boot files changed outside their recorded copy",
            ));
        }
        // Inventory also rejects unexpected empty directories and nested mounts.
        stage_files::verify_tree(&self.directory, &self.directory, self.owner, &bounds, false)
    }

    fn pending_limit(&self, state: &CanonicalState) -> Result<u64> {
        if let Some(path) = &state.pending {
            return Ok(state.plan.files[path].bytes);
        }
        if matches!(
            state.phase,
            CanonicalPhase::Selecting | CanonicalPhase::Failed
        ) {
            return Ok(
                (state.plan.config.len() as u64).max(self.inputs.plan.normal_config.len() as u64)
            );
        }
        Err(invalid("canonical temporary has no durable copy intent"))
    }

    fn capacity(&self, state: &CanonicalState) -> Result<()> {
        let actual = stage_files::inventory(&self.directory, self.owner)?;
        let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        io_result(unsafe { libc::fstatvfs(self.directory.as_raw_fd(), status.as_mut_ptr()) })?;
        let status = unsafe { status.assume_init() };
        let cluster = status.f_frsize;
        if !cluster.is_power_of_two() || !(512..=65536).contains(&cluster) {
            return Err(invalid("unsupported canonical allocation size"));
        }
        let allocation = |bytes: u64| bytes.div_ceil(cluster) * cluster;
        let mut growth = 0u64;
        let mut largest = allocation(self.inputs.plan.normal_config.len() as u64);
        for (path, contents) in &state.plan.files {
            if state.completed.contains(path) {
                continue;
            }
            largest = largest.max(allocation(contents.bytes));
            growth = growth
                .checked_add(
                    allocation(contents.bytes)
                        .saturating_sub(actual.get(path).map(|b| allocation(b.bytes)).unwrap_or(0)),
                )
                .ok_or_else(|| invalid("canonical capacity overflow"))?;
        }
        let free = status
            .f_bavail
            .checked_mul(cluster)
            .ok_or_else(|| invalid("canonical free-space overflow"))?;
        // Only a bounded, journal-authorized pending file can supply this credit.
        let credit = actual
            .get(PENDING)
            .map(|b| allocation(b.bytes))
            .unwrap_or(0);
        if free.saturating_add(credit) < growth.saturating_add(largest).saturating_add(RESERVE) {
            return Err(invalid(
                "insufficient canonical migration space and reserve",
            ));
        }
        Ok(())
    }

    pub fn prepare(
        &self,
        store: &mut TrialStore,
        mut source: impl FnMut(&str) -> Result<File>,
        mut guard: impl FnMut(&TrialStore) -> Result<()>,
        mut checkpoint: impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        self.verify(store.canonical()?)?;
        self.capacity(store.canonical()?)?;
        guard(store)?;
        if !matches!(
            store.canonical()?.phase,
            CanonicalPhase::Preparing | CanonicalPhase::Prepared
        ) {
            return Err(invalid(
                "canonical preparation has already finished or failed",
            ));
        }
        let mut paths: Vec<_> = store
            .canonical()?
            .plan
            .files
            .keys()
            .filter(|p| p.as_str() != "config.txt")
            .cloned()
            .collect();
        if let Some(pending) = &store.canonical()?.pending {
            paths.sort_by_key(|path| (path != pending, path.clone()));
        }
        for path in paths {
            if store.canonical()?.completed.contains(&path) {
                continue;
            }
            guard(store)?;
            store.advance_canonical(|c, _| c.begin_file(&path))?;
            checkpoint("canonical-file-intent")?;
            let expected = store.canonical()?.plan.files[&path].clone();
            let mut reader = source(&path)?;
            reader.rewind()?;
            let before = reader.metadata()?;
            if !before.is_file() || before.len() != expected.bytes {
                return Err(invalid("canonical source size or type differs"));
            }
            self.replace(
                &path,
                &expected,
                &mut reader,
                store,
                &mut guard,
                &mut checkpoint,
            )?;
            if !unchanged(&before, &reader.metadata()?) {
                return Err(invalid("canonical source changed during copy"));
            }
            store.advance_canonical(|c, _| c.finish_file(&path))?;
            checkpoint("canonical-file-completed")?;
        }
        self.verify(store.canonical()?)?;
        guard(store)?;
        store.advance_canonical(|c, _| c.prepared())?;
        checkpoint("canonical-prepared")?;
        Ok(())
    }

    pub fn select(
        &self,
        canonical: bool,
        store: &TrialStore,
        mut guard: impl FnMut(&TrialStore) -> Result<()>,
        mut checkpoint: impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let state = store.canonical()?;
        if if canonical {
            state.phase != CanonicalPhase::Selecting
        } else {
            state.phase != CanonicalPhase::Failed
        } {
            return Err(invalid("canonical selection lacks durable authority"));
        }
        self.verify(state)?;
        let bytes = if canonical {
            state.plan.config.as_bytes()
        } else {
            self.inputs.plan.normal_config.as_bytes()
        };
        self.replace(
            "config.txt",
            &blob(bytes),
            &mut Cursor::new(bytes),
            store,
            &mut guard,
            &mut checkpoint,
        )?;
        if self.selected(store.canonical()?)?
            != if canonical {
                CanonicalDefault::Canonical
            } else {
                CanonicalDefault::Staged
            }
        {
            return Err(invalid("canonical selector readback differs"));
        }
        self.verify(store.canonical()?)
    }

    fn replace(
        &self,
        path: &str,
        expected: &Blob,
        reader: &mut impl Read,
        store: &TrialStore,
        guard: &mut impl FnMut(&TrialStore) -> Result<()>,
        checkpoint: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        self.verify(store.canonical()?)?;
        self.capacity(store.canonical()?)?;
        let (parent, component) =
            stage_files::parent(&self.directory, &self.directory, path, self.owner)?;
        checkpoint("canonical-parent-ready")?;
        let current = contents(&parent, component, &self.directory, self.owner)?;
        if current.as_ref() == Some(expected) {
            if stage_files::inventory(&self.directory, self.owner)?.contains_key(PENDING) {
                return Err(invalid(
                    "completed canonical destination conflicts with a pending copy",
                ));
            }
            // A rename may have reached disk before its completion record. Flush
            // again before accepting that complete file; never repair changed bytes.
            crate::verify_blob(reader, expected)?;
            parent.sync_all()?;
            self.flush()?;
            guard(store)?;
            return Ok(());
        }
        let mut pending = stage_files::writable(
            &self.directory,
            &self.directory,
            PENDING,
            self.owner,
            self.pending_limit(store.canonical()?)?,
        )?;
        pending.set_len(0)?;
        checkpoint("canonical-file-opened")?;
        if super::package::transfer(reader, &mut pending, expected.bytes)? != *expected {
            return Err(invalid("canonical source differs from the selected image"));
        }
        checkpoint("canonical-file-written")?;
        pending.sync_all()?;
        checkpoint("canonical-file-synced")?;
        guard(store)?;
        self.verify(store.canonical()?)?;
        let (named_parent, named_component) =
            stage_files::parent(&self.directory, &self.directory, path, self.owner)?;
        if named_parent.metadata()?.ino() != parent.metadata()?.ino()
            || named_parent.metadata()?.dev() != parent.metadata()?.dev()
            || contents(&parent, named_component, &self.directory, self.owner)? != current
        {
            return Err(invalid("canonical destination changed before replacement"));
        }
        let named_pending = open_at(&self.directory, PENDING, libc::O_RDONLY, 0)?;
        if !unchanged(&pending.metadata()?, &named_pending.metadata()?) {
            return Err(invalid("canonical temporary was replaced"));
        }
        checkpoint("canonical-before-rename")?;
        let from = name(PENDING)?;
        let to = name(component)?;
        io_result(unsafe {
            libc::renameat2(
                self.directory.as_raw_fd(),
                from.as_ptr(),
                parent.as_raw_fd(),
                to.as_ptr(),
                if current.is_none() {
                    libc::RENAME_NOREPLACE
                } else {
                    0
                },
            )
        })?;
        checkpoint("canonical-renamed")?;
        parent.sync_all()?;
        self.directory.sync_all()?;
        checkpoint("canonical-directory-synced")?;
        self.flush()?;
        checkpoint("canonical-filesystem-synced")?;
        if contents(&parent, component, &self.directory, self.owner)?.as_ref() != Some(expected) {
            return Err(invalid("canonical copy readback differs"));
        }
        guard(store)?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        io_result(unsafe { libc::syncfs(self.directory.as_raw_fd()) })
    }
}

fn contents(parent: &File, component: &str, boot: &File, owner: u32) -> Result<Option<Blob>> {
    let mut file = match open_at(parent, component, libc::O_RDONLY, 0) {
        Ok(file) => file,
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let before = file.metadata()?;
    owned(&before, false, owner, false)?;
    if before.len() > crate::boot::MAX_BOOT_ASSET_BYTES
        || crate::recovery_linux::opened_mount_id(&file)?
            != crate::recovery_linux::opened_mount_id(boot)?
    {
        return Err(invalid("unsafe canonical destination"));
    }
    let blob = super::package::transfer(&mut file, &mut std::io::sink(), before.len())?;
    if !unchanged(&before, &file.metadata()?) {
        return Err(invalid("canonical destination changed during read"));
    }
    Ok(Some(blob))
}

#[cfg(test)]
#[path = "bootstrap_canonical_files_tests.rs"]
pub(super) mod tests;
