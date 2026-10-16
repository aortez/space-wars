//! Root-only device admission around the restartable bootstrap image writer.
use super::*;
use crate::bootstrap::{self, RootInstallProgress, RootInstallReport, SshCheckConnection};
use crate::install_linux::BlockTarget;

/// Writes the fixed package only to the newly observed inactive root. The
/// operator must have independently exported the selected backups. Successful
/// logins and a complete privilege audit remain prerequisites for a later trial.
pub fn install_bootstrap_root(
    expected: Sha256,
    raw_copies: Sha256,
    data_copy: Sha256,
    originals: Sha256,
    connection: SshCheckConnection,
    mut report: impl FnMut(RootInstallProgress),
) -> Result<RootInstallReport> {
    let lock = InstallLock::acquire()?;
    report(RootInstallProgress::CheckingPrerequisites);
    let quiet = bootstrap::quiesce::QuietRuntime::acquire()?;
    let inspected = Installed::open(expected, &lock, false)?;
    let binding = inspected
        .existing
        .as_ref()
        .ok_or_else(|| invalid("bootstrap must be prepared before installing its root"))?;
    let active =
        bootstrap::state::optional_directory(&inspected.parent, bootstrap::state::ACTIVE, 0)?
            .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
    let root = crate::owned_fs::open_directory(std::path::Path::new("/"), 0)?;
    require_masked_writers(&inspected.data)?;
    crate::access_linux::preserved_for_bootstrap(&lock)?;
    crate::network_linux::preserved_for_bootstrap(&lock)?;
    let legacy_inputs = legacy_inputs(&inspected.package, binding, raw_copies, data_copy)?;
    let replacement_journal = bootstrap::legacy_replace::completed(
        &root,
        &active,
        0,
        binding,
        &legacy_inputs,
        originals,
    )?;
    bootstrap::access_check::check(&root, &inspected.data, &connection)?;

    // Rehash the recovery copies before every explicit attempt, including a
    // resume. A report produced earlier is not installation authority.
    report(RootInstallProgress::CheckingRecoveryCopies);
    let raw = bootstrap::backup::verify_backup_export(
        std::path::Path::new("/data/spacewars-bootstrap/active"),
        raw_copies,
    )?;
    if raw.package_sha256 != expected
        || raw.device_id != binding.device_id
        || raw.layout_id != binding.layout_id
        || raw.inactive_root_bytes != binding.inactive_partition_bytes
    {
        return Err(invalid(
            "root-install recovery copies belong to another preparation",
        ));
    }
    bootstrap::data_backup::verified_receipt(&active, 0, binding, data_copy)?;
    let package = &inspected.package.record;
    let inputs = bootstrap::root_install::Inputs {
        binding_sha256: Sha256::digest(&serde_json::to_vec(binding)?),
        package_sha256: expected,
        raw_copies_sha256: raw_copies,
        data_receipt_sha256: data_copy,
        originals_sha256: originals,
        replacement_journal_sha256: replacement_journal,
        original_inactive: Blob {
            bytes: raw.inactive_root_bytes,
            sha256: raw.inactive_root_sha256,
        },
        compressed: package.compressed.clone(),
        expanded: package.expanded.clone(),
        image: package.image.clone(),
        target: binding.target,
    };
    let mut payload = inspected
        .package
        .files
        .get("rootfs.ext4.zst")
        .ok_or_else(|| invalid("missing checked bootstrap root image"))?
        .file
        .try_clone()?;
    let guard = |target: Option<&File>| -> Result<()> {
        inspected.check(&lock)?;
        bootstrap::backup::check_named_directory(
            &inspected.parent,
            bootstrap::state::ACTIVE,
            &active,
            0,
        )?;
        require_masked_writers(&inspected.data)?;
        quiet.check_with_target(target)?;
        crate::access_linux::preserved_for_bootstrap(&lock)?;
        crate::network_linux::preserved_for_bootstrap(&lock)?;
        let actual = inspect_legacy_boot()?;
        if actual.layout != inspected.legacy.layout || actual.boot_id != inspected.legacy.boot_id {
            return Err(invalid(
                "live boot or layout changed during root installation",
            ));
        }
        let observed = bootstrap::BootstrapBinding::observed(expected, binding.target, &actual)?;
        bootstrap::state::existing(&inspected.parent, 0, &observed)?
            .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
        if let Some(file) = target {
            BlockTarget::check_opened(file, &actual.layout)?;
        }
        if bootstrap::legacy_replace::completed(
            &root,
            &active,
            0,
            binding,
            &legacy_inputs,
            originals,
        )? != replacement_journal
        {
            return Err(invalid(
                "fallback-root replacement changed during root installation",
            ));
        }
        Ok(())
    };
    let mut tools = crate::image_tools::ImageTools::default();
    let result = bootstrap::root_install::apply(
        &active,
        0,
        &inputs,
        &mut payload,
        || BlockTarget::open(&inspected.legacy.layout, package.expanded.bytes),
        guard,
        |file| tools.check_bootstrap(file),
        &mut |event| {
            report(event);
            Ok(())
        },
    )?;
    bootstrap::access_check::check(&root, &inspected.data, &connection)?;
    quiet.check()?;
    inspected.check(&lock)?;
    Ok(result)
}
