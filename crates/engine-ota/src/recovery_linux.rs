//! Read-only Linux evidence. Device names and /boot/boot_slot are not authority
//! for choosing a writable slot. A writer must recheck this under its shared
//! installation lock and verify the opened block device before any write.
use crate::recovery::Slot;
use crate::{Error, Result, Sha256};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
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
    id: u64,
    device: DeviceNumber,
    root: String,
    path: String,
    filesystem: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MountView {
    id: u64,
    device: DeviceNumber,
}

struct OpenedMount {
    file: File,
    view: MountView,
}

impl OpenedMount {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)?;
        let id = opened_mount_id(&file)?;
        let device = DeviceNumber::from_dev(file.metadata()?.dev());
        Ok(Self {
            file,
            view: MountView { id, device },
        })
    }
}

fn required_mount_handles() -> Result<[OpenedMount; 3]> {
    Ok([
        OpenedMount::open(Path::new("/"))?,
        OpenedMount::open(Path::new("/boot"))?,
        OpenedMount::open(Path::new("/data"))?,
    ])
}

/// SSH access must remain usable even when recovery/boot evidence is broken.
/// Require the real persistent mount, without depending on an inactive slot,
/// boot selector, image identity or a writable recovery journal.
pub(crate) fn open_data_mount() -> Result<File> {
    let file = crate::owned_fs::open_directory(Path::new("/data"), 0)?;
    let view = MountView {
        id: opened_mount_id(&file)?,
        device: DeviceNumber::from_dev(file.metadata()?.dev()),
    };
    let mounts = parse_mountinfo(&bounded_text(
        Path::new("/proc/self/mountinfo"),
        1024 * 1024,
    )?)?;
    visible_mount(&mounts, view, "/data", "ext4")?;
    let root = OpenedMount::open(Path::new("/"))?;
    if root.view.device == view.device {
        return Err(invalid("persistent data is on the root filesystem"));
    }
    Ok(file)
}

pub(crate) fn opened_mount_id(file: &File) -> Result<u64> {
    fd_mount_id(&bounded_text(
        &PathBuf::from(format!("/proc/self/fdinfo/{}", file.as_raw_fd())),
        4096,
    )?)
}

/// Permit the recovery service's writable bind of this exact private subtree,
/// while rejecting a different directory/filesystem mounted over its name.
/// The caller has already verified the complete persistent /data mount.
pub(crate) fn check_data_subtree(directory: &File, data: &File, relative: &str) -> Result<()> {
    let view = MountView {
        id: opened_mount_id(directory)?,
        device: DeviceNumber::from_dev(directory.metadata()?.dev()),
    };
    let source = MountView {
        id: opened_mount_id(data)?,
        device: DeviceNumber::from_dev(data.metadata()?.dev()),
    };
    if view.device != source.device {
        return Err(invalid("private state is on a different filesystem"));
    }
    if view.id == source.id {
        return Ok(());
    }
    let mounts = parse_mountinfo(&bounded_text(
        Path::new("/proc/self/mountinfo"),
        1024 * 1024,
    )?)?;
    data_subtree(&mounts, source, view, relative)
}

fn data_subtree(
    mounts: &[Mount],
    source: MountView,
    target: MountView,
    relative: &str,
) -> Result<()> {
    let find = |id| -> Result<&Mount> {
        let mut found = mounts.iter().filter(|mount| mount.id == id);
        let mount = found
            .next()
            .ok_or_else(|| invalid("missing private-state mount"))?;
        if found.next().is_some() {
            return Err(invalid("ambiguous private-state mount"));
        }
        Ok(mount)
    };
    let (data, bound) = (find(source.id)?, find(target.id)?);
    if data.path != "/data"
        || data.root != "/"
        || data.filesystem != "ext4"
        || data.device != source.device
        || bound.device != source.device
        || target.device != source.device
        || bound.filesystem != "ext4"
        || bound.root != format!("/{relative}")
        || bound.path != format!("/data/{relative}")
    {
        return Err(invalid(
            "private-state bind does not identify its persistent subtree",
        ));
    }
    Ok(())
}

fn fd_mount_id(info: &str) -> Result<u64> {
    let mut ids = info.lines().filter_map(|line| line.strip_prefix("mnt_id:"));
    let id = ids
        .next()
        .ok_or_else(|| invalid("kernel omitted the opened mount ID"))?
        .trim();
    if ids.next().is_some() {
        return Err(invalid("ambiguous opened mount ID"));
    }
    mount_id(id)
}

fn mount_id(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("invalid mount ID"));
    }
    value
        .parse()
        .ok()
        .filter(|id| *id != 0)
        .ok_or_else(|| invalid("invalid mount ID"))
}

/// Inspect /proc, sysfs and block-node metadata. Does not open a block device
/// or watchdog, mount a filesystem, or change anything on the running system.
pub fn inspect_layout() -> Result<DiskLayout> {
    let mountinfo = bounded_text(Path::new("/proc/self/mountinfo"), 1024 * 1024)?;
    let mounts = parse_mountinfo(&mountinfo)?;
    let opened = required_mount_handles()?;
    let views = opened.each_ref().map(|mount| mount.view);
    let root_mount = visible_mount(&mounts, views[0], "/", "ext4")?;
    visible_mount(&mounts, views[1], "/boot", "vfat")?;
    visible_mount(&mounts, views[2], "/data", "ext4")?;
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
    for (before, after) in opened.iter().zip(required_mount_handles()?) {
        if before.view != after.view
            || before.file.metadata()?.ino() != after.file.metadata()?.ino()
        {
            return Err(invalid("visible mount changed during root discovery"));
        }
    }
    validate_layout(
        disk_name,
        disk_device,
        disk_sectors,
        partitions,
        &mounts,
        views,
    )
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
    views: [MountView; 3],
) -> Result<DiskLayout> {
    let root = visible_mount(mounts, views[0], "/", "ext4")?;
    let boot = visible_mount(mounts, views[1], "/boot", "vfat")?;
    let data = visible_mount(mounts, views[2], "/data", "ext4")?;
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

fn visible_mount<'a>(
    mounts: &'a [Mount],
    view: MountView,
    path: &str,
    filesystem: &str,
) -> Result<&'a Mount> {
    let mut matches = mounts.iter().filter(|mount| mount.id == view.id);
    let mount = matches
        .next()
        .ok_or_else(|| invalid("required filesystem is not mounted"))?;
    if matches.next().is_some()
        || mount.path != path
        || mount.device != view.device
        || mount.filesystem != filesystem
        || mount.root != "/"
    {
        return Err(invalid(
            "ambiguous, partial, replaced or unsupported required filesystem",
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
            id: mount_id(fields[0])?,
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
