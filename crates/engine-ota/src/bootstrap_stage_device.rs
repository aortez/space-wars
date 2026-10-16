//! Explicit root-operator staging, separate from the later one-shot reboot.
use super::*;
use crate::bootstrap::{self, TrialPhase, TrialStore};
use crate::install_linux::{BlockTarget, Target, check_written_ext4};
use crate::owned_fs::open_directory;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "stage", content = "checkpoint", rename_all = "kebab-case")]
pub enum BootStageProgress {
    CheckingPrerequisites,
    CheckingRecoveryCopies,
    CheckingWrittenRoot,
    Preparing(String),
    Ready,
}

#[derive(Debug, Serialize)]
pub struct BootStageReport {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub root_install_sha256: Sha256,
    pub trial_id: Sha256,
    pub phase: TrialPhase,
    pub original_default_retained: bool,
    pub reboot_requested: bool,
    pub automatic_updates_enabled: bool,
}

/// Prepare only the recorded temporary assets. No operation in this function
/// asks firmware/systemd to use tryboot or changes the retained normal default.
pub fn stage_bootstrap_boot(
    expected: Sha256,
    raw_copies: Sha256,
    data_copy: Sha256,
    originals: Sha256,
    written_root: Sha256,
    connection: bootstrap::SshCheckConnection,
    mut report: impl FnMut(BootStageProgress),
) -> Result<BootStageReport> {
    let lock = InstallLock::acquire()?;
    report(BootStageProgress::CheckingPrerequisites);
    let quiet = bootstrap::quiesce::QuietRuntime::acquire()?;
    let mut store = TrialStore::for_staging(&lock)?;
    let previous = store
        .as_ref()
        .map(|store| staging_inputs(store).cloned())
        .transpose()?;
    let mut inspected = Installed::open_staging(expected, &lock, false, previous.as_ref())?;
    let binding = inspected
        .existing
        .as_ref()
        .ok_or_else(|| invalid("bootstrap preparation must exist before boot staging"))?;
    let active =
        bootstrap::state::optional_directory(&inspected.parent, bootstrap::state::ACTIVE, 0)?
            .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
    let root = open_directory(Path::new("/"), 0)?;
    let boot = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    crate::boot_linux::check_mount(&boot, &inspected.legacy.layout, true)?;
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
    report(BootStageProgress::CheckingRecoveryCopies);
    let raw = bootstrap::backup::verify_backup_export(
        Path::new("/data/spacewars-bootstrap/active"),
        raw_copies,
    )?;
    if raw.package_sha256 != expected
        || raw.device_id != binding.device_id
        || raw.layout_id != binding.layout_id
        || raw.inactive_root_bytes != binding.inactive_partition_bytes
    {
        return Err(invalid(
            "boot staging recovery copies differ from preparation",
        ));
    }
    bootstrap::data_backup::verified_receipt(&active, 0, binding, data_copy)?;
    let utf8 =
        |bytes| String::from_utf8(bytes).map_err(|_| invalid("invalid bootstrap text input"));
    let kernel_build_id = utf8(
        inspected
            .package
            .small("boot/spacewars-kernel-build-id", 41)?,
    )?
    .trim()
    .to_owned();
    let command_line_template = utf8(
        inspected
            .package
            .small("boot/spacewars-cmdline.txt", 4096)?,
    )?;
    let package = &inspected.package.record;
    let root_inputs = bootstrap::root_install::Inputs {
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
    if bootstrap::root_install::completed(&active, 0, &root_inputs)? != written_root {
        return Err(invalid(
            "completed root installation differs from selected receipt",
        ));
    }
    let mut inputs = bootstrap::trial::TrialInputs {
        binding: binding.clone(),
        root_install_sha256: written_root,
        image: package.image.clone(),
        kernel_build_id,
        command_line_template,
        legacy_config: utf8(read_file(
            open_at(&boot, "config.txt", libc::O_RDONLY, 0)?,
            0,
            16 * 1024,
        )?)?,
        plan: package.plan.clone(),
        origin_boot_id: inspected.legacy.boot_id.clone(),
    };
    if let Some(previous) = previous {
        // An ordinary legacy reboot may resume copying. Preserve the original
        // staging boot ID and never change the attempt or installed-image binding.
        inputs.origin_boot_id.clone_from(&previous.origin_boot_id);
        if previous != inputs {
            return Err(invalid("staging intent belongs to different inputs"));
        }
    }
    inputs.validate()?;
    let mut target = BlockTarget::open(&inspected.legacy.layout, package.expanded.bytes)?;
    let held = target.file().try_clone()?;
    let guard = || -> Result<()> {
        inspected.check(&lock)?;
        bootstrap::backup::check_named_directory(
            &inspected.parent,
            bootstrap::state::ACTIVE,
            &active,
            0,
        )?;
        if bootstrap::state::read_binding(&active, 0)? != *binding {
            return Err(invalid("bootstrap binding changed during staging"));
        }
        require_masked_writers(&inspected.data)?;
        quiet.check_with_target(Some(&held))?;
        crate::access_linux::preserved_for_bootstrap(&lock)?;
        crate::network_linux::preserved_for_bootstrap(&lock)?;
        let (actual, _) = bootstrap::inspect::inspect_staging(Some(&inputs))?;
        let observed = bootstrap::BootstrapBinding::observed(expected, binding.target, &actual)?;
        bootstrap::state::existing(&inspected.parent, 0, &observed)?
            .ok_or_else(|| invalid("bootstrap reservation disappeared"))?;
        if actual.boot_id != inspected.legacy.boot_id || actual.layout != inspected.legacy.layout {
            return Err(invalid("running boot or layout changed during staging"));
        }
        let named_boot = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        if named_boot.metadata()?.ino() != boot.metadata()?.ino()
            || named_boot.metadata()?.dev() != boot.metadata()?.dev()
            || crate::recovery_linux::opened_mount_id(&named_boot)?
                != crate::recovery_linux::opened_mount_id(&boot)?
        {
            return Err(invalid("boot mount changed during staging"));
        }
        crate::boot_linux::check_mount(&boot, &actual.layout, true)?;
        BlockTarget::check_opened(&held, &actual.layout)?;
        if bootstrap::legacy_replace::completed(
            &root,
            &active,
            0,
            binding,
            &legacy_inputs,
            originals,
        )? != replacement_journal
            || bootstrap::root_install::completed(&active, 0, &root_inputs)? != written_root
        {
            return Err(invalid(
                "bootstrap completion receipts changed during staging",
            ));
        }
        Ok(())
    };
    let check_root = |target: &mut BlockTarget| -> Result<()> {
        guard()?;
        target.flush_to_device()?;
        let mut tools = crate::image_tools::ImageTools::default();
        check_written_ext4(
            &package.expanded,
            target,
            |file| {
                if tools.check_bootstrap(file)? != package.image {
                    return Err(invalid("installed bootstrap image identity changed"));
                }
                Ok(())
            },
            &mut |_| guard(),
        )?;
        guard()
    };
    report(BootStageProgress::CheckingWrittenRoot);
    check_root(&mut target)?;
    if store.is_none() {
        bootstrap::trial_store::prepare(&active, 0, &inputs, |point| {
            guard()?;
            report(BootStageProgress::Preparing(point.into()));
            Ok(())
        })?;
        store = TrialStore::for_staging(&lock)?;
    }
    let store = store
        .as_mut()
        .ok_or_else(|| invalid("staging journal disappeared"))?;
    if staging_inputs(store)? != &inputs {
        return Err(invalid("staging inputs changed"));
    }
    if store.phase() == TrialPhase::TrialReady {
        bootstrap::trial_files::TrialBootFiles::open(boot.try_clone()?, 0, inputs.clone())?;
    } else {
        bootstrap::stage_files::apply(
            &boot,
            0,
            &inputs,
            |path| {
                let source = inspected
                    .package
                    .files
                    .get(&format!("{STAGE_DIRECTORY}/{path}"))
                    .ok_or_else(|| invalid("missing checked staging source"))?;
                Ok(source.file.try_clone()?)
            },
            |point| {
                guard()?;
                if staging_inputs(store)? != &inputs {
                    return Err(invalid("staging journal changed"));
                }
                report(BootStageProgress::Preparing(point.into()));
                Ok(())
            },
        )?;
    }
    report(BootStageProgress::CheckingWrittenRoot);
    check_root(&mut target)?;
    bootstrap::trial_files::TrialBootFiles::open(boot.try_clone()?, 0, inputs.clone())?;
    store.advance(|record| {
        if !matches!(record.phase, TrialPhase::Staging | TrialPhase::TrialReady) {
            return Err(invalid(
                "bootstrap trial has already been requested or observed",
            ));
        }
        record.phase = TrialPhase::TrialReady;
        Ok(())
    })?;
    bootstrap::access_check::check(&root, &inspected.data, &connection)?;
    guard()?;
    report(BootStageProgress::Ready);
    Ok(BootStageReport {
        schema: 1,
        scope: "temporary boot tree and trial file prepared; normal default retained; one-shot reboot and physical qualification remain separate",
        package_sha256: expected,
        root_install_sha256: written_root,
        trial_id: inputs.plan.trial_id,
        phase: store.phase(),
        original_default_retained: true,
        reboot_requested: false,
        automatic_updates_enabled: false,
    })
}

fn staging_inputs(store: &TrialStore) -> Result<&bootstrap::trial::TrialInputs> {
    let record = store.durable()?;
    if !matches!(record.phase, TrialPhase::Staging | TrialPhase::TrialReady) {
        return Err(invalid(
            "bootstrap has already requested or observed a trial",
        ));
    }
    Ok(&record.inputs)
}
