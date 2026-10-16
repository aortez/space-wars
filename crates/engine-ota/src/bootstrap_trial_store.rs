//! Private first-boot journal. Opening never initializes an absent attempt.
//! No state transition/action escapes until file and directory flush complete.
#[cfg(test)]
use super::trial::TrialAction;
use super::trial::TrialRecord;
use super::{backup, device, invalid, state};
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file};
use crate::{Error, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) const DIRECTORY: &str = "boot-trial";
pub(super) const PREPARING: &str = ".boot-trial.pending";
const RECORD: &str = "record.json";
const PENDING: &str = ".record.pending";
const LIMIT: u64 = 512 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: TrialRecord,
    checksum: Sha256,
}

pub struct TrialStore {
    root: File,
    data: File,
    parent: File,
    active: File,
    directory: File,
    owner: u32,
    bytes: Vec<u8>,
    record: TrialRecord,
    uncertain: bool,
    #[cfg(test)]
    pub(super) fault: Option<&'static str>,
}

impl TrialStore {
    pub fn open(lock: &InstallLock) -> Result<Option<Self>> {
        lock.check()?;
        Self::under(
            open_directory(Path::new("/"), 0)?,
            crate::recovery_linux::open_data_mount()?,
            0,
        )
    }

    /// A staging operator may find no trial directory yet; the runtime path
    /// still rejects that incomplete reservation rather than initializing it.
    pub(super) fn for_staging(lock: &InstallLock) -> Result<Option<Self>> {
        lock.check()?;
        Self::under_mode(
            open_directory(Path::new("/"), 0)?,
            crate::recovery_linux::open_data_mount()?,
            0,
            true,
        )
    }

    fn under(root: File, data: File, owner: u32) -> Result<Option<Self>> {
        Self::under_mode(root, data, owner, false)
    }

