//! Read-only inventory of supported legacy and unprepared fresh installations. Passing
//! this preflight is not permission to install or proof of rollback capability.
use super::{Board, BootstrapOrigin, active_lines, invalid};
use crate::boot::{
    MAX_BOOT_ASSET_BYTES, MAX_BOOT_TOTAL_BYTES, asset_path, hardware_profile, hostname,
    kernel_build_id,
};
use crate::boot_linux::{PhysicalIdentity, check_mount, parse_tryboot, read, text};
use crate::owned_fs::{io_result, open_at, open_directory, owned, read_file, unchanged};
use crate::recovery_linux::{DiskLayout, inspect_layout, open_data_mount};
use crate::{Blob, Result, Sha256};
use serde::Serialize;
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) const LEGACY_CONFIG: &str = "disable_splash=1\n[pi4]\ndtoverlay=vc4-kms-v3d-pi4\n[pi5]\ndtoverlay=vc4-kms-v3d-pi5\n[all]\narm_64bit=1\ninclude spacewars-device.txt\n[all]\n";

#[derive(Debug, Serialize)]
pub struct BootstrapBootEvidence {
    pub schema: u32,
    pub purpose: &'static str,
    pub origin: BootstrapOrigin,
    pub physical: PhysicalIdentity,
    pub boot_id: String,
    pub layout: DiskLayout,
    pub kernel_build_id: String,
    pub hardware_profile: String,
    pub hardware_configuration: Vec<String>,
    pub hostname: Option<String>,
    pub boot_files: BTreeMap<String, Blob>,
    pub boot_free_bytes: u64,
    pub boot_allocation_bytes: u64,
    pub data_free_bytes: u64,
    /// No privileged recovery/key inspection is performed by this inventory.
    pub root_preflight_complete: bool,
}

/// Inspect fixed installed paths without opening a block device or watchdog,
/// obtaining an installation lock, changing state or requiring root. Reports
/// the live inactive slot/UUIDs; package planning arguments cannot supply them.
pub fn inspect_bootstrap_boot() -> Result<BootstrapBootEvidence> {
    inspect_staging(None).map(|(legacy, _)| legacy)
}

/// Retain the narrower legacy-only probe for callers that have not opted in to
/// fresh images. A manifest alone must never make an old caller accept a trial.
pub fn inspect_legacy_boot() -> Result<BootstrapBootEvidence> {
    let evidence = inspect_bootstrap_boot()?;
    if evidence.origin != BootstrapOrigin::Legacy {
        return Err(invalid(
            "legacy-only inspection does not accept fresh images",
        ));
    }
    Ok(evidence)
}

