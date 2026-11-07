//! Retire a provisioned unit's production records before a supervised boot-set
//! upgrade. Entries move, never copy, into one private record on the data
//! filesystem, and nothing is deleted or rewritten. The unit then has no update
//! authority until a new bootstrap baseline is published and trust is
//! provisioned again. Only a published retirement of the exact boot set lets
//! bootstrap accept that set's canonical selectors; lost state never does.
use super::finish_files::{absent, flush, rename};
use super::invalid;
use crate::owned_fs::{io_result, name, open_at, owned, read_file};
use crate::recovery::{BootBinding, Slot};
use crate::{Blob, Error, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

pub(super) const DIRECTORY: &str = "spacewars-retired";
/// Fresh images run these services before OTA is provisioned. Bootstrap keeps
/// their stores; none grants recovery or signed-feed authority.
pub(super) const PRESERVED: &[&str] = &["network", "time", "time-check", "update-check"];
const SYSTEM_DIRECTORY: &str = "spacewars-system";
const RECOVERY: &str = "recovery";
const BOOTSTRAP_ENTRIES: &[&str] = &["completed", "package"];
const RECORD: &str = "retirement.json";
const SYSTEM: &str = "system";
const BOOTSTRAP: &str = "bootstrap";
const MAX_RECORD_BYTES: u64 = 512 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_RETIREMENTS: usize = 64;
const ID_LENGTH: usize = 16;

/// What was retired and the boot set it described. The production journal
/// moves right after the bootstrap entries, so a completed bootstrap never
/// outlives it and kiosk startup never recreates a store that already moved.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retirement {
    pub schema: u32,
    /// The boot that planned the retirement; a resumed one may finish later.
    pub boot_id: String,
    pub binding: BootBinding,
    pub recovery: Sha256,
    pub good_slot: Slot,
    pub good_build_id: String,
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub boot_files: BTreeMap<String, Blob>,
    /// Entries moved from /data/spacewars-system, sorted.
    pub system: Vec<String>,
    /// Entries moved from /data/spacewars-bootstrap, sorted.
    pub bootstrap: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RetirementReport {
    pub schema: u32,
    pub scope: &'static str,
    pub directory: String,
    pub resumed: bool,
    pub retirement: Retirement,
    pub automatic_updates_enabled: bool,
}

/// Observed by the device wrapper under the install lock with writers masked.
pub(super) struct Evidence {
    pub boot_id: String,
    pub binding: BootBinding,
    pub boot_files: BTreeMap<String, Blob>,
    /// The production journal while it is still in place.
    pub journal: Option<Journal>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Journal {
    pub fingerprint: Sha256,
    pub good_slot: Slot,
    pub good_build_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    retirement: Retirement,
    checksum: Sha256,
}

#[derive(Debug, Eq, PartialEq)]
enum Kind {
    Published,
    Pending,
    Creating,
}

impl Retirement {
    fn validate(&self) -> Result<()> {
        let sorted = |entries: &[String]| entries.windows(2).all(|pair| pair[0] < pair[1]);
        if self.schema != 1
            || !crate::recovery::boot_id(&self.boot_id)
            || self.good_build_id.is_empty()
            || self.good_build_id.len() > 256
            || self.boot_files.is_empty()
            || self.boot_files.len() > 1024
            || self
                .boot_files
                .keys()
                .any(|path| !crate::boot::asset_path(path))
            || self.system.len() > MAX_ENTRIES
            || !sorted(&self.system)
            || !sorted(&self.bootstrap)
            || !self.system.iter().any(|entry| entry == RECOVERY)
            || self
                .system
                .iter()
                .any(|entry| !entry_name(entry) || PRESERVED.contains(&entry.as_str()))
            || self
                .bootstrap
                .iter()
                .any(|entry| !BOOTSTRAP_ENTRIES.contains(&entry.as_str()))
        {
            return Err(invalid("invalid production retirement record"));
        }
        Ok(())
    }

    fn id(&self) -> Result<String> {
        Ok(Sha256::digest(&serde_json::to_vec(self)?).to_string()[..ID_LENGTH].to_owned())
    }

    /// Bootstrap state, then the production journal, then the rest. A reboot
    /// drops the runtime masks, and kiosk startup recreates a moved install
    /// handoff store while the journal remains; once it moves, startup creates
    /// nothing.
    fn moves(&self) -> Vec<(&'static str, &'static str, &str)> {
        let mut moves: Vec<_> = self
            .bootstrap
            .iter()
            .map(|entry| (super::device::DIRECTORY, BOOTSTRAP, entry.as_str()))
            .collect();
        moves.push((SYSTEM_DIRECTORY, SYSTEM, RECOVERY));
        moves.extend(
            self.system
                .iter()
                .filter(|entry| *entry != RECOVERY)
                .map(|entry| (SYSTEM_DIRECTORY, SYSTEM, entry.as_str())),
        );
        moves
    }

    fn describes(&self, evidence: &Evidence) -> bool {
        self.binding == evidence.binding
            && self.boot_files == evidence.boot_files
            && evidence.journal.as_ref().is_none_or(|journal| {
                journal.fingerprint == self.recovery
                    && journal.good_slot == self.good_slot
                    && journal.good_build_id == self.good_build_id
            })
    }
}

/// Root operator only. `good_build_id` names the confirmed image whose records
/// are retired. Runs from a normal boot of that good image with the kiosk and
/// device service masked and stopped; never installs, provisions or reboots.
pub fn retire_production_records(good_build_id: &str) -> Result<RetirementReport> {
    let lock = crate::install_lock::InstallLock::acquire()?;
    super::require_no_active_bootstrap(&lock)?;
    let data = crate::recovery_linux::open_data_mount()?;
    super::device::require_masked_writers(&data)?;
    let evidence = observe(&lock, &data)?;
    let (record, id, resumed) = retire_under(&data, 0, &evidence, good_build_id, || {
        lock.check()?;
        super::require_no_active_bootstrap(&lock)?;
        super::device::require_masked_writers(&data)?;
        let current = observe(&lock, &data)?;
        if current.boot_id != evidence.boot_id
            || current.binding != evidence.binding
            || current.boot_files != evidence.boot_files
        {
            return Err(invalid("boot changed during production retirement"));
        }
        Ok(())
    })?;
    Ok(RetirementReport {
        schema: 1,
        scope: "production records retired; bootstrap the new boot set, then provision updates again",
        directory: format!("/data/{DIRECTORY}/{id}"),
        resumed,
        retirement: record,
        automatic_updates_enabled: false,
    })
}

fn observe(lock: &crate::install_lock::InstallLock, data: &File) -> Result<Evidence> {
    let controller = crate::boot_linux::BootController::open(lock)?;
    let boot = controller.observe()?;
    if boot.observation.tryboot {
        return Err(invalid("retire production records from a normal boot"));
    }
    let system = private_directory(data, SYSTEM_DIRECTORY, 0, false)?
        .ok_or_else(|| invalid("no device-service state to retire"))?;
    let journal = if absent(&system, RECOVERY)? {
        None
    } else {
        let store =
            crate::RecoveryStore::open(std::path::Path::new("/data/spacewars-system/recovery"))?;
        controller.authorize_maintenance(&store)?;
        let record = store.durable_record()?;
        Some(Journal {
            fingerprint: record.fingerprint(),
            good_slot: record.good().slot,
            good_build_id: record.good().build_id.clone(),
        })
    };
    Ok(Evidence {
        boot_id: boot.observation.boot_id.clone(),
        binding: boot.observation.binding.clone(),
        boot_files: super::inspect::boot_inventory(&boot.layout)?,
        journal,
    })
}

fn entry_name(entry: &str) -> bool {
    !entry.starts_with('.') && entry.len() <= 255 && name(entry).is_ok()
}

/// Retire, or finish an interrupted retirement. `good_build_id` is the
/// operator's confirmation of the image whose records retire, checked before
/// anything is created, moved or published. `guard` rechecks the lock and
/// masked writers before each move. Returns the record, its id and whether an
/// earlier invocation had already begun or completed it.
pub(super) fn retire_under(
    data: &File,
    owner: u32,
    evidence: &Evidence,
    good_build_id: &str,
    mut guard: impl FnMut() -> Result<()>,
) -> Result<(Retirement, String, bool)> {
    guard()?;
    let confirm = |record: &Retirement| {
        if record.good_build_id == good_build_id {
            Ok(())
        } else {
            Err(invalid(
                "these production records belong to another good image; inspect the journal",
            ))
        }
    };
    let parent = private_directory(data, DIRECTORY, owner, true)?
        .ok_or_else(|| invalid("retirement directory disappeared"))?;
    let mut pending = None;
    let mut published = Vec::new();
    for (id, kind) in entries_of(&parent)? {
        match kind {
            // Nothing moves before a record is published as pending, so an
            // interrupted creation holds only this tool's own scratch files.
            Kind::Creating => discard_creating(&parent, &id, owner)?,
            Kind::Pending if pending.is_some() => {
                return Err(invalid("conflicting interrupted production retirements"));
            }
            Kind::Pending => pending = Some(id),
            Kind::Published => published.push(id),
        }
    }
    let (record, id, resumed) = match pending {
        Some(id) => {
            let directory = private_directory(&parent, &pending_name(&id), owner, false)?
                .ok_or_else(|| invalid("interrupted retirement disappeared"))?;
            let record = read_record(&directory, owner, &id)?;
            if !record.describes(evidence) {
                return Err(invalid(
                    "production state changed during an interrupted retirement",
                ));
            }
            confirm(&record)?;
            (record, id, true)
        }
        None if evidence.journal.is_none() => {
            // Repeating a completed retirement reports it; anything else has
            // lost its production journal and is never treated as retirable.
            for id in published {
                let directory = private_directory(&parent, &id, owner, false)?
                    .ok_or_else(|| invalid("published retirement disappeared"))?;
                let record = read_record(&directory, owner, &id)?;
                if record.describes(evidence) {
                    confirm(&record)?;
                    return Ok((record, id, true));
                }
            }
            return Err(invalid("no production recovery journal to retire"));
        }
        None => {
            let record = plan(data, owner, evidence)?;
            confirm(&record)?;
            let id = record.id()?;
            // Check every source before creating anything, so an unsafe entry
            // never leaves a partly retired unit.
            for (source, _, entry) in record.moves() {
                let from = private_directory(data, source, owner, false)?
                    .ok_or_else(|| invalid("retired state's parent disappeared"))?;
                safe_entry(&from, entry, owner, data)?;
            }
            create_pending(&parent, owner, &record, &id)?;
            (record, id, false)
        }
    };
    let directory = private_directory(&parent, &pending_name(&id), owner, false)?
        .ok_or_else(|| invalid("interrupted retirement disappeared"))?;
    let mut remaining = Vec::new();
    for (source, destination, entry) in record.moves() {
        let from = private_directory(data, source, owner, false)?
            .ok_or_else(|| invalid("retired state's parent disappeared"))?;
        let to = private_directory(&directory, destination, owner, false)?
            .ok_or_else(|| invalid("retirement destination disappeared"))?;
        match (absent(&from, entry)?, absent(&to, entry)?) {
            (false, true) => {
                safe_entry(&from, entry, owner, data)?;
                remaining.push((from, to, entry));
            }
            (true, false) => {}
            _ => return Err(invalid("retired entry is missing or present twice")),
        }
    }
    for (from, to, entry) in remaining {
        guard()?;
        rename(&from, entry, &to, entry)?;
        to.sync_all()?;
        from.sync_all()?;
    }
    let system = private_directory(data, SYSTEM_DIRECTORY, owner, false)?
        .ok_or_else(|| invalid("device-service state disappeared"))?;
    if names(&system)?
        .iter()
        .any(|entry| !PRESERVED.contains(&entry.as_str()))
    {
        return Err(invalid("new production state appeared during retirement"));
    }
    directory.sync_all()?;
    flush(data)?;
    guard()?;
    rename(&parent, &pending_name(&id), &parent, &id)?;
    parent.sync_all()?;
    flush(data)?;
    Ok((record, id, resumed))
}

/// True when a published retirement describes exactly these boot files. An
/// unfinished retirement must complete first, so it never authorizes anything.
pub(super) fn retired_boot(
    data: &File,
    owner: u32,
    files: &BTreeMap<String, Blob>,
) -> Result<bool> {
    let Some(parent) = private_directory(data, DIRECTORY, owner, false)? else {
        return Ok(false);
    };
    let mut matched = false;
    for (id, kind) in entries_of(&parent)? {
        if kind != Kind::Published {
            return Err(invalid(
                "an interrupted production retirement must be completed first",
            ));
        }
        let directory = private_directory(&parent, &id, owner, false)?
            .ok_or_else(|| invalid("published retirement disappeared"))?;
        matched |= &read_record(&directory, owner, &id)?.boot_files == files;
    }
    Ok(matched)
}

fn plan(data: &File, owner: u32, evidence: &Evidence) -> Result<Retirement> {
    let journal = evidence
        .journal
        .as_ref()
        .ok_or_else(|| invalid("no production recovery journal to retire"))?;
    let system = private_directory(data, SYSTEM_DIRECTORY, owner, false)?
        .ok_or_else(|| invalid("no device-service state to retire"))?;
    let retired = names(&system)?
        .into_iter()
        .filter(|entry| !PRESERVED.contains(&entry.as_str()))
        .collect();
    let bootstrap = match private_directory(data, super::device::DIRECTORY, owner, false)? {
        Some(directory) => names(&directory)?,
        None => Vec::new(),
    };
    let record = Retirement {
        schema: 1,
        boot_id: evidence.boot_id.clone(),
        binding: evidence.binding.clone(),
        recovery: journal.fingerprint,
        good_slot: journal.good_slot,
        good_build_id: journal.good_build_id.clone(),
        boot_files: evidence.boot_files.clone(),
        system: retired,
        bootstrap,
    };
    record.validate()?;
    Ok(record)
}

fn create_pending(parent: &File, owner: u32, record: &Retirement, id: &str) -> Result<()> {
    let bytes = serde_json::to_vec(&Envelope {
        retirement: record.clone(),
        checksum: Sha256::digest(&serde_json::to_vec(record)?),
    })?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(invalid("production retirement record exceeds its bound"));
    }
    let creating = creating_name(id);
    make_directory(parent, &creating)?;
    let directory = private_directory(parent, &creating, owner, false)?
        .ok_or_else(|| invalid("retirement scratch disappeared"))?;
    for child in [SYSTEM, BOOTSTRAP] {
        make_directory(&directory, child)?;
    }
    let mut file = open_at(
        &directory,
        RECORD,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    directory.sync_all()?;
    if read_record(&directory, owner, id)? != *record {
        return Err(invalid("production retirement record readback differs"));
    }
    rename(parent, &creating, parent, &pending_name(id))?;
    parent.sync_all()?;
    Ok(())
}

/// Remove only an unpublished record and its two empty destinations.
fn discard_creating(parent: &File, id: &str, owner: u32) -> Result<()> {
    let creating = creating_name(id);
    let directory = private_directory(parent, &creating, owner, false)?
        .ok_or_else(|| invalid("retirement scratch disappeared"))?;
    for entry in names(&directory)? {
        match entry.as_str() {
            RECORD => {
                owned(
                    &open_at(&directory, RECORD, libc::O_RDONLY, 0)?.metadata()?,
                    false,
                    owner,
                    true,
                )?;
                unlink(&directory, RECORD, false)?;
            }
            SYSTEM | BOOTSTRAP => {
                let child = private_directory(&directory, &entry, owner, false)?
                    .ok_or_else(|| invalid("retirement scratch disappeared"))?;
                if !names(&child)?.is_empty() {
                    return Err(invalid("unpublished retirement scratch holds moved state"));
                }
                unlink(&directory, &entry, true)?;
            }
            _ => return Err(invalid("unexpected retirement scratch entry")),
        }
    }
    directory.sync_all()?;
    unlink(parent, &creating, true)?;
    parent.sync_all()?;
    Ok(())
}

fn read_record(directory: &File, owner: u32, id: &str) -> Result<Retirement> {
    let mut found = names(directory)?;
    found.retain(|entry| entry != RECORD && entry != SYSTEM && entry != BOOTSTRAP);
    if !found.is_empty() {
        return Err(invalid("unexpected production retirement entry"));
    }
    let file = open_at(directory, RECORD, libc::O_RDONLY, 0)?;
    owned(&file.metadata()?, false, owner, true)?;
    let envelope: Envelope = serde_json::from_slice(&read_file(file, owner, MAX_RECORD_BYTES)?)?;
    if Sha256::digest(&serde_json::to_vec(&envelope.retirement)?) != envelope.checksum {
        return Err(invalid("production retirement checksum mismatch"));
    }
    envelope.retirement.validate()?;
    if envelope.retirement.id()? != id {
        return Err(invalid("production retirement is filed under another name"));
    }
    Ok(envelope.retirement)
}

fn entries_of(parent: &File) -> Result<Vec<(String, Kind)>> {
    let mut entries = Vec::new();
    for entry in names(parent)? {
        let (id, kind) = if let Some(id) = entry
            .strip_prefix('.')
            .and_then(|e| e.strip_suffix(".pending"))
        {
            (id, Kind::Pending)
        } else if let Some(id) = entry
            .strip_prefix('.')
            .and_then(|e| e.strip_suffix(".creating"))
        {
            (id, Kind::Creating)
        } else {
            (entry.as_str(), Kind::Published)
        };
        if id.len() != ID_LENGTH
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid("unexpected production retirement entry"));
        }
        entries.push((id.to_owned(), kind));
    }
    if entries.len() > MAX_RETIREMENTS {
        return Err(invalid("too many production retirements"));
    }
    Ok(entries)
}

