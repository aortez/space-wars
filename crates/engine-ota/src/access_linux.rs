//! Fixed, persistent SSH login keys. Boot imports each role once; rollback must
//! never resurrect keys from a root image. Operator imports require root access.
use crate::file_command::FileCommand;
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, unchanged};
use crate::{Error, Result};
use serde::Serialize;
use std::fs::{File, Metadata, Permissions};
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const DIRECTORY: &str = "spacewars-access";
const MAX_BYTES: usize = 64 * 1024;
const MAX_LINE: usize = 8 * 1024;
const MAX_KEYS: usize = 64;

#[derive(Clone, Copy)]
struct Owners {
    root: u32,
    kiosk: u32,
    admin: u32,
}
const OWNERS: Owners = Owners {
    root: 0,
    kiosk: 1000,
    admin: 1001,
};

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct AccessStatus {
    pub initialized: bool,
    pub root_keys: usize,
    pub runtime_keys: usize,
    pub admin_keys: usize,
}

/// First-boot entry point; no path, key bytes or account can be supplied.
/// Existing persistent files are authoritative, including deliberately empty ones.
pub fn initialize() -> Result<AccessStatus> {
    let lock = InstallLock::acquire()?;
    let root = open_directory(Path::new("/"), 0)?;
    let data = crate::recovery_linux::open_data_mount()?;
    lock.check()?;
    initialize_under(&root, &data, OWNERS)
}

/// Explicit root operator action, never automatic on boot or exposed to the
/// kiosk socket. Import only /root/.ssh/authorized_keys into an empty root file.
/// The caller also checks maintenance eligibility under this same live lock.
pub fn import_root_key(lock: &InstallLock) -> Result<AccessStatus> {
    lock.check()?;
    let root = open_directory(Path::new("/"), 0)?;
    let data = crate::recovery_linux::open_data_mount()?;
    import_under(&root, &data, OWNERS, Role::Root)
}

/// Explicit operator provisioning; never promote the kiosk's keys or import
/// an image's admin key file automatically into an existing persistent store.
pub fn import_admin_key(lock: &InstallLock) -> Result<AccessStatus> {
    lock.check()?;
    let root = open_directory(Path::new("/"), 0)?;
    let data = crate::recovery_linux::open_data_mount()?;
    import_under(&root, &data, OWNERS, Role::Admin)
}

/// Bootstrap must preserve operator and runtime roles before reserving an attempt, because a
/// later root image must never import new/default keys if this store is lost.
pub(crate) fn preserved_for_bootstrap(lock: &InstallLock) -> Result<AccessStatus> {
    lock.check()?;
    let data = crate::recovery_linux::open_data_mount()?;
    preserved_under(&data, OWNERS.root)
}

