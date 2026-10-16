//! Restartable boot preparation under a previously durable staging intent.
//! The retained normal boot is immutable; only the new tree and tryboot are added.
use super::trial::TrialInputs;
use super::{STAGE_DIRECTORY, inspect, invalid};
use crate::owned_fs::{io_result, name, open_at, owned, unchanged};
use crate::{Blob, Error, Result, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

pub(super) const PENDING: &str = ".sw-bootstrap-pending";
const SELECTOR_PENDING: &str = ".sw-bootstrap-tryboot.pending";

pub(super) fn expected(inputs: &TrialInputs) -> BTreeMap<String, Blob> {
    let mut files = inputs.plan.staged_assets.clone();
    files.insert(
        "trial-cmdline.txt".into(),
        blob(inputs.plan.trial_cmdline.as_bytes()),
    );
    files.insert(
        "normal-cmdline.txt".into(),
        blob(inputs.plan.normal_cmdline.as_bytes()),
    );
    files
}

fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}

/// Strip only files authorized by the durable intent. Completed assets are
/// immutable; only bounded incomplete files inside the pending tree may differ.
pub(super) fn retained(
    files: &BTreeMap<String, Blob>,
    inputs: &TrialInputs,
    cluster: u64,
) -> Result<(BTreeMap<String, Blob>, u64)> {
    inputs.validate()?;
    if !cluster.is_power_of_two() || !(512..=65536).contains(&cluster) {
        return Err(invalid("invalid boot allocation size"));
    }
    let expected = expected(inputs);
    let mut original = BTreeMap::new();
    let mut published = BTreeMap::new();
    let mut pending = false;
    let mut credit = 0u64;
    for (path, contents) in files {
        if let Some(relative) = path.strip_prefix(&format!("{STAGE_DIRECTORY}/")) {
            if expected.get(relative) != Some(contents) {
                return Err(invalid("published bootstrap asset changed"));
            }
            published.insert(relative.to_owned(), contents.clone());
        } else if let Some(relative) = path.strip_prefix(&format!("{PENDING}/")) {
            if expected
                .get(relative)
                .is_none_or(|limit| contents.bytes > limit.bytes)
            {
                return Err(invalid("unknown or oversized pending bootstrap asset"));
            }
            pending = true;
        } else if path == "tryboot.txt" {
            if contents != &blob(inputs.plan.trial_config.as_bytes()) {
                return Err(invalid("trial selector differs from staging intent"));
            }
            continue;
        } else if path == SELECTOR_PENDING {
            if contents.bytes > inputs.plan.trial_config.len() as u64 {
                return Err(invalid("oversized pending trial selector"));
            }
            continue;
        } else {
            original.insert(path.clone(), contents.clone());
            continue;
        }
        credit = credit
            .checked_add(contents.bytes.div_ceil(cluster) * cluster)
            .ok_or_else(|| invalid("staging allocation overflow"))?;
    }
    if (!published.is_empty() && (published != expected || pending))
        || (files.contains_key("tryboot.txt") && published != expected)
        || (files.contains_key(SELECTOR_PENDING) && published != expected)
        || (files.contains_key(SELECTOR_PENDING) && files.contains_key("tryboot.txt"))
        || original != inputs.binding.original_boot_files
    {
        return Err(invalid(
            "bootstrap stage or retained legacy inventory is inconsistent",
        ));
    }
    Ok((original, credit))
}

pub(super) fn inventory(boot: &File, owner: u32) -> Result<BTreeMap<String, Blob>> {
    let mut files = BTreeMap::new();
    inspect::inventory(boot, "", boot.metadata()?.dev(), owner, &mut files, &mut 0)?;
    Ok(files)
}

