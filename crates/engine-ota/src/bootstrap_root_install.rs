//! Restartable inactive-root copy for the separately authenticated bootstrap.
//! Its journal is not an OTA recovery journal and cannot select or authorize a boot.
use super::{backup, invalid};
use crate::install_linux::{Target, WriteProgress, check_written_ext4, write_checked_ext4_guarded};
use crate::owned_fs::{io_result, name, read_file};
use crate::{Blob, Error, ImageIdentity, Result, Sha256, verify_blob};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;

pub(super) const DIRECTORY: &str = "root-install";
const RECORD: &str = "record.json";
const PENDING: &str = ".record.pending";
const LIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    pub binding_sha256: Sha256,
    pub package_sha256: Sha256,
    pub raw_copies_sha256: Sha256,
    pub data_receipt_sha256: Sha256,
    pub originals_sha256: Sha256,
    pub replacement_journal_sha256: Sha256,
    pub original_inactive: Blob,
    pub compressed: Blob,
    pub expanded: Blob,
    pub image: ImageIdentity,
    pub target: crate::recovery::Slot,
}

impl Inputs {
    fn validate(&self) -> Result<()> {
        self.image.validate()?;
        if self.image.source.dirty
            || self.compressed.bytes == 0
            || self.compressed.bytes > crate::MAX_COMPRESSED_BYTES
            || self.expanded.bytes < 4096
            || self.expanded.bytes > self.original_inactive.bytes
            || self.original_inactive.bytes > super::package::MAX_ROOT_BYTES
        {
            return Err(invalid("invalid bootstrap root-install inputs"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Writing,
    Written,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    inputs: Inputs,
    phase: Phase,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: Record,
    checksum: Sha256,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "stage", content = "progress", rename_all = "kebab-case")]
pub enum RootInstallProgress {
    CheckingPrerequisites,
    CheckingRecoveryCopies,
    CheckingPayload,
    CheckingOriginal,
    WritingRecorded,
    Image(WriteProgress),
    WrittenRecorded,
}

#[derive(Debug, Serialize)]
pub struct RootInstallReport {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub journal_sha256: Sha256,
    pub target: crate::recovery::Slot,
    pub expanded: Blob,
    pub reused_written: bool,
    pub ready_for_trial: bool,
}

struct Journal<'a> {
    active: &'a File,
    directory: File,
    owner: u32,
    persisted: Option<Vec<u8>>,
    record: Record,
}

impl<'a> Journal<'a> {
    fn open(active: &'a File, owner: u32, inputs: &Inputs) -> Result<Self> {
        inputs.validate()?;
        let directory = backup::directory(active, DIRECTORY, owner)?;
        // SAFETY: this independently opened directory stays alive through commit.
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
            let error = std::io::Error::last_os_error();
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                Error::Busy
            } else {
                error.into()
            });
        }
        let persisted = optional_record(&directory, RECORD, owner)?;
        let record = if let Some(bytes) = &persisted {
            let envelope: Envelope = serde_json::from_slice(bytes)?;
            if envelope.record.schema != 1
                || envelope.record.inputs != *inputs
                || envelope.checksum != Sha256::digest(&serde_json::to_vec(&envelope.record)?)
            {
                return Err(invalid(
                    "root-install journal belongs to different or corrupt inputs",
                ));
            }
            envelope.record
        } else {
            Record {
                schema: 1,
                inputs: inputs.clone(),
                phase: Phase::Writing,
            }
        };
        let journal = Self {
            active,
            directory,
            owner,
            persisted,
            record,
        };
        journal.check()?;
        Ok(journal)
    }

    fn check(&self) -> Result<()> {
        backup::check_named_directory(self.active, DIRECTORY, &self.directory, self.owner)?;
        if crate::recovery_linux::opened_mount_id(self.active)?
            != crate::recovery_linux::opened_mount_id(&self.directory)?
        {
            return Err(invalid("root-install journal crossed mounts"));
        }
        backup::inventory(&self.directory, &[RECORD, PENDING])?;
        if optional_record(&self.directory, RECORD, self.owner)? != self.persisted {
            return Err(invalid("root-install journal changed"));
        }
        optional_record(&self.directory, PENDING, self.owner)?;
        Ok(())
    }

    fn persist(&mut self) -> Result<()> {
        self.check()?;
        let bytes = serde_json::to_vec(&Envelope {
            record: self.record.clone(),
            checksum: Sha256::digest(&serde_json::to_vec(&self.record)?),
        })?;
        if bytes.len() as u64 > LIMIT || backup::free_space(&self.directory)? < 2 * LIMIT {
            return Err(invalid("insufficient root-install journal capacity"));
        }
        let mut pending = backup::writable(&self.directory, PENDING, self.owner)?;
        pending.set_len(0)?;
        pending.write_all(&bytes)?;
        pending.sync_all()?;
        self.check()?;
        let from = name(PENDING)?;
        let to = name(RECORD)?;
        // SAFETY: fixed private names and the held directory remain live.
        io_result(unsafe {
            libc::renameat2(
                self.directory.as_raw_fd(),
                from.as_ptr(),
                self.directory.as_raw_fd(),
                to.as_ptr(),
                if self.persisted.is_none() {
                    libc::RENAME_NOREPLACE
                } else {
                    0
                },
            )
        })?;
        self.directory.sync_all()?;
        self.persisted = Some(bytes);
        self.check()
    }
}

