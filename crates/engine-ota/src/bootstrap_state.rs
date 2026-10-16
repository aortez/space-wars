//! A durable reservation for the supervised migration. This record is not a
//! recovery journal or permission to write; backups and login checks are pending.
use super::{LegacyBootEvidence, PackageCheck, invalid};
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, owned, read_file};
use crate::recovery::Slot;
use crate::{Blob, Error, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

pub(super) const ACTIVE: &str = "active";
const RECORD: &str = "binding.json";
const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapBinding {
    pub schema: u32,
    pub package_sha256: Sha256,
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub original_boot_id: String,
    pub original_slot: Slot,
    pub target: Slot,
    pub kernel_build_id: String,
    pub hardware_profile: String,
    pub boot_partition_bytes: u64,
    pub inactive_partition_bytes: u64,
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub original_boot_files: BTreeMap<String, Blob>,
}

#[derive(Debug, Serialize)]
pub struct PreparationReport {
    pub check: PackageCheck,
    pub binding: BootstrapBinding,
    pub resumed: bool,
    pub preserved_access: Option<crate::access_linux::AccessStatus>,
    pub scope: &'static str,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    binding: BootstrapBinding,
    checksum: Sha256,
}

impl BootstrapBinding {
    pub(super) fn observed(
        package_sha256: Sha256,
        target: Slot,
        legacy: &LegacyBootEvidence,
    ) -> Result<Self> {
        let bytes = |sectors: u64| {
            sectors
                .checked_mul(512)
                .ok_or_else(|| invalid("bootstrap partition size overflow"))
        };
        let binding = Self {
            schema: 1,
            package_sha256,
            device_id: legacy.physical.fingerprint(),
            layout_id: legacy.layout.layout_id,
            original_boot_id: legacy.boot_id.clone(),
            original_slot: legacy.layout.active,
            target,
            kernel_build_id: legacy.kernel_build_id.clone(),
            hardware_profile: legacy.hardware_profile.clone(),
            boot_partition_bytes: bytes(legacy.layout.partitions[0].sectors)?,
            inactive_partition_bytes: bytes(legacy.layout.inactive_partition().sectors)?,
            original_boot_files: legacy.boot_files.clone(),
        };
        binding.validate()?;
        Ok(binding)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !crate::recovery::boot_id(&self.original_boot_id)
            || self.original_slot.other() != self.target
            || self.kernel_build_id.len() != 40
            || !self
                .kernel_build_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !matches!(
                self.hardware_profile.as_str(),
                "hdmi" | "hyperpixel" | "picade"
            )
            || self.boot_partition_bytes == 0
            || self.inactive_partition_bytes == 0
            || self.boot_partition_bytes > super::package::MAX_ROOT_BYTES
            || self.inactive_partition_bytes > super::package::MAX_ROOT_BYTES
            || self.original_boot_files.is_empty()
            || self.original_boot_files.len() > 1024
            || self
                .original_boot_files
                .keys()
                .any(|p| !crate::boot::asset_path(p))
            || !["config.txt", "cmdline.txt", "spacewars-device.txt"]
                .iter()
                .all(|p| self.original_boot_files.contains_key(*p))
        {
            return Err(invalid("invalid bootstrap preparation binding"));
        }
        Ok(())
    }

    fn matches(&self, current: &Self) -> Result<()> {
        self.validate()?;
        current.validate()?;
        // An ordinary reboot before installation is allowed. Preserve the
        // first observed boot ID, while requiring all physical/boot inputs.
        let mut comparison = current.clone();
        comparison
            .original_boot_id
            .clone_from(&self.original_boot_id);
        if self != &comparison {
            return Err(invalid(
                "bootstrap preparation belongs to different inputs; reconciliation required",
            ));
        }
        Ok(())
    }
}

/// Fixed-path guard for operator mutations, called with the shared lock held.
/// Presence alone blocks maintenance, including corrupt or incomplete state.
/// A package awaiting preparation does not yet reserve the device.
pub fn require_no_active_bootstrap(lock: &InstallLock) -> Result<()> {
    lock.check()?;
    let data = crate::recovery_linux::open_data_mount()?;
    require_no_active_under(&data, 0)
}

pub(crate) fn require_no_active_under(data: &File, owner: u32) -> Result<()> {
    let Some(parent) = optional_directory(data, super::device::DIRECTORY, owner)? else {
        return Ok(());
    };
    match open_at(&parent, ACTIVE, libc::O_PATH, 0) {
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(invalid(
            "supervised bootstrap is active; resume/reconcile it before maintenance",
        )),
    }
}

pub(super) fn existing(
    parent: &File,
    owner: u32,
    current: &BootstrapBinding,
) -> Result<Option<BootstrapBinding>> {
    let Some(active) = optional_directory(parent, ACTIVE, owner)? else {
        return Ok(None);
    };
    let binding = read_binding(&active, owner)?;
    binding.matches(current)?;
    Ok(Some(binding))
}

pub(super) fn read_binding(active: &File, owner: u32) -> Result<BootstrapBinding> {
    owned(&active.metadata()?, true, owner, true)?;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", active.as_raw_fd()))? {
        let name = entry?.file_name();
        if name != RECORD
            && name != super::backup::DIRECTORY
            && name != super::data_backup::DIRECTORY
            && name != super::data_backup::PENDING
        {
            return Err(invalid(
                "unexpected bootstrap preparation state; reconciliation required",
            ));
        }
    }
    let file = super::device::private_asset(active, RECORD, owner)?;
    let bytes = read_file(file, owner, MAX_BYTES)?;
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    if Sha256::digest(&serde_json::to_vec(&envelope.binding)?) != envelope.checksum {
        return Err(invalid("bootstrap preparation checksum mismatch"));
    }
    envelope.binding.validate()?;
    Ok(envelope.binding)
}

/// A failed initialization deliberately leaves its reservation in place. Never
/// remove an incomplete attempt automatically, or reinterpret it as fresh.
pub(super) fn begin(
    parent: &File,
    owner: u32,
    current: &BootstrapBinding,
) -> Result<(BootstrapBinding, bool)> {
    owned(&parent.metadata()?, true, owner, true)?;
    let resumed = existing(parent, owner, current)?.is_some();
    if !resumed {
        current.validate()?;
        let bytes = serde_json::to_vec(&Envelope {
            binding: current.clone(),
            checksum: Sha256::digest(&serde_json::to_vec(current)?),
        })?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("bootstrap preparation exceeds its size bound"));
        }
        let component = name(ACTIVE)?;
        // SAFETY: the held private parent and fixed C component remain live.
        io_result(unsafe { libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o700) })?;
        parent.sync_all()?;
        let active = optional_directory(parent, ACTIVE, owner)?
            .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
        let mut file = open_at(
            &active,
            RECORD,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        active.sync_all()?;
    }
    let binding = existing(parent, owner, current)?
        .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
    let active = optional_directory(parent, ACTIVE, owner)?
        .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
    super::device::private_asset(&active, RECORD, owner)?.sync_all()?;
    active.sync_all()?;
    parent.sync_all()?;
    Ok((binding, resumed))
}

pub(super) fn optional_directory(
    parent: &File,
    component: &str,
    owner: u32,
) -> Result<Option<File>> {
    let directory = match open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(directory) => directory,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    owned(&directory.metadata()?, true, owner, true)?;
    if directory.metadata()?.dev() != parent.metadata()?.dev() {
        return Err(invalid("bootstrap state crossed filesystems"));
    }
    Ok(Some(directory))
}

#[cfg(test)]
#[path = "bootstrap_state_tests.rs"]
mod tests;