pub(crate) fn preserved_under(data: &File, owner: u32) -> Result<AccessStatus> {
    let directory = open_at(data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    if directory.metadata()?.dev() != data.metadata()?.dev() {
        return Err(invalid("persistent access is not on the data filesystem"));
    }
    let status = status(&directory, owner, false)?;
    if (status.root_keys == 0 && status.admin_keys == 0) || status.runtime_keys == 0 {
        return Err(invalid(
            "bootstrap requires preserved root or administrator keys and separate runtime keys",
        ));
    }
    Ok(status)
}

fn initialize_under(root: &File, data: &File, owners: Owners) -> Result<AccessStatus> {
    check_directory(data, owners.root, 0o755)?;
    if let Some(directory) = optional_directory(data, DIRECTORY)? {
        status(&directory, owners.root, false)?;
        // Older stores predate the admin role. Add an empty role, without
        // importing credentials or undoing a deliberately empty revocation.
        if admin_keys(&directory, owners.root)?.is_none() {
            publish_keys(&directory, "swadmin", b"", 0o644, None)?;
        }
        return status(&directory, owners.root, false);
    }
    // An interrupted migration must retain its prepared roles. Importing the
    // new root's empty/default files would silently replace missing device keys.
    crate::bootstrap::require_no_active_under(data, owners.root)?;
    // Once recovery has been provisioned, a missing store is data loss, not
    // another first boot. Never fall back to keys shipped in a later rootfs.
    if let Some(system) = optional_directory(data, "spacewars-system")? {
        owned(&system.metadata()?, true, owners.root, true)?;
        if optional_directory(&system, "recovery")?.is_some() {
            return Err(invalid("missing access store on a provisioned device"));
        }
    }
    let root_keys = source(root, Role::Root, owners)?;
    let runtime_keys = source(root, Role::Runtime, owners)?;
    let admin_keys = source(root, Role::Admin, owners)?;
    let pending = tempfile::Builder::new()
        .prefix(".spacewars-access-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir_in(format!("/proc/self/fd/{}", data.as_raw_fd()))?;
    let pending_name = pending.path().file_name().unwrap().to_str().unwrap();
    let directory = open_at(data, pending_name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    check_directory(&directory, owners.root, 0o700)?;
    write_keys(&directory, "root", &root_keys.bytes, 0o600)?;
    write_keys(&directory, "spacewars", &runtime_keys.bytes, 0o644)?;
    write_keys(&directory, "swadmin", &admin_keys.bytes, 0o644)?;
    directory.set_permissions(Permissions::from_mode(0o755))?;
    directory.sync_all()?;
    // Publish all roles together. An interruption leaves either no store or a
    // complete one, never a half-populated directory that looks initialized.
    rename(data, pending_name, data, DIRECTORY, libc::RENAME_NOREPLACE)?;
    data.sync_all()?;
    status(&directory, owners.root, true)
}

fn import_under(root: &File, data: &File, owners: Owners, role: Role) -> Result<AccessStatus> {
    check_directory(data, owners.root, 0o755)?;
    let directory = open_at(data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let current = status(&directory, owners.root, false)?;
    let (component, count, mode) = match role {
        Role::Root => ("root", current.root_keys, 0o600),
        Role::Admin => ("swadmin", current.admin_keys, 0o644),
        Role::Runtime => return Err(invalid("operator imports cannot select the runtime role")),
    };
    if count != 0 {
        return Ok(current);
    }
    let previous = match open_at(&directory, component, libc::O_RDONLY, 0) {
        Ok(file) => Some((file.metadata()?, file)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let keys = source(root, role, owners)?;
    if keys.count == 0 {
        return Err(invalid(
            "no keys have been provisioned in the selected operator's .ssh/authorized_keys",
        ));
    }
    publish_keys(&directory, component, &keys.bytes, mode, previous)?;
    status(&directory, owners.root, false)
}

fn publish_keys(
    directory: &File,
    component: &str,
    bytes: &[u8],
    mode: u32,
    previous: Option<(Metadata, File)>,
) -> Result<()> {
    let mut pending = tempfile::Builder::new()
        .prefix(".access-")
        .tempfile_in(format!("/proc/self/fd/{}", directory.as_raw_fd()))?;
    pending.write_all(bytes)?;
    pending
        .as_file()
        .set_permissions(Permissions::from_mode(mode))?;
    pending.as_file().sync_all()?;
    if let Some((original, previous)) = &previous {
        let named = open_at(directory, component, libc::O_RDONLY, 0)?;
        if !unchanged(original, &named.metadata()?) || !unchanged(original, &previous.metadata()?) {
            return Err(invalid("access file changed during operator import"));
        }
    }
    rename(
        directory,
        pending.path().file_name().unwrap().to_str().unwrap(),
        directory,
        component,
        if previous.is_some() {
            0
        } else {
            libc::RENAME_NOREPLACE
        },
    )?;
    directory.sync_all()?;
    Ok(())
}

fn status(directory: &File, owner: u32, initialized: bool) -> Result<AccessStatus> {
    check_directory(directory, owner, 0o755)?;
    let root_keys = stored(directory, "root", owner, 0o600)?.count;
    let runtime_keys = stored(directory, "spacewars", owner, 0o644)?.count;
    let admin_keys = admin_keys(directory, owner)?.map_or(0, |keys| keys.count);
    Ok(AccessStatus {
        initialized,
        root_keys,
        runtime_keys,
        admin_keys,
    })
}

struct Keys {
    bytes: Vec<u8>,
    count: usize,
}

fn stored(parent: &File, component: &str, owner: u32, mode: u32) -> Result<Keys> {
    let file = open_at(parent, component, libc::O_RDONLY, 0)?;
    let metadata = file.metadata()?;
    owned(&metadata, false, owner, false)?;
    if metadata.mode() & 0o7777 != mode || metadata.dev() != parent.metadata()?.dev() {
        return Err(invalid("unexpected persistent key mode or filesystem"));
    }
    read_keys(parent, component, file, metadata)
}

fn admin_keys(directory: &File, owner: u32) -> Result<Option<Keys>> {
    match stored(directory, "swadmin", owner, 0o644) {
        Ok(keys) => Ok(Some(keys)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Clone, Copy)]
enum Role {
    Root,
    Runtime,
    Admin,
}

fn source(root: &File, role: Role, owners: Owners) -> Result<Keys> {
    let (components, account): (&[&str], u32) = match role {
        Role::Root => (&["root", ".ssh"], owners.root),
        Role::Runtime => (&["home", "spacewars", ".ssh"], owners.kiosk),
        Role::Admin => (&["home", "swadmin", ".ssh"], owners.admin),
    };
    let mut parent = root.try_clone()?;
    for (index, component) in components.iter().enumerate() {
        let Some(child) = optional_directory(&parent, component)? else {
            return Ok(Keys {
                bytes: Vec::new(),
                count: 0,
            });
        };
        let metadata = child.metadata()?;
        let owner = metadata.uid();
        if owner != owners.root && (index == 0 || owner != account) {
            return Err(invalid("unexpected owner in source key directory"));
        }
        owned(&metadata, true, owner, false)?;
        parent = child;
    }
    let file = match open_at(&parent, "authorized_keys", libc::O_RDONLY, 0) {
        Ok(file) => file,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Keys {
                bytes: Vec::new(),
                count: 0,
            });
        }
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    let owner = metadata.uid();
    if owner != owners.root && owner != account {
        return Err(invalid("unexpected source key owner"));
    }
    owned(&metadata, false, owner, false)?;
    read_keys(&parent, "authorized_keys", file, metadata)
}

fn read_keys(parent: &File, component: &str, mut file: File, original: Metadata) -> Result<Keys> {
    if original.len() > MAX_BYTES as u64 {
        return Err(invalid("key file exceeds its size bound"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let named = open_at(parent, component, libc::O_RDONLY, 0)?;
    if original.len() != bytes.len() as u64
        || !unchanged(&original, &file.metadata()?)
        || !unchanged(&original, &named.metadata()?)
    {
        return Err(invalid("key file changed while being read"));
    }
    let count = validate(&bytes)?;
    Ok(Keys { bytes, count })
}

fn validate(bytes: &[u8]) -> Result<usize> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("key file is not UTF-8 text"))?;
    if bytes.len() > MAX_BYTES
        || bytes
            .iter()
            .any(|b| b.is_ascii_control() && !matches!(b, b'\n' | b'\r' | b'\t'))
    {
        return Err(invalid("invalid key file length or control bytes"));
    }
    let mut count = 0;
    for line in text.lines() {
        if line.len() > MAX_LINE {
            return Err(invalid("key line exceeds its size bound"));
        }
        let line = line.trim_start_matches([' ', '\t', '\r']);
        if !line.is_empty() && !line.starts_with('#') {
            count += 1;
            if line.contains("PRIVATE KEY") || count > MAX_KEYS {
                return Err(invalid("expected a bounded public authorized_keys file"));
            }
        }
    }
    if count == 0 {
        return Ok(0);
    }
    // Validate the snapshotted public keys with the same OpenSSH package as
    // sshd. Options/comments are retained verbatim, never interpreted here.
    let mut snapshot = tempfile::tempfile()?;
    snapshot.write_all(bytes)?;
    snapshot.rewind()?;
    let mut command = Command::new("/usr/bin/ssh-keygen");
    command.args([
        "-l",
        "-E",
        "sha256",
        "-f",
        &format!("/proc/self/fd/{}", snapshot.as_raw_fd()),
    ]);
    let output = FileCommand::default()
        .run(command, &snapshot, Duration::from_secs(5), MAX_BYTES)
        .map_err(|_| invalid("OpenSSH could not validate the public keys"))?;
    let fingerprints =
        std::str::from_utf8(&output).map_err(|_| invalid("invalid OpenSSH key summary"))?;
    // ssh-keygen can skip malformed lines yet exit successfully when another
    // line is valid. Require one fingerprint for every non-comment input line.
    if fingerprints.lines().count() != count
        || fingerprints.lines().any(|line| {
            !line
                .split_whitespace()
                .nth(1)
                .is_some_and(|s| s.starts_with("SHA256:"))
        })
    {
        return Err(invalid("OpenSSH skipped one or more public key lines"));
    }
    Ok(count)
}

fn check_directory(directory: &File, owner: u32, mode: u32) -> Result<()> {
    let metadata = directory.metadata()?;
    owned(&metadata, true, owner, false)?;
    if metadata.mode() & 0o7777 != mode {
        return Err(invalid("unexpected access directory permissions"));
    }
    Ok(())
}

fn optional_directory(parent: &File, component: &str) -> Result<Option<File>> {
    match open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(file) => Ok(Some(file)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_keys(directory: &File, component: &str, bytes: &[u8], mode: u32) -> Result<()> {
    let mut file = open_at(
        directory,
        component,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    file.write_all(bytes)?;
    file.set_permissions(Permissions::from_mode(mode))?;
    file.sync_all()?;
    Ok(())
}

fn rename(from: &File, old: &str, to: &File, new: &str, flags: u32) -> Result<()> {
    let old = name(old)?;
    let new = name(new)?;
    // SAFETY: descriptors and validated C strings remain live across renameat2.
    io_result(unsafe {
        libc::renameat2(
            from.as_raw_fd(),
            old.as_ptr(),
            to.as_raw_fd(),
            new.as_ptr(),
            flags,
        )
    })
}

fn invalid(message: &str) -> Error {
    Error::Access(message.into())
}

#[cfg(test)]
#[path = "access_linux_tests.rs"]
mod tests;
