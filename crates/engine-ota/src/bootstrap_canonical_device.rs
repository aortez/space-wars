//! Separate root-operator prepare and activation commands for the permanent
//! boot layout. Neither operation writes a root partition or provisions OTA.
use super::*;
use crate::bootstrap::canonical::CanonicalPlan;
use crate::bootstrap::{self, CanonicalBoot, CanonicalPhase, TrialStore};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct CanonicalReport {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub root_install_sha256: Sha256,
    pub trial_id: Sha256,
    pub phase: CanonicalPhase,
    pub reboot_job_accepted: bool,
    pub automatic_updates_enabled: bool,
}

/// Opaque permission held only during the checked root-operator handoff.
pub struct CanonicalReboot<'a> {
    guard: &'a mut dyn FnMut() -> Result<()>,
    authorized: bool,
    queued: bool,
}
impl CanonicalReboot<'_> {
    pub fn authorize_reboot(&mut self) -> Result<()> {
        self.authorized = false;
        if self.queued {
            return Err(invalid("canonical reboot job was already accepted"));
        }
        (self.guard)()?;
        self.authorized = true;
        Ok(())
    }
    pub fn reboot_queued(&mut self) -> Result<()> {
        if !self.authorized || self.queued {
            return Err(invalid("canonical reboot was not authorized"));
        }
        self.queued = true;
        Ok(())
    }
}

pub fn prepare_canonical_boot(
    package: Sha256,
    written_root: Sha256,
    trial: Sha256,
    report: impl FnMut(&str),
) -> Result<CanonicalReport> {
    operate(package, written_root, trial, false, report, |_| Ok(()))
}