/// The caller holds the install lock, journal and inactive-root claim, and
/// revalidates the live device/package on each checkpoint. No reboot occurs.
pub(super) fn apply(
    boot: &File,
    owner: u32,
    inputs: &TrialInputs,
    mut source: impl FnMut(&str) -> Result<File>,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    inputs.validate()?;
    owned(&boot.metadata()?, true, owner, false)?;
    let expected = expected(inputs);
    check(boot, owner, inputs)?;
    checkpoint("stage-checked")?;
    let completed = directory(boot, STAGE_DIRECTORY, owner, false)?;
    let pending = directory(boot, PENDING, owner, false)?;
    if completed.is_some() && pending.is_some() {
        return Err(invalid("published and pending bootstrap trees conflict"));
    }
    if let Some(completed) = completed {
        verify_tree(&completed, boot, owner, &expected, true)?;
    } else {
        let pending = match pending {
            Some(pending) => pending,
            None => directory(boot, PENDING, owner, true)?.expect("created directory"),
        };
        checkpoint("stage-directory-created")?;
        verify_tree(&pending, boot, owner, &expected, false)?;
        for (path, expected) in &expected {
            let mut reader = source(path)?;
            reader.rewind()?;
            let before = reader.metadata()?;
            if !before.is_file() || before.len() != expected.bytes {
                return Err(invalid("staging source size or type differs"));
            }
            let (parent, component) = parent(&pending, boot, path, owner)?;
            let mut target = writable(&parent, boot, component, owner, expected.bytes)?;
            checkpoint("stage-file-opened")?;
            target.set_len(0)?;
            let actual = super::package::transfer(&mut reader, &mut target, expected.bytes)?;
            checkpoint("stage-file-written")?;
            if &actual != expected || !unchanged(&before, &reader.metadata()?) {
                return Err(invalid("staging source contents changed"));
            }
            target.sync_all()?;
            parent.sync_all()?;
            checkpoint("stage-file-synced")?;
        }
        verify_tree(&pending, boot, owner, &expected, true)?;
        pending.sync_all()?;
        flush(boot)?;
        check(boot, owner, inputs)?;
        same_directory(boot, PENDING, &pending, owner)?;
        checkpoint("stage-before-publish")?;
        rename(boot, PENDING, STAGE_DIRECTORY)?;
        checkpoint("stage-published")?;
        boot.sync_all()?;
        flush(boot)?;
        checkpoint("stage-publication-synced")?;
    }
    check(boot, owner, inputs)?;
    let files = inventory(boot, owner)?;
    if !files.contains_key("tryboot.txt") {
        let bytes = inputs.plan.trial_config.as_bytes();
        let mut selector = writable(boot, boot, SELECTOR_PENDING, owner, bytes.len() as u64)?;
        checkpoint("trial-file-opened")?;
        selector.set_len(0)?;
        selector.write_all(bytes)?;
        checkpoint("trial-file-written")?;
        selector.sync_all()?;
        checkpoint("trial-file-synced")?;
        check(boot, owner, inputs)?;
        checkpoint("trial-before-publish")?;
        rename(boot, SELECTOR_PENDING, "tryboot.txt")?;
        checkpoint("trial-published")?;
    }
    boot.sync_all()?;
    flush(boot)?;
    checkpoint("trial-publication-synced")?;
    // Same complete-tree check used by the subsequent first-boot runtime.
    super::trial_files::TrialBootFiles::open(boot.try_clone()?, owner, inputs.clone())?;
    Ok(())
}

fn check(boot: &File, owner: u32, inputs: &TrialInputs) -> Result<()> {
    retained(&inventory(boot, owner)?, inputs, 512)?;
    Ok(())
}

fn rename(parent: &File, from: &str, to: &str) -> Result<()> {
    let from = name(from)?;
    let to = name(to)?;
    // SAFETY: validated single-component names and the held directory stay live.
    io_result(unsafe {
        libc::renameat2(
            parent.as_raw_fd(),
            from.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })
}

fn flush(boot: &File) -> Result<()> {
    // SAFETY: the checked boot directory remains open across the filesystem flush.
    io_result(unsafe { libc::syncfs(boot.as_raw_fd()) })
}

fn same_mount(file: &File, boot: &File) -> Result<()> {
    if file.metadata()?.dev() != boot.metadata()?.dev()
        || crate::recovery_linux::opened_mount_id(file)?
            != crate::recovery_linux::opened_mount_id(boot)?
    {
        return Err(invalid("staging path crossed boot mounts"));
    }
    Ok(())
}

fn directory(parent: &File, component: &str, owner: u32, create: bool) -> Result<Option<File>> {
    let file = match open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(file) => file,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound && create => {
            let name = name(component)?;
            // SAFETY: the validated component and parent descriptor stay live.
            io_result(unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) })?;
            parent.sync_all()?;
            open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?
        }
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    owned(&file.metadata()?, true, owner, false)?;
    same_mount(&file, parent)?;
    Ok(Some(file))
}

