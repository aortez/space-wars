//! Fixed-path Linux boot evidence and journal-gated selector changes.
//! This module does not reboot, open a watchdog or write a root partition.
use crate::boot::{SlotRoots, kernel_build_id};
use crate::boot_files::BootFiles;
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, open_at, open_directory};
use crate::recovery::{Action, BootBinding, BootObservation, boot_id};
use crate::recovery_linux::{DeviceNumber, DiskLayout, inspect_layout};
use crate::{Error, ImageIdentity, RecoveryStore, Result, Sha256};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PhysicalIdentity {
    pub model: String,
    pub serial: String,
    pub bootloader_revision: String,
    pub medium_sectors: u64,
    pub medium: BTreeMap<String, String>,
}

impl PhysicalIdentity {
    pub fn fingerprint(&self) -> Sha256 {
        let mut bytes = b"spacewars-device-v1\n".to_vec();
        bytes.extend(serde_json::to_vec(self).expect("device evidence serializes"));
        Sha256::digest(&bytes)
    }

    fn inspect(layout: &DiskLayout) -> Result<Self> {
        let model = property("/sys/firmware/devicetree/base/model", 128)?;
        let serial = property("/sys/firmware/devicetree/base/serial-number", 64)?;
        let firmware = property(
            "/sys/firmware/devicetree/base/chosen/bootloader/version",
            64,
        )?;
        let medium = text(
            &format!(
                "/run/udev/data/b{}:{}",
                layout.disk_device.major, layout.disk_device.minor
            ),
            64 * 1024,
        )?;
        Self::parse(model, serial, firmware, layout.disk_sectors, &medium)
    }