pub fn activate_canonical_boot(
    package: Sha256,
    written_root: Sha256,
    trial: Sha256,
    report: impl FnMut(&str),
    request: impl FnOnce(&mut CanonicalReboot<'_>) -> Result<()>,
) -> Result<CanonicalReport> {
    operate(package, written_root, trial, true, report, request)
}

fn operate(
    expected: Sha256,
    written_root: Sha256,
    trial: Sha256,
    activate: bool,
    mut report: impl FnMut(&str),
    request: impl FnOnce(&mut CanonicalReboot<'_>) -> Result<()>,
) -> Result<CanonicalReport> {
    let lock = InstallLock::acquire()?;
    report("canonical-checking-prerequisites");
    let mut store = TrialStore::open(&lock)?
        .ok_or_else(|| invalid("canonical migration requires the existing bootstrap journal"))?;
    let inputs = store.durable()?.inputs.clone();
    if inputs.binding.package_sha256 != expected
        || inputs.root_install_sha256 != written_root
        || inputs.plan.trial_id != trial
    {
        return Err(invalid(
            "canonical migration does not match the operator-selected attempt",
        ));
    }
    let data = crate::recovery_linux::open_data_mount()?;
    let parent = open_at(&data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&parent.metadata()?, true, 0, true)?;
    let mut package = OperatorPackage::under(
        open_at(&parent, PACKAGE, libc::O_RDONLY | libc::O_DIRECTORY, 0)?,
        0,
        expected,
    )?;
    let runtime = crate::boot_linux::Runtime::inspect()?;
    let observed = bootstrap::trial_linux::observation(&runtime, &inputs, true)?;
    store.durable()?.check_boot(&observed)?;
    if observed.tryboot
        || package.record.image != inputs.image
        || package.record.plan != inputs.plan
    {
        return Err(invalid(
            "canonical migration requires the selected image's normal boot",
        ));
    }
    let config = String::from_utf8(package.small("boot/config.txt", 16 * 1024)?)
        .map_err(|_| invalid("invalid canonical config text"))?;
    let profile = String::from_utf8(package.small(
        &format!("boot/spacewars-profiles/{}.txt", inputs.plan.profile),
        16 * 1024,
    )?)
    .map_err(|_| invalid("invalid canonical profile text"))?;
    let manifest_text = String::from_utf8(package.small(
        "boot/spacewars-boot-manifest.json",
        MAX_BOOT_MANIFEST_BYTES as u64,
    )?)
    .map_err(|_| invalid("invalid canonical manifest text"))?;
    let manifest = BootManifest::parse(manifest_text.as_bytes(), inputs.image.boot_identity)?;
    let derived = BootPlan::create(
        &inputs.image,
        &manifest,
        &config,
        &profile,
        &BootCommandLine::parse(inputs.command_line_template.as_bytes())?,
        &PlanRequest {
            board: Board::from_model(&runtime.physical.model)?,
            profile: runtime.profile.clone(),
            roots: runtime.roots()?,
            target: runtime.layout.active,
            trial_id: trial,
        },
    )?;
    if derived != inputs.plan {
        return Err(invalid(
            "canonical plan differs from the actual board, profile or root pair",
        ));
    }
    let plan = CanonicalPlan {
        manifest: manifest_text,
        config,
        files: package
            .record
            .files
            .iter()
            .filter_map(|(p, b)| p.strip_prefix("boot/").map(|p| (p.into(), b.clone())))
            .collect(),
    };
    plan.validate(&inputs)?;
    let (installed, digest) =
        bootstrap::root_install::read_completed(store.active_directory()?, 0)?;
    if digest != written_root
        || installed.binding_sha256 != Sha256::digest(&serde_json::to_vec(&inputs.binding)?)
        || installed.package_sha256 != expected
        || installed.image != inputs.image
        || installed.compressed != package.record.compressed
        || installed.expanded != package.record.expanded
        || installed.target != inputs.plan.target
    {
        return Err(invalid(
            "canonical migration root receipt differs from the prepared image",
        ));
    }
    report("canonical-checking-recovery-copies");
    let raw = bootstrap::backup::verify_backup_export(
        Path::new("/data/spacewars-bootstrap/active"),
        installed.raw_copies_sha256,
    )?;
    if raw.device_id != inputs.binding.device_id
        || raw.layout_id != inputs.binding.layout_id
        || raw.package_sha256 != expected
        || raw.inactive_root_sha256 != installed.original_inactive.sha256
        || raw.inactive_root_bytes != installed.original_inactive.bytes
    {
        return Err(invalid(
            "canonical recovery copies do not match the prepared device",
        ));
    }
    bootstrap::data_backup::verified_receipt(
        store.active_directory()?,
        0,
        &inputs.binding,
        installed.data_receipt_sha256,
    )?;
    bootstrap::legacy_state::verified_receipt(
        store.active_directory()?,
        0,
        &inputs.binding,
        installed.originals_sha256,
    )?;
    let guard = |store: &TrialStore| -> Result<()> {
        lock.check()?;
        store.durable()?;
        package.check()?;
        let current_data = crate::recovery_linux::open_data_mount()?;
        bootstrap::backup::check_named_directory(&current_data, DIRECTORY, &parent, 0)?;
        bootstrap::backup::check_named_directory(&parent, PACKAGE, &package.directory, 0)?;
        if current_data.metadata()?.dev() != data.metadata()?.dev()
            || current_data.metadata()?.ino() != data.metadata()?.ino()
            || crate::recovery_linux::opened_mount_id(&current_data)?
                != crate::recovery_linux::opened_mount_id(&data)?
            || bootstrap::root_install::completed(store.active_directory()?, 0, &installed)?
                != written_root
            || !runtime.same_boot(&crate::boot_linux::Runtime::inspect()?)
        {
            return Err(invalid(
                "canonical migration device, package or completion receipt changed",
            ));
        }
        crate::access_linux::preserved_for_bootstrap(&lock)?;
        crate::network_linux::preserved_for_bootstrap(&lock)?;
        Ok(())
    };
    guard(&store)?;
    if store.canonical_phase().is_none() {
        if activate {
            return Err(invalid(
                "canonical activation cannot initialize preparation",
            ));
        }
        let staged = bootstrap::BootstrapBoot::open(&lock, &store)?;
        let current = staged.observe(&store)?;
        store.advance(|record| record.start_canonical(plan.clone(), &current))?;
        report("canonical-migration-intent");
    } else if store.canonical()?.plan != plan {
        return Err(invalid("canonical migration belongs to a different plan"));
    }
    let mut boot = CanonicalBoot::open(&lock, &store)?;
    let mut result = CanonicalReport {
        schema: 1,
        scope: "canonical assets prepared; staged normal default and recovery tree retained; activation and separate normal-boot confirmation remain",
        package_sha256: expected,
        root_install_sha256: written_root,
        trial_id: trial,
        phase: store.canonical()?.phase,
        reboot_job_accepted: false,
        automatic_updates_enabled: false,
    };
    if !activate {
        boot.files.prepare(
            &mut store,
            |path| {
                Ok(package
                    .files
                    .get(&format!("boot/{path}"))
                    .ok_or_else(|| invalid("missing checked canonical source"))?
                    .file
                    .try_clone()?)
            },
            |store| {
                guard(store)?;
                boot.guard(store)?;
                store
                    .canonical()?
                    .authorize_copy(&inputs, &boot.observe(store)?)
            },
            |point| {
                report(point);
                Ok(())
            },
        )?;
        guard(&store)?;
        boot.guard(&store)?;
        result.phase = store.canonical()?.phase;
        return Ok(result);
    }
    // Preparation never runs in this branch. An incomplete or changed copy
    // cannot be repaired as a side effect of asking to activate it.
    if matches!(
        store.canonical()?.phase,
        CanonicalPhase::Prepared | CanonicalPhase::Selecting
    ) {
        guard(&store)?;
        let current = boot.observe(&store)?;
        store.advance_canonical(|c, inputs| c.begin_selection(inputs, &current))?;
        report("canonical-selection-intent");
        let current = boot.select(&store, true)?;
        store.advance_canonical(|c, inputs| c.selected(inputs, &current))?;
        report("canonical-default-selected");
    }
    guard(&store)?;
    boot.guard(&store)?;
    let requested = boot.observe(&store)?;
    store.advance_canonical(|c, inputs| c.request_reboot(inputs, &requested))?;
    report("canonical-reboot-consumed");
    result.phase = CanonicalPhase::Selected;
    result.scope = "canonical normal reboot job accepted; separate boot health and production provisioning remain unobserved";
    let outcome = {
        let mut authorize = || {
            guard(&store)?;
            boot.authorize_reboot(&store, false)
        };
        let mut permit = CanonicalReboot {
            guard: &mut authorize,
            authorized: false,
            queued: false,
        };
        request(&mut permit).and_then(|()| {
            if permit.queued {
                Ok(())
            } else {
                Err(invalid("canonical reboot job was not accepted"))
            }
        })
    };
    if let Err(error) = outcome {
        let failed = store.advance_canonical(|c, inputs| {
            c.fail(
                inputs,
                &requested,
                bootstrap::CanonicalFailure::RebootRequestFailed,
            )
        });
        return Err(invalid(&format!(
            "{error}; canonical reboot attempt consumed; operator reconciliation required{}",
            failed
                .err()
                .map(|e| format!("; failure record: {e}"))
                .unwrap_or_default()
        )));
    }
    result.reboot_job_accepted = true;
    Ok(result)
}
