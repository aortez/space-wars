//! Standard tar recovery archives, built only from fixed operator-backup roots.
//! No links are traversed and no archive is ever extracted onto a live device.
use super::invalid;
use crate::owned_fs::{open_at, unchanged};
use crate::{Blob, Result, Sha256};
use serde::Serialize;
use sha2::{Digest, Sha256 as Hasher};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use xattr::FileExt;

pub(super) const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_ENTRIES: u64 = 100_000;
const MAX_ATTRIBUTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_PATH: usize = 4096;

// Exclusions contain only backup staging and disposable diagnostics. Unknown
// /data children are included, so an application adding user data is not lost.
pub(super) const DATA_EXCLUSIONS: &[&str] = &[
    "data/spacewars-bootstrap",
    "data/logs",
    "data/spacewars/coredumps",
];
const ROOT_PATHS: &[&str] = &[
    "etc",
    "root/.ssh",
    "home/spacewars/.ssh",
    "home/swadmin/.ssh",
    "var/lib/NetworkManager",
    "usr/sbin/ab-update",
    "usr/sbin/ab-update-with-key",
    "usr/sbin/ab-boot-manager",
    "usr/sbin/spacewars-fast-update",
    "usr/sbin/spacewars-renderer",
    "usr/sbin/persistent-data-init",
    "usr/bin/engine-os-manager",
    "usr/libexec/spacewars",
    "usr/lib/systemd/system/persistent-data-init.service",
    "usr/lib/systemd/system/persistent-data-bind.service",
    "usr/lib/systemd/system/spacewars-kiosk.service",
    "usr/lib/systemd/system/spacewars-access.service",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Inventory {
    pub source_sha256: Sha256,
    pub entries: u64,
    pub payload_bytes: u64,
    pub archive_upper_bound: u64,
}

/// Metadata-only sizing and change detection. Secret bytes are never reported.
pub(super) fn inspect(root: &File, data: &File) -> Result<Inventory> {
    walk(root, data, &mut |_, _, _, _, _| Ok(()))
}

pub(super) fn capture(
    root: &File,
    data: &File,
    output: impl Write,
    limit: u64,
    progress: &mut impl FnMut(u64),
) -> Result<(Blob, Inventory)> {
    let writer = CheckedWriter {
        output,
        hash: Hasher::new(),
        bytes: 0,
        limit: limit.min(MAX_ARCHIVE_BYTES),
        progress,
        last_report: Instant::now(),
    };
    let mut archive = tar::Builder::new(writer);
    let inventory = walk(root, data, &mut |path, file, metadata, attributes, link| {
        let mut extensions = BTreeMap::new();
        extensions.insert(
            "mtime".to_owned(),
            timestamp(metadata.mtime(), metadata.mtime_nsec()),
        );
        for (key, value) in attributes {
            extensions.insert(format!("SCHILY.xattr.{key}"), value.clone());
        }
        archive
            .append_pax_extensions(extensions.iter().map(|(k, v)| (k.as_str(), v.as_slice())))?;
        let mut header = tar::Header::new_gnu();
        header.set_metadata(metadata);
        // The PAX timestamp carries pre-epoch and nanosecond values as well.
        header.set_mtime(metadata.mtime().max(0) as u64);
        if let Some(link) = link {
            header.set_size(0);
            header.set_entry_type(if metadata.file_type().is_symlink() {
                tar::EntryType::Symlink
            } else {
                tar::EntryType::Link
            });
            archive.append_link(&mut header, path, link)?;
        } else if metadata.is_dir() {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            archive.append_data(&mut header, path, std::io::empty())?;
        } else {
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(metadata.len());
            archive.append_data(&mut header, path, file.take(metadata.len()))?;
        }
        Ok(())
    })?;
    let mut writer = archive.into_inner()?;
    writer.flush()?;
    (writer.progress)(writer.bytes);
    Ok((
        Blob {
            bytes: writer.bytes,
            sha256: hex(writer.hash),
        },
        inventory,
    ))
}

fn timestamp(seconds: i64, nanos: i64) -> Vec<u8> {
    let value = i128::from(seconds) * 1_000_000_000 + i128::from(nanos);
    let magnitude = value.abs();
    format!(
        "{}{}.{:09}",
        if value < 0 { "-" } else { "" },
        magnitude / 1_000_000_000,
        magnitude % 1_000_000_000
    )
    .into_bytes()
}

type Attributes = BTreeMap<String, Vec<u8>>;
type Visitor<'a> =
    dyn FnMut(&str, &File, &Metadata, &Attributes, Option<&PathBuf>) -> Result<()> + 'a;

struct Walk<'a> {
    visit: &'a mut Visitor<'a>,
    hash: Hasher,
    entries: u64,
    payload_bytes: u64,
    archive_upper_bound: u64,
    directories: BTreeSet<(u64, u64)>,
    files: BTreeMap<(u64, u64), String>,
    network_profiles: Option<(u64, u64)>,
}

