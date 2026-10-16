//! Supervised bootstrap planning, separate from signed rootfs OTA authority.
//! Preparation may reserve a root-private attempt on /data. It never writes
//! a root partition, enables OTA, changes selectors or reboots. The explicit
//! root-operator phases preserve/replace fixed legacy files and install the
//! inactive image. First-boot recovery uses its own durable journal; it cannot
//! provision production recovery authority or initialize a trial itself.
use crate::boot::{BootCommandLine, BootManifest, SlotRoots};
use crate::recovery::Slot;
use crate::{Blob, Error, ImageIdentity, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(target_os = "linux")]
#[path = "bootstrap_package.rs"]
mod package;
#[cfg(target_os = "linux")]
pub use package::{BootstrapPackage, build_package};

#[path = "bootstrap_inspect.rs"]
mod inspect;
pub use inspect::{LegacyBootEvidence, inspect_legacy_boot};

#[path = "bootstrap_device.rs"]
mod device;
pub use device::{PackageCheck, check_installed_package, prepare_installed_package};

#[path = "bootstrap_state.rs"]
mod state;
pub(crate) use state::require_no_active_under;

#[path = "bootstrap_backup.rs"]
mod backup;
pub use backup::{
    BackupExportCheck, BackupPartition, BackupProgress, BackupReceipt, BackupReport, BackupStage,
    verify_backup_export,
};
pub use device::backup_installed_package;
pub use state::{BootstrapBinding, PreparationReport, require_no_active_bootstrap};

#[path = "bootstrap_access_check.rs"]
mod access_check;
#[path = "bootstrap_archive.rs"]
mod archive;
#[path = "bootstrap_data_backup.rs"]
mod data_backup;
#[path = "bootstrap_legacy_assets.rs"]
mod legacy_assets;
#[path = "bootstrap_legacy_files.rs"]
mod legacy_files;
#[path = "bootstrap_legacy_replace.rs"]
mod legacy_replace;
#[path = "bootstrap_legacy_state.rs"]
mod legacy_state;
#[path = "bootstrap_quiesce.rs"]
mod quiesce;
#[path = "bootstrap_root_install.rs"]
mod root_install;
#[path = "bootstrap_trial.rs"]
mod trial;
pub use trial::{TrialFailure, TrialPhase};
#[path = "bootstrap_trial_store.rs"]
mod trial_store;
pub use trial_store::TrialStore;
#[cfg(test)]
pub(crate) use trial_store::tests::Fixture as TrialFixture;
#[path = "bootstrap_trial_coordinator.rs"]
mod trial_coordinator;
pub use trial_coordinator::{BootstrapBackend, BootstrapOutcome, recover_bootstrap};
#[path = "bootstrap_trial_files.rs"]
mod trial_files;
#[path = "bootstrap_trial_linux.rs"]
mod trial_linux;
pub use access_check::{LegacyAccessCheck, SshCheckConnection};
pub use data_backup::{
    DataBackupExportCheck, DataBackupProgress, DataBackupReceipt, DataBackupReport,
    DataBackupStage, verify_data_backup_export,
};
pub use device::backup_installed_data;
pub use device::check_installed_legacy_access;
pub use device::install_bootstrap_root;
pub use device::prepare_installed_legacy;
pub use device::replace_installed_legacy;
pub use legacy_replace::{
    LegacyReplacementProgress, LegacyReplacementReport, LegacyReplacementStage,
};
pub use legacy_state::{
    LegacyExportCheck, LegacyPreparationProgress, LegacyPreparationReport, LegacyPreparationStage,
    verify_legacy_export,
};
pub use root_install::{RootInstallProgress, RootInstallReport};
pub use trial_linux::BootstrapBoot;

pub const STAGE_DIRECTORY: &str = "sw-bootstrap";
/// First-image health, persistent-store checks and separate normal confirmation.
pub const BOOTSTRAP_PROTOCOL: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Board {
    Pi4,
    Pi5,
}

impl Board {
    pub fn from_model(model: &str) -> Result<Self> {
        if model.starts_with("Raspberry Pi 4 Model B Rev ") {
            Ok(Self::Pi4)
        } else if model.starts_with("Raspberry Pi 5 Model B Rev ") {
            Ok(Self::Pi5)
        } else {
            Err(invalid(
                "bootstrap supports Pi 4 Model B and Pi 5 Model B only",
            ))
        }
    }

