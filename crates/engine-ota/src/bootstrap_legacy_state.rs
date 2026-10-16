//! Immutable original files for the supervised fallback-root migration.
//! Preparing this record writes only private /data files, never the live root.
use super::backup::{check_named_directory, directory, free_space, inventory, writable};
use super::legacy_files::{self, Snapshot};
use super::{BootstrapBinding, invalid};
use crate::owned_fs::{io_result, name, open_directory, read_file, unchanged};
use crate::{Blob, Error, Result, Sha256, verify_blob};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) const DIRECTORY: &str = "legacy-root";
pub(super) const PENDING: &str = ".legacy-root.pending";
const RECORD: &str = "record.json";
const RECORD_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyPreparationStage {
    Inspecting,
    Copying,
    Verifying,
    Publishing,
    Prepared,
    Reusing,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct LegacyPreparationProgress {
    pub stage: LegacyPreparationStage,
    pub files_copied: usize,
}

#[derive(Debug, Serialize)]
pub struct LegacyPreparationReport {
    pub schema: u32,
    pub scope: &'static str,
    pub receipt_sha256: Sha256,
    pub original_files: usize,
    pub original_bytes: u64,
    pub reused: bool,
    /// Only the fixed migration paths; this is not a new whole-data snapshot.
    pub original_files_match: bool,
    pub ready_for_write: bool,
}

#[derive(Debug, Serialize)]
pub struct LegacyExportCheck {
    pub schema: u32,
    pub scope: &'static str,
    pub receipt_sha256: Sha256,
    pub package_sha256: Sha256,
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub original_files: usize,
    pub original_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Replacement {
    pub(super) path: String,
    pub(super) mode: u32,
    pub(super) contents: Blob,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    pub(super) binding_sha256: Sha256,
    pub(super) raw_copies_sha256: Sha256,
    pub(super) data_copy_sha256: Sha256,
    /// Exactly the versioned fixed target list, with modes supplied by code.
    pub(super) replacements: Vec<Replacement>,
}

impl Inputs {
    fn validate(&self, binding: &BootstrapBinding) -> Result<()> {
        if self.binding_sha256 != Sha256::digest(&serde_json::to_vec(binding)?)
            || self.replacements.len() != legacy_files::targets().len()
            || self
                .replacements
                .iter()
                .zip(legacy_files::targets())
                .enumerate()
                .any(|(i, (replacement, (path, mode)))| {
                    replacement.path != path
                        || replacement.mode != mode
                        || !(1..=legacy_files::limit(i)).contains(&replacement.contents.bytes)
                })
        {
            return Err(invalid(
                "legacy originals differ from their fixed preparation inputs",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    inputs: Inputs,
    source: Snapshot,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    receipt: Receipt,
    checksum: Sha256,
}

pub fn verify_legacy_export(path: &Path, expected: Sha256) -> Result<LegacyExportCheck> {
    let owner = unsafe { libc::geteuid() };
    let active = open_directory(path, owner)?;
    let binding = super::state::read_binding(&active, owner)?;
    reject_pending(&active, owner)?;
    let saved = super::state::optional_directory(&active, DIRECTORY, owner)?
        .ok_or_else(|| invalid("missing exported legacy originals"))?;
    let receipt = read_receipt(&saved, owner, &binding)?;
    if digest(&receipt)? != expected {
        return Err(invalid(
            "legacy export differs from the independently selected receipt",
        ));
    }
    if verify(&saved, owner, &binding)? != receipt {
        return Err(invalid(
            "legacy originals changed during export verification",
        ));
    }
    check_named_directory(&active, DIRECTORY, &saved, owner)?;
    let named = open_directory(path, owner)?;
    if named.metadata()?.dev() != active.metadata()?.dev()
        || named.metadata()?.ino() != active.metadata()?.ino()
    {
        return Err(invalid("legacy export directory was replaced"));
    }
    Ok(LegacyExportCheck {
        schema: 1,
        scope: "verified original-file export; no live-state, restore or migration authority",
        receipt_sha256: expected,
        package_sha256: binding.package_sha256,
        device_id: binding.device_id,
        layout_id: binding.layout_id,
        original_files: receipt.source.files.iter().flatten().count(),
        original_bytes: receipt.source.bytes(),
    })
}

/// `check(true)` additionally requires the still-current full data backup.
/// Retrying an existing immutable record never recaptures later operator edits.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    root: &File,
    data: &File,
    parent: &File,
    owner: u32,
    binding: &BootstrapBinding,
    inputs: &Inputs,
    reserve: u64,
    mut check: impl FnMut(bool) -> Result<()>,
    progress: &mut impl FnMut(LegacyPreparationProgress) -> Result<()>,
) -> Result<LegacyPreparationReport> {
    inputs.validate(binding)?;
    let active = super::state::optional_directory(parent, super::state::ACTIVE, owner)?
        .ok_or_else(|| invalid("missing bootstrap reservation"))?;
    check(false)?;
    if let Some(saved) = super::state::optional_directory(&active, DIRECTORY, owner)? {
        reject_pending(&active, owner)?;
        progress(LegacyPreparationProgress {
            stage: LegacyPreparationStage::Reusing,
            files_copied: 0,
        })?;
        let receipt = verify(&saved, owner, binding)?;
        if &receipt.inputs != inputs {
            return Err(invalid(
                "legacy originals belong to different package or backup inputs",
            ));
        }
        let matches = receipt.source == legacy_files::inspect(root, owner)?;
        check_named_directory(&active, DIRECTORY, &saved, owner)?;
        check_named_directory(parent, super::state::ACTIVE, &active, owner)?;
        check(false)?;
        return report(receipt, true, matches);
    }
    check(true)?;
    progress(LegacyPreparationProgress {
        stage: LegacyPreparationStage::Inspecting,
        files_copied: 0,
    })?;
    let pending = directory(&active, PENDING, owner)?;
    let names = names();
    inventory(
        &pending,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    // Check every unpublished object before opening or truncating any of them.
    for (index, component) in names.iter().enumerate() {
        let bound = if component == RECORD {
            RECORD_LIMIT
        } else {
            legacy_files::limit(index)
        };
        match super::device::private_asset(&pending, component, owner) {
            Ok(file) if file.metadata()?.len() <= bound => (),
            Ok(_) => return Err(invalid("oversized unpublished legacy original")),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    let mut files = names
        .iter()
        .map(|component| writable(&pending, component, owner))
        .collect::<Result<Vec<_>>>()?;
    check(true)?;
    for file in &files {
        file.set_len(0)?;
        file.sync_all()?;
    }
    let before = legacy_files::inspect(root, owner)?;
    let required = before
        .bytes()
        .checked_add(RECORD_LIMIT)
        .and_then(|n| n.checked_add(reserve))
        .ok_or_else(|| invalid("legacy preservation capacity overflow"))?;
    if required > free_space(data)? {
        return Err(invalid(
            "insufficient space for legacy originals and recovery reserve",
        ));
    }
    let captured = legacy_files::capture(root, owner, |index| {
        progress(LegacyPreparationProgress {
            stage: LegacyPreparationStage::Copying,
            files_copied: index,
        })?;
        check(false)?;
        Ok(files[index].try_clone()?)
    })?;
    for file in &files {
        file.sync_all()?;
    }
    progress(LegacyPreparationProgress {
        stage: LegacyPreparationStage::Verifying,
        files_copied: before.files.len(),
    })?;
    if captured != before || legacy_files::inspect(root, owner)? != before {
        return Err(invalid("legacy source changed while preserving originals"));
    }
    let receipt = Receipt {
        schema: 1,
        inputs: inputs.clone(),
        source: before,
    };
    let bytes = serde_json::to_vec(&Envelope {
        checksum: digest(&receipt)?,
        receipt: receipt.clone(),
    })?;
    if bytes.len() as u64 > RECORD_LIMIT {
        return Err(invalid("legacy original record exceeds its bound"));
    }
    let record = files.last_mut().expect("record file");
    record.write_all(&bytes)?;
    record.sync_all()?;
    pending.sync_all()?;
    if verify(&pending, owner, binding)? != receipt {
        return Err(invalid("legacy originals changed before publication"));
    }
    progress(LegacyPreparationProgress {
        stage: LegacyPreparationStage::Publishing,
        files_copied: receipt.source.files.len(),
    })?;
    check_named_directory(&active, PENDING, &pending, owner)?;
    check_named_directory(parent, super::state::ACTIVE, &active, owner)?;
    check(true)?;
    if legacy_files::inspect(root, owner)? != receipt.source {
        return Err(invalid("legacy source changed before publication"));
    }
    let from = name(PENDING)?;
    let to = name(DIRECTORY)?;
    // SAFETY: live private directory and fixed C names, never replace an existing record.
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
    progress(LegacyPreparationProgress {
        stage: LegacyPreparationStage::Prepared,
        files_copied: receipt.source.files.len(),
    })?;
    report(receipt, false, true)
}

fn names() -> Vec<String> {
    (0..legacy_files::targets().len())
        .map(|index| format!("original-{index:02}.bin"))
        .chain(std::iter::once(RECORD.to_owned()))
        .collect()
}

fn reject_pending(active: &File, owner: u32) -> Result<()> {
    if super::state::optional_directory(active, PENDING, owner)?.is_some() {
        return Err(invalid("completed and pending legacy originals conflict"));
    }
    Ok(())
}

fn digest(receipt: &Receipt) -> Result<Sha256> {
    Ok(Sha256::digest(&serde_json::to_vec(receipt)?))
}

fn read_receipt(saved: &File, owner: u32, binding: &BootstrapBinding) -> Result<Receipt> {
    let names = names();
    inventory(saved, &names.iter().map(String::as_str).collect::<Vec<_>>())?;
    let bytes = read_file(
        super::device::private_asset(saved, RECORD, owner)?,
        owner,
        RECORD_LIMIT,
    )?;
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    let receipt = envelope.receipt;
    if receipt.schema != 1 || envelope.checksum != digest(&receipt)? {
        return Err(invalid("invalid legacy original receipt"));
    }
    receipt.inputs.validate(binding)?;
    receipt.source.validate()?;
    Ok(receipt)
}

fn verify(saved: &File, owner: u32, binding: &BootstrapBinding) -> Result<Receipt> {
    let receipt = read_receipt(saved, owner, binding)?;
    for (index, component) in names().iter().take(receipt.source.files.len()).enumerate() {
        let mut file = super::device::private_asset(saved, component, owner)?;
        let before = file.metadata()?;
        let empty = Blob {
            bytes: 0,
            sha256: Sha256::digest(b""),
        };
        let expected = receipt.source.files[index]
            .as_ref()
            .map_or(&empty, |file| &file.contents);
        if before.len() != expected.bytes {
            return Err(invalid("legacy original length differs from its record"));
        }
        verify_blob(&mut file, expected)?;
        let named = super::device::private_asset(saved, component, owner)?;
        if !unchanged(&before, &file.metadata()?) || !unchanged(&before, &named.metadata()?) {
            return Err(invalid("legacy original changed during verification"));
        }
    }
    Ok(receipt)
}

fn report(receipt: Receipt, reused: bool, matches: bool) -> Result<LegacyPreparationReport> {
    Ok(LegacyPreparationReport {
        schema: 1,
        scope: "immutable original files only; independent export, restartable replacements and effective privilege/service checks remain pending",
        receipt_sha256: digest(&receipt)?,
        original_files: receipt.source.files.iter().flatten().count(),
        original_bytes: receipt.source.bytes(),
        reused,
        original_files_match: matches,
        ready_for_write: false,
    })
}

#[cfg(test)]
#[path = "bootstrap_legacy_state_tests.rs"]
mod tests;