    fn under_mode(root: File, data: File, owner: u32, preparing: bool) -> Result<Option<Self>> {
        let Some(parent) = state::optional_directory(&data, device::DIRECTORY, owner)? else {
            return Ok(None);
        };
        let Some(active) = state::optional_directory(&parent, state::ACTIVE, owner)? else {
            return Ok(None);
        };
        let binding = state::read_binding(&active, owner)?;
        let directory = match state::optional_directory(&active, DIRECTORY, owner)? {
            Some(directory) => directory,
            None if preparing => return Ok(None),
            None => return Err(invalid("bootstrap reservation has no staged trial record")),
        };
        // The install lock excludes other tools; this also excludes separately
        // opened journal handles, including in fixtures without a system lock.
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
            let error = std::io::Error::last_os_error();
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                Error::Busy
            } else {
                error.into()
            });
        }
        let bytes = read(&directory, RECORD, owner)?;
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if envelope.checksum != Sha256::digest(&serde_json::to_vec(&envelope.record)?)
            || envelope.record.inputs.binding != binding
        {
            return Err(invalid(
                "bootstrap trial checksum or preparation binding differs",
            ));
        }
        envelope.record.validate()?;
        let store = Self {
            root,
            data,
            parent,
            active,
            directory,
            owner,
            bytes,
            record: envelope.record,
            uncertain: false,
            #[cfg(test)]
            fault: None,
        };
        store.check()?;
        Ok(Some(store))
    }

    pub fn phase(&self) -> super::TrialPhase {
        self.record.phase
    }
    pub fn image(&self) -> &crate::ImageIdentity {
        &self.record.inputs.image
    }
    pub fn trial_id(&self) -> Sha256 {
        self.record.inputs.plan.trial_id
    }
    pub fn failure(&self) -> Option<super::TrialFailure> {
        self.record.failure
    }

    pub fn canonical_phase(&self) -> Option<super::CanonicalPhase> {
        self.record.canonical.as_ref().map(|c| c.phase)
    }

    pub(super) fn canonical(&self) -> Result<&super::canonical::CanonicalState> {
        self.durable()?
            .canonical
            .as_ref()
            .ok_or_else(|| invalid("canonical migration is not initialized"))
    }

    pub(super) fn advance_canonical<T>(
        &mut self,
        transition: impl FnOnce(
            &mut super::canonical::CanonicalState,
            &super::trial::TrialInputs,
        ) -> Result<T>,
    ) -> Result<T> {
        self.advance(|record| {
            let canonical = record
                .canonical
                .as_mut()
                .ok_or_else(|| invalid("canonical migration is not initialized"))?;
            transition(canonical, &record.inputs)
        })
    }

    pub(crate) fn watchdog_token(&self, boot: &crate::recovery::BootObservation) -> Result<Sha256> {
        self.durable()?.watchdog_completion(boot)
    }

    fn check(&self) -> Result<()> {
        if self.uncertain {
            return Err(invalid(
                "bootstrap trial durability is uncertain; reopen required",
            ));
        }
        self.check_storage()
    }

    fn check_storage(&self) -> Result<()> {
        let data = open_at(&self.root, "data", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&data.metadata()?, true, self.owner, false)?;
        if data.metadata()?.dev() != self.data.metadata()?.dev()
            || data.metadata()?.ino() != self.data.metadata()?.ino()
            || crate::recovery_linux::opened_mount_id(&data)?
                != crate::recovery_linux::opened_mount_id(&self.data)?
        {
            return Err(invalid("bootstrap data mount changed"));
        }
        backup::check_named_directory(&self.data, device::DIRECTORY, &self.parent, self.owner)?;
        backup::check_named_directory(&self.parent, state::ACTIVE, &self.active, self.owner)?;
        backup::check_named_directory(&self.active, DIRECTORY, &self.directory, self.owner)?;
        let mount = crate::recovery_linux::opened_mount_id(&self.data)?;
        for directory in [&self.parent, &self.active] {
            if crate::recovery_linux::opened_mount_id(directory)? != mount {
                return Err(invalid("bootstrap trial store crossed mounts"));
            }
        }
        crate::recovery_linux::check_data_subtree(
            &self.directory,
            &self.data,
            "spacewars-bootstrap/active/boot-trial",
        )?;
        if state::optional_directory(&self.active, PREPARING, self.owner)?.is_some() {
            return Err(invalid(
                "published and pending bootstrap trial records conflict",
            ));
        }
        if state::read_binding(&self.active, self.owner)? != self.record.inputs.binding {
            return Err(invalid("bootstrap preparation changed during trial"));
        }
        // A bootstrap attempt and production recovery authority cannot coexist.
        if let Some(system) = state::optional_directory(&self.data, "spacewars-system", self.owner)?
        {
            match open_at(&system, "recovery", libc::O_PATH, 0) {
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(invalid(
                        "bootstrap conflicts with production recovery state",
                    ));
                }
            }
        }
        backup::inventory(&self.directory, &[RECORD, PENDING])?;
        if read(&self.directory, RECORD, self.owner)? != self.bytes {
            return Err(invalid("bootstrap trial record changed"));
        }
        match read(&self.directory, PENDING, self.owner) {
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result?;
            }
        }
        Ok(())
    }

    pub(super) fn durable(&self) -> Result<&TrialRecord> {
        self.check()?;
        Ok(&self.record)
    }

    pub(super) fn active_directory(&self) -> Result<&File> {
        self.check()?;
        Ok(&self.active)
    }

    pub(super) fn advance<T>(
        &mut self,
        transition: impl FnOnce(&mut TrialRecord) -> Result<T>,
    ) -> Result<T> {
        self.check()?;
        let mut next = self.record.clone();
        let action = transition(&mut next)?;
        next.validate()?;
        if next.inputs != self.record.inputs {
            return Err(invalid("bootstrap transition changed its immutable inputs"));
        }
        if let Some(previous) = &self.record.canonical {
            let Some(current) = &next.canonical else {
                return Err(invalid(
                    "canonical migration cannot return to legacy recovery",
                ));
            };
            current.check_successor(previous)?;
        }
        if next != self.record {
            self.persist(&next)?;
            self.record = next;
        }
        Ok(action)
    }

    fn persist(&mut self, next: &TrialRecord) -> Result<()> {
        let bytes = serde_json::to_vec(&Envelope {
            record: next.clone(),
            checksum: Sha256::digest(&serde_json::to_vec(next)?),
        })?;
        if bytes.len() as u64 > LIMIT || backup::free_space(&self.directory)? < 2 * LIMIT {
            return Err(invalid("insufficient bootstrap trial journal capacity"));
        }
        self.check()?;
        let mut pending = backup::writable(&self.directory, PENDING, self.owner)?;
        // Once mutation starts, any error poisons this handle. Reopening reads
        // the published record, never an unpublished partial or torn pending file.
        self.uncertain = true;
        pending.set_len(0)?;
        pending.write_all(&bytes)?;
        self.inject("write")?;
        pending.sync_all()?;
        self.inject("file-sync")?;
        self.check_storage()?;
        let from = name(PENDING)?;
        let to = name(RECORD)?;
        // SAFETY: the private held directory and fixed C names remain live.
        io_result(unsafe {
            libc::renameat2(
                self.directory.as_raw_fd(),
                from.as_ptr(),
                self.directory.as_raw_fd(),
                to.as_ptr(),
                0,
            )
        })?;
        self.inject("rename")?;
        self.directory.sync_all()?;
        self.inject("directory-sync")?;
        if read(&self.directory, RECORD, self.owner)? != bytes {
            return Err(invalid("bootstrap trial journal readback differs"));
        }
        self.bytes = bytes;
        self.check_storage()?;
        self.uncertain = false;
        Ok(())
    }

    fn inject(&self, point: &str) -> Result<()> {
        #[cfg(test)]
        if self.fault == Some(point) {
            return Err(std::io::Error::other("bootstrap journal interruption fixture").into());
        }
        let _ = point;
        Ok(())
    }
}