/// Only the explicit staging path supplies an already validated durable intent.
/// The public legacy probe continues to reject all existing staging/trial files.
pub(super) fn inspect_staging(
    stage: Option<&super::trial::TrialInputs>,
) -> Result<(BootstrapBootEvidence, u64)> {
    let layout = inspect_layout()?;
    let physical = PhysicalIdentity::inspect(&layout)?;
    let board = Board::from_model(&physical.model)?;
    let boot_id = text("/proc/sys/kernel/random/boot_id", 64)?
        .trim()
        .to_owned();
    if !crate::recovery::boot_id(&boot_id)
        || parse_tryboot(&read(
            "/sys/firmware/devicetree/base/chosen/bootloader/tryboot",
            4,
        )?)?
    {
        return Err(invalid(
            "bootstrap preflight requires an identified normal boot",
        ));
    }
    let root = open_directory(Path::new("/"), 0)?;
    let boot = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    // Inventory also runs during the read-only window used for boot backups.
    check_mount(&boot, &layout, false)?;
    let data = open_data_mount()?;
    let kernel = kernel_build_id(&read("/sys/kernel/notes", 64 * 1024)?)?;
    let mut files = BTreeMap::new();
    inventory(&boot, "", boot.metadata()?.dev(), 0, &mut files, &mut 0)?;
    let all_files = files.clone();
    let (boot_free_bytes, boot_allocation_bytes) = space(&boot)?;
    let credit = if let Some(stage) = stage {
        let (retained, credit) =
            super::stage_files::retained(&files, stage, boot_allocation_bytes)?;
        files = retained;
        credit
    } else {
        0
    };
    if files.keys().any(|path| {
        path == "autoboot.txt"
            || path.starts_with("sw-bootstrap/")
            || path.starts_with(".sw-bootstrap-")
    }) {
        return Err(invalid(
            "existing trial, bootstrap or partition-selection state needs explicit reconciliation",
        ));
    }
    let contents = |path: &str, limit: u64| -> Result<Vec<u8>> {
        let mut parent = boot.try_clone()?;
        let parts: Vec<_> = path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            parent = open_at(&parent, part, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        }
        let bytes = read_file(
            open_at(&parent, parts[parts.len() - 1], libc::O_RDONLY, 0)?,
            0,
            limit,
        )?;
        let actual = Blob {
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes),
        };
        if files.get(path) != Some(&actual) {
            return Err(invalid("boot input changed during preflight"));
        }
        Ok(bytes)
    };
    let origin = if files.contains_key("spacewars-boot-manifest.json") {
        super::fresh::inspect(
            crate::ImageIdentity::read_installed()?,
            &files,
            &layout.active_partition().name,
            &kernel,
            &text("/proc/cmdline", 16 * 1024)?,
            &contents,
        )?
    } else {
        if files.contains_key("tryboot.txt") {
            return Err(invalid(
                "unidentified trial selector requires reconciliation",
            ));
        }
        let config = contents("config.txt", 16 * 1024)?;
        legacy_selection(
            &String::from_utf8(config).map_err(|_| invalid("non-UTF8 boot configuration"))?,
            &contents("cmdline.txt", 4096)?,
            &text("/proc/cmdline", 16 * 1024)?,
            &layout.active_partition().name,
        )?;
        BootstrapOrigin::Legacy
    };
    let selection = contents("spacewars-device.txt", 4096)?;
    let profile = hardware_profile(
        std::str::from_utf8(&selection).map_err(|_| invalid("non-UTF8 hardware selection"))?,
    )?
    .to_owned();
    if board == Board::Pi5 && profile == "picade" {
        return Err(invalid("Picade profile is unsupported on Pi 5"));
    }
    let profile_bytes = contents(&format!("spacewars-profiles/{profile}.txt"), 16 * 1024)?;
    let hardware_configuration = active_lines(
        std::str::from_utf8(&profile_bytes).map_err(|_| invalid("non-UTF8 hardware profile"))?,
    )?;
    if hardware_configuration.iter().any(|line| {
        !super::filter(line) && !super::parameter(line) && !super::overlay(line, &files)
    }) {
        return Err(invalid(
            "legacy hardware profile has unsupported boot directives",
        ));
    }
    let device_hostname = if files.contains_key("hostname.txt") {
        Some(hostname(&contents("hostname.txt", 64)?)?.to_owned())
    } else {
        None
    };
    if files.contains_key("boot_slot") {
        // Retain this breadcrumb in backups, but never use it to choose a slot.
        let _ = contents("boot_slot", 16)?;
    }
    let (data_free_bytes, _) = space(&data)?;
    let mut after = BTreeMap::new();
    inventory(&boot, "", boot.metadata()?.dev(), 0, &mut after, &mut 0)?;
    let current = inspect_layout()?;
    let named = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    if after != all_files
        || current != layout
        || physical != PhysicalIdentity::inspect(&current)?
        || named.metadata()?.dev() != boot.metadata()?.dev()
        || named.metadata()?.ino() != boot.metadata()?.ino()
        || text("/proc/sys/kernel/random/boot_id", 64)?.trim() != boot_id
        || matches!(&origin, BootstrapOrigin::FreshImage { image }
            if **image != crate::ImageIdentity::read_installed()?)
    {
        return Err(invalid("boot context changed during legacy preflight"));
    }
    Ok((
        BootstrapBootEvidence {
            schema: 2,
            purpose: "read-only-bootstrap-inventory",
            origin,
            physical,
            boot_id,
            layout,
            kernel_build_id: kernel,
            hardware_profile: profile,
            hardware_configuration,
            hostname: device_hostname,
            boot_files: files,
            boot_free_bytes,
            boot_allocation_bytes,
            data_free_bytes,
            root_preflight_complete: false,
        },
        credit,
    ))
}

fn legacy_selection(config: &str, default: &[u8], running: &str, active: &str) -> Result<()> {
    if active_lines(config)? != active_lines(LEGACY_CONFIG)? {
        return Err(invalid(
            "legacy default boot configuration needs manual review",
        ));
    }
    let default =
        std::str::from_utf8(default).map_err(|_| invalid("invalid legacy command line"))?;
    let expected = format!(
        "dwc_otg.lpm_enable=0 root=/dev/{active} rootfstype=ext4 rootwait net.ifnames=0 console=tty1 fbcon=map:01"
    );
    let tokens: Vec<_> = default.split_ascii_whitespace().collect();
    if tokens.join(" ") != expected {
        return Err(invalid("unsupported legacy default command line"));
    }
    running_selection(default, running)
}

