//! Root-operator development installs over SSH; see docs/ota-development-installs.md.
//! The unit must opt in, its tryboot and watchdog must be qualified, and the
//! image goes through the ordinary journal, writer, trial and recovery.
use crate::boot_linux::{BootController, BootEvidence};
use crate::development::{DevelopmentImage, MAX_DEVELOPMENT_BYTES};
use crate::install_linux::{StagedPayload, WriteProgress, write_development};
use crate::install_lock::InstallLock;
use crate::native_install::TrialPermit;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file};
use crate::recovery::{Action, BootBinding, FailureReason, Phase};
use crate::{DeviceIdentity, Error, RecoveryStore, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

const RECOVERY: &str = "/data/spacewars-system/recovery";
const MARKER: &str = "development.json";
const TEMPORARY: &str = ".development.next";
const MARKER_BYTES: u64 = 4096;
/// The descriptor's name in the operator's staging directory, beside
/// `<compressed-sha256>.ext4.zst`.
pub const DESCRIPTOR: &str = "development.json";

/// Opt-in for one unit and layout. Its absence refuses every development install.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    binding: BootBinding,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    /// Installs start only from a confirmed normal boot, as official ones do.
    /// After a committed trial, reboot once before the next install.
    pub normal_boot: bool,
    /// Generation 0 is an unpublished image: a development install or the
    /// bootstrap baseline. Official images still install over it.
    pub good_generation: u64,
    pub good_build_id: String,
    pub highest_attempted_generation: u64,
}

pub fn enable() -> Result<Status> {
    let lock = InstallLock::acquire()?;
    crate::bootstrap::require_no_active_bootstrap(&lock)?;
    let boot = BootController::open(&lock)?.observe()?;
    // Refuse up front on a unit whose tryboot and watchdog aren't qualified.
    crate::recovery_admission::Qualification::open(&boot)?;
    let system = crate::provision_linux::system_directory(&boot.layout)?;
    write_marker(&system, 0, &boot.observation.binding)?;
    status_under(&system, &boot)
}

pub fn disable() -> Result<Status> {
    let lock = InstallLock::acquire()?;
    let boot = BootController::open(&lock)?.observe()?;
    let system = crate::provision_linux::system_directory(&boot.layout)?;
    remove_marker(&system)?;
    status_under(&system, &boot)
}

pub fn status() -> Result<Status> {
    let lock = InstallLock::acquire()?;
    let boot = BootController::open(&lock)?.observe()?;
    let system = crate::provision_linux::system_directory(&boot.layout)?;
    status_under(&system, &boot)
}

fn status_under(system: &File, boot: &BootEvidence) -> Result<Status> {
    let store = RecoveryStore::open(Path::new(RECOVERY))?;
    let record = store.record();
    Ok(Status {
        enabled: enabled(system, 0, &boot.observation.binding)?,
        normal_boot: !boot.observation.tryboot,
        good_generation: record.good().generation,
        good_build_id: record.good().build_id.clone(),
        highest_attempted_generation: record.image_history().highest_attempted_generation,
    })
}

fn write_marker(system: &File, owner: u32, binding: &BootBinding) -> Result<()> {
    let bytes = serde_json::to_vec(&Marker {
        schema: 1,
        binding: binding.clone(),
    })?;
    let mut file = open_at(
        system,
        TEMPORARY,
        libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        0o600,
    )?;
    owned(&file.metadata()?, false, owner, true)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    rename(system, TEMPORARY, MARKER)?;
    system.sync_all()?;
    Ok(())
}

fn remove_marker(system: &File) -> Result<()> {
    let component = name(MARKER)?;
    // SAFETY: the directory descriptor and C string remain live.
    match io_result(unsafe { libc::unlinkat(system.as_raw_fd(), component.as_ptr(), 0) }) {
        Err(Error::Io(error)) if error.kind() == ErrorKind::NotFound => {}
        other => other?,
    }
    system.sync_all()?;
    Ok(())
}