fn same_directory(parent: &File, component: &str, held: &File, owner: u32) -> Result<()> {
    let named = directory(parent, component, owner, false)?
        .ok_or_else(|| invalid("staging directory disappeared"))?;
    if named.metadata()?.ino() != held.metadata()?.ino()
        || named.metadata()?.dev() != held.metadata()?.dev()
    {
        return Err(invalid("staging directory was replaced"));
    }
    Ok(())
}

fn parent<'a>(stage: &File, boot: &File, path: &'a str, owner: u32) -> Result<(File, &'a str)> {
    let mut parts = path.split('/').peekable();
    let mut parent = stage.try_clone()?;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return Ok((parent, part));
        }
        parent = directory(&parent, part, owner, true)?.expect("created directory");
        same_mount(&parent, boot)?;
    }
    Err(invalid("empty staging path"))
}

fn writable(parent: &File, boot: &File, component: &str, owner: u32, limit: u64) -> Result<File> {
    let file = open_at(parent, component, libc::O_RDWR | libc::O_CREAT, 0o644)?;
    owned(&file.metadata()?, false, owner, false)?;
    same_mount(&file, boot)?;
    if file.metadata()?.len() > limit {
        return Err(invalid("oversized pending staging file"));
    }
    Ok(file)
}

fn verify_tree(
    directory: &File,
    boot: &File,
    owner: u32,
    expected: &BTreeMap<String, Blob>,
    complete: bool,
) -> Result<()> {
    fn walk(
        directory: &File,
        boot: &File,
        owner: u32,
        prefix: &str,
        expected: &BTreeMap<String, Blob>,
        found: &mut BTreeSet<String>,
    ) -> Result<()> {
        same_mount(directory, boot)?;
        for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
            let entry = entry?;
            let component = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("invalid staging filename"))?;
            let path = format!("{prefix}{component}");
            if entry.file_type()?.is_dir() {
                if !expected
                    .keys()
                    .any(|key| key.starts_with(&format!("{path}/")))
                {
                    return Err(invalid("unknown staging directory"));
                }
                let child = self::directory(directory, &component, owner, false)?
                    .ok_or_else(|| invalid("staging directory disappeared during verification"))?;
                walk(&child, boot, owner, &format!("{path}/"), expected, found)?;
            } else {
                let limit = expected
                    .get(&path)
                    .ok_or_else(|| invalid("unknown staging file"))?;
                let mut file = open_at(directory, &component, libc::O_RDONLY, 0)?;
                owned(&file.metadata()?, false, owner, false)?;
                same_mount(&file, boot)?;
                let before = file.metadata()?;
                if before.len() > limit.bytes {
                    return Err(invalid("oversized staging file"));
                }
                let bytes =
                    super::package::transfer(&mut file, &mut std::io::sink(), before.len())?;
                let named = open_at(directory, &component, libc::O_RDONLY, 0)?;
                if !unchanged(&before, &file.metadata()?) || !unchanged(&before, &named.metadata()?)
                {
                    return Err(invalid("staging file changed during verification"));
                }
                if &bytes == limit {
                    found.insert(path);
                }
            }
        }
        Ok(())
    }
    let mut found = BTreeSet::new();
    walk(directory, boot, owner, "", expected, &mut found)?;
    if complete && found != expected.keys().cloned().collect() {
        return Err(invalid("published staging tree is incomplete or changed"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_stage_files_tests.rs"]
mod tests;
