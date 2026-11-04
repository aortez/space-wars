//! Read-only snapshots of the fixed files that bootstrap will replace.
use super::{invalid, legacy_assets};
use crate::owned_fs::{open_at, owned, unchanged};
use crate::{Blob, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

pub(super) const MANAGER: &str = "usr/bin/engine-os-manager";

pub(super) fn targets() -> Vec<(&'static str, u32)> {
    std::iter::once((MANAGER, 0o755))
        .chain(legacy_assets::FILES.iter().copied())
        .collect()
}

pub(super) fn limit(index: usize) -> u64 {
    if index == 0 {
        crate::MAX_IMAGE_EXECUTABLE_BYTES
    } else {
        legacy_assets::MAX_BYTES
    }
}

pub(super) fn parents() -> Vec<String> {
    let mut paths = BTreeSet::new();
    for (path, _) in targets() {
        let mut parent = path;
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            paths.insert(prefix.to_owned());
            parent = prefix;
        }
    }
    paths.into_iter().collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) uid: u32,
    pub(super) gid: u32,
    pub(super) mode: u32,
}

impl Identity {
    pub(super) fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.mode() & 0o7777,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Original {
    pub(super) identity: Identity,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    pub(super) attributes: BTreeMap<String, Vec<u8>>,
    pub(super) contents: Blob,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectoryEntry {
    pub(super) path: String,
    pub(super) original: Option<Identity>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub(super) root: Identity,
    /// The fixed parent list is ordered; None records an absent directory.
    pub(super) directories: Vec<DirectoryEntry>,
    /// The target list is versioned by the containing record; None is absence,
    /// distinct from a present empty file, with its metadata and empty hash.
    pub(super) files: Vec<Option<Original>>,
}

impl Original {
    /// Renaming/linking changes ctime. The inode, contents, permissions,
    /// attributes and original mtime must still be exactly the recorded ones.
    pub(super) fn after_rename(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.modified_seconds == other.modified_seconds
            && self.modified_nanoseconds == other.modified_nanoseconds
            && self.attributes == other.attributes
            && self.contents == other.contents
    }
}

impl Snapshot {
    pub(super) fn validate(&self) -> Result<()> {
        let valid = |identity: &Identity| {
            identity.device == self.root.device
                && identity.inode != 0
                && identity.uid == self.root.uid
                && identity.mode & !0o777 == 0
                && identity.mode & 0o022 == 0
        };
        if !valid(&self.root)
            || self.directories.len() != parents().len()
            || self.files.len() != targets().len()
            || self.directories.iter().zip(parents()).any(|(entry, path)| {
                entry.path != path || entry.original.as_ref().is_some_and(|d| !valid(d))
            })
        {
            return Err(invalid("invalid legacy source directory record"));
        }
        for (index, original) in self.files.iter().enumerate() {
            if let Some(original) = original
                && (!valid(&original.identity)
                    || original.contents.bytes > limit(index)
                    || !(0..1_000_000_000).contains(&original.modified_nanoseconds)
                    || !(0..1_000_000_000).contains(&original.changed_nanoseconds)
                    || original.attributes.len() > 256
                    || original
                        .attributes
                        .iter()
                        .map(|(key, value)| key.len() + value.len())
                        .sum::<usize>()
                        > 64 * 1024)
            {
                return Err(invalid("invalid legacy source file record"));
            }
        }
        Ok(())
    }

    pub(super) fn bytes(&self) -> u64 {
        self.files
            .iter()
            .flatten()
            .map(|file| file.contents.bytes)
            .sum()
    }
}

pub(super) fn inspect(root: &File, owner: u32) -> Result<Snapshot> {
    capture(root, owner, |_| Ok(std::io::sink()))
}

pub(super) fn capture<W: Write>(
    root: &File,
    owner: u32,
    mut destination: impl FnMut(usize) -> Result<W>,
) -> Result<Snapshot> {
    let metadata = root.metadata()?;
    owned(&metadata, true, owner, false)?;
    if metadata.mode() & 0o7000 != 0 {
        return Err(invalid("unsafe legacy source root mode"));
    }
    let identity = Identity::from(&metadata);
    let directories = inspect_directories(root, owner)?;
    let mut files = Vec::new();
    for (index, (path, _)) in targets().into_iter().enumerate() {
        let mut output = destination(index)?;
        let (parent_path, component) = path.rsplit_once('/').expect("fixed target parent");
        let Some(parent) = directory(root, parent_path, owner)? else {
            files.push(None);
            continue;
        };
        files.push(file_at(
            root,
            &parent,
            component,
            owner,
            limit(index),
            &mut output,
        )?);
    }
    if identity != Identity::from(&root.metadata()?)
        || directories != inspect_directories(root, owner)?
    {
        return Err(invalid("legacy source directory changed during capture"));
    }
    let snapshot = Snapshot {
        root: identity,
        directories,
        files,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

pub(super) fn file_at(
    root: &File,
    parent: &File,
    component: &str,
    owner: u32,
    limit: u64,
    output: &mut impl Write,
) -> Result<Option<Original>> {
    let file = match open_at(parent, component, libc::O_PATH, 0) {
        Ok(file) => file,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let before = file.metadata()?;
    let result = opened_file(root, &file, owner, limit, false, output)?;
    let named = open_at(parent, component, libc::O_PATH, 0)?;
    if !unchanged(&before, &named.metadata()?) {
        return Err(invalid("legacy source name changed during capture"));
    }
    Ok(Some(result))
}

/// `anonymous` is only for a freshly allocated O_TMPFILE owned by the caller.
pub(super) fn opened_file(
    root: &File,
    file: &File,
    owner: u32,
    limit: u64,
    anonymous: bool,
    output: &mut impl Write,
) -> Result<Original> {
    let before = file.metadata()?;
    if before.uid() != owner
        || !before.is_file()
        || before.nlink() != u64::from(!anonymous)
        || before.dev() != root.metadata()?.dev()
        || crate::recovery_linux::opened_mount_id(file)?
            != crate::recovery_linux::opened_mount_id(root)?
        || before.mode() & 0o7022 != 0
        || before.len() > limit
    {
        return Err(invalid("unsafe or oversized legacy source file"));
    }
    let mut reader = File::open(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
    if !unchanged(&before, &reader.metadata()?) {
        return Err(invalid("legacy source changed while opening"));
    }
    let attributes = super::archive::attributes(&reader)?;
    let mut hash = Hasher::new();
    let mut bytes = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > before.len() {
            return Err(invalid("legacy source grew during capture"));
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    output.flush()?;
    if bytes != before.len() || !unchanged(&before, &reader.metadata()?) {
        return Err(invalid("legacy source changed during capture"));
    }
    Ok(Original {
        identity: Identity::from(&before),
        modified_seconds: before.mtime(),
        modified_nanoseconds: before.mtime_nsec(),
        changed_seconds: before.ctime(),
        changed_nanoseconds: before.ctime_nsec(),
        attributes,
        contents: Blob {
            bytes,
            sha256: format!("{:x}", hash.finalize())
                .try_into()
                .expect("SHA-256 hex"),
        },
    })
}

fn inspect_directories(root: &File, owner: u32) -> Result<Vec<DirectoryEntry>> {
    parents()
        .iter()
        .map(|path| {
            let original = directory(root, path, owner)?
                .map(|file| file.metadata().map(|metadata| Identity::from(&metadata)))
                .transpose()?;
            Ok(DirectoryEntry {
                path: path.clone(),
                original,
            })
        })
        .collect()
}

pub(super) fn directory(root: &File, path: &str, owner: u32) -> Result<Option<File>> {
    let mut parent = root.try_clone()?;
    if path.is_empty() {
        return Ok(Some(parent));
    }
    let mount = crate::recovery_linux::opened_mount_id(root)?;
    for component in path.split('/') {
        let child = match open_at(&parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
            Ok(child) => child,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let metadata = child.metadata()?;
        owned(&metadata, true, owner, false)?;
        if metadata.dev() != root.metadata()?.dev()
            || crate::recovery_linux::opened_mount_id(&child)? != mount
            || metadata.mode() & 0o7000 != 0
        {
            return Err(invalid("unsafe legacy source directory or mount"));
        }
        parent = child;
    }
    Ok(Some(parent))
}
