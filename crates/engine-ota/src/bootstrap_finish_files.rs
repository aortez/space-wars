//! Preserve every retired boot file on /data before deleting any of them.
//! The immutable journal map gives each content-addressed copy its original name.
use super::canonical_files::CanonicalFiles;
use super::finish::FinishState;
use super::{FinishPhase, TrialStore, backup, invalid, state};
use crate::owned_fs::{io_result, name, open_at, owned, unchanged};
use crate::{Blob, Error, Result, verify_blob};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Seek;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

pub(super) const DIRECTORY: &str = "retired-boot";
const PENDING: &str = ".copy.pending";
const RESERVE: u64 = 8 * 1024 * 1024;

pub(super) struct RetiredFiles {
    active: File,
    directory: File,
    owner: u32,
}

impl RetiredFiles {
    pub fn open(active: &File, owner: u32, finish: &FinishState) -> Result<Self> {
        let directory = if finish.phase == FinishPhase::Archiving {
            backup::directory(active, DIRECTORY, owner)?
        } else {
            state::optional_directory(active, DIRECTORY, owner)?
                .ok_or_else(|| invalid("bootstrap retirement archive disappeared"))?
        };
        let result = Self {
            active: active.try_clone()?,
            directory,
            owner,
        };
        result.verify(finish, finish.phase != FinishPhase::Archiving)?;
        Ok(result)
    }

    fn blobs(finish: &FinishState) -> Result<BTreeMap<String, Blob>> {
        let mut blobs = BTreeMap::new();
        for blob in finish.files.values() {
            if blobs
                .insert(blob.sha256.to_string(), blob.clone())
                .is_some_and(|old| old != *blob)
            {
                return Err(invalid("conflicting retired content identity"));
            }
        }
        Ok(blobs)
    }

    pub fn verify(&self, finish: &FinishState, complete: bool) -> Result<()> {
        backup::check_named_directory(&self.active, DIRECTORY, &self.directory, self.owner)?;
        same_mount(&self.directory, &self.active)?;
        let blobs = Self::blobs(finish)?;
        let mut found = BTreeSet::new();
        for entry in fs::read_dir(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))? {
            let component = entry?
                .file_name()
                .into_string()
                .map_err(|_| invalid("invalid retired filename"))?;
            let mut file = open_at(&self.directory, &component, libc::O_RDONLY, 0)?;
            let before = file.metadata()?;
            owned(&before, false, self.owner, true)?;
            same_mount(&file, &self.directory)?;
            if component == PENDING && !complete && finish.phase == FinishPhase::Archiving {
                if before.len() > blobs.values().map(|b| b.bytes).max().unwrap_or(0) {
                    return Err(invalid("oversized retired copy temporary"));
                }
            } else {
                let expected = blobs
                    .get(&component)
                    .ok_or_else(|| invalid("unexpected retired file"))?;
                if before.len() != expected.bytes {
                    return Err(invalid("retired file length differs"));
                }
                verify_blob(&mut file, expected)?;
                if !unchanged(&before, &file.metadata()?) {
                    return Err(invalid("retired file changed"));
                }
                found.insert(component);
            }
        }
        if complete && found != blobs.keys().cloned().collect() {
            return Err(invalid("bootstrap retirement archive is incomplete"));
        }
        Ok(())
    }

    fn archive(
        &self,
        boot: &CanonicalFiles,
        store: &TrialStore,
        guard: &mut impl FnMut(&TrialStore) -> Result<()>,
        checkpoint: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let finish = store.canonical()?.finish()?;
        if finish.phase != FinishPhase::Archiving {
            return Err(invalid("bootstrap archive is not accepting copies"));
        }
        let blobs = Self::blobs(finish)?;
        self.verify(finish, false)?;
        let mut remaining = 0u64;
        for (component, expected) in &blobs {
            if absent(&self.directory, component)? {
                remaining = remaining
                    .checked_add(expected.bytes)
                    .ok_or_else(|| invalid("retired archive capacity overflow"))?;
            }
        }
        let credit = if absent(&self.directory, PENDING)? {
            0
        } else {
            let metadata = open_at(&self.directory, PENDING, libc::O_RDONLY, 0)?.metadata()?;
            metadata.len().min(metadata.blocks().saturating_mul(512))
        };
        if backup::free_space(&self.directory)?.saturating_add(credit)
            < remaining.saturating_add(RESERVE)
        {
            return Err(invalid("insufficient data space for retired boot copies"));
        }
        for (path, expected) in &finish.files {
            guard(store)?;
            let component = expected.sha256.to_string();
            if !absent(&self.directory, &component)? {
                continue;
            }
            let (parent, leaf) = boot_parent(&boot.directory, path, self.owner)?;
            let mut source = open_at(&parent, leaf, libc::O_RDONLY, 0)?;
            let before = source.metadata()?;
            owned(&before, false, self.owner, false)?;
            same_mount(&source, &boot.directory)?;
            if before.len() != expected.bytes {
                return Err(invalid("retired source length changed"));
            }
            let mut pending = backup::writable(&self.directory, PENDING, self.owner)?;
            pending.set_len(0)?;
            pending.rewind()?;
            checkpoint("retirement-copy-opened")?;
            if super::package::transfer(&mut source, &mut pending, expected.bytes)? != *expected
                || !unchanged(&before, &source.metadata()?)
            {
                return Err(invalid("retired source changed while archiving"));
            }
            checkpoint("retirement-copy-written")?;
            pending.sync_all()?;
            checkpoint("retirement-copy-synced")?;
            guard(store)?;
            let named = open_at(&self.directory, PENDING, libc::O_RDONLY, 0)?;
            if !unchanged(&pending.metadata()?, &named.metadata()?) {
                return Err(invalid("retired temporary changed before publication"));
            }
            rename(&self.directory, PENDING, &self.directory, &component)?;
            checkpoint("retirement-copy-renamed")?;
            self.directory.sync_all()?;
            flush(&self.directory)?;
            checkpoint("retirement-copy-directory-synced")?;
        }
        self.verify(finish, true)?;
        for component in blobs.keys() {
            open_at(&self.directory, component, libc::O_RDONLY, 0)?.sync_all()?;
        }
        self.directory.sync_all()?;
        self.active.sync_all()?;
        flush(&self.directory)?;
        guard(store)?;
        Ok(())
    }
}