fn pending_name(id: &str) -> String {
    format!(".{id}.pending")
}

fn creating_name(id: &str) -> String {
    format!(".{id}.creating")
}

/// Sorted names, bounded. Unlisted or non-UTF8 names are refused.
fn names(directory: &File) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        let entry = entry?
            .file_name()
            .into_string()
            .map_err(|_| invalid("non-UTF8 production state name"))?;
        names.push(entry);
        if names.len() > MAX_ENTRIES.max(MAX_RETIREMENTS) + 2 {
            return Err(invalid("excessive production state entries"));
        }
    }
    names.sort();
    Ok(names)
}

/// Root-owned regular files or directories on the data filesystem only.
fn safe_entry(parent: &File, entry: &str, owner: u32, data: &File) -> Result<()> {
    let file = open_at(parent, entry, libc::O_PATH, 0)?;
    let metadata = file.metadata()?;
    owned(&metadata, metadata.is_dir(), owner, false)?;
    if metadata.dev() != data.metadata()?.dev() {
        return Err(invalid("production state crossed data filesystems"));
    }
    Ok(())
}

fn private_directory(
    parent: &File,
    component: &str,
    owner: u32,
    create: bool,
) -> Result<Option<File>> {
    let directory = match open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(directory) => directory,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound && create => {
            make_directory(parent, component)?;
            open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?
        }
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    owned(&directory.metadata()?, true, owner, true)?;
    if directory.metadata()?.dev() != parent.metadata()?.dev() {
        return Err(invalid("production retirement crossed filesystems"));
    }
    Ok(Some(directory))
}

fn make_directory(parent: &File, component: &str) -> Result<()> {
    let component = name(component)?;
    // SAFETY: the held directory descriptor and C component remain live.
    io_result(unsafe { libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o700) })?;
    parent.sync_all()?;
    Ok(())
}

fn unlink(parent: &File, component: &str, directory: bool) -> Result<()> {
    let component = name(component)?;
    // SAFETY: the held directory descriptor and C component remain live.
    io_result(unsafe {
        libc::unlinkat(
            parent.as_raw_fd(),
            component.as_ptr(),
            if directory { libc::AT_REMOVEDIR } else { 0 },
        )
    })
}

#[cfg(test)]
#[path = "bootstrap_retire_tests.rs"]
mod tests;