    fn device_tree(self) -> &'static str {
        match self {
            Self::Pi4 => "bcm2711-rpi-4-b.dtb",
            Self::Pi5 => "bcm2712-rpi-5-b.dtb",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootPlan {
    pub schema: u32,
    pub boot_identity: Sha256,
    pub build_id: String,
    pub board: Board,
    pub profile: String,
    pub target: Slot,
    pub trial_id: Sha256,
    /// Copy these exact artifact assets beneath /boot/sw-bootstrap. Only one
    /// copy of the common Pi kernel is needed in the temporary staging area.
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub staged_assets: BTreeMap<String, Blob>,
    pub staged_asset_bytes: u64,
    /// Must match the live firmware before staging; bootstrap does not update
    /// EEPROM or replace GPU firmware to make an incompatible boot set work.
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub unchanged_firmware: BTreeMap<String, Blob>,
    pub trial_config: String,
    pub normal_config: String,
    pub trial_cmdline: String,
    pub normal_cmdline: String,
}

impl BootPlan {
    /// The strings must come from hash-checked manifest assets. The artifact
    /// builder below does that before calling this pure renderer. Device apply
    /// must derive board/profile/roots/target from live evidence under its lock.
    pub(crate) fn create(
        image: &ImageIdentity,
        manifest: &BootManifest,
        config: &str,
        profile_text: &str,
        template: &BootCommandLine,
        request: &PlanRequest,
    ) -> Result<Self> {
        image.validate()?;
        if image.source.dirty || image.boot_identity != manifest.identity() {
            return Err(invalid(
                "bootstrap requires a clean image and its exact boot manifest",
            ));
        }
        if !matches!(request.profile.as_str(), "hdmi" | "hyperpixel" | "picade")
            || (request.board == Board::Pi5 && request.profile == "picade")
            || request.trial_id.as_bytes() == &[0; 32]
        {
            return Err(invalid(
                "unsupported board/profile or missing trial identity",
            ));
        }
        let mut staged_assets = BTreeMap::new();
        let mut unchanged_firmware = BTreeMap::new();
        for (path, blob) in manifest.files() {
            if path == "kernel8.img"
                || path == request.board.device_tree()
                || path == "spacewars-recovery.cpio.gz"
                || path.starts_with("overlays/")
            {
                staged_assets.insert(path.clone(), blob.clone());
            } else if firmware(path) {
                unchanged_firmware.insert(path.clone(), blob.clone());
            } else if !matches!(
                path.as_str(),
                "kernel_2712.img"
                    | "bcm2711-rpi-4-b.dtb"
                    | "bcm2712-rpi-5-b.dtb"
                    | "spacewars-kernel-build-id"
                    | "config.txt"
                    | "tryboot.txt"
                    | "spacewars-cmdline.txt"
                    | "spacewars-profiles/hdmi.txt"
                    | "spacewars-profiles/hyperpixel.txt"
                    | "spacewars-profiles/picade.txt"
            ) {
                return Err(invalid("unsupported boot asset in bootstrap bundle"));
            }
        }
        for required in [
            "kernel8.img",
            request.board.device_tree(),
            "spacewars-recovery.cpio.gz",
        ] {
            if !staged_assets.contains_key(required) {
                return Err(invalid("missing staged kernel, DTB or initramfs"));
            }
        }
        for required in ["bootcode.bin", "start4.elf", "fixup4.dat"] {
            if !unchanged_firmware.contains_key(required) {
                return Err(invalid("missing retained firmware identity"));
            }
        }
        let profile = active_lines(profile_text)?;
        let marker = format!("dtoverlay=spacewars-profile-{}", request.profile);
        if profile.iter().filter(|line| *line == &marker).count() != 1 {
            return Err(invalid(
                "profile does not select exactly its own marker overlay",
            ));
        }
        let mut section = "[all]";
        for line in &profile {
            if filter(line) {
                section = line;
            }
            if line.starts_with("dtoverlay=spacewars-profile-")
                && (line != &marker || section != "[all]")
            {
                return Err(invalid("profile marker must be unique and unconditional"));
            }
            if !filter(line) && !overlay(line, &staged_assets) && !parameter(line) {
                return Err(invalid("unsupported bootstrap profile directive"));
            }
        }
        let mut rendered = format!(
            "# Supervised bootstrap; original default is retained for the first trial.\n[all]\noverlay_prefix={STAGE_DIRECTORY}/overlays/\n"
        );
        let (mut initramfs, mut include, mut arm64) = (0, 0, 0);
        let mut section = "[all]".to_owned();
        for line in active_lines(config)? {
            if filter(&line) {
                section.clone_from(&line);
            }
            if (line.starts_with("initramfs ")
                || line.starts_with("include ")
                || line.starts_with("arm_64bit="))
                && section != "[all]"
            {
                return Err(invalid(
                    "bootstrap root configuration must apply to both boards",
                ));
            }
            if line == "initramfs spacewars-recovery.cpio.gz followkernel" {
                initramfs += 1;
                rendered.push_str(&format!(
                    "initramfs /{STAGE_DIRECTORY}/spacewars-recovery.cpio.gz followkernel\n"
                ));
            } else if line == "include spacewars-device.txt" {
                include += 1;
                rendered.push_str(&profile.join("\n"));
                rendered.push('\n');
            } else if line == "arm_64bit=1" {
                arm64 += 1;
                rendered.push_str("arm_64bit=1\n");
            } else if filter(&line) || line == "disable_splash=1" || overlay(&line, &staged_assets)
            {
                rendered.push_str(&line);
                rendered.push('\n');
            } else {
                return Err(invalid(
                    "unsupported base configuration for staged bootstrap",
                ));
            }
        }
        if (initramfs, include, arm64) != (1, 1, 1) {
            return Err(invalid(
                "ambiguous bootstrap initramfs, profile or kernel architecture",
            ));
        }
        // Explicit paths avoid os_prefix's silent fallback to old kernel/DTB
        // or shared overlays when a prefix/README viability check fails.
        rendered.push_str(&format!(
            "[all]\nkernel=/{STAGE_DIRECTORY}/kernel8.img\ndevice_tree=/{STAGE_DIRECTORY}/{}\n",
            request.board.device_tree()
        ));
        let trial_config = format!("{rendered}cmdline=/{STAGE_DIRECTORY}/trial-cmdline.txt\n");
        let normal_config = format!("{rendered}cmdline=/{STAGE_DIRECTORY}/normal-cmdline.txt\n");
        if trial_config
            .lines()
            .any(|line| !line.starts_with('#') && line.len() > 98)
        {
            return Err(invalid(
                "bootstrap configuration exceeds firmware line limit",
            ));
        }
        Ok(Self {
            schema: 1,
            boot_identity: image.boot_identity,
            build_id: image.build_id.clone(),
            board: request.board,
            profile: request.profile.clone(),
            target: request.target,
            trial_id: request.trial_id,
            staged_asset_bytes: staged_assets.values().map(|blob| blob.bytes).sum(),
            staged_assets,
            unchanged_firmware,
            trial_config,
            normal_config,
            trial_cmdline: template.render(&request.roots, request.target, Some(request.trial_id)),
            normal_cmdline: template.render(&request.roots, request.target, None),
        })
    }
}

pub struct PlanRequest {
    pub board: Board,
    pub profile: String,
    pub roots: SlotRoots,
    pub target: Slot,
    pub trial_id: Sha256,
}

fn active_lines(text: &str) -> Result<Vec<String>> {
    if text.len() > 16 * 1024 || !text.is_ascii() || text.contains('\0') {
        return Err(invalid("invalid or oversized boot configuration"));
    }
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}
fn filter(line: &str) -> bool {
    matches!(line, "[all]" | "[pi4]" | "[pi5]")
}
fn parameter(line: &str) -> bool {
    line.strip_prefix("dtparam=").is_some_and(|value| {
        !value.is_empty()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_=.,-".contains(&b))
    })
}
fn overlay(line: &str, assets: &BTreeMap<String, Blob>) -> bool {
    let Some(value) = line.strip_prefix("dtoverlay=") else {
        return false;
    };
    let name = value.split(',').next().unwrap_or("");
    !name.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-=,.".contains(&b))
        && assets.contains_key(&format!("overlays/{name}.dtbo"))
}
fn firmware(path: &str) -> bool {
    matches!(
        path,
        "bootcode.bin"
            | "start.elf"
            | "start4.elf"
            | "start_cd.elf"
            | "start_db.elf"
            | "start_x.elf"
            | "start4cd.elf"
            | "start4db.elf"
            | "start4x.elf"
            | "fixup.dat"
            | "fixup4.dat"
            | "fixup_cd.dat"
            | "fixup_db.dat"
            | "fixup_x.dat"
            | "fixup4cd.dat"
            | "fixup4db.dat"
            | "fixup4x.dat"
    ) || (path.starts_with("rpi-bootfiles-") && path.ends_with(".stamp") && !path.contains('/'))
}
fn invalid(message: &str) -> Error {
    Error::Boot(message.into())
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
pub(crate) mod tests;