    fn parse(
        model: String,
        serial: String,
        bootloader_revision: String,
        medium_sectors: u64,
        record: &str,
    ) -> Result<Self> {
        if !(model.starts_with("Raspberry Pi 4 Model B Rev ")
            || model.starts_with("Raspberry Pi 5 Model B Rev "))
            || !hex(&serial, 16)
            || serial.bytes().all(|b| b == b'0')
            || !hex(&bootloader_revision, 40)
            || medium_sectors == 0
        {
            return Err(invalid("missing or unsupported board/firmware identity"));
        }
        let mut medium = BTreeMap::new();
        for line in record.lines() {
            let Some((key, value)) = line
                .strip_prefix("E:")
                .and_then(|line| line.split_once('='))
            else {
                continue;
            };
            if !matches!(
                key,
                "ID_BUS"
                    | "ID_TYPE"
                    | "ID_SERIAL"
                    | "ID_VENDOR"
                    | "ID_VENDOR_ID"
                    | "ID_MODEL"
                    | "ID_MODEL_ID"
                    | "ID_REVISION"
                    | "ID_WWN"
                    | "ID_WWN_WITH_EXTENSION"
            ) {
                continue;
            }
            if value.is_empty()
                || value.len() > 512
                || !value.bytes().all(|b| b.is_ascii_graphic())
                || medium.insert(key.into(), value.into()).is_some()
            {
                return Err(invalid("ambiguous or invalid USB medium identity"));
            }
        }
        if medium.get("ID_BUS").map(String::as_str) != Some("usb")
            || medium.get("ID_TYPE").map(String::as_str) != Some("disk")
            || ["ID_SERIAL", "ID_VENDOR", "ID_MODEL", "ID_REVISION"]
                .iter()
                .any(|key| !medium.contains_key(*key))
        {
            return Err(invalid("root medium has no supported stable USB identity"));
        }
        Ok(Self {
            model,
            serial,
            bootloader_revision,
            medium_sectors,
            medium,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BootEvidence {
    pub physical: PhysicalIdentity,
    pub image: ImageIdentity,
    pub layout: DiskLayout,
    pub kernel_build_id: String,
    pub hardware_profile: String,
    pub observation: BootObservation,
}

#[derive(Clone, Debug)]
struct Runtime {
    physical: PhysicalIdentity,
    image: ImageIdentity,
    layout: DiskLayout,
    kernel: String,
    profile: String,
    command_line: String,
    boot_id: String,
    uptime_ms: u64,
    tryboot: bool,
}

impl Runtime {
    fn inspect() -> Result<Self> {
        let image = ImageIdentity::read_installed().map_err(|error| {
            invalid(format!(
                "installed recovery image record is unavailable or invalid: {error}"
            ))
        })?;
        let layout = inspect_layout()?;
        let physical = PhysicalIdentity::inspect(&layout)?;
        let kernel = kernel_build_id(&read("/sys/kernel/notes", 64 * 1024)?)?;
        let profile = property("/sys/firmware/devicetree/base/chosen/spacewars,profile", 32)?;
        let tryboot = parse_tryboot(&read(
            "/sys/firmware/devicetree/base/chosen/bootloader/tryboot",
            4,
        )?)?;
        let command_line = text("/proc/cmdline", 16 * 1024)?;
        let boot = text("/proc/sys/kernel/random/boot_id", 64)?;
        let id = boot.trim().to_owned();
        if !boot_id(&id) {
            return Err(invalid("invalid running boot ID"));
        }
        Ok(Self {
            physical,
            image,
            layout,
            kernel,
            profile,
            command_line,
            boot_id: id,
            uptime_ms: boottime_ms()?,
            tryboot,
        })
    }

    fn roots(&self) -> Result<SlotRoots> {
        SlotRoots::new(
            self.layout.partitions[1].partuuid.clone(),
            self.layout.partitions[2].partuuid.clone(),
        )
    }

    fn same_boot(&self, other: &Self) -> bool {
        self.physical == other.physical
            && self.image == other.image
            && self.layout == other.layout
            && self.kernel == other.kernel
            && self.profile == other.profile
            && self.command_line == other.command_line
            && self.boot_id == other.boot_id
            && self.tryboot == other.tryboot
            && other.uptime_ms >= self.uptime_ms
    }

    fn evidence(&self, files: &BootFiles) -> Result<BootEvidence> {
        if self.image.boot_identity != files.identity() {
            return Err(invalid("root image and boot assets disagree"));
        }
        let roots = self.roots()?;
        let selection = files.selections(&roots)?;
        let kernel = files.asset("spacewars-kernel-build-id", 41)?;
        if kernel != format!("{}\n", self.kernel).as_bytes() {
            return Err(invalid("loaded kernel differs from the immutable boot set"));
        }
        if self.profile != selection.profile
            || (self.profile == "picade" && !self.physical.model.starts_with("Raspberry Pi 4 "))
        {
            return Err(invalid(
                "loaded hardware profile differs from the selected supported profile",
            ));
        }
        let trial_id = if self.tryboot {
            let trial = selection
                .trial
                .ok_or_else(|| invalid("firmware reports a trial without a prepared selector"))?;
            if trial.slot != self.layout.active {
                return Err(invalid("trial selector disagrees with actual root"));
            }
            Some(trial.transaction)
        } else {
            None
        };
        files
            .template()
            .check_running(&roots, self.layout.active, trial_id, &self.command_line)?;
        let mut profile = format!("spacewars-profile-v1\n{}\0", self.profile).into_bytes();
        profile.extend(files.asset(
            &format!("spacewars-profiles/{}.txt", self.profile),
            16 * 1024,
        )?);
        Ok(BootEvidence {
            physical: self.physical.clone(),
            image: self.image.clone(),
            layout: self.layout.clone(),
            kernel_build_id: self.kernel.clone(),
            hardware_profile: self.profile.clone(),
            observation: BootObservation {
                binding: BootBinding {
                    device_id: self.physical.fingerprint(),
                    layout_id: self.layout.layout_id,
                    boot_identity: files.identity(),
                    hardware_profile: Sha256::digest(&profile),
                },
                boot_id: self.boot_id.clone(),
                uptime_ms: self.uptime_ms,
                actual_slot: Some(self.layout.active),
                actual_build_id: Some(self.image.build_id.clone()),
                recovery_protocol: self.image.recovery_protocol,
                default_slot: Some(selection.default),
                tryboot: self.tryboot,
                trial_id,
            },
        })
    }
}

struct CheckedBoot {
    files: BootFiles,
    runtime: Runtime,
}

impl CheckedBoot {
    fn open() -> Result<Self> {
        let runtime = Runtime::inspect()?;
        let root = open_directory(Path::new("/"), 0)?;
        let directory = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        check_mount(&directory, &runtime.layout, false)?;
        let files = BootFiles::open(directory, 0, runtime.image.boot_identity)?;
        let checked = Self { files, runtime };
        checked.observe()?;
        Ok(checked)
    }

    fn observe(&self) -> Result<BootEvidence> {
        observe_current(&self.runtime, &self.files)
    }
}

fn observe_current(expected: &Runtime, files: &BootFiles) -> Result<BootEvidence> {
    let current = Runtime::inspect()?;
    if !expected.same_boot(&current) {
        return Err(invalid("boot context changed during recovery"));
    }
    let root = open_directory(Path::new("/"), 0)?;
    let named = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let held = files.directory().metadata()?;
    let named_metadata = named.metadata()?;
    if held.dev() != named_metadata.dev() || held.ino() != named_metadata.ino() {
        return Err(invalid("boot mount was replaced"));
    }
    check_mount(files.directory(), &current.layout, false)?;
    current.evidence(files)
}

/// Read-only, fixed-path inspection. A legacy or unprovisioned system fails
/// closed; this function never creates state, opens devices or acquires a lock.
pub fn inspect_boot() -> Result<BootEvidence> {
    CheckedBoot::open()?.observe()
}

/// Requires the root-only shared installation lock for its entire lifetime.
/// The caller must persist recovery transitions before passing their actions.
pub struct BootController<'a> {
    lock: &'a InstallLock,
    boot: CheckedBoot,
    uncertain: bool,
}

impl<'a> BootController<'a> {
    pub fn open(lock: &'a InstallLock) -> Result<Self> {
        lock.check()?;
        Ok(Self {
            lock,
            boot: CheckedBoot::open()?,
            uncertain: false,
        })
    }

    pub fn observe(&self) -> Result<BootEvidence> {
        if self.uncertain {
            return Err(invalid(
                "selector outcome is uncertain; reopen and reconcile",
            ));
        }
        self.lock.check()?;
        self.boot.observe()
    }

    pub fn apply(&mut self, journal: &RecoveryStore, action: &Action) -> Result<BootEvidence> {
        let before = self.observe()?;
        let record = journal.durable_record()?;
        let (slot, trial) = record.selection_change(action, &before.observation)?;
        let roots = self.boot.runtime.roots()?;
        let lock = self.lock;
        let runtime = &self.boot.runtime;
        self.uncertain = true;
        self.boot.files.select(&roots, slot, trial, |files| {
            lock.check()?;
            let current = observe_current(runtime, files)?;
            check_mount(files.directory(), &current.layout, true)?;
            if record.selection_change(action, &current.observation)? != (slot, trial) {
                return Err(invalid("journal selection changed during preparation"));
            }
            Ok(())
        })?;
        let after = self.boot.observe()?;
        self.uncertain = false;
        Ok(after)
    }
}

fn check_mount(directory: &File, layout: &DiskLayout, writable: bool) -> Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: directory is live and fstatfs initializes stat on success.
    io_result(unsafe { libc::fstatfs(directory.as_raw_fd(), stat.as_mut_ptr()) })?;
    // SAFETY: the successful fstatfs above initialized stat.
    let stat = unsafe { stat.assume_init() };
    if stat.f_type != 0x4d44
        || DeviceNumber::from_dev(directory.metadata()?.dev()) != layout.partitions[0].device
    {
        return Err(invalid("boot is not the expected FAT partition"));
    }
    if writable {
        let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: directory is live and fstatvfs initializes status on success.
        io_result(unsafe { libc::fstatvfs(directory.as_raw_fd(), status.as_mut_ptr()) })?;
        // SAFETY: the successful fstatvfs above initialized status.
        if unsafe { status.assume_init() }.f_flag & libc::ST_RDONLY != 0 {
            return Err(invalid("boot filesystem is read-only"));
        }
    }
    Ok(())
}

fn read(path: &str, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| invalid(format!("{path}: {error}")))?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("kernel/device evidence exceeds its bound"));
    }
    Ok(bytes)
}

