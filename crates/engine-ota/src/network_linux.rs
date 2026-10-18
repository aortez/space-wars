//! Import seeded NetworkManager profiles once. The persistent directory, even
//! when empty, wins over profiles in both newer and rolled-back root images.
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file, unchanged};
use crate::{Error, Result};
use serde::Serialize;
use std::fs::{self, File, Metadata, Permissions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

const DIRECTORY: &str = "NetworkManager";
const CONNECTIONS: &str = "system-connections";
const MAX_PROFILES: usize = 128;
const MAX_PROFILE_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct NetworkStatus {
    pub initialized: bool,
    /// Zero on subsequent boots; existing credentials are not read or logged.
    pub imported_profiles: usize,
}

/// Fixed-path root entry point, run before the persistent bind and NM startup.
pub fn initialize() -> Result<NetworkStatus> {
    let lock = InstallLock::acquire()?;
    let root = open_directory(Path::new("/"), 0)?;
    let data = crate::recovery_linux::open_data_mount()?;
    lock.check()?;
    initialize_under(&root, &data, 0)
}

/// Preparation cannot reserve a migration that would need to import profiles
/// from a new root. Existing legacy persistent directories satisfy this check.
pub(crate) fn preserved_for_bootstrap(lock: &InstallLock) -> Result<()> {
    lock.check()?;
    let data = crate::recovery_linux::open_data_mount()?;
    let directory = open_at(&data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    preserved(&data, &directory, 0)
}

fn initialize_under(root: &File, data: &File, owner: u32) -> Result<NetworkStatus> {
    check_directory(data, owner, 0o755)?;
    if let Some(directory) = optional_directory(data, DIRECTORY)? {
        preserved(data, &directory, owner)?;
        return Ok(NetworkStatus {
            initialized: false,
            imported_profiles: 0,
        });
    }
    crate::bootstrap::require_no_active_under(data, owner)?;
    if let Some(system) = optional_directory(data, "spacewars-system")? {
        owned(&system.metadata()?, true, owner, true)?;
        if optional_directory(&system, "recovery")?.is_some() {
            return Err(invalid("missing profile store on a provisioned device"));
        }
    }
    let source = source_directory(root, owner)?;
    let original = source.metadata()?;
    let profiles = snapshot(&source, owner)?;
    let pending = tempfile::Builder::new()
        .prefix(".spacewars-network-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir_in(format!("/proc/self/fd/{}", data.as_raw_fd()))?;
    let pending_name = pending.path().file_name().unwrap().to_str().unwrap();
    let directory = open_at(data, pending_name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let component = name(CONNECTIONS)?;
    // SAFETY: the pinned directory and validated name remain live.
    io_result(unsafe { libc::mkdirat(directory.as_raw_fd(), component.as_ptr(), 0o700) })?;
    let connections = open_at(
        &directory,
        CONNECTIONS,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    for profile in &profiles {
        let mut file = open_at(
            &connections,
            &profile.name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        file.write_all(&profile.bytes)?;
        file.set_permissions(Permissions::from_mode(0o600))?;
        file.sync_all()?;
    }
    connections.sync_all()?;
    directory.set_permissions(Permissions::from_mode(0o755))?;
    directory.sync_all()?;
    recheck_source(&source, &original, &profiles)?;
    let named = source_directory(root, owner)?;
    if !unchanged(&original, &named.metadata()?) {
        return Err(invalid("profile source directory changed during import"));
    }
    // A first import publishes the parent and its complete child together.
    // After a crash, no published parent can look like an empty first boot.
    let old = name(pending_name)?;
    let new = name(DIRECTORY)?;
    // SAFETY: descriptors and C strings remain live across renameat2.
    io_result(unsafe {
        libc::renameat2(
            data.as_raw_fd(),
            old.as_ptr(),
            data.as_raw_fd(),
            new.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })?;
    data.sync_all()?;
    preserved(data, &directory, owner)?;
    Ok(NetworkStatus {
        initialized: true,
        imported_profiles: profiles.len(),
    })
}

fn preserved(data: &File, directory: &File, owner: u32) -> Result<()> {
    check_directory(data, owner, 0o755)?;
    check_directory(directory, owner, 0o755)?;
    let connections = open_at(
        directory,
        CONNECTIONS,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    check_directory(&connections, owner, 0o700)?;
    let device = data.metadata()?.dev();
    if directory.metadata()?.dev() != device || connections.metadata()?.dev() != device {
        return Err(invalid(
            "persistent profiles are not on the data filesystem",
        ));
    }
    Ok(())
}

fn source_directory(root: &File, owner: u32) -> Result<File> {
    let mut directory = root.try_clone()?;
    for component in ["etc", DIRECTORY, CONNECTIONS] {
        directory = open_at(&directory, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&directory.metadata()?, true, owner, false)?;
        if directory.metadata()?.dev() != root.metadata()?.dev() {
            return Err(invalid("seed profiles are not on the root filesystem"));
        }
    }
    Ok(directory)
}

struct Profile {
    name: String,
    bytes: Vec<u8>,
    metadata: Metadata,
}

fn snapshot(directory: &File, owner: u32) -> Result<Vec<Profile>> {
    let mut profiles = Vec::new();
    let mut total = 0;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        if profiles.len() == MAX_PROFILES {
            return Err(invalid("too many seeded profiles"));
        }
        let component = entry?
            .file_name()
            .into_string()
            .map_err(|_| invalid("profile filename is not UTF-8"))?;
        let file = open_at(directory, &component, libc::O_RDONLY, 0)?;
        let metadata = file.metadata()?;
        owned(&metadata, false, owner, true)?;
        if !matches!(metadata.mode() & 0o7777, 0o400 | 0o600)
            || metadata.dev() != directory.metadata()?.dev()
        {
            return Err(invalid(
                "unexpected seeded profile permissions or filesystem",
            ));
        }
        let bytes = read_file(file, owner, MAX_PROFILE_BYTES)?;
        total += bytes.len();
        if total > MAX_TOTAL_BYTES {
            return Err(invalid("seeded profiles exceed the total size bound"));
        }
        profiles.push(Profile {
            name: component,
            bytes,
            metadata,
        });
    }
    Ok(profiles)
}

fn recheck_source(directory: &File, original: &Metadata, profiles: &[Profile]) -> Result<()> {
    if !unchanged(original, &directory.metadata()?) {
        return Err(invalid("profile source directory changed during import"));
    }
    for profile in profiles {
        let named = open_at(directory, &profile.name, libc::O_RDONLY, 0)?;
        if !unchanged(&profile.metadata, &named.metadata()?) {
            return Err(invalid("seeded profile changed during import"));
        }
    }
    Ok(())
}

fn check_directory(directory: &File, owner: u32, mode: u32) -> Result<()> {
    let metadata = directory.metadata()?;
    owned(&metadata, true, owner, false)?;
    if metadata.mode() & 0o7777 != mode {
        return Err(invalid(
            "unexpected persistent profile directory permissions",
        ));
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

fn invalid(message: &str) -> Error {
    Error::Network(message.into())
}

#[cfg(test)]
#[path = "network_linux_tests.rs"]
mod tests;