/// Called only after the root-operator admission and written-image checks.
/// The immutable staging intent is durable before any boot file may change.
pub(super) fn prepare(
    active: &File,
    owner: u32,
    inputs: &super::trial::TrialInputs,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let record = TrialRecord::staging(inputs.clone())?;
    if state::read_binding(active, owner)? != inputs.binding {
        return Err(invalid("staging preparation binding differs"));
    }
    if state::optional_directory(active, DIRECTORY, owner)?.is_some() {
        return Err(invalid("bootstrap trial record already exists"));
    }
    let pending = backup::directory(active, PREPARING, owner)?;
    if crate::recovery_linux::opened_mount_id(&pending)?
        != crate::recovery_linux::opened_mount_id(active)?
    {
        return Err(invalid("pending staging record crossed mounts"));
    }
    // SAFETY: the independently opened private directory remains live.
    if unsafe { libc::flock(pending.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
        let error = std::io::Error::last_os_error();
        return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
            Error::Busy
        } else {
            error.into()
        });
    }
    backup::inventory(&pending, &[RECORD])?;
    let bytes = serde_json::to_vec(&Envelope {
        checksum: Sha256::digest(&serde_json::to_vec(&record)?),
        record,
    })?;
    if bytes.len() as u64 > LIMIT || backup::free_space(active)? < 2 * LIMIT {
        return Err(invalid("insufficient staging record capacity"));
    }
    let mut file = backup::writable(&pending, RECORD, owner)?;
    if file.metadata()?.len() > LIMIT
        || crate::recovery_linux::opened_mount_id(&file)?
            != crate::recovery_linux::opened_mount_id(&pending)?
    {
        return Err(invalid("oversized or over-mounted pending staging record"));
    }
    file.set_len(0)?;
    file.write_all(&bytes)?;
    checkpoint("trial-record-written")?;
    file.sync_all()?;
    pending.sync_all()?;
    checkpoint("trial-record-synced")?;
    backup::check_named_directory(active, PREPARING, &pending, owner)?;
    if state::read_binding(active, owner)? != inputs.binding {
        return Err(invalid("staging binding changed before publication"));
    }
    let from = name(PREPARING)?;
    let to = name(DIRECTORY)?;
    // SAFETY: fixed validated names and the held private parent remain live.
    io_result(unsafe {
        libc::renameat2(
            active.as_raw_fd(),
            from.as_ptr(),
            active.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })?;
    checkpoint("trial-record-published")?;
    active.sync_all()?;
    checkpoint("trial-record-directory-synced")?;
    backup::check_named_directory(active, DIRECTORY, &pending, owner)?;
    if read(&pending, RECORD, owner)? != bytes {
        return Err(invalid("staging record readback differs"));
    }
    Ok(())
}

fn read(directory: &File, name: &str, owner: u32) -> Result<Vec<u8>> {
    let file = device::private_asset(directory, name, owner)?;
    owned(&file.metadata()?, false, owner, true)?;
    if crate::recovery_linux::opened_mount_id(&file)?
        != crate::recovery_linux::opened_mount_id(directory)?
        || file.metadata()?.dev() != directory.metadata()?.dev()
    {
        return Err(invalid("bootstrap trial record crossed mounts"));
    }
    read_file(file, owner, LIMIT)
}

#[cfg(test)]
#[path = "bootstrap_trial_store_tests.rs"]
pub(super) mod tests;
