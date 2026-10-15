//! Read-only Linux evidence. Device names and /boot/boot_slot are not authority
//! for choosing a writable slot. A writer must recheck this under its shared
//! installation lock and verify the opened block device before any write.
use crate::recovery::Slot;
use crate::{Error, Result, Sha256};
use serde::Serialize;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DeviceNumber {
    pub major: u32,
    pub minor: u32,
}

impl DeviceNumber {
    fn parse(value: &str) -> Result<Self> {
        let (major, minor) = value
            .trim()
            .split_once(':')
            .ok_or_else(|| invalid("invalid block device number"))?;
        let number = |s: &str| -> Result<u32> {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("invalid block device number"));
            }
            s.parse()
                .map_err(|_| invalid("invalid block device number"))
        };
        Ok(Self {
            major: number(major)?,
            minor: number(minor)?,
        })
    }

    fn sysfs_name(self) -> String {
        format!("{}:{}", self.major, self.minor)
    }

    pub(crate) fn from_dev(dev: u64) -> Self {
        Self {
            major: libc::major(dev),
            minor: libc::minor(dev),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Partition {
    pub number: u32,
    pub name: String,
    pub device: DeviceNumber,
    pub start_sector: u64,
    pub sectors: u64,
    pub partuuid: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiskLayout {
    pub disk_name: String,
    pub disk_device: DeviceNumber,
    pub disk_sectors: u64,
    pub active: Slot,
    pub partitions: [Partition; 4],
    /// Stable across /dev/sdX enumeration changes, but not a claim that the USB
    /// medium has passed power-loss testing. Cloned media may share UUIDs.
    pub layout_id: Sha256,
}

impl DiskLayout {
    pub fn active_partition(&self) -> &Partition {
        &self.partitions[match self.active {
            Slot::A => 1,
            Slot::B => 2,
        }]
    }
    pub fn inactive_partition(&self) -> &Partition {
        &self.partitions[match self.active {
            Slot::A => 2,
            Slot::B => 1,
        }]
    }
}

#[derive(Clone, Debug)]
struct Mount {
    device: DeviceNumber,
    root: String,
    path: String,
    filesystem: String,
}

/// Inspect /proc, sysfs and block-node metadata. Does not open a block device
/// or watchdog, mount a filesystem, or change anything on the running system.
pub fn inspect_layout() -> Result<DiskLayout> {
    let mountinfo = bounded_text(Path::new("/proc/self/mountinfo"), 1024 * 1024)?;
    let mounts = parse_mountinfo(&mountinfo)?;
    let root_mount = exact_mount(&mounts, "/", "ext4")?;
    let boot_mount = exact_mount(&mounts, "/boot", "vfat")?;
    let data_mount = exact_mount(&mounts, "/data", "ext4")?;
    for mount in [root_mount, boot_mount, data_mount] {
        if DeviceNumber::from_dev(fs::metadata(&mount.path)?.dev()) != mount.device {
            return Err(invalid("mount changed during root discovery"));
        }
    }
    let root_argument = root_argument(&bounded_text(Path::new("/proc/cmdline"), 16 * 1024)?)?;
    let root_node = root_node(&root_argument)?;
    if block_number(&root_node)? != root_mount.device {
        return Err(invalid("kernel root argument disagrees with mounted root"));
    }
    let root_sysfs =
        fs::canonicalize(Path::new("/sys/dev/block").join(root_mount.device.sysfs_name()))?;
    if !root_sysfs.starts_with("/sys/devices") || !root_sysfs.join("partition").is_file() {
        return Err(invalid("root is not a physical disk partition"));
    }
    let disk_sysfs = root_sysfs
        .parent()
        .ok_or_else(|| invalid("missing parent disk"))?;
    let disk_name = component(disk_sysfs)?;
    let disk_device = DeviceNumber::parse(&bounded_text(&disk_sysfs.join("dev"), 64)?)?;
    if block_number(&Path::new("/dev").join(&disk_name))? != disk_device {
        return Err(invalid("parent block device disagrees with sysfs"));
    }
    let disk_sectors = number(&disk_sysfs.join("size"))?;
    let mut partitions = Vec::new();
    for (index, entry) in fs::read_dir(disk_sysfs)?.enumerate() {
        if index >= 256 {
            return Err(invalid("too many parent disk entries"));
        }
        let path = entry?.path();
        if !path.join("partition").is_file() {
            continue;
        }
        if fs::canonicalize(&path)?.parent() != Some(disk_sysfs) {
            return Err(invalid("partition escaped its actual parent disk"));
        }
        let name = component(&path)?;
        let device = DeviceNumber::parse(&bounded_text(&path.join("dev"), 64)?)?;
        if block_number(&Path::new("/dev").join(&name))? != device {
            return Err(invalid("partition node disagrees with sysfs"));
        }
        partitions.push(Partition {
            number: number(&path.join("partition"))?
                .try_into()
                .map_err(|_| invalid("invalid partition index"))?,
            name,
            device,
            start_sector: number(&path.join("start"))?,
            sectors: number(&path.join("size"))?,
            partuuid: partition_uuid(device)?,
        });
    }
    partitions.sort_by_key(|p| p.number);
    let partitions: [Partition; 4] = partitions
        .try_into()
        .map_err(|_| invalid("expected exactly four A/B layout partitions"))?;
    check_uuid_records(&partitions, &udev_partition_ids()?)?;
    if mountinfo != bounded_text(Path::new("/proc/self/mountinfo"), 1024 * 1024)?
        || block_number(&root_node)? != root_mount.device
    {
        return Err(invalid("mounts or root changed during inspection"));
    }
    validate_layout(disk_name, disk_device, disk_sectors, partitions, &mounts)
}

fn udev_partition_ids() -> Result<Vec<(DeviceNumber, String)>> {
    let mut records = Vec::new();
    for (index, entry) in fs::read_dir("/run/udev/data")?.enumerate() {
        if index >= 4096 {
            return Err(invalid("too many udev records"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(device) = name.to_str().and_then(|name| name.strip_prefix('b')) else {
            continue;
        };
        let device = DeviceNumber::parse(device)?;
        if !Path::new("/sys/dev/block")
            .join(device.sysfs_name())
            .exists()
        {
            continue;
        }
        let record = bounded_text(&entry.path(), 64 * 1024)?;
        let mut ids = record
            .lines()
            .filter_map(|line| line.strip_prefix("E:ID_PART_ENTRY_UUID="));
        if let Some(id) = ids.next() {
            if ids.next().is_some() {
                return Err(invalid("ambiguous udev partition record"));
            }
            records.push((device, id.to_ascii_lowercase()));
        }
    }
    Ok(records)
}

fn check_uuid_records(
    partitions: &[Partition; 4],
    records: &[(DeviceNumber, String)],
) -> Result<()> {
    for partition in partitions {
        let matching: Vec<_> = records
            .iter()
            .filter(|(_, uuid)| uuid == &partition.partuuid)
            .collect();
        if matching.len() != 1 || matching[0].0 != partition.device {
            return Err(invalid(
                "partition UUID is absent, duplicated on another disk, or disagrees with udev",
            ));
        }
    }
    Ok(())
}

fn validate_layout(
    disk_name: String,
    disk_device: DeviceNumber,
    disk_sectors: u64,
    partitions: [Partition; 4],
    mounts: &[Mount],
) -> Result<DiskLayout> {
    let root = exact_mount(mounts, "/", "ext4")?;
    let boot = exact_mount(mounts, "/boot", "vfat")?;
    let data = exact_mount(mounts, "/data", "ext4")?;
    let mut previous_end = 0;
    for (index, partition) in partitions.iter().enumerate() {
        let end = partition
            .start_sector
            .checked_add(partition.sectors)
            .ok_or_else(|| invalid("partition geometry overflow"))?;
        if partition.number as usize != index + 1
            || partition.sectors == 0
            || partition.start_sector == 0
            || partition.start_sector < previous_end
            || end > disk_sectors
            || partition.device == disk_device
            || !uuid(&partition.partuuid)
            || partitions[..index]
                .iter()
                .any(|p| p.device == partition.device || p.partuuid == partition.partuuid)
        {
            return Err(invalid("inconsistent A/B partition geometry or identity"));
        }
        previous_end = end;
    }
    if partitions[1].sectors != partitions[2].sectors
        || boot.device != partitions[0].device
        || data.device != partitions[3].device
    {
        return Err(invalid(
            "boot, root pair and data are not the supported shared-disk layout",
        ));
    }
    let active = if root.device == partitions[1].device {
        Slot::A
    } else if root.device == partitions[2].device {
        Slot::B
    } else {
        return Err(invalid("mounted root is neither A nor B"));
    };
    let inactive = match active {
        Slot::A => &partitions[2],
        Slot::B => &partitions[1],
    };
    if mounts.iter().any(|mount| mount.device == inactive.device) {
        return Err(invalid(
            "inactive root is mounted; installation unavailable",
        ));
    }
    let geometry: Vec<_> = partitions
        .iter()
        .map(|p| (p.number, &p.partuuid, p.start_sector, p.sectors))
        .collect();
    let layout_id = Sha256::digest(&serde_json::to_vec(&(disk_sectors, geometry))?);
    Ok(DiskLayout {
        disk_name,
        disk_device,
        disk_sectors,
        active,
        partitions,
        layout_id,
    })
}

fn exact_mount<'a>(mounts: &'a [Mount], path: &str, filesystem: &str) -> Result<&'a Mount> {
    let mut matches = mounts.iter().filter(|mount| mount.path == path);
    let mount = matches
        .next()
        .ok_or_else(|| invalid("required filesystem is not mounted"))?;
    if matches.next().is_some() || mount.filesystem != filesystem || mount.root != "/" {
        return Err(invalid(
            "ambiguous, bind-mounted or unsupported required filesystem",
        ));
    }
    Ok(mount)
}

fn parse_mountinfo(text: &str) -> Result<Vec<Mount>> {
    let mut mounts = Vec::new();
    for line in text.lines() {
        if mounts.len() >= 4096 {
            return Err(invalid("too many mounts"));
        }
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        let separator = fields
            .iter()
            .position(|field| *field == "-")
            .ok_or_else(|| invalid("malformed mountinfo"))?;
        if separator < 6 || fields.len() != separator + 4 {
            return Err(invalid("malformed mountinfo"));
        }
        mounts.push(Mount {
            device: DeviceNumber::parse(fields[2])?,
            root: fields[3].into(),
            path: fields[4].into(),
            filesystem: fields[separator + 1].into(),
        });
    }
    Ok(mounts)
}

fn root_argument(cmdline: &str) -> Result<String> {
    let roots: Vec<_> = cmdline
        .split_ascii_whitespace()
        .filter_map(|arg| arg.strip_prefix("root="))
        .collect();
    if roots.len() != 1 || roots[0].is_empty() {
        return Err(invalid("kernel command line must contain exactly one root"));
    }
    Ok(roots[0].into())
}

fn root_node(argument: &str) -> Result<PathBuf> {
    if let Some(name) = argument.strip_prefix("/dev/") {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid("unsupported root device path"));
        }
        return Ok(Path::new("/dev").join(name));
    }
    for (prefix, directory) in [
        ("PARTUUID=", "/dev/disk/by-partuuid"),
        ("UUID=", "/dev/disk/by-uuid"),
    ] {
        if let Some(value) = argument.strip_prefix(prefix) {
            if !uuid(value) {
                return Err(invalid("invalid root UUID"));
            }
            return Ok(Path::new(directory).join(value));
        }
    }
    Err(invalid("unsupported kernel root selector"))
}

fn partition_uuid(device: DeviceNumber) -> Result<String> {
    let mut found = None;
    for (index, entry) in fs::read_dir("/dev/disk/by-partuuid")?.enumerate() {
        if index >= 256 {
            return Err(invalid("too many partition aliases"));
        }
        let entry = entry?;
        if block_number(&entry.path())? != device {
            continue;
        }
        let name = component(&entry.path())?;
        if !uuid(&name) || found.replace(name).is_some() {
            return Err(invalid("ambiguous partition UUID"));
        }
    }
    found.ok_or_else(|| invalid("partition UUID is unavailable"))
}

fn block_number(path: &Path) -> Result<DeviceNumber> {
    let metadata = fs::metadata(path)?;
    if !metadata.file_type().is_block_device() {
        return Err(invalid("expected a block device node"));
    }
    Ok(DeviceNumber::from_dev(metadata.rdev()))
}

fn bounded_text(path: &Path, maximum: u64) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("kernel evidence exceeds size limit"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("kernel evidence is not UTF-8"))
}

fn number(path: &Path) -> Result<u64> {
    let text = bounded_text(path, 64)?;
    let value = text.trim();
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("invalid sysfs integer"));
    }
    value.parse().map_err(|_| invalid("sysfs integer overflow"))
}

fn component(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid("invalid block device name"))
}

fn uuid(value: &str) -> bool {
    (value.len() == 11 || value.len() == 36)
        && value.bytes().enumerate().all(|(i, b)| {
            if i == 8 || (value.len() == 36 && [13, 18, 23].contains(&i)) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

fn invalid(message: &str) -> Error {
    Error::Store(format!("root layout: {message}"))
}

#[cfg(test)]
#[path = "recovery_linux_tests.rs"]
mod tests;