pub(super) fn retire(
    boot: &CanonicalFiles,
    store: &mut TrialStore,
    owner: u32,
    mut guard: impl FnMut(&TrialStore) -> Result<()>,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    guard(store)?;
    let archive = RetiredFiles::open(
        store.active_directory()?,
        owner,
        store.canonical()?.finish()?,
    )?;
    if store.canonical()?.finish()?.phase == FinishPhase::Archiving {
        archive.archive(boot, store, &mut guard, &mut checkpoint)?;
        store.advance_finish(FinishState::archived)?;
        checkpoint("retirement-archive-completed")?;
    }
    if store.canonical()?.finish()?.phase != FinishPhase::Removing {
        archive.verify(store.canonical()?.finish()?, true)?;
        boot.verify(store.canonical()?)?;
        return Ok(());
    }
    let mut paths: Vec<_> = store.canonical()?.finish()?.files.keys().cloned().collect();
    if let Some(pending) = &store.canonical()?.finish()?.pending {
        paths.sort_by_key(|p| (p != pending, p.clone()));
    }
    for path in paths {
        if store.canonical()?.finish()?.removed.contains(&path) {
            continue;
        }
        guard(store)?;
        archive.verify(store.canonical()?.finish()?, true)?;
        store.advance_finish(|f| f.begin_file(&path))?;
        checkpoint("retirement-remove-intent")?;
        let expected = store.canonical()?.finish()?.files[&path].clone();
        match boot_parent(&boot.directory, &path, owner) {
            Ok((parent, leaf)) => {
                if !absent(&parent, leaf)? {
                    let mut file = open_at(&parent, leaf, libc::O_RDONLY, 0)?;
                    let before = file.metadata()?;
                    owned(&before, false, owner, false)?;
                    same_mount(&file, &boot.directory)?;
                    verify_blob(&mut file, &expected)?;
                    guard(store)?;
                    let (named_parent, _) = boot_parent(&boot.directory, &path, owner)?;
                    if named_parent.metadata()?.dev() != parent.metadata()?.dev()
                        || named_parent.metadata()?.ino() != parent.metadata()?.ino()
                    {
                        return Err(invalid("retired boot directory changed before removal"));
                    }
                    let named = open_at(&parent, leaf, libc::O_RDONLY, 0)?;
                    if !unchanged(&before, &named.metadata()?)
                        || !unchanged(&before, &file.metadata()?)
                    {
                        return Err(invalid("retired boot file changed before removal"));
                    }
                    unlink(&parent, leaf, false)?;
                    checkpoint("retirement-file-unlinked")?;
                }
                parent.sync_all()?;
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        boot.directory.sync_all()?;
        flush(&boot.directory)?;
        checkpoint("retirement-removal-synced")?;
        guard(store)?;
        store.advance_finish(|f| f.removed(&path))?;
        checkpoint("retirement-file-completed")?;
    }
    let canonical = store.canonical()?;
    let keep = directories(canonical.plan.files.keys());
    let mut remove: Vec<_> = directories(canonical.finish()?.files.keys())
        .difference(&keep)
        .cloned()
        .collect();
    remove.sort_by_key(|p| (std::cmp::Reverse(p.matches('/').count()), p.clone()));
    for path in remove {
        guard(store)?;
        match boot_parent(&boot.directory, &path, owner) {
            Ok((parent, leaf)) => {
                if !absent(&parent, leaf)? {
                    let child = open_at(&parent, leaf, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                    owned(&child.metadata()?, true, owner, false)?;
                    same_mount(&child, &boot.directory)?;
                    guard(store)?;
                    let (named_parent, _) = boot_parent(&boot.directory, &path, owner)?;
                    let named =
                        open_at(&named_parent, leaf, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                    if !unchanged(&child.metadata()?, &named.metadata()?)
                        || named_parent.metadata()?.ino() != parent.metadata()?.ino()
                    {
                        return Err(invalid("retired directory changed before removal"));
                    }
                    unlink(&parent, leaf, true)?;
                    checkpoint("retirement-directory-removed")?;
                }
                parent.sync_all()?;
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        boot.directory.sync_all()?;
        flush(&boot.directory)?;
        checkpoint("retirement-directory-synced")?;
    }
    archive.verify(store.canonical()?.finish()?, true)?;
    crate::boot_files::BootFiles::open(
        boot.directory.try_clone()?,
        owner,
        boot.inputs.image.boot_identity,
    )?;
    guard(store)?;
    store.advance_finish(FinishState::ready)?;
    checkpoint("retirement-ready")?;
    Ok(())
}

fn directories<'a>(paths: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    paths
        .flat_map(|p| p.match_indices('/').map(|(i, _)| p[..i].to_string()))
        .collect()
}

fn boot_parent<'a>(root: &File, path: &'a str, owner: u32) -> Result<(File, &'a str)> {
    let mut parts = path.split('/').peekable();
    let mut directory = root.try_clone()?;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return Ok((directory, part));
        }
        directory = open_at(&directory, part, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&directory.metadata()?, true, owner, false)?;
        same_mount(&directory, root)?;
    }
    Err(invalid("empty retired path"))
}

pub(super) fn same_mount(file: &File, root: &File) -> Result<()> {
    if crate::recovery_linux::opened_mount_id(file)?
        != crate::recovery_linux::opened_mount_id(root)?
    {
        return Err(invalid("bootstrap completion crossed mounts"));
    }
    Ok(())
}

pub(super) fn absent(parent: &File, component: &str) -> Result<bool> {
    match open_at(parent, component, libc::O_PATH, 0) {
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
        Ok(_) => Ok(false),
    }
}

pub(super) fn rename(from: &File, source: &str, to: &File, destination: &str) -> Result<()> {
    let source = name(source)?;
    let destination = name(destination)?;
    io_result(unsafe {
        libc::renameat2(
            from.as_raw_fd(),
            source.as_ptr(),
            to.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })
}

fn unlink(parent: &File, component: &str, directory: bool) -> Result<()> {
    let component = name(component)?;
    io_result(unsafe {
        libc::unlinkat(
            parent.as_raw_fd(),
            component.as_ptr(),
            if directory { libc::AT_REMOVEDIR } else { 0 },
        )
    })
}

pub(super) fn flush(directory: &File) -> Result<()> {
    io_result(unsafe { libc::syncfs(directory.as_raw_fd()) })
}

#[cfg(test)]
#[path = "bootstrap_finish_files_tests.rs"]
pub(super) mod tests;
