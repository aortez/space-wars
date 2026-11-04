//! Recognize an unprepared flashed image, not a provisioned device whose state
//! was lost. The original manifest and image identity remain bound across staging.
use super::{BootstrapOrigin, active_lines, inspect::running_selection, invalid};
use crate::boot::{
    BootCommandLine, BootManifest, MAX_BOOT_MANIFEST_BYTES, MUTABLE_BOOT_FILES,
    OPTIONAL_BOOT_FILES, UNPREPARED_TRIAL,
};
use crate::{Blob, ImageIdentity, Result, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check_image(image: &ImageIdentity) -> Result<()> {
    image.validate()?;
    if image.source.dirty
        || image.machine != "raspberrypi-spacewars"
        || image.architecture != "aarch64"
        || image.updater_protocol != crate::UPDATER_PROTOCOL
        || image.recovery_protocol != crate::recovery::RECOVERY_PROTOCOL
    {
        return Err(invalid("unsupported fresh bootstrap image"));
    }
    Ok(())
}

/// `files` is the retained inventory, reconstructed only under a durable stage
/// intent. Reads must be bounded and checked against that inventory by the caller.
pub(super) fn inspect(
    image: ImageIdentity,
    files: &BTreeMap<String, Blob>,
    active: &str,
    kernel: &str,
    running: &str,
    read: &impl Fn(&str, u64) -> Result<Vec<u8>>,
) -> Result<BootstrapOrigin> {
    check_image(&image)?;
    let manifest = BootManifest::parse(
        &read(
            "spacewars-boot-manifest.json",
            MAX_BOOT_MANIFEST_BYTES as u64,
        )?,
        image.boot_identity,
    )?;
    let mut expected: BTreeSet<_> = manifest.files().keys().map(String::as_str).collect();
    expected.extend(MUTABLE_BOOT_FILES.iter().copied());
    let found: BTreeSet<_> = files
        .keys()
        .map(String::as_str)
        .filter(|path| !OPTIONAL_BOOT_FILES.contains(path))
        .collect();
    if found != expected
        || manifest
            .files()
            .iter()
            .any(|(path, blob)| files.get(path) != Some(blob))
    {
        return Err(invalid(
            "fresh boot assets differ from the installed image manifest",
        ));
    }
    if read("spacewars-boot-id", 65)? != format!("{}\n", image.boot_identity).as_bytes()
        || read("spacewars-trial.txt", 8192)? != UNPREPARED_TRIAL.as_bytes()
        || read("spacewars-kernel-build-id", 41)? != format!("{kernel}\n").as_bytes()
    {
        return Err(invalid(
            "fresh image is prepared, inconsistent or running another kernel",
        ));
    }
    let config = read("config.txt", 16 * 1024)?;
    let config = std::str::from_utf8(&config).map_err(|_| invalid("invalid fresh boot config"))?;
    let supported = super::inspect::LEGACY_CONFIG.replace(
        "include spacewars-device.txt\n",
        "initramfs spacewars-recovery.cpio.gz followkernel\ninclude spacewars-device.txt\n",
    );
    if active_lines(config)? != active_lines(&supported)? {
        return Err(invalid("unsupported fresh boot configuration"));
    }
    // The factory selector may already have been atomically replaced by the
    // staged selector. Validate its retained digest from the exact build recipe;
    // never reinterpret the replacement as part of the original image.
    let original_trial = format!(
        "{}\n[all]\ncmdline=spacewars-trial.txt\n",
        config.trim_end()
    );
    if files.get("tryboot.txt")
        != Some(&Blob {
            bytes: original_trial.len() as u64,
            sha256: Sha256::digest(original_trial.as_bytes()),
        })
    {
        return Err(invalid("fresh image has an unsupported trial selector"));
    }
    let template = read("spacewars-cmdline.txt", 4096)?;
    BootCommandLine::parse(&template)?;
    let template =
        std::str::from_utf8(&template).map_err(|_| invalid("invalid fresh command line"))?;
    let default = read("cmdline.txt", 4096)?;
    let default = std::str::from_utf8(&default).map_err(|_| invalid("invalid fresh default"))?;
    let expected = template.replace("@SPACEWARS_ROOT@", &format!("/dev/{active}"));
    if default
        .split_ascii_whitespace()
        .ne(expected.split_ascii_whitespace())
    {
        return Err(invalid(
            "fresh default is already provisioned or selects another root",
        ));
    }
    running_selection(default, running)?;
    Ok(BootstrapOrigin::FreshImage {
        image: Box::new(image),
    })
}

#[cfg(test)]
#[path = "bootstrap_fresh_tests.rs"]
pub(super) mod tests;
