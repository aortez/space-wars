//! Root-operator package validation. This is deliberately separate from TUF
//! VerifiedCandidate authority and never writes a root or boot selector.
use super::{
    Board, BootPlan, BootstrapPackage, LegacyBootEvidence, PlanRequest, STAGE_DIRECTORY,
    active_lines, inspect_legacy_boot, invalid,
};
use crate::boot::{
    BootCommandLine, BootManifest, MAX_BOOT_MANIFEST_BYTES, SlotRoots, UNPREPARED_TRIAL,
};
use crate::install_lock::InstallLock;
use crate::owned_fs::{open_at, owned, read_file, unchanged};
use crate::{
    Blob, Error, ImageIdentity, MAX_COMPRESSED_BYTES, MAX_IMAGE_IDENTITY_BYTES, Result, Sha256,
    verify_blob,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata};
use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

const MAX_RECORD_BYTES: u64 = 256 * 1024;
pub(super) const DIRECTORY: &str = "spacewars-bootstrap";
const PACKAGE: &str = "package";

#[derive(Debug, Serialize)]
pub struct PackageCheck {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub image: ImageIdentity,
    pub plan: BootPlan,
    pub device_id: Sha256,
    pub layout_id: Sha256,
    pub boot_id: String,
    pub boot_space: Capacity,
    pub data_required_bytes: u64,
    pub data_available_bytes: u64,
    /// Keys, backups, legacy privileges and handoff must still be established.
    pub ready_for_write: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Capacity {
    pub available_bytes: u64,
    pub staged_allocation_bytes: u64,
    pub canonical_growth_bytes: u64,
    pub largest_replacement_bytes: u64,
    pub directory_reserve_bytes: u64,
    pub spare_reserve_bytes: u64,
    pub required_bytes: u64,
}

/// Root-only, fixed-path check of /data/spacewars-bootstrap/package. The hash
/// is supplied through the separately authenticated operator connection after
/// reviewing the workstation package, never by the kiosk/update socket.
pub fn check_installed_package(expected: Sha256) -> Result<PackageCheck> {
    Ok(prepare_or_check(expected, false)?.check)
}

/// Records the first observed inputs without installing anything. Repeat calls
/// resume only the same unchanged legacy device and package; an incomplete or
/// conflicting reservation requires explicit operator reconciliation.
pub fn prepare_installed_package(expected: Sha256) -> Result<super::PreparationReport> {
    prepare_or_check(expected, true)
}

fn prepare_or_check(expected: Sha256, reserve: bool) -> Result<super::PreparationReport> {
    let lock = InstallLock::acquire()?;
    let data = crate::recovery_linux::open_data_mount()?;
    reject_provisioned(&data)?;
    let parent = open_at(&data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&parent.metadata()?, true, 0, true)?;
    if parent.metadata()?.dev() != data.metadata()?.dev() {
        return Err(invalid("bootstrap package is not on the data filesystem"));
    }
    for entry in fs::read_dir(format!("/proc/self/fd/{}", parent.as_raw_fd()))? {
        let name = entry?.file_name();
        if name != PACKAGE && name != super::state::ACTIVE {
            return Err(invalid(
                "existing bootstrap state needs resume/reconciliation",
            ));
        }
    }
    let directory = open_at(&parent, PACKAGE, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let mut package = OperatorPackage::under(directory, 0, expected)?;
    let legacy = inspect_legacy_boot()?;
    if crate::recovery_linux::DeviceNumber::from_dev(data.metadata()?.dev())
        != legacy.layout.partitions[3].device
    {
        return Err(invalid("persistent data changed during package inspection"));
    }
    let report = package.check_device(&legacy)?;
    let observed = super::BootstrapBinding::observed(&report, &legacy)?;
    let existing = super::state::existing(&parent, 0, &observed)?;
    lock.check()?;
    package.check()?;
    let named_parent = open_at(&data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let named = open_at(
        &named_parent,
        PACKAGE,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    let current_data = crate::recovery_linux::open_data_mount()?;
    if parent.metadata()?.dev() != named_parent.metadata()?.dev()
        || parent.metadata()?.ino() != named_parent.metadata()?.ino()
        || named.metadata()?.dev() != package.directory.metadata()?.dev()
        || named.metadata()?.ino() != package.directory.metadata()?.ino()
        || current_data.metadata()?.dev() != data.metadata()?.dev()
        || current_data.metadata()?.ino() != data.metadata()?.ino()
    {
        return Err(invalid("bootstrap package directory was replaced"));
    }
    let preserved_access = if reserve {
        Some(crate::access_linux::preserved_for_bootstrap(&lock)?)
    } else {
        None
    };
    let (binding, resumed) = if reserve {
        super::state::begin(&parent, 0, &observed)?
    } else {
        let resumed = existing.is_some();
        (existing.unwrap_or(observed), resumed)
    };
    lock.check()?;
    Ok(super::PreparationReport {
        check: report,
        binding,
        resumed,
        preserved_access,
        scope: if reserve {
            "durable preparation and preserved key roles; recovery copies, login verification and installation still pending"
        } else {
            "read-only package check; no new preparation recorded"
        },
    })
}

fn reject_provisioned(data: &File) -> Result<()> {
    let system = match open_at(
        data,
        "spacewars-system",
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    ) {
        Ok(system) => system,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    owned(&system.metadata()?, true, 0, true)?;
    // Existing time metadata is preserved. Recovery, trust, staged OTA bytes
    // or unknown service state must not be silently treated as a fresh device.
    for entry in fs::read_dir(format!("/proc/self/fd/{}", system.as_raw_fd()))? {
        if entry?.file_name() != "time" {
            return Err(invalid(
                "existing device-service state requires explicit reconciliation",
            ));
        }
    }
    Ok(())
}

struct CheckedFile {
    file: File,
    original: Metadata,
}

pub(super) struct OperatorPackage {
    directory: File,
    owner: u32,
    expected: Sha256,
    record: BootstrapPackage,
    files: BTreeMap<String, CheckedFile>,
}

impl OperatorPackage {
    fn under(directory: File, owner: u32, expected: Sha256) -> Result<Self> {
        owned(&directory.metadata()?, true, owner, true)?;
        let record_file = private_asset(&directory, "bundle.json", owner)?;
        let original = record_file.metadata()?;
        let bytes = read_file(record_file.try_clone()?, owner, MAX_RECORD_BYTES)?;
        if Sha256::digest(&bytes) != expected {
            return Err(invalid(
                "bootstrap package differs from the operator-selected hash",
            ));
        }
        let record: BootstrapPackage = serde_json::from_slice(&bytes)?;
        record.image.validate()?;
        if record.schema != 1
            || record.purpose != "supervised-bootstrap"
            || record.image.source.dirty
            || record.image.machine != "raspberrypi-spacewars"
            || record.image.architecture != "aarch64"
            || record.image.updater_protocol != crate::UPDATER_PROTOCOL
            || record.image.recovery_protocol != crate::recovery::RECOVERY_PROTOCOL
            || record.compressed.bytes == 0
            || record.compressed.bytes > MAX_COMPRESSED_BYTES
            || !(4096..=super::package::MAX_ROOT_BYTES).contains(&record.expanded.bytes)
            || record.files.len() > 1024
            || record.files.is_empty()
        {
            return Err(invalid(
                "invalid or unsupported supervised bootstrap package",
            ));
        }
        let mut files = BTreeMap::new();
        files.insert(
            "bundle.json".into(),
            CheckedFile {
                file: record_file,
                original,
            },
        );
        let total = record.files.values().try_fold(0u64, |total, blob| {
            total
                .checked_add(blob.bytes)
                .ok_or_else(|| invalid("bootstrap package size overflow"))
        })?;
        if total > 3 * 1024 * 1024 * 1024 {
            return Err(invalid("bootstrap package exceeds its total bound"));
        }
        for (path, blob) in &record.files {
            if !package_path(path)
                || blob.bytes
                    > if path == "rootfs.ext4.zst" {
                        MAX_COMPRESSED_BYTES
                    } else {
                        crate::MAX_IMAGE_EXECUTABLE_BYTES
                    }
            {
                return Err(invalid("unsafe or oversized bootstrap package file"));
            }
            let mut file = private_asset(&directory, path, owner)?;
            let original = file.metadata()?;
            if original.len() != blob.bytes {
                return Err(invalid("bootstrap package file length changed"));
            }
            verify_blob(&mut file, blob)?;
            files.insert(path.clone(), CheckedFile { file, original });
        }
        let checksum_file = private_asset(&directory, "SHA256SUMS", owner)?;
        let original = checksum_file.metadata()?;
        let checksum = read_file(checksum_file.try_clone()?, owner, MAX_RECORD_BYTES)?;
        let mut expected_checksum = record
            .files
            .iter()
            .map(|(p, b)| format!("{}  {p}\n", b.sha256))
            .collect::<String>();
        expected_checksum.push_str(&format!("{expected}  bundle.json\n"));
        if checksum != expected_checksum.as_bytes() {
            return Err(invalid("bootstrap checksum list disagrees with its record"));
        }
        files.insert(
            "SHA256SUMS".into(),
            CheckedFile {
                file: checksum_file,
                original,
            },
        );
        let mut package = Self {
            directory,
            owner,
            expected,
            record,
            files,
        };
        package.validate_contents()?;
        package.check()?;
        Ok(package)
    }

    fn small(&mut self, path: &str, bound: u64) -> Result<Vec<u8>> {
        let file = self
            .files
            .get_mut(path)
            .ok_or_else(|| invalid("required bootstrap file is missing"))?;
        if file.original.len() > bound {
            return Err(invalid("bootstrap metadata exceeds its bound"));
        }
        file.file.rewind()?;
        let mut bytes = Vec::new();
        (&mut file.file).take(bound + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != file.original.len()
            || !unchanged(&file.original, &file.file.metadata()?)
        {
            return Err(invalid("bootstrap metadata changed"));
        }
        Ok(bytes)
    }

    fn validate_contents(&mut self) -> Result<()> {
        let image =
            ImageIdentity::parse(&self.small("image.json", MAX_IMAGE_IDENTITY_BYTES as u64)?)?;
        if image != self.record.image {
            return Err(invalid("bootstrap image records disagree"));
        }
        let manifest_bytes = self.small(
            "boot/spacewars-boot-manifest.json",
            MAX_BOOT_MANIFEST_BYTES as u64,
        )?;
        let manifest = BootManifest::parse(&manifest_bytes, image.boot_identity)?;
        let mut expected: BTreeMap<String, Blob> = manifest
            .files()
            .iter()
            .map(|(p, b)| (format!("boot/{p}"), b.clone()))
            .collect();
        for (p, b) in &self.record.plan.staged_assets {
            expected.insert(format!("{STAGE_DIRECTORY}/{p}"), b.clone());
        }
        expected.insert("rootfs.ext4.zst".into(), self.record.compressed.clone());
        expected.insert(
            "engine-os-manager".into(),
            image.binaries.os_manager.clone(),
        );
        for name in [
            "image.json",
            "README.txt",
            "boot/spacewars-boot-manifest.json",
        ] {
            expected.insert(
                name.into(),
                self.record
                    .files
                    .get(name)
                    .ok_or_else(|| invalid("required metadata is missing"))?
                    .clone(),
            );
        }
        if self.record.files["README.txt"].bytes > 4096 {
            return Err(invalid("oversized bootstrap notice"));
        }
        for (name, bytes) in [
            (
                "boot/spacewars-boot-id".into(),
                format!("{}\n", image.boot_identity),
            ),
            (
                "boot/cmdline.txt".into(),
                self.record.plan.normal_cmdline.clone(),
            ),
            ("boot/spacewars-trial.txt".into(), UNPREPARED_TRIAL.into()),
            (
                "boot/spacewars-device.txt".into(),
                format!(
                    "[all]\ninclude spacewars-profiles/{}.txt\n[all]\n",
                    self.record.plan.profile
                ),
            ),
            (
                "trial-config.txt".into(),
                self.record.plan.trial_config.clone(),
            ),
            (
                "normal-config.txt".into(),
                self.record.plan.normal_config.clone(),
            ),
            (
                format!("{STAGE_DIRECTORY}/trial-cmdline.txt"),
                self.record.plan.trial_cmdline.clone(),
            ),
            (
                format!("{STAGE_DIRECTORY}/normal-cmdline.txt"),
                self.record.plan.normal_cmdline.clone(),
            ),
        ] {
            expected.insert(
                name,
                Blob {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(bytes.as_bytes()),
                },
            );
        }
        if expected != self.record.files {
            return Err(invalid(
                "bootstrap inventory does not match its image and plan",
            ));
        }
        Ok(())
    }

    fn check(&self) -> Result<()> {
        owned(&self.directory.metadata()?, true, self.owner, true)?;
        let expected: BTreeSet<_> = self.files.keys().cloned().collect();
        inventory(&self.directory, "", self.owner, &expected)?;
        for (path, checked) in &self.files {
            let named = private_asset(&self.directory, path, self.owner)?;
            if !unchanged(&checked.original, &checked.file.metadata()?)
                || !unchanged(&checked.original, &named.metadata()?)
            {
                return Err(invalid("bootstrap package was changed or replaced"));
            }
        }
        Ok(())
    }

    fn check_device(&mut self, legacy: &LegacyBootEvidence) -> Result<PackageCheck> {
        self.check()?;
        let roots = SlotRoots::new(
            legacy.layout.partitions[1].partuuid.clone(),
            legacy.layout.partitions[2].partuuid.clone(),
        )?;
        let template = BootCommandLine::parse(&self.small("boot/spacewars-cmdline.txt", 4096)?)?;
        let target = legacy.layout.active.other();
        if self.record.plan.board != Board::from_model(&legacy.physical.model)?
            || self.record.plan.profile != legacy.hardware_profile
            || self.record.plan.target != target
            || self.record.plan.normal_cmdline != template.render(&roots, target, None)
            || self.record.plan.trial_cmdline
                != template.render(&roots, target, Some(self.record.plan.trial_id))
            || self.record.expanded.bytes
                > legacy
                    .layout
                    .inactive_partition()
                    .sectors
                    .checked_mul(512)
                    .ok_or_else(|| invalid("inactive capacity overflow"))?
        {
            return Err(invalid(
                "package targets a different board, profile, root or layout",
            ));
        }
        // Rebuild every generated plan field from the checked assets and the
        // real board/profile/UUIDs. Serialized plan fields are not authority.
        let manifest = BootManifest::parse(
            &self.small(
                "boot/spacewars-boot-manifest.json",
                MAX_BOOT_MANIFEST_BYTES as u64,
            )?,
            self.record.image.boot_identity,
        )?;
        let config = String::from_utf8(self.small("boot/config.txt", 16 * 1024)?)
            .map_err(|_| invalid("invalid boot configuration"))?;
        let profile = String::from_utf8(self.small(
            &format!("boot/spacewars-profiles/{}.txt", legacy.hardware_profile),
            16 * 1024,
        )?)
        .map_err(|_| invalid("invalid hardware profile"))?;
        let request = PlanRequest {
            board: Board::from_model(&legacy.physical.model)?,
            profile: legacy.hardware_profile.clone(),
            roots,
            target,
            trial_id: self.record.plan.trial_id,
        };
        let plan = BootPlan::create(
            &self.record.image,
            &manifest,
            &config,
            &profile,
            &template,
            &request,
        )?;
        if plan != self.record.plan {
            return Err(invalid(
                "serialized bootstrap plan does not match checked assets and device",
            ));
        }
        for (path, blob) in &self.record.plan.unchanged_firmware {
            if legacy.boot_files.get(path) != Some(blob) {
                return Err(invalid(
                    "bootstrap would require changing retained firmware",
                ));
            }
        }
        let profile = self.small(
            &format!("boot/spacewars-profiles/{}.txt", self.record.plan.profile),
            16 * 1024,
        )?;
        let new_lines = active_lines(
            std::str::from_utf8(&profile).map_err(|_| invalid("invalid profile encoding"))?,
        )?;
        if normalized_profile(&new_lines, &self.record.plan.profile)
            != normalized_profile(&legacy.hardware_configuration, &legacy.hardware_profile)
        {
            return Err(invalid(
                "bootstrap would alter the device's hardware configuration",
            ));
        }
        let boot_space = capacity(&self.record.files, legacy)?;
        let package_bytes = self.files.values().try_fold(0u64, |n, f| {
            n.checked_add(f.original.len())
                .ok_or_else(|| invalid("package size overflow"))
        })?;
        let raw_backups = legacy.layout.partitions[0]
            .sectors
            .checked_add(legacy.layout.inactive_partition().sectors)
            .and_then(|n| n.checked_mul(512))
            .ok_or_else(|| invalid("backup capacity overflow"))?;
        // The package is already present. Require room for both raw recovery
        // copies, a second package-sized allowance for small/data preservation
        // work, and the common 256 MiB free-space reserve. Data backup planning
        // must additionally account for its observed content before writing.
        let data_required_bytes = raw_backups
            .checked_add(package_bytes)
            .and_then(|n| n.checked_add(256 * 1024 * 1024))
            .ok_or_else(|| invalid("data capacity overflow"))?;
        if data_required_bytes > legacy.data_free_bytes {
            return Err(invalid(
                "insufficient data space for bootstrap recovery copies",
            ));
        }
        Ok(PackageCheck {
            schema: 1,
            scope: "operator package and legacy compatibility; backup/access/handoff still required",
            package_sha256: self.expected,
            image: self.record.image.clone(),
            plan: self.record.plan.clone(),
            device_id: legacy.physical.fingerprint(),
            layout_id: legacy.layout.layout_id,
            boot_id: legacy.boot_id.clone(),
            boot_space,
            data_required_bytes,
            data_available_bytes: legacy.data_free_bytes,
            ready_for_write: false,
        })
    }
}

fn normalized_profile(lines: &[String], profile: &str) -> Vec<String> {
    let marker = format!("dtoverlay=spacewars-profile-{profile}");
    let mut result: Vec<String> = Vec::new();
    for line in lines.iter().filter(|line| *line != &marker) {
        if super::filter(line)
            && result
                .last()
                .is_some_and(|previous| super::filter(previous))
        {
            result.pop();
        }
        result.push(line.clone());
    }
    while result.last().is_some_and(|line| super::filter(line)) {
        result.pop();
    }
    result
}

fn capacity(files: &BTreeMap<String, Blob>, legacy: &LegacyBootEvidence) -> Result<Capacity> {
    let cluster = legacy.boot_allocation_bytes;
    if !cluster.is_power_of_two() || !(512..=65536).contains(&cluster) {
        return Err(invalid("unsupported FAT allocation size"));
    }
    let allocated = |n: u64| {
        n.checked_add(cluster - 1)
            .and_then(|n| (n / cluster).checked_mul(cluster))
            .ok_or_else(|| invalid("allocation overflow"))
    };
    let (mut stage, mut growth, mut replacement) = (0u64, 0u64, 0u64);
    for (name, blob) in files {
        let bytes = allocated(blob.bytes)?;
        if name.starts_with("sw-bootstrap/") {
            stage = stage
                .checked_add(bytes)
                .ok_or_else(|| invalid("stage allocation overflow"))?;
        }
        if let Some(name) = name.strip_prefix("boot/") {
            replacement = replacement.max(bytes);
            let old = allocated(legacy.boot_files.get(name).map_or(0, |b| b.bytes))?;
            growth = growth
                .checked_add(bytes.saturating_sub(old))
                .ok_or_else(|| invalid("canonical allocation overflow"))?;
        }
    }
    let (directories, reserve) = (64 * 1024, 4 * 1024 * 1024);
    let required = stage
        .checked_add(growth)
        .and_then(|n| n.checked_add(replacement))
        .and_then(|n| n.checked_add(directories + reserve))
        .ok_or_else(|| invalid("boot capacity overflow"))?;
    if required > legacy.boot_free_bytes {
        return Err(invalid(
            "insufficient boot space for staged assets and atomic canonical replacements",
        ));
    }
    Ok(Capacity {
        available_bytes: legacy.boot_free_bytes,
        staged_allocation_bytes: stage,
        canonical_growth_bytes: growth,
        largest_replacement_bytes: replacement,
        directory_reserve_bytes: directories,
        spare_reserve_bytes: reserve,
        required_bytes: required,
    })
}

fn package_path(path: &str) -> bool {
    matches!(
        path,
        "image.json"
            | "README.txt"
            | "engine-os-manager"
            | "rootfs.ext4.zst"
            | "trial-config.txt"
            | "normal-config.txt"
    ) || path
        .strip_prefix("boot/")
        .or_else(|| path.strip_prefix("sw-bootstrap/"))
        .is_some_and(crate::boot::asset_path)
}

pub(super) fn private_asset(root: &File, path: &str, owner: u32) -> Result<File> {
    let mut directory = root.try_clone()?;
    let device = root.metadata()?.dev();
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        let is_directory = parts.peek().is_some();
        let file = open_at(
            &directory,
            part,
            libc::O_PATH | if is_directory { libc::O_DIRECTORY } else { 0 },
            0,
        )?;
        let metadata = file.metadata()?;
        owned(&metadata, is_directory, owner, true)?;
        if metadata.dev() != device {
            return Err(invalid("bootstrap package crossed filesystems"));
        }
        if !is_directory {
            return Ok(File::open(format!("/proc/self/fd/{}", file.as_raw_fd()))?);
        }
        directory = file;
    }
    Err(invalid("empty bootstrap file path"))
}

fn inventory(directory: &File, prefix: &str, owner: u32, files: &BTreeSet<String>) -> Result<()> {
    owned(&directory.metadata()?, true, owner, true)?;
    for (count, entry) in
        fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?.enumerate()
    {
        if count >= 1024 {
            return Err(invalid("excessive package inventory"));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("invalid package filename"))?;
        let path = format!("{prefix}{name}");
        if entry.file_type()?.is_dir() {
            let sub = format!("{path}/");
            if !files.iter().any(|p| p.starts_with(&sub)) {
                return Err(invalid("unexpected bootstrap directory"));
            }
            let child = open_at(directory, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            if child.metadata()?.dev() != directory.metadata()?.dev() {
                return Err(invalid("foreign bootstrap directory"));
            }
            inventory(&child, &sub, owner, files)?;
        } else if !files.contains(&path) {
            return Err(invalid("unexpected bootstrap file"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_device_tests.rs"]
mod tests;
