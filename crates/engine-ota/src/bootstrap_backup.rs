//! Read-only raw-device recovery copies for supervised bootstrap. Completed
//! copies are immutable; a crash may restart only an unpublished pending copy.
use super::{BootstrapBinding, invalid};
use crate::boot_linux::{check_mount, text};
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file, unchanged};
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use crate::{Blob, Error, Result, Sha256, verify_blob};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const DIRECTORY: &str = "recovery-copies";
const IMAGE: &str = "partition.img";
const RECORD: &str = "record.json";
const RECORD_LIMIT: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupPartition {
    Boot,
    InactiveRoot,
}
impl BackupPartition {
    fn name(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::InactiveRoot => "inactive-root",
        }
    }
    fn bytes(self, binding: &BootstrapBinding) -> u64 {
        match self {
            Self::Boot => binding.boot_partition_bytes,
            Self::InactiveRoot => binding.inactive_partition_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupStage {
    Copying,
    CheckingCopy,
    CheckingSource,
    Reusing,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct BackupProgress {
    pub partition: BackupPartition,
    pub stage: BackupStage,
    pub bytes: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupReceipt {
    pub schema: u32,
    pub binding_sha256: Sha256,
    pub partition: BackupPartition,
    pub image: Blob,
}

#[derive(Debug, Serialize)]
pub struct BackupReport {
    pub schema: u32,
    pub scope: &'static str,
    pub copies_sha256: Sha256,
    pub boot: BackupReceipt,
    pub inactive_root: BackupReceipt,
    pub reused_copies: u32,
    pub ready_for_write: bool,
}

#[derive(Debug, Serialize)]
pub struct BackupExportCheck {
    pub schema: u32,
    pub scope: &'static str,
    pub copies_sha256: Sha256,
    pub package_sha256: Sha256,
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub boot_bytes: u64,
    pub inactive_root_bytes: u64,
}

/// Workstation-only verification of an independently copied active directory.
/// The digest must come from the authenticated device backup report, separately
/// from these files. This reads regular files and performs no device operations.
pub fn verify_backup_export(path: &Path, expected: Sha256) -> Result<BackupExportCheck> {
    let owner = unsafe { libc::geteuid() };
    let active = open_directory(path, owner)?;
    let binding = super::state::read_binding(&active, owner)?;
    let copies = super::state::optional_directory(&active, DIRECTORY, owner)?
        .ok_or_else(|| invalid("missing exported recovery copies"))?;
    inventory(&copies, &["boot", "inactive-root"])?;
    let open_copy = |part: BackupPartition| -> Result<(File, BackupReceipt)> {
        let directory = super::state::optional_directory(&copies, part.name(), owner)?
            .ok_or_else(|| invalid("incomplete recovery export"))?;
        let receipt = read_receipt(&directory, owner, &binding, part)?;
        check_named_directory(&copies, part.name(), &directory, owner)?;
        Ok((directory, receipt))
    };
    let (boot_directory, boot) = open_copy(BackupPartition::Boot)?;
    let (inactive_directory, inactive) = open_copy(BackupPartition::InactiveRoot)?;
    let actual = copies_digest(&boot, &inactive)?;
    if actual != expected {
        return Err(invalid(
            "export differs from the independently selected device backup digest",
        ));
    }
    // Compare the separately selected digest before reading multi-gigabyte
    // copies, then recheck each record/name along with the actual file bytes.
    for (directory, receipt) in [(&boot_directory, &boot), (&inactive_directory, &inactive)] {
        if &verify_copy(directory, owner, &binding, receipt.partition)? != receipt {
            return Err(invalid("export record changed during verification"));
        }
        check_named_directory(&copies, receipt.partition.name(), directory, owner)?;
    }
    check_named_directory(&active, DIRECTORY, &copies, owner)?;
    let named = open_directory(path, owner)?;
    if named.metadata()?.dev() != active.metadata()?.dev()
        || named.metadata()?.ino() != active.metadata()?.ino()
    {
        return Err(invalid("export directory was replaced"));
    }
    Ok(BackupExportCheck {
        schema: 1,
        scope: "verified raw recovery export; data/config copies, live-device state and physical restore qualification are separate",
        copies_sha256: actual,
        package_sha256: binding.package_sha256,
        device_id: binding.device_id,
        layout_id: binding.layout_id,
        boot_bytes: boot.image.bytes,
        inactive_root_bytes: inactive.image.bytes,
    })
}

fn copies_digest(boot: &BackupReceipt, inactive: &BackupReceipt) -> Result<Sha256> {
    Ok(Sha256::digest(&serde_json::to_vec(&(boot, inactive))?))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    receipt: BackupReceipt,
    checksum: Sha256,
}

/// Completed, hash-verified copies already occupy /data; do not demand room
/// for a second copy when checking or resuming the same reservation.
pub(super) fn verified_bytes(parent: &File, owner: u32, binding: &BootstrapBinding) -> Result<u64> {
    let active = super::state::optional_directory(parent, super::state::ACTIVE, owner)?
        .ok_or_else(|| invalid("missing bootstrap reservation"))?;
    let Some(copies) = super::state::optional_directory(&active, DIRECTORY, owner)? else {
        return Ok(0);
    };
    inventory(
        &copies,
        &[
            "boot",
            ".boot.pending",
            "inactive-root",
            ".inactive-root.pending",
        ],
    )?;
    let mut bytes = 0u64;
    for part in [BackupPartition::Boot, BackupPartition::InactiveRoot] {
        if let Some(completed) = super::state::optional_directory(&copies, part.name(), owner)? {
            if super::state::optional_directory(
                &copies,
                &format!(".{}.pending", part.name()),
                owner,
            )?
            .is_some()
            {
                return Err(invalid("completed and pending recovery copies conflict"));
            }
            let receipt = verify_copy(&completed, owner, binding, part)?;
            check_named_directory(&copies, part.name(), &completed, owner)?;
            bytes = bytes
                .checked_add(receipt.image.bytes)
                .ok_or_else(|| invalid("recovery-copy credit overflow"))?;
        }
    }
    check_named_directory(&active, DIRECTORY, &copies, owner)?;
    check_named_directory(parent, super::state::ACTIVE, &active, owner)?;
    Ok(bytes)
}

/// This operation does not remount filesystems, stop services or initialize a
/// reservation. It requires the operator's prepared, read-only backup window.
pub(super) fn create(
    parent: &File,
    data: &File,
    binding: &BootstrapBinding,
    layout: &DiskLayout,
    lock: &InstallLock,
    needed: u64,
    report: &mut impl FnMut(BackupProgress),
) -> Result<BackupReport> {
    lock.check()?;
    let root = open_directory(Path::new("/"), 0)?;
    let boot = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    require_read_only_boot(&boot, layout)?;
    let active = super::state::optional_directory(parent, super::state::ACTIVE, 0)?
        .ok_or_else(|| invalid("missing bootstrap reservation"))?;
    let copies = directory(&active, DIRECTORY, 0)?;
    inventory(
        &copies,
        &[
            "boot",
            ".boot.pending",
            "inactive-root",
            ".inactive-root.pending",
        ],
    )?;
    let boot_source = RawSource::open(layout, BackupPartition::Boot)?;
    let inactive_source = RawSource::open(layout, BackupPartition::InactiveRoot)?;
    if boot_source.bytes != binding.boot_partition_bytes
        || inactive_source.bytes != binding.inactive_partition_bytes
    {
        return Err(invalid(
            "recovery-copy capacities differ from the reservation",
        ));
    }
    // These are unpublished scratch files from this same durable reservation.
    // Reclaim them before measuring free space instead of guessing how many
    // blocks a sparse/partial file might return to the filesystem.
    reset_pending(&copies, 0, binding)?;
    let available = free_space(data)?;
    if needed > available {
        return Err(invalid(
            "insufficient data space for recovery copies and reserve",
        ));
    }
    let mut reused = 0;
    let mut copy = |source: RawSource, part| -> Result<BackupReceipt> {
        let (receipt, existing) = copy_one(
            &copies,
            0,
            binding,
            part,
            source.file.try_clone()?,
            report,
            || {
                lock.check()?;
                source.check()?;
                require_read_only_boot(&boot, layout)
            },
        )?;
        reused += u32::from(existing);
        Ok(receipt)
    };
    let boot_receipt = copy(boot_source, BackupPartition::Boot)?;
    let inactive_root = copy(inactive_source, BackupPartition::InactiveRoot)?;
    check_named_directory(parent, super::state::ACTIVE, &active, 0)?;
    check_named_directory(&active, DIRECTORY, &copies, 0)?;
    lock.check()?;
    require_read_only_boot(&boot, layout)?;
    Ok(BackupReport {
        schema: 1,
        scope: "verified raw boot and inactive-root recovery copies; data/config backup, independent export, legacy closure and installation remain pending",
        copies_sha256: copies_digest(&boot_receipt, &inactive_root)?,
        boot: boot_receipt,
        inactive_root,
        reused_copies: reused,
        ready_for_write: false,
    })
}

struct RawSource {
    file: File,
    partition: Partition,
    bytes: u64,
}
nix::ioctl_read!(block_size, 0x12, 114, u64);
impl RawSource {
    fn open(layout: &DiskLayout, part: BackupPartition) -> Result<Self> {
        let partition = match part {
            BackupPartition::Boot => &layout.partitions[0],
            BackupPartition::InactiveRoot => layout.inactive_partition(),
        }
        .clone();
        if partition.device == layout.active_partition().device
            || partition.device == layout.partitions[3].device
        {
            return Err(invalid("recovery-copy source is not boot or inactive root"));
        }
        let bytes = partition
            .sectors
            .checked_mul(512)
            .ok_or_else(|| invalid("raw backup capacity overflow"))?;
        if bytes == 0 || bytes > super::package::MAX_ROOT_BYTES {
            return Err(invalid("unsupported recovery-copy size"));
        }
        let root = open_directory(Path::new("/"), 0)?;
        let dev = open_at(&root, "dev", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&dev.metadata()?, true, 0, false)?;
        let node = open_at(&dev, &partition.name, libc::O_PATH, 0)?;
        Self::node(&node.metadata()?, &partition)?;
        // The inactive filesystem must be unmounted everywhere. Boot is mounted
        // read-only and cannot be claimed with O_EXCL; both descriptors are RO.
        let file = open_at(
            &dev,
            &partition.name,
            libc::O_RDONLY
                | if part == BackupPartition::InactiveRoot {
                    libc::O_EXCL
                } else {
                    0
                },
            0,
        )?;
        let source = Self {
            file,
            partition,
            bytes,
        };
        source.check()?;
        Ok(source)
    }
    fn node(metadata: &Metadata, partition: &Partition) -> Result<()> {
        if !metadata.file_type().is_block_device()
            || metadata.uid() != 0
            || ![0, 6].contains(&metadata.gid())
            || metadata.mode() & 0o007 != 0
            || DeviceNumber::from_dev(metadata.rdev()) != partition.device
        {
            return Err(invalid(
                "recovery-copy node differs from the observed root-owned device",
            ));
        }
        Ok(())
    }
    fn check(&self) -> Result<()> {
        Self::node(&self.file.metadata()?, &self.partition)?;
        let mut bytes = 0u64;
        // SAFETY: the live descriptor's BLKGETSIZE64 fills a u64.
        unsafe { block_size(self.file.as_raw_fd(), &mut bytes) }
            .map_err(|e| Error::Io(e.into()))?;
        if bytes != self.bytes {
            return Err(invalid("recovery-copy source capacity changed"));
        }
        Ok(())
    }
}

fn require_read_only_boot(boot: &File, layout: &DiskLayout) -> Result<()> {
    check_mount(boot, layout, false)?;
    let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: boot remains live and fstatvfs initializes status on success.
    io_result(unsafe { libc::fstatvfs(boot.as_raw_fd(), status.as_mut_ptr()) })?;
    if unsafe { status.assume_init() }.f_flag & libc::ST_RDONLY == 0 {
        return Err(invalid(
            "boot must be mounted read-only before taking recovery copies",
        ));
    }
    read_only_mounts(
        &text("/proc/self/mountinfo", 1024 * 1024)?,
        layout.partitions[0].device,
    )
}

fn read_only_mounts(mounts: &str, device: DeviceNumber) -> Result<()> {
    let device = format!("{}:{}", device.major, device.minor);
    let mut found = false;
    for line in mounts.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        let separator = fields
            .iter()
            .position(|s| *s == "-")
            .ok_or_else(|| invalid("malformed mount inventory"))?;
        if separator < 6 || fields.len() != separator + 4 {
            return Err(invalid("malformed mount inventory"));
        }
        if fields[2] != device {
            continue;
        }
        found = true;
        let readonly = |options: &str| {
            options
                .split(',')
                .filter(|s| matches!(*s, "ro" | "rw"))
                .collect::<Vec<_>>()
                == ["ro"]
        };
        if fields[separator + 1] != "vfat"
            || !readonly(fields[5])
            || !readonly(fields[separator + 3])
        {
            return Err(invalid("boot has a writable mount or superblock"));
        }
    }
    if !found {
        return Err(invalid("boot mount disappeared"));
    }
    Ok(())
}

fn copy_one(
    copies: &File,
    owner: u32,
    binding: &BootstrapBinding,
    part: BackupPartition,
    mut source: impl Read + Seek,
    report: &mut impl FnMut(BackupProgress),
    mut recheck: impl FnMut() -> Result<()>,
) -> Result<(BackupReceipt, bool)> {
    let pending_name = format!(".{}.pending", part.name());
    if let Some(completed) = super::state::optional_directory(copies, part.name(), owner)? {
        if super::state::optional_directory(copies, &pending_name, owner)?.is_some() {
            return Err(invalid("completed and pending recovery copies conflict"));
        }
        report(BackupProgress {
            partition: part,
            stage: BackupStage::Reusing,
            bytes: 0,
            total: part.bytes(binding),
        });
        let receipt = verify_copy(&completed, owner, binding, part)?;
        recheck()?;
        check_named_directory(copies, part.name(), &completed, owner)?;
        completed.sync_all()?;
        copies.sync_all()?;
        return Ok((receipt, true));
    }
    let pending = directory(copies, &pending_name, owner)?;
    inventory(&pending, &[IMAGE, RECORD])?;
    let mut destination = writable(&pending, IMAGE, owner)?;
    let bytes = part.bytes(binding);
    if destination.metadata()?.len() > bytes {
        return Err(invalid("oversized pending recovery copy"));
    }
    // Only this unpublished, fixed-name copy is restarted. Completed copies
    // above are verified and never opened for writing.
    recheck()?;
    destination.set_len(0)?;
    source.rewind()?;
    let copied = stream(
        &mut source,
        &mut destination,
        bytes,
        part,
        BackupStage::Copying,
        report,
    )?;
    destination.sync_all()?;
    destination.rewind()?;
    let checked = stream(
        &mut destination,
        &mut std::io::sink(),
        bytes,
        part,
        BackupStage::CheckingCopy,
        report,
    )?;
    source.rewind()?;
    let observed_again = stream(
        &mut source,
        &mut std::io::sink(),
        bytes,
        part,
        BackupStage::CheckingSource,
        report,
    )?;
    if copied != checked || copied != observed_again {
        return Err(invalid("recovery copy or source changed while copying"));
    }
    recheck()?;
    let receipt = BackupReceipt {
        schema: 1,
        binding_sha256: Sha256::digest(&serde_json::to_vec(binding)?),
        partition: part,
        image: copied,
    };
    let encoded = serde_json::to_vec(&Envelope {
        checksum: Sha256::digest(&serde_json::to_vec(&receipt)?),
        receipt: receipt.clone(),
    })?;
    let mut record = writable(&pending, RECORD, owner)?;
    if record.metadata()?.len() > RECORD_LIMIT || encoded.len() as u64 > RECORD_LIMIT {
        return Err(invalid("oversized recovery-copy record"));
    }
    record.set_len(0)?;
    record.write_all(&encoded)?;
    record.sync_all()?;
    pending.sync_all()?;
    verify_copy(&pending, owner, binding, part)?;
    check_named_directory(copies, &pending_name, &pending, owner)?;
    recheck()?;
    let from = name(&pending_name)?;
    let to = name(part.name())?;
    // SAFETY: fixed validated C names and the private directory stay live.
    io_result(unsafe {
        libc::renameat2(
            copies.as_raw_fd(),
            from.as_ptr(),
            copies.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    })?;
    copies.sync_all()?;
    check_named_directory(copies, part.name(), &pending, owner)?;
    Ok((receipt, false))
}

fn verify_copy(
    directory: &File,
    owner: u32,
    binding: &BootstrapBinding,
    part: BackupPartition,
) -> Result<BackupReceipt> {
    let receipt = read_receipt(directory, owner, binding, part)?;
    let mut image = super::device::private_asset(directory, IMAGE, owner)?;
    let original = image.metadata()?;
    if original.len() != receipt.image.bytes {
        return Err(invalid("recovery copy length changed"));
    }
    verify_blob(&mut image, &receipt.image)?;
    let named = super::device::private_asset(directory, IMAGE, owner)?;
    if !unchanged(&original, &image.metadata()?) || !unchanged(&original, &named.metadata()?) {
        return Err(invalid("recovery copy changed during verification"));
    }
    Ok(receipt)
}

fn read_receipt(
    directory: &File,
    owner: u32,
    binding: &BootstrapBinding,
    part: BackupPartition,
) -> Result<BackupReceipt> {
    inventory(directory, &[IMAGE, RECORD])?;
    let record = super::device::private_asset(directory, RECORD, owner)?;
    let envelope: Envelope = serde_json::from_slice(&read_file(record, owner, RECORD_LIMIT)?)?;
    let receipt = envelope.receipt;
    if receipt.schema != 1
        || receipt.partition != part
        || receipt.image.bytes == 0
        || receipt.image.bytes > super::package::MAX_ROOT_BYTES
        || receipt.image.bytes != part.bytes(binding)
        || receipt.binding_sha256 != Sha256::digest(&serde_json::to_vec(binding)?)
        || envelope.checksum != Sha256::digest(&serde_json::to_vec(&receipt)?)
    {
        return Err(invalid(
            "recovery copy does not match its preparation binding",
        ));
    }
    Ok(receipt)
}

fn stream(
    source: &mut impl Read,
    output: &mut impl Write,
    bytes: u64,
    part: BackupPartition,
    stage: BackupStage,
    report: &mut impl FnMut(BackupProgress),
) -> Result<Blob> {
    if bytes == 0 || bytes > super::package::MAX_ROOT_BYTES {
        return Err(invalid("unsupported recovery-copy length"));
    }
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = Hasher::new();
    let mut copied = 0;
    let mut last = Instant::now();
    report(BackupProgress {
        partition: part,
        stage,
        bytes: 0,
        total: bytes,
    });
    while copied < bytes {
        let limit = (bytes - copied).min(buffer.len() as u64) as usize;
        let count = source.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(invalid("truncated recovery-copy source"));
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        copied += count as u64;
        if copied == bytes || last.elapsed() >= Duration::from_millis(250) {
            report(BackupProgress {
                partition: part,
                stage,
                bytes: copied,
                total: bytes,
            });
            last = Instant::now();
        }
    }
    if source.read(&mut buffer[..1])? != 0 {
        return Err(invalid(
            "recovery-copy source exceeds its observed capacity",
        ));
    }
    Ok(Blob {
        bytes,
        sha256: format!("{:x}", hash.finalize())
            .try_into()
            .expect("SHA-256 hex"),
    })
}

fn writable(directory: &File, component: &str, owner: u32) -> Result<File> {
    let pinned = match super::device::private_asset(directory, component, owner) {
        Ok(file) => file,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => open_at(
            directory,
            component,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?,
        Err(error) => return Err(error),
    };
    owned(&pinned.metadata()?, false, owner, true)?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?)
}

fn directory(parent: &File, component: &str, owner: u32) -> Result<File> {
    if let Some(directory) = super::state::optional_directory(parent, component, owner)? {
        return Ok(directory);
    }
    let component_c = name(component)?;
    // SAFETY: the held parent and validated C name remain live.
    io_result(unsafe { libc::mkdirat(parent.as_raw_fd(), component_c.as_ptr(), 0o700) })?;
    parent.sync_all()?;
    super::state::optional_directory(parent, component, owner)?
        .ok_or_else(|| invalid("recovery-copy directory disappeared"))
}

fn check_named_directory(parent: &File, component: &str, held: &File, owner: u32) -> Result<()> {
    let named = super::state::optional_directory(parent, component, owner)?
        .ok_or_else(|| invalid("recovery-copy directory disappeared"))?;
    if named.metadata()?.dev() != held.metadata()?.dev()
        || named.metadata()?.ino() != held.metadata()?.ino()
    {
        return Err(invalid("recovery-copy directory was replaced"));
    }
    Ok(())
}

fn inventory(directory: &File, allowed: &[&str]) -> Result<()> {
    for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        let name = entry?.file_name();
        if !allowed.iter().any(|allowed| name == *allowed) {
            return Err(invalid("unexpected recovery-copy state"));
        }
    }
    Ok(())
}

fn reset_pending(copies: &File, owner: u32, binding: &BootstrapBinding) -> Result<()> {
    let mut images = Vec::new();
    for part in [BackupPartition::Boot, BackupPartition::InactiveRoot] {
        let pending_name = format!(".{}.pending", part.name());
        let Some(pending) = super::state::optional_directory(copies, &pending_name, owner)? else {
            continue;
        };
        if super::state::optional_directory(copies, part.name(), owner)?.is_some() {
            return Err(invalid("completed and pending recovery copies conflict"));
        }
        inventory(&pending, &[IMAGE, RECORD])?;
        for (component, limit) in [(IMAGE, part.bytes(binding)), (RECORD, RECORD_LIMIT)] {
            match super::device::private_asset(&pending, component, owner) {
                Ok(file) => {
                    if file.metadata()?.len() > limit {
                        return Err(invalid("oversized pending recovery copy"));
                    }
                    if component == IMAGE {
                        images.push(writable(&pending, IMAGE, owner)?);
                    }
                }
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        check_named_directory(copies, &pending_name, &pending, owner)?;
    }
    // Validate every pending object before changing any of them. Completed
    // copies and the original source partitions are never opened for writing.
    for image in images {
        image.set_len(0)?;
        image.sync_all()?;
    }
    copies.sync_all()?;
    Ok(())
}

fn free_space(directory: &File) -> Result<u64> {
    let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: directory remains live and fstatvfs initializes status on success.
    io_result(unsafe { libc::fstatvfs(directory.as_raw_fd(), status.as_mut_ptr()) })?;
    let status = unsafe { status.assume_init() };
    if status.f_flag & libc::ST_RDONLY != 0 {
        return Err(invalid("data filesystem is read-only"));
    }
    status
        .f_bavail
        .checked_mul(status.f_frsize)
        .ok_or_else(|| invalid("recovery-copy free-space overflow"))
}

#[cfg(test)]
#[path = "bootstrap_backup_tests.rs"]
mod tests;