fn optional_record(directory: &File, component: &str, owner: u32) -> Result<Option<Vec<u8>>> {
    match super::device::private_asset(directory, component, owner) {
        Ok(file) => {
            if crate::recovery_linux::opened_mount_id(&file)?
                != crate::recovery_linux::opened_mount_id(directory)?
            {
                return Err(invalid("root-install record crossed mounts"));
            }
            Ok(Some(read_file(file, owner, LIMIT)?))
        }
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Observe only a completed copy; this cannot initialize, resume or repair an
/// install. The staging caller separately rehashes the exclusively held target.
pub(super) fn completed(active: &File, owner: u32, inputs: &Inputs) -> Result<Sha256> {
    inputs.validate()?;
    let (actual, hash) = read_completed(active, owner)?;
    if actual != *inputs {
        return Err(invalid(
            "completed root installation belongs to different inputs",
        ));
    }
    Ok(hash)
}

/// Read only a published completion. Callers must compare its digest with the
/// operator-selected receipt and revalidate every input against the device.
pub(super) fn read_completed(active: &File, owner: u32) -> Result<(Inputs, Sha256)> {
    let directory = super::state::optional_directory(active, DIRECTORY, owner)?
        .ok_or_else(|| invalid("bootstrap root installation is missing"))?;
    backup::inventory(&directory, &[RECORD, PENDING])?;
    if crate::recovery_linux::opened_mount_id(&directory)?
        != crate::recovery_linux::opened_mount_id(active)?
    {
        return Err(invalid("root-install journal crossed mounts"));
    }
    let bytes = optional_record(&directory, RECORD, owner)?
        .ok_or_else(|| invalid("bootstrap root installation is incomplete"))?;
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    let hash = Sha256::digest(&serde_json::to_vec(&envelope.record)?);
    if envelope.record.schema != 1
        || envelope.record.phase != Phase::Written
        || envelope.checksum != hash
    {
        return Err(invalid(
            "bootstrap root installation is not the selected completed copy",
        ));
    }
    envelope.record.inputs.validate()?;
    optional_record(&directory, PENDING, owner)?;
    backup::check_named_directory(active, DIRECTORY, &directory, owner)?;
    if optional_record(&directory, RECORD, owner)?.as_deref() != Some(bytes.as_slice()) {
        return Err(invalid("completed root-install record changed"));
    }
    Ok((envelope.record.inputs, hash))
}

/// The caller supplies checked package/backup identities, holds the installation
/// lock and excludes other writers. Only the device wrapper can open a raw target.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply<T: Target>(
    active: &File,
    owner: u32,
    inputs: &Inputs,
    source: &mut (impl Read + Seek),
    open_target: impl FnOnce() -> Result<T>,
    mut guard: impl FnMut(Option<&File>) -> Result<()>,
    mut inspect: impl FnMut(&File) -> Result<ImageIdentity>,
    report: &mut impl FnMut(RootInstallProgress) -> Result<()>,
) -> Result<RootInstallReport> {
    inputs.validate()?;
    guard(None)?;
    report(RootInstallProgress::CheckingPayload)?;
    source.rewind()?;
    verify_blob(source, &inputs.compressed)?;
    guard(None)?;
    let mut journal = Journal::open(active, owner, inputs)?;
    let reused_written = journal.record.phase == Phase::Written;
    let mut target = open_target()?;
    if target.capacity() != inputs.original_inactive.bytes {
        return Err(invalid("root-install target capacity changed"));
    }
    // The clone retains the exclusive kernel claim during progress callbacks.
    let held = target.file().try_clone()?;
    guard(Some(&held))?;
    if journal.persisted.is_none() {
        report(RootInstallProgress::CheckingOriginal)?;
        target.rewind()?;
        verify_blob(
            &mut (&mut target).take(inputs.original_inactive.bytes),
            &inputs.original_inactive,
        )?;
        guard(Some(&held))?;
        journal.persist()?;
    }
    if !reused_written {
        report(RootInstallProgress::WritingRecorded)?;
    }
    let mut last_stage = None;
    let mut progress = |event: WriteProgress| -> Result<()> {
        journal.check()?;
        if last_stage != Some(event.stage) {
            guard(Some(&held))?;
            last_stage = Some(event.stage);
        }
        report(RootInstallProgress::Image(event))
    };
    let check_image = |file: &File| -> Result<()> {
        if inspect(file)? != inputs.image {
            return Err(invalid(
                "written bootstrap image identity differs from the selected package",
            ));
        }
        Ok(())
    };
    if reused_written {
        target.flush_to_device()?;
        check_written_ext4(&inputs.expanded, &mut target, check_image, &mut progress)?;
    } else {
        write_checked_ext4_guarded(
            &inputs.compressed,
            &inputs.expanded,
            source,
            &mut target,
            check_image,
            &mut progress,
        )?;
    }
    guard(Some(&held))?;
    journal.check()?;
    if !reused_written {
        journal.record.phase = Phase::Written;
        journal.persist()?;
    }
    report(RootInstallProgress::WrittenRecorded)?;
    guard(Some(&held))?;
    journal.check()?;
    Ok(RootInstallReport {
        schema: 1,
        scope: "verified inactive-root copy only; original boot selection retained; no login, complete privilege audit, watchdog or boot authority",
        package_sha256: inputs.package_sha256,
        journal_sha256: Sha256::digest(&serde_json::to_vec(&journal.record)?),
        target: inputs.target,
        expanded: inputs.expanded.clone(),
        reused_written,
        ready_for_trial: false,
    })
}

#[cfg(test)]
#[path = "bootstrap_root_install_tests.rs"]
mod tests;