fn enabled(system: &File, owner: u32, binding: &BootBinding) -> Result<bool> {
    let file = match open_at(system, MARKER, libc::O_RDONLY, 0) {
        Ok(file) => file,
        Err(Error::Io(error)) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    owned(&file.metadata()?, false, owner, true)?;
    let marker: Marker = serde_json::from_slice(&read_file(file, owner, MARKER_BYTES)?)?;
    if marker.schema != 1 {
        return Err(invalid("unsupported development marker"));
    }
    // A USB reflash with other boot files leaves a stale marker; it stays off.
    Ok(&marker.binding == binding)
}

/// Writes the inactive slot and requests the trial reboot. `payload` is the
/// operator's selected digest; the staged descriptor must name the same bytes.
/// `handoff` refuses while an official install's handoff still names the
/// recovery transaction that admission replaces.
pub fn install(
    stage: &Path,
    payload: Sha256,
    handoff: impl FnOnce(&InstallLock, &RecoveryStore) -> Result<()>,
    report: impl FnMut(WriteProgress),
    reboot: impl FnOnce(&mut TrialPermit<'_, '_>) -> Result<()>,
) -> Result<Sha256> {
    let lock = InstallLock::acquire()?;
    crate::bootstrap::require_no_active_bootstrap(&lock)?;
    let mut recovery = RecoveryStore::open(Path::new(RECOVERY))?;
    let mut boot = BootController::open(&lock)?;
    boot.authorize_maintenance(&recovery)?;
    handoff(&lock, &recovery)?;
    let evidence = boot.observe()?;
    // As for official installs, a normal boot must prove the promoted default
    // before another trial relies on it to return.
    if evidence.observation.tryboot {
        return Err(invalid(
            "development installs require a confirmed normal boot; reboot the unit first",
        ));
    }
    let system = crate::provision_linux::system_directory(&evidence.layout)?;
    if !enabled(&system, 0, &evidence.observation.binding)? {
        return Err(invalid(
            "development installs are not enabled on this unit; run `maintenance development enable` on the unit first",
        ));
    }
    // A broken image is safe only where tryboot and the watchdog were qualified.
    crate::recovery_admission::Qualification::open(&evidence)?;
    let setup = crate::provision_linux::inspect(&evidence.layout, &evidence.observation.binding)?;
    let device = DeviceIdentity {
        machine: evidence.image.machine.clone(),
        architecture: evidence.image.architecture.clone(),
        layout: evidence.image.layout.clone(),
        boot_identity: evidence.image.boot_identity,
        updater_protocol: evidence.image.updater_protocol,
        recovery_protocol: evidence.observation.recovery_protocol,
        inactive_slot_bytes: evidence
            .layout
            .inactive_partition()
            .sectors
            .checked_mul(512)
            .ok_or_else(|| invalid("inactive capacity overflow"))?,
        persistent_schema: setup.persistent_schema,
        rollback_reads: recovery.record().good().data.reads,
    };
    let directory = open_directory(stage, 0)?;
    owned(&directory.metadata()?, true, 0, true)?;
    let descriptor = open_at(&directory, DESCRIPTOR, libc::O_RDONLY, 0)?;
    owned(&descriptor.metadata()?, false, 0, true)?;
    // Only these parsed bytes are used from here on.
    let image = DevelopmentImage::parse(&read_file(descriptor, 0, MAX_DEVELOPMENT_BYTES as u64)?)?;
    if image.compressed.sha256 != payload {
        return Err(invalid(
            "staged descriptor names other bytes than the selected payload",
        ));
    }
    image.check_device(&device)?;
    let mut staged = StagedPayload::development(directory, image.compressed.clone())?;
    let observation = boot.observe()?.observation;
    let action = recovery.advance(|record| record.begin_development(&image, &observation))?;
    let Action::WriteInactive { transaction, .. } = action else {
        return Err(invalid(
            "development admission did not authorize its writer",
        ));
    };
    // From here, every failure belongs to this durable transaction. The prior
    // good/default stays intact, and an interrupted process cannot start again.
    let outcome = (|| {
        let action = write_development(&boot, &mut recovery, &image, &mut staged, report)?;
        boot.apply(&recovery, &action)?;
        let mut permit = TrialPermit::development(&boot, &mut recovery, transaction);
        reboot(&mut permit)?;
        if !permit.is_queued() {
            return Err(invalid("development trial reboot was not acknowledged"));
        }
        Ok(transaction)
    })();
    if outcome.is_err()
        && recovery.durable_record()?.transaction().is_some_and(|tx| {
            tx.id == transaction && !matches!(tx.phase, Phase::Committed | Phase::RolledBack)
        })
    {
        let boot = boot.observe()?;
        recovery
            .advance(|record| record.fail(FailureReason::InstallationFailed, &boot.observation))?;
    }
    outcome
}

fn rename(directory: &File, from: &str, to: &str) -> Result<()> {
    let from = name(from)?;
    let to = name(to)?;
    // SAFETY: both names are validated components of the same live directory.
    io_result(unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
        )
    })
}

fn invalid(message: &str) -> Error {
    Error::Install(message.into())
}

#[cfg(test)]
#[path = "development_linux_tests.rs"]
mod tests;
