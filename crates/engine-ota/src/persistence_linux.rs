//! Read-only local persistence evidence for a bounded health worker. This does
//! not import credentials, repair stores, contact a network or certify logins.
//! Keep the handles through the app probe, then recheck before accepting it.
use crate::owned_fs::{open_at, open_directory, owned, unchanged};
use crate::recovery_linux::{open_data_mount, opened_mount_id};
use crate::{Error, Result};
use std::fs::{File, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const MAX_KEY_BYTES: u64 = 64 * 1024;
const NETWORK: &str = "data/NetworkManager/system-connections";
const NETWORK_BIND: &str = "etc/NetworkManager/system-connections";
const BLUETOOTH: &str = "data/bluetooth";
const BLUETOOTH_BIND: &str = "var/lib/bluetooth";

struct Requirement {
    path: &'static str,
    mode: u32,
    directory: bool,
    empty: bool,
}

impl Requirement {
    const fn directory(path: &'static str, mode: u32) -> Self {
        Self {
            path,
            mode,
            directory: true,
            empty: true,
        }
    }

    const fn key(path: &'static str, mode: u32, empty: bool) -> Self {
        Self {
            path,
            mode,
            directory: false,
            empty,
        }
    }
}

const REQUIRED: &[Requirement] = &[
    Requirement::directory("data", 0o755),
    Requirement::directory("data/NetworkManager", 0o755),
    Requirement::directory(NETWORK, 0o700),
    Requirement::directory(NETWORK_BIND, 0o700),
    Requirement::directory(BLUETOOTH, 0o700),
    Requirement::directory(BLUETOOTH_BIND, 0o700),
    Requirement::directory("data/ssh", 0o700),
    Requirement::key("data/ssh/ssh_host_rsa_key", 0o600, false),
    Requirement::key("data/ssh/ssh_host_ecdsa_key", 0o600, false),
    Requirement::key("data/ssh/ssh_host_ed25519_key", 0o600, false),
    Requirement::directory("data/spacewars-access", 0o755),
    // Empty login files are intentional revocations, never a reason to import
    // old keys. The supervised bootstrap separately requires both login roles.
    Requirement::key("data/spacewars-access/root", 0o600, true),
    Requirement::key("data/spacewars-access/spacewars", 0o644, true),
];

struct Entry {
    file: File,
    metadata: Metadata,
    mount: u64,
}

impl Entry {
    fn read(file: File) -> Result<Self> {
        let metadata = file.metadata()?;
        let mount = opened_mount_id(&file)?;
        Ok(Self {
            file,
            metadata,
            mount,
        })
    }

    fn same_object(&self, other: &Self) -> bool {
        self.metadata.dev() == other.metadata.dev() && self.metadata.ino() == other.metadata.ino()
    }

    fn matches(&self, other: &Self) -> bool {
        self.mount == other.mount
            && if self.metadata.is_dir() {
                // Daemons may legitimately add profiles or pairing keys while
                // we observe. Directory timestamps are not a content snapshot.
                self.same_object(other)
                    && self.metadata.uid() == other.metadata.uid()
                    && self.metadata.gid() == other.metadata.gid()
                    && self.metadata.mode() == other.metadata.mode()
            } else {
                unchanged(&self.metadata, &other.metadata)
            }
    }
}

/// Structural health, not proof that a network is connected, keys authenticate,
/// or every saved byte is intact. The recovery adapter also checks the actual
/// disk layout and makes a temporary write as the kiosk user. Run inside its
/// existing bounded process worker; filesystem I/O may block.
pub struct PersistentStores {
    root: Entry,
    data: Entry,
    entries: Vec<Entry>,
}

impl PersistentStores {
    /// Fixed paths only. No lock is acquired: the recovery parent already owns
    /// it and this worker performs no persistent writes or initialization.
    pub fn open() -> Result<Self> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(invalid("the persistence health worker requires root"));
        }
        Self::capture(open_directory(Path::new("/"), 0)?, open_data_mount()?, 0)
    }

    pub fn check(&self) -> Result<()> {
        let current = Self::open()?;
        self.compare(&current)
    }

    fn capture(root: File, data: File, owner: u32) -> Result<Self> {
        let root = Entry::read(root)?;
        let data = Entry::read(data)?;
        let mut entries = Vec::with_capacity(REQUIRED.len());
        for requirement in REQUIRED {
            let entry = entry(&root.file, requirement, owner)?;
            if requirement.path == "data" || requirement.path.starts_with("data/") {
                if entry.mount != data.mount || entry.metadata.dev() != data.metadata.dev() {
                    return Err(at(
                        requirement.path,
                        "store is not on the persistent data mount",
                    ));
                }
                if requirement.path == "data" && !entry.same_object(&data) {
                    return Err(invalid("persistent data mount changed during observation"));
                }
            }
            entries.push(entry);
        }
        let stores = Self {
            root,
            data,
            entries,
        };
        for (source, target) in [(NETWORK, NETWORK_BIND), (BLUETOOTH, BLUETOOTH_BIND)] {
            let get = |path| {
                let index = REQUIRED.iter().position(|item| item.path == path).unwrap();
                &stores.entries[index]
            };
            let (source_entry, target_entry) = (get(source), get(target));
            if !source_entry.same_object(target_entry) || source_entry.mount == target_entry.mount {
                return Err(at(target, "bind does not refer to the preserved store"));
            }
        }
        Ok(stores)
    }

    fn compare(&self, current: &Self) -> Result<()> {
        if !self.root.matches(&current.root) || !self.data.matches(&current.data) {
            return Err(invalid(
                "root or data mount changed during health observation",
            ));
        }
        for ((requirement, before), after) in
            REQUIRED.iter().zip(&self.entries).zip(&current.entries)
        {
            let held = Entry::read(before.file.try_clone()?)?;
            if !before.matches(after) || !before.matches(&held) {
                return Err(at(
                    requirement.path,
                    "store changed during health observation",
                ));
            }
        }
        Ok(())
    }
}

fn entry(root: &File, requirement: &Requirement, owner: u32) -> Result<Entry> {
    let mut parent = root.try_clone()?;
    let components = requirement.path.split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let final_component = index + 1 == components.len();
        let directory = !final_component || requirement.directory;
        // O_PATH classifies keys without opening their contents or a substituted
        // device. O_NOFOLLOW is added by open_at, including on every ancestor.
        parent = open_at(
            &parent,
            component,
            libc::O_PATH | if directory { libc::O_DIRECTORY } else { 0 },
            0,
        )
        .map_err(|error| at(requirement.path, error))?;
        let metadata = parent.metadata()?;
        owned(&metadata, directory, owner, false).map_err(|error| at(requirement.path, error))?;
        if metadata.gid() != owner || metadata.mode() & 0o7000 != 0 {
            return Err(at(requirement.path, "unexpected ownership or special mode"));
        }
        if final_component
            && (metadata.mode() & 0o7777 != requirement.mode
                || (!directory
                    && (metadata.len() > MAX_KEY_BYTES
                        || (!requirement.empty && metadata.len() == 0))))
        {
            return Err(at(
                requirement.path,
                "unexpected key size or store permissions",
            ));
        }
    }
    Entry::read(parent)
}

fn at(path: &str, message: impl std::fmt::Display) -> Error {
    invalid(format!("/{path}: {message}"))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Persistence(message.into())
}

#[cfg(test)]
#[path = "persistence_linux_tests.rs"]
mod tests;