pub(super) fn running_selection(default: &str, running: &str) -> Result<()> {
    let tokens: Vec<_> = default.split_ascii_whitespace().collect();
    let actual: Vec<_> = running.split_ascii_whitespace().collect();
    if !actual.ends_with(&tokens)
        || actual.iter().filter(|arg| arg.starts_with("root=")).count() != 1
        || actual.iter().any(|arg| {
            matches!(
                arg.split('=').next(),
                Some(
                    "spacewars.trial"
                        | "init"
                        | "rdinit"
                        | "init_fatal_sh"
                        | "systemd.unit"
                        | "rd.systemd.unit"
                )
            )
        })
    {
        return Err(invalid(
            "legacy default does not match the running supported root",
        ));
    }
    Ok(())
}

pub(super) fn inventory(
    directory: &File,
    prefix: &str,
    device: u64,
    owner: u32,
    files: &mut BTreeMap<String, Blob>,
    total: &mut u64,
) -> Result<()> {
    let before = directory.metadata()?;
    owned(&before, true, owner, false)?;
    if before.dev() != device || prefix.matches('/').count() > 2 {
        return Err(invalid("unexpected nested boot filesystem"));
    }
    for (index, entry) in
        fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?.enumerate()
    {
        if index >= 1024 || files.len() >= 1024 {
            return Err(invalid("excessive legacy boot inventory"));
        }
        let entry = entry?;
        let component = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("non-UTF8 boot name"))?;
        let path = format!("{prefix}{component}");
        if !asset_path(&path) {
            return Err(invalid("unsupported legacy boot filename"));
        }
        if entry.file_type()?.is_dir() {
            let child = open_at(directory, &component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            inventory(&child, &format!("{path}/"), device, owner, files, total)?;
        } else {
            let mut file = open_at(directory, &component, libc::O_RDONLY, 0)?;
            let metadata = file.metadata()?;
            owned(&metadata, false, owner, false)?;
            if metadata.dev() != device || metadata.len() > MAX_BOOT_ASSET_BYTES {
                return Err(invalid("oversized or foreign boot file"));
            }
            *total = total
                .checked_add(metadata.len())
                .ok_or_else(|| invalid("boot inventory overflow"))?;
            if *total > MAX_BOOT_TOTAL_BYTES {
                return Err(invalid("boot inventory exceeds its bound"));
            }
            let mut hash = Hasher::new();
            let mut count = 0u64;
            let mut buffer = [0; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                count += read as u64;
                if count > metadata.len() {
                    return Err(invalid("boot file grew during preflight"));
                }
                hash.update(&buffer[..read]);
            }
            let named = open_at(directory, &component, libc::O_RDONLY, 0)?;
            if count != metadata.len()
                || !unchanged(&metadata, &file.metadata()?)
                || !unchanged(&metadata, &named.metadata()?)
            {
                return Err(invalid("boot file changed during preflight"));
            }
            files.insert(
                path,
                Blob {
                    bytes: count,
                    sha256: format!("{:x}", hash.finalize())
                        .try_into()
                        .expect("SHA256 hex"),
                },
            );
        }
    }
    if !unchanged(&before, &directory.metadata()?) {
        return Err(invalid("boot directory changed during preflight"));
    }
    Ok(())
}

fn space(directory: &File) -> Result<(u64, u64)> {
    let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: directory is live and fstatvfs initializes status on success.
    io_result(unsafe { libc::fstatvfs(directory.as_raw_fd(), status.as_mut_ptr()) })?;
    let status = unsafe { status.assume_init() };
    let free = status
        .f_bavail
        .checked_mul(status.f_frsize)
        .ok_or_else(|| invalid("free-space overflow"))?;
    if status.f_frsize == 0 {
        return Err(invalid("invalid filesystem allocation size"));
    }
    Ok((free, status.f_frsize))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_default_must_match_running_root_without_alternate_boot_directives() {
        let default = b"dwc_otg.lpm_enable=0 root=/dev/sda3 rootfstype=ext4 rootwait  net.ifnames=0 console=tty1 fbcon=map:01\n";
        let running = format!("firmware=value {}", std::str::from_utf8(default).unwrap());
        legacy_selection(LEGACY_CONFIG, default, &running, "sda3").unwrap();
        assert!(legacy_selection(LEGACY_CONFIG, default, &running, "sda2").is_err());
        for prefix in [
            "init=/bin/sh",
            "spacewars.trial=bad",
            "root=/dev/sda2",
            "systemd.unit=rescue.target",
        ] {
            assert!(
                legacy_selection(
                    LEGACY_CONFIG,
                    default,
                    &format!("{prefix} {running}"),
                    "sda3"
                )
                .is_err()
            );
        }
        for directive in [
            "cmdline=other.txt",
            "os_prefix=old/",
            "tryboot_a_b=1",
            "include custom.txt",
            "[tryboot]",
        ] {
            assert!(
                legacy_selection(
                    &format!("{LEGACY_CONFIG}{directive}\n"),
                    default,
                    &running,
                    "sda3"
                )
                .is_err()
            );
        }
    }
}