fn walk(root: &File, data: &File, visit: &mut Visitor<'_>) -> Result<Inventory> {
    let network_profiles = relative(data, "NetworkManager/system-connections")?
        .map(|(_, _, file)| file.metadata().map(|m| (m.dev(), m.ino())))
        .transpose()?;
    let mut walker = Walk {
        visit,
        hash: Hasher::new(),
        entries: 0,
        payload_bytes: 0,
        archive_upper_bound: 1024,
        directories: BTreeSet::new(),
        files: BTreeMap::new(),
        network_profiles,
    };
    let data_metadata = data.metadata()?;
    walker.node(None, data.try_clone()?, "data", data_metadata.dev(), 0)?;
    for path in ROOT_PATHS {
        match relative(root, path)? {
            Some((parent, component, file)) => {
                walker.node(
                    Some((&parent, &component)),
                    file,
                    path,
                    root.metadata()?.dev(),
                    0,
                )?;
            }
            None if *path == "etc" => return Err(invalid("missing root configuration directory")),
            None => {
                walker.hash.update(serde_json::to_vec(&(path, "absent"))?);
                walker.hash.update(b"\n");
            }
        }
    }
    Ok(Inventory {
        source_sha256: hex(walker.hash),
        entries: walker.entries,
        payload_bytes: walker.payload_bytes,
        archive_upper_bound: walker.archive_upper_bound,
    })
}

impl Walk<'_> {
    fn node(
        &mut self,
        named: Option<(&File, &str)>,
        pinned: File,
        path: &str,
        device: u64,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_DEPTH || path.len() > MAX_PATH || self.entries >= MAX_ENTRIES {
            return Err(invalid("preservation tree exceeds its path or entry bound"));
        }
        let before = pinned.metadata()?;
        if before.dev() != device {
            return Err(invalid(
                "unexpected mounted filesystem in preservation tree",
            ));
        }
        if !before.is_dir() && !before.is_file() && !before.file_type().is_symlink() {
            return Err(invalid("unsupported special file in preservation tree"));
        }
        let inode = (before.dev(), before.ino());
        if before.is_dir() && !self.directories.insert(inode) {
            return Err(invalid(
                "duplicate or cyclic directory in preservation tree",
            ));
        }
        let symlink = if before.file_type().is_symlink() {
            Some(read_link(&pinned)?)
        } else {
            None
        };
        // O_PATH pins even symlinks without opening a FIFO/device or following
        // a link. Open regular data through that inode and recheck its identity.
        let file = if symlink.is_some() {
            pinned
        } else {
            File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?
        };
        if !unchanged(&before, &file.metadata()?) {
            return Err(invalid("preservation source changed while opening"));
        }
        let attributes = if symlink.is_some() {
            let (parent, component) = named.ok_or_else(|| invalid("unexpected symlink root"))?;
            link_attributes(parent, component)?
        } else {
            attributes(&file)?
        };
        let hardlink = if before.is_file() {
            self.files.insert(inode, path.to_owned())
        } else {
            None
        };
        // Keep the first archive path as the target of every subsequent link.
        if let Some(first) = hardlink.as_ref() {
            self.files.insert(inode, first.clone());
        }
        let link = symlink.or_else(|| hardlink.as_ref().map(PathBuf::from));
        self.record(path, &before, &attributes, link.as_ref())?;
        if before.is_file() && hardlink.is_none() {
            self.payload_bytes = bounded_add(self.payload_bytes, before.len())?;
            self.archive_upper_bound =
                bounded_add(self.archive_upper_bound, aligned(before.len())?)?;
        }
        (self.visit)(path, &file, &before, &attributes, link.as_ref())?;
        if before.is_dir() {
            let mut names = BTreeSet::new();
            for entry in fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd()))? {
                if names.len() as u64 + self.entries >= MAX_ENTRIES {
                    return Err(invalid("preservation tree exceeds its entry bound"));
                }
                let name = entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| invalid("non-UTF-8 preservation filename"))?;
                names.insert(name);
            }
            for component in names {
                let child_path = format!("{path}/{component}");
                if DATA_EXCLUSIONS.contains(&child_path.as_str()) {
                    continue;
                }
                let child = open_at(&file, &component, libc::O_PATH, 0)?;
                // Exclude only the actual bind alias of the captured store.
                // An unbound rootfs seed directory is separate original state
                // and must be retained, not silently omitted by its pathname.
                if child_path == "etc/NetworkManager/system-connections" {
                    let metadata = child.metadata()?;
                    if self.network_profiles == Some((metadata.dev(), metadata.ino())) {
                        self.hash.update(serde_json::to_vec(&(
                            child_path,
                            "network-profile-alias",
                            metadata.dev(),
                            metadata.ino(),
                        ))?);
                        self.hash.update(b"\n");
                        continue;
                    }
                }
                self.node(
                    Some((&file, &component)),
                    child,
                    &child_path,
                    device,
                    depth + 1,
                )?;
            }
        }
        if !unchanged(&before, &file.metadata()?) {
            return Err(invalid("preservation source changed during capture"));
        }
        if let Some((parent, component)) = named {
            let current = open_at(parent, component, libc::O_PATH, 0)?;
            if !unchanged(&before, &current.metadata()?) {
                return Err(invalid("preservation source name changed during capture"));
            }
        }
        Ok(())
    }

    fn record(
        &mut self,
        path: &str,
        metadata: &Metadata,
        attributes: &Attributes,
        link: Option<&PathBuf>,
    ) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let target = link.map(|p| p.as_os_str().as_bytes());
        let stamp = (
            metadata.dev(),
            metadata.ino(),
            metadata.mode(),
            metadata.uid(),
            metadata.gid(),
            metadata.nlink(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        );
        self.hash
            .update(serde_json::to_vec(&(path, stamp, attributes, target))?);
        self.hash.update(b"\n");
        self.entries += 1;
        let attribute_bytes: usize = attributes
            .iter()
            .map(|(k, v)| k.len() + v.len() + 512)
            .sum();
        let variable = path.len() + target.map_or(0, <[u8]>::len) + attribute_bytes;
        self.archive_upper_bound = bounded_add(
            self.archive_upper_bound,
            aligned(8192 + 2 * variable as u64)?,
        )?;
        Ok(())
    }
}

