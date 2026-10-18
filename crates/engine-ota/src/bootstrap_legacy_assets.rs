//! Fixed fallback-root replacements, extracted from the selected rootfs.
//! Packaging never installs them or treats them as an executable migration hook.
use crate::Result;
use crate::image_tools::ImageReader;
use std::fs::File;

pub(super) const PREFIX: &str = "legacy-root/";
pub(super) const MAX_BYTES: u64 = 64 * 1024;

// Destination modes are fixed by the migration protocol, not package input.
// The manager binary is already separately checked against the image identity.
pub(super) const FILES: &[(&str, u32)] = &[
    ("etc/sudoers.d/spacewars-ota", 0o440),
    ("etc/ssh/sshd_config.d/10-spacewars-access.conf", 0o644),
    ("usr/sbin/ab-update", 0o755),
    ("usr/sbin/ab-update-with-key", 0o755),
    ("usr/sbin/ab-boot-manager", 0o755),
    ("usr/sbin/spacewars-fast-update", 0o755),
    ("usr/sbin/spacewars-renderer", 0o755),
    ("usr/sbin/persistent-data-init", 0o755),
    ("usr/sbin/spacewars-ssh-host-keys", 0o755),
    ("usr/sbin/spacewars-bluetooth-data-init", 0o755),
    ("usr/libexec/spacewars/fast-update", 0o700),
    ("usr/libexec/spacewars/renderer", 0o700),
    ("usr/lib/tmpfiles.d/spacewars-install.conf", 0o644),
    ("usr/lib/systemd/system/persistent-data-init.service", 0o644),
    ("usr/lib/systemd/system/persistent-data-bind.service", 0o644),
    (
        "usr/lib/systemd/system/NetworkManager.service.d/spacewars-persistent-network.conf",
        0o644,
    ),
    ("usr/lib/systemd/system/spacewars-access.service", 0o644),
    (
        "usr/lib/systemd/system/sshdgenkeys.service.d/spacewars-host-keys.conf",
        0o644,
    ),
    (
        "usr/lib/systemd/system/sshd.service.d/spacewars-access.conf",
        0o644,
    ),
    (
        "usr/lib/systemd/system/sshd@.service.d/spacewars-access.conf",
        0o644,
    ),
];

pub(super) fn package_path(path: &str) -> Option<u32> {
    let relative = path.strip_prefix(PREFIX)?;
    FILES
        .iter()
        .find_map(|(p, mode)| (*p == relative).then_some(*mode))
}

/// Read only known files from an already-pinned, checked ext4 artifact. Reject
/// links in every source component and refuse to silently normalize bad modes.
pub(super) fn extract(root: &File, mut save: impl FnMut(&str, &[u8]) -> Result<()>) -> Result<()> {
    let mut reader = ImageReader::new(root);
    for &(path, mode) in FILES {
        let contents = reader.file(&format!("/{path}"), mode, MAX_BYTES as usize)?;
        save(&format!("{PREFIX}{path}"), &contents)?;
    }
    Ok(())
}
