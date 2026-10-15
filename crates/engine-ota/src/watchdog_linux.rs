//! Leave the immutable early watchdog untouched until successful promotion is
//! durable. Closing an active handle can refresh its timeout, so observation and
//! failure paths must not open it. This module has no keepalive/arm operation.
use crate::install_lock::InstallLock;
use crate::owned_fs::{open_at, open_directory, owned, read_file, unchanged};
use crate::recovery::{BootObservation, TRIAL_DEADLINE_MS, boot_id};
use crate::{Error, RecoveryStore, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const EARLY_TIMEOUT_SECONDS: u32 = 210;
const MAX_RECORD_BYTES: u64 = 1024;
const DRIVER_IDENTITY: [u8; 32] = *b"Broadcom BCM2835 Watchdog timer\0";
const REQUIRED_OPTIONS: u32 = 0x0080 | 0x0100 | 0x8000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EarlyWatchdog {
    schema: u32,
    boot_id: String,
    trial_id: Sha256,
    armed_boottime_ms: u64,
    timeout_seconds: u32,
}

impl EarlyWatchdog {
    pub fn armed_boottime_ms(&self) -> u64 {
        self.armed_boottime_ms
    }
    pub fn timeout_seconds(&self) -> u32 {
        self.timeout_seconds
    }

    fn validate(&self, boot: &BootObservation) -> Result<()> {
        if self.schema != 1
            || !boot_id(&self.boot_id)
            || self.boot_id != boot.boot_id
            || !boot.tryboot
            || Some(self.trial_id) != boot.trial_id
            || self.timeout_seconds != EARLY_TIMEOUT_SECONDS
            || self.armed_boottime_ms >= TRIAL_DEADLINE_MS
            || self.armed_boottime_ms > boot.uptime_ms
        {
            return Err(invalid("early watchdog evidence does not match this trial"));
        }
        Ok(())
    }

    fn before_expiry(&self, boot: &BootObservation) -> Result<()> {
        self.validate(boot)?;
        if boot.uptime_ms >= self.armed_boottime_ms + u64::from(self.timeout_seconds) * 1000 {
            return Err(invalid("early watchdog allowance has expired"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WatchdogCompletion {
    Stopped,
    AlreadyStopped,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    schema: u32,
    boot_id: String,
    trial_id: Sha256,
}

struct RuntimeDirectory {
    directory: File,
    owner: u32,
}

impl RuntimeDirectory {
    fn open() -> Result<Self> {
        let root = open_directory(Path::new("/"), 0)?;
        let run = open_at(&root, "run", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&run.metadata()?, true, 0, false)?;
        let directory = open_at(
            &run,
            "spacewars-recovery",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        Self::from_directory(directory, 0)
    }

    fn from_directory(directory: File, owner: u32) -> Result<Self> {
        owned(&directory.metadata()?, true, owner, true)?;
        Ok(Self { directory, owner })
    }

    fn read(&self, name: &str) -> Result<Vec<u8>> {
        owned(&self.directory.metadata()?, true, self.owner, true)?;
        let file = open_at(&self.directory, name, libc::O_RDONLY, 0)?;
        owned(&file.metadata()?, false, self.owner, true)?;
        read_file(file, self.owner, MAX_RECORD_BYTES)
    }

    fn early(&self, boot: &BootObservation) -> Result<EarlyWatchdog> {
        let record: EarlyWatchdog = serde_json::from_slice(&self.read("early-watchdog.json")?)?;
        record.validate(boot)?;
        Ok(record)
    }

    fn completed(&self, boot: &BootObservation, token: Sha256) -> Result<bool> {
        let bytes = match self.read("completed-watchdog.json") {
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(false);
            }
            result => result?,
        };
        let record: Completion = serde_json::from_slice(&bytes)?;
        if record.schema != 1 || record.boot_id != boot.boot_id || record.trial_id != token {
            return Err(invalid(
                "watchdog completion belongs to another boot or transaction",
            ));
        }
        Ok(true)
    }

    fn record_completion(&self, boot: &BootObservation, token: Sha256) -> Result<()> {
        let directory = PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()));
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        let record = Completion {
            schema: 1,
            boot_id: boot.boot_id.clone(),
            trial_id: token,
        };
        temporary.write_all(&serde_json::to_vec(&record)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(directory.join("completed-watchdog.json"))
            .map_err(|error| error.error)?;
        self.directory.sync_all()?;
        if !self.completed(boot, token)? {
            return Err(invalid("watchdog completion was not retained"));
        }
        Ok(())
    }
}

pub(crate) fn inspect(boot: &BootObservation) -> Result<EarlyWatchdog> {
    let record = RuntimeDirectory::open()?.early(boot)?;
    record.before_expiry(boot)?;
    Ok(record)
}

pub(crate) fn require_completed(journal: &RecoveryStore, boot: &BootObservation) -> Result<()> {
    check_completed(journal, boot, &RuntimeDirectory::open()?)
}

fn check_completed(
    journal: &RecoveryStore,
    boot: &BootObservation,
    runtime: &RuntimeDirectory,
) -> Result<()> {
    let token = journal.durable_record()?.watchdog_completion(boot)?;
    runtime.early(boot)?;
    if !runtime.completed(boot, token)? {
        return Err(invalid("trial watchdog completion is still pending"));
    }
    Ok(())
}

pub(crate) fn complete(
    lock: &InstallLock,
    journal: &RecoveryStore,
    boot: &BootObservation,
) -> Result<WatchdogCompletion> {
    lock.check()?;
    // Authorize before even accessing the watchdog's volatile records.
    journal.durable_record()?.watchdog_completion(boot)?;
    let runtime = RuntimeDirectory::open()?;
    finish_with(journal, boot, &runtime, || {
        lock.check()?;
        LinuxWatchdog::open()
    })
}

fn finish_with<D: WatchdogDevice>(
    journal: &RecoveryStore,
    boot: &BootObservation,
    runtime: &RuntimeDirectory,
    open: impl FnOnce() -> Result<D>,
) -> Result<WatchdogCompletion> {
    let token = journal.durable_record()?.watchdog_completion(boot)?;
    let early = runtime.early(boot)?;
    if runtime.completed(boot, token)? {
        return Ok(WatchdogCompletion::AlreadyStopped);
    }
    early.before_expiry(boot)?;
    let mut device = open()?;
    let support = device.support()?;
    if support.identity != DRIVER_IDENTITY || support.options & REQUIRED_OPTIONS != REQUIRED_OPTIONS
    {
        return Err(invalid(
            "watchdog driver is not the qualified BCM2835 implementation",
        ));
    }
    if device.timeout()? != EARLY_TIMEOUT_SECONDS as i32 {
        return Err(invalid(
            "watchdog timeout differs from the early recovery contract",
        ));
    }
    // An explicit disable reports a nowayout/stop error. Magic close alone
    // returns success even when the pinned kernel cannot stop the watchdog.
    device.disable()?;
    drop(device);
    runtime.record_completion(boot, token)?;
    Ok(WatchdogCompletion::Stopped)
}

#[repr(C)]
#[derive(Default)]
struct WatchdogInfo {
    options: u32,
    firmware_version: u32,
    identity: [u8; 32],
}
nix::ioctl_read!(get_support, b'W', 0, WatchdogInfo);
nix::ioctl_read!(set_options, b'W', 4, libc::c_int);
nix::ioctl_read!(get_timeout, b'W', 7, libc::c_int);

trait WatchdogDevice {
    fn support(&mut self) -> Result<WatchdogInfo>;
    fn timeout(&mut self) -> Result<i32>;
    fn disable(&mut self) -> Result<()>;
}

struct LinuxWatchdog(File);

impl LinuxWatchdog {
    fn open() -> Result<Self> {
        // These inspections do not start the device. The sole opening below is
        // reachable only after a committed good boot and matching early record.
        let driver = fs::canonicalize("/sys/class/watchdog/watchdog0/device/driver")?;
        if driver != Path::new("/sys/bus/platform/drivers/bcm2835-wdt") {
            return Err(invalid("unexpected watchdog platform driver"));
        }
        let mut number = String::new();
        File::open("/sys/class/watchdog/watchdog0/dev")?
            .take(65)
            .read_to_string(&mut number)?;
        if number.len() > 64 {
            return Err(invalid("oversized watchdog device number"));
        }
        let (major, minor) = number
            .trim()
            .split_once(':')
            .ok_or_else(|| invalid("invalid watchdog device number"))?;
        let parse = |s: &str| {
            s.parse::<u32>()
                .map_err(|_| invalid("invalid watchdog device number"))
        };
        let expected = libc::makedev(parse(major)?, parse(minor)?);
        let before = fs::symlink_metadata("/dev/watchdog0")?;
        if !before.file_type().is_char_device()
            || before.uid() != 0
            || before.mode() & 0o022 != 0
            || before.rdev() != expected
        {
            return Err(invalid("watchdog node differs from the kernel device"));
        }
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open("/dev/watchdog0")?;
        if !unchanged(&before, &file.metadata()?) || file.metadata()?.rdev() != expected {
            return Err(invalid("watchdog node changed while opening"));
        }
        Ok(Self(file))
    }
}

impl WatchdogDevice for LinuxWatchdog {
    fn support(&mut self) -> Result<WatchdogInfo> {
        let mut info = WatchdogInfo::default();
        // SAFETY: the live watchdog descriptor and repr(C) output match UAPI.
        unsafe { get_support(self.0.as_raw_fd(), &mut info) }
            .map_err(|error| Error::Io(error.into()))?;
        Ok(info)
    }
    fn timeout(&mut self) -> Result<i32> {
        let mut timeout = 0;
        // SAFETY: the live descriptor and integer output match UAPI.
        unsafe { get_timeout(self.0.as_raw_fd(), &mut timeout) }
            .map_err(|error| Error::Io(error.into()))?;
        Ok(timeout)
    }
    fn disable(&mut self) -> Result<()> {
        let mut options = 0x0001; // WDIOS_DISABLECARD from linux/watchdog.h.
        // SAFETY: this UAPI ioctl takes an integer pointer despite its _IOR encoding.
        unsafe { set_options(self.0.as_raw_fd(), &mut options) }
            .map_err(|error| Error::Io(error.into()))?;
        Ok(())
    }
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::Boot(reason.into())
}

#[cfg(test)]
#[path = "watchdog_linux_tests.rs"]
mod tests;