fn relative(root: &File, path: &str) -> Result<Option<(File, String, File)>> {
    let mut parent = root.try_clone()?;
    let components: Vec<_> = path.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        let flags = if last {
            libc::O_PATH
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY
        };
        match open_at(&parent, component, flags, 0) {
            Ok(file) => {
                if file.metadata()?.dev() != root.metadata()?.dev() {
                    return Err(invalid("unexpected mount in fixed root preservation path"));
                }
                if last {
                    return Ok(Some((parent, (*component).to_owned(), file)));
                }
                parent = file;
            }
            Err(crate::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
    }
    Err(invalid("empty preservation path"))
}

pub(super) fn attributes(file: &File) -> Result<Attributes> {
    collect_attributes(file.list_xattr()?, |name| file.get_xattr(name))
}

fn link_attributes(parent: &File, component: &str) -> Result<Attributes> {
    let path = PathBuf::from(format!(
        "/proc/self/fd/{}/{}",
        parent.as_raw_fd(),
        component
    ));
    collect_attributes(xattr::list(&path)?, |name| xattr::get(&path, name))
}

fn collect_attributes(
    names: impl Iterator<Item = std::ffi::OsString>,
    mut get: impl FnMut(&str) -> std::io::Result<Option<Vec<u8>>>,
) -> Result<Attributes> {
    let mut result = BTreeMap::new();
    let mut bytes = 0;
    for name in names {
        let name = name
            .into_string()
            .map_err(|_| invalid("non-UTF-8 extended attribute name"))?;
        if name
            .bytes()
            .any(|b| b.is_ascii_control() || matches!(b, b'=' | b' '))
        {
            return Err(invalid("unsupported extended attribute name"));
        }
        let value =
            get(&name)?.ok_or_else(|| invalid("extended attribute disappeared during capture"))?;
        bytes += name.len() + value.len();
        if bytes > MAX_ATTRIBUTES || result.len() >= 256 {
            return Err(invalid("preservation attributes exceed their bound"));
        }
        result.insert(name, value);
    }
    Ok(result)
}

fn read_link(file: &File) -> Result<PathBuf> {
    let mut buffer = vec![0; MAX_PATH + 1];
    // SAFETY: Linux readlinkat with an empty name reads the pinned O_PATH link.
    let count = unsafe {
        libc::readlinkat(
            file.as_raw_fd(),
            c"".as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if count < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if count as usize > MAX_PATH {
        return Err(invalid("preservation symlink exceeds its bound"));
    }
    buffer.truncate(count as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

fn aligned(bytes: u64) -> Result<u64> {
    Ok(bounded_add(bytes, 511)? / 512 * 512)
}
fn bounded_add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|n| *n <= MAX_ARCHIVE_BYTES)
        .ok_or_else(|| invalid("preservation archive exceeds its size bound"))
}
fn hex(hash: Hasher) -> Sha256 {
    format!("{:x}", hash.finalize())
        .try_into()
        .expect("SHA-256 hex")
}

struct CheckedWriter<'a, W, F> {
    output: W,
    hash: Hasher,
    bytes: u64,
    limit: u64,
    progress: &'a mut F,
    last_report: Instant,
}
impl<W: Write, F: FnMut(u64)> Write for CheckedWriter<'_, W, F> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if buffer.len() as u64 > self.limit.saturating_sub(self.bytes) {
            return Err(std::io::Error::other(
                "preservation archive exceeded its reserved bound",
            ));
        }
        let count = self.output.write(buffer)?;
        self.hash.update(&buffer[..count]);
        self.bytes += count as u64;
        if self.last_report.elapsed() >= Duration::from_millis(250) {
            (self.progress)(self.bytes);
            self.last_report = Instant::now();
        }
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
#[path = "bootstrap_archive_tests.rs"]
mod tests;