fn text(path: &str, limit: u64) -> Result<String> {
    String::from_utf8(read(path, limit)?).map_err(|_| invalid("non-UTF8 kernel/device evidence"))
}

fn property(path: &str, limit: u64) -> Result<String> {
    let bytes = read(path, limit)?;
    let value = bytes
        .strip_suffix(b"\0")
        .ok_or_else(|| invalid("unterminated device-tree property"))?;
    if value.is_empty() || !value.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        return Err(invalid("invalid device-tree string"));
    }
    Ok(String::from_utf8(value.to_vec()).expect("ASCII property"))
}

fn parse_tryboot(bytes: &[u8]) -> Result<bool> {
    match bytes {
        [0, 0, 0, 0] => Ok(false),
        [0, 0, 0, 1] => Ok(true),
        _ => Err(invalid("missing or ambiguous firmware tryboot state")),
    }
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn boottime_ms() -> Result<u64> {
    let mut clock = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock_gettime initializes clock on success.
    io_result(unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, clock.as_mut_ptr()) })?;
    // SAFETY: the successful clock_gettime above initialized clock.
    let clock = unsafe { clock.assume_init() };
    if clock.tv_sec < 0 || !(0..1_000_000_000).contains(&clock.tv_nsec) {
        return Err(invalid("invalid boot-relative clock"));
    }
    (clock.tv_sec as u64)
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(clock.tv_nsec as u64 / 1_000_000))
        .ok_or_else(|| invalid("boot-relative clock overflow"))
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::Boot(reason.into())
}

#[cfg(test)]
#[path = "boot_linux_tests.rs"]
mod tests;
