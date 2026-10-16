//! Durable private data/configuration recovery copy for supervised bootstrap.
use super::archive::{self, MAX_ARCHIVE_BYTES};
use super::backup::{check_named_directory, directory, free_space, inventory, writable};
use super::{BootstrapBinding, invalid};
use crate::owned_fs::{io_result, name, open_directory, read_file, unchanged};
use crate::{Blob, Result, Sha256, verify_blob};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) const DIRECTORY: &str = "data-copy";
pub(super) const PENDING: &str = ".data-copy.pending";
const ARCHIVE: &str = "archive.tar";
const RECORD: &str = "record.json";
const RECORD_LIMIT: u64 = 4096;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataBackupStage {
    Inspecting,
    Copying,
    CheckingSource,
    CheckingCopy,
    Reusing,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DataBackupProgress {
    pub stage: DataBackupStage,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataBackupReceipt {
    pub schema: u32,
    pub selection_version: u32,
    pub binding_sha256: Sha256,
    pub source_sha256: Sha256,
    pub entries: u64,
    pub payload_bytes: u64,
    pub archive: Blob,
}

#[derive(Debug, Serialize)]
pub struct DataBackupReport {
    pub schema: u32,
    pub scope: &'static str,
    pub receipt: DataBackupReceipt,
    pub receipt_sha256: Sha256,
    pub reused: bool,
    /// A retained original backup remains useful after intentional migration
    /// edits. Reusing it must not claim it captures newer source state.
    pub source_matches: bool,
    pub ready_for_write: bool,
}

#[derive(Debug, Serialize)]
pub struct DataBackupExportCheck {
    pub schema: u32,
    pub scope: &'static str,
    pub receipt_sha256: Sha256,
    pub package_sha256: Sha256,
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub archive_bytes: u64,
    pub entries: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    receipt: DataBackupReceipt,
    checksum: Sha256,
}

pub fn verify_data_backup_export(path: &Path, expected: Sha256) -> Result<DataBackupExportCheck> {
    let owner = unsafe { libc::geteuid() };
    let active = open_directory(path, owner)?;
    let binding = super::state::read_binding(&active, owner)?;
    let receipt = verified_receipt(&active, owner, &binding, expected)?;
    let named = open_directory(path, owner)?;
    if named.metadata()?.dev() != active.metadata()?.dev()
        || named.metadata()?.ino() != active.metadata()?.ino()
    {
        return Err(invalid("data export directory was replaced"));
    }
    Ok(DataBackupExportCheck {
        schema: 1,
        scope: "verified data/configuration archive export; no extraction, live-state or restore authority",
        receipt_sha256: expected,
        package_sha256: binding.package_sha256,
        device_id: binding.device_id,
        layout_id: binding.layout_id,
        archive_bytes: receipt.archive.bytes,
        entries: receipt.entries,
    })
}

pub(super) fn verified_receipt(
    active: &File,
    owner: u32,
    binding: &BootstrapBinding,
    expected: Sha256,
) -> Result<DataBackupReceipt> {
    if super::state::optional_directory(active, PENDING, owner)?.is_some() {
        return Err(invalid("unpublished data copy in exported backup"));
    }
    let completed = super::state::optional_directory(active, DIRECTORY, owner)?
        .ok_or_else(|| invalid("missing exported data/configuration copy"))?;
    let receipt = read_receipt(&completed, owner, binding)?;
    // The export's self-checksum is not an independent source of authority.
    if receipt_digest(&receipt)? != expected {
        return Err(invalid(
            "data export differs from the authenticated device receipt",
        ));
    }
    if verify_copy(&completed, owner, binding)? != receipt {
        return Err(invalid("data export changed during verification"));
    }
    check_named_directory(active, DIRECTORY, &completed, owner)?;
    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create(
    root: &File,
    data: &File,
    parent: &File,
    owner: u32,
    binding: &BootstrapBinding,
    reserve: u64,
    mut check: impl FnMut() -> Result<()>,
    report: &mut impl FnMut(DataBackupProgress),
) -> Result<DataBackupReport> {
    let active = super::state::optional_directory(parent, super::state::ACTIVE, owner)?
        .ok_or_else(|| invalid("missing bootstrap reservation"))?;
    check()?;
    report(DataBackupProgress {
        stage: DataBackupStage::Inspecting,
        bytes: 0,
    });
    if let Some(completed) = super::state::optional_directory(&active, DIRECTORY, owner)? {
        if super::state::optional_directory(&active, PENDING, owner)?.is_some() {
            return Err(invalid("completed and pending data copies conflict"));
        }
        report(DataBackupProgress {
            stage: DataBackupStage::Reusing,
            bytes: 0,
        });
        let receipt = verify_copy(&completed, owner, binding)?;
        let current = archive::inspect(root, data)?;
        check_named_directory(&active, DIRECTORY, &completed, owner)?;
        check_named_directory(parent, super::state::ACTIVE, &active, owner)?;
        check()?;
        let source_matches = current.source_sha256 == receipt.source_sha256;
        return finish(receipt, true, source_matches);
    }
    let pending = directory(&active, PENDING, owner)?;
    inventory(&pending, &[ARCHIVE, RECORD])?;
    // Validate both unpublished objects before truncating either. No completed
    // archive is ever replaced, even when the original source has since changed.
    for (component, limit) in [(ARCHIVE, MAX_ARCHIVE_BYTES), (RECORD, RECORD_LIMIT)] {
        match super::device::private_asset(&pending, component, owner) {
            Ok(file) if file.metadata()?.len() <= limit => (),
            Ok(_) => return Err(invalid("oversized unpublished data copy")),
            Err(crate::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    let mut output = writable(&pending, ARCHIVE, owner)?;
    let mut record = writable(&pending, RECORD, owner)?;
    check()?;
    output.set_len(0)?;
    record.set_len(0)?;
    output.sync_all()?;
    record.sync_all()?;
    let observed = archive::inspect(root, data)?;
    let required = observed
        .archive_upper_bound
        .checked_add(reserve)
        .ok_or_else(|| invalid("data archive capacity overflow"))?;
    if required > free_space(data)? {
        return Err(invalid(
            "insufficient data space for archive and remaining recovery reserve",
        ));
    }
    check()?;
    report(DataBackupProgress {
        stage: DataBackupStage::Copying,
        bytes: 0,
    });
    let (image, copied) = archive::capture(
        root,
        data,
        &mut output,
        observed.archive_upper_bound,
        &mut |bytes| {
            report(DataBackupProgress {
                stage: DataBackupStage::Copying,
                bytes,
            });
        },
    )?;
    output.sync_all()?;
    report(DataBackupProgress {
        stage: DataBackupStage::CheckingSource,
        bytes: 0,
    });
    let (again, inspected_again) = archive::capture(
        root,
        data,
        std::io::sink(),
        observed.archive_upper_bound,
        &mut |bytes| {
            report(DataBackupProgress {
                stage: DataBackupStage::CheckingSource,
                bytes,
            });
        },
    )?;
    if copied != observed
        || inspected_again != observed
        || image != again
        || archive::inspect(root, data)? != observed
    {
        return Err(invalid(
            "data/configuration source changed during preservation",
        ));
    }
    report(DataBackupProgress {
        stage: DataBackupStage::CheckingCopy,
        bytes: 0,
    });
    output.rewind()?;
    verify_blob(&mut output, &image)?;
    let receipt = DataBackupReceipt {
        schema: 1,
        selection_version: 1,
        binding_sha256: Sha256::digest(&serde_json::to_vec(binding)?),
        source_sha256: observed.source_sha256,
        entries: observed.entries,
        payload_bytes: observed.payload_bytes,
        archive: image,
    };
    let encoded = serde_json::to_vec(&Envelope {
        checksum: receipt_digest(&receipt)?,
        receipt: receipt.clone(),
    })?;
    if encoded.len() as u64 > RECORD_LIMIT {
        return Err(invalid("data backup receipt exceeds its bound"));
    }
    record.write_all(&encoded)?;
    record.sync_all()?;
    pending.sync_all()?;
    verify_copy(&pending, owner, binding)?;
    check_named_directory(&active, PENDING, &pending, owner)?;
    check_named_directory(parent, super::state::ACTIVE, &active, owner)?;
    check()?;
    let from = name(PENDING)?;
    let to = name(DIRECTORY)?;
    // SAFETY: live private directory and fixed C names; publication is no-replace.
    io_result(unsafe {
        libc::renameat2(
            active.as_raw_fd(),
            from.as_ptr(),
            active.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })?;
    active.sync_all()?;
    check_named_directory(&active, DIRECTORY, &pending, owner)?;
    finish(receipt, false, true)
}

fn finish(
    receipt: DataBackupReceipt,
    reused: bool,
    source_matches: bool,
) -> Result<DataBackupReport> {
    Ok(DataBackupReport {
        schema: 1,
        scope: "verified original data/configuration recovery archive; independent export, legacy closure, installation and physical restore qualification remain pending",
        receipt_sha256: receipt_digest(&receipt)?,
        receipt,
        reused,
        source_matches,
        ready_for_write: false,
    })
}

fn receipt_digest(receipt: &DataBackupReceipt) -> Result<Sha256> {
    Ok(Sha256::digest(&serde_json::to_vec(receipt)?))
}

fn read_receipt(
    directory: &File,
    owner: u32,
    binding: &BootstrapBinding,
) -> Result<DataBackupReceipt> {
    inventory(directory, &[ARCHIVE, RECORD])?;
    let file = super::device::private_asset(directory, RECORD, owner)?;
    let envelope: Envelope = serde_json::from_slice(&read_file(file, owner, RECORD_LIMIT)?)?;
    let receipt = envelope.receipt;
    if receipt.schema != 1
        || receipt.selection_version != 1
        || receipt.entries == 0
        || receipt.entries > 100_000
        || receipt.archive.bytes < 1024
        || receipt.archive.bytes > MAX_ARCHIVE_BYTES
        || !receipt.archive.bytes.is_multiple_of(512)
        || receipt.payload_bytes > receipt.archive.bytes
        || receipt.binding_sha256 != Sha256::digest(&serde_json::to_vec(binding)?)
        || envelope.checksum != receipt_digest(&receipt)?
    {
        return Err(invalid("data backup receipt differs from the preparation"));
    }
    Ok(receipt)
}

fn verify_copy(
    directory: &File,
    owner: u32,
    binding: &BootstrapBinding,
) -> Result<DataBackupReceipt> {
    let receipt = read_receipt(directory, owner, binding)?;
    let mut archive = super::device::private_asset(directory, ARCHIVE, owner)?;
    let before = archive.metadata()?;
    if before.len() != receipt.archive.bytes {
        return Err(invalid("data archive length differs from its receipt"));
    }
    verify_blob(&mut archive, &receipt.archive)?;
    let named = super::device::private_asset(directory, ARCHIVE, owner)?;
    if !unchanged(&before, &archive.metadata()?) || !unchanged(&before, &named.metadata()?) {
        return Err(invalid("data archive changed during verification"));
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "bootstrap_data_backup_tests.rs"]
mod tests;
