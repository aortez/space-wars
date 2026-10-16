//! Fixed-path Linux adapter for an explicitly staged first-image trial.
use super::trial_files::TrialBootFiles;
use super::{Board, TrialStore, invalid};
use crate::boot::BootCommandLine;
use crate::boot_linux::{Runtime, check_mount};
use crate::install_lock::InstallLock;
use crate::owned_fs::{open_at, open_directory};
use crate::recovery::BootObservation;
use crate::{EarlyWatchdog, Result, WatchdogCompletion};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub struct BootstrapBoot<'a> {
    lock: &'a InstallLock,
    runtime: Runtime,
    files: TrialBootFiles,
    uncertain: bool,
}

impl<'a> BootstrapBoot<'a> {
    pub fn open(lock: &'a InstallLock, store: &TrialStore) -> Result<Self> {
        lock.check()?;
        let runtime = Runtime::inspect()?;
        let root = open_directory(Path::new("/"), 0)?;
        let directory = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        check_mount(&directory, &runtime.layout, false)?;
        let files = TrialBootFiles::open(directory, 0, store.durable()?.inputs.clone())?;
        let boot = Self {
            lock,
            runtime,
            files,
            uncertain: false,
        };
        boot.observe(store)?;
        Ok(boot)
    }

    pub fn observe(&self, store: &TrialStore) -> Result<BootObservation> {
        if self.uncertain {
            return Err(invalid(
                "bootstrap selector result is uncertain; reopen required",
            ));
        }
        self.lock.check()?;
        observe(&self.runtime, &self.files, store)
    }

    pub fn trial_watchdog(&self, store: &TrialStore) -> Result<EarlyWatchdog> {
        crate::watchdog_linux::inspect(&self.observe(store)?)
    }

    pub fn select_default(&mut self, store: &TrialStore, staged: bool) -> Result<BootObservation> {
        let before = self.observe(store)?;
        store.durable()?.selection(&before, staged)?;
        self.uncertain = true;
        self.files.select(staged, |files| {
            self.lock.check()?;
            let current = observe(&self.runtime, files, store)?;
            check_mount(&files.directory, &self.runtime.layout, true)?;
            store.durable()?.selection(&current, staged)?;
            Ok(())
        })?;
        let after = observe(&self.runtime, &self.files, store)?;
        self.uncertain = false;
        Ok(after)
    }

    pub fn complete_watchdog(&self, store: &TrialStore) -> Result<WatchdogCompletion> {
        crate::watchdog_linux::complete_bootstrap(self.lock, store, &self.observe(store)?)
    }

    pub fn authorize_reboot(&self, store: &TrialStore, staged: bool) -> Result<()> {
        let boot = self.observe(store)?;
        store.durable()?.reboot(&boot, staged)?;
        if staged {
            crate::watchdog_linux::require_bootstrap_completed(store, &boot)?;
        }
        Ok(())
    }
}

fn observe(
    expected: &Runtime,
    files: &TrialBootFiles,
    store: &TrialStore,
) -> Result<BootObservation> {
    let current = Runtime::inspect()?;
    let inputs = &store.durable()?.inputs;
    if !expected.same_boot(&current) || inputs != &files.inputs {
        return Err(invalid("bootstrap runtime or immutable context changed"));
    }
    let root = open_directory(Path::new("/"), 0)?;
    let named = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    if named.metadata()?.dev() != files.directory.metadata()?.dev()
        || named.metadata()?.ino() != files.directory.metadata()?.ino()
        || crate::recovery_linux::opened_mount_id(&named)?
            != crate::recovery_linux::opened_mount_id(&files.directory)?
    {
        return Err(invalid("bootstrap boot mount was replaced"));
    }
    check_mount(&files.directory, &current.layout, false)?;
    let boot = observation(&current, inputs, files.staged_default()?)?;
    store.durable()?.check_boot(&boot)?;
    Ok(boot)
}

fn observation(
    current: &Runtime,
    inputs: &super::trial::TrialInputs,
    staged: bool,
) -> Result<BootObservation> {
    if current.physical.fingerprint() != inputs.binding.device_id
        || current.layout.layout_id != inputs.binding.layout_id
        || current.image != inputs.image
        || current.kernel != inputs.kernel_build_id
        || current.profile != inputs.plan.profile
        || Board::from_model(&current.physical.model)? != inputs.plan.board
        || current.layout.active != inputs.plan.target
    {
        return Err(invalid(
            "loaded bootstrap root, kernel, profile or physical device differs",
        ));
    }
    let roots = current.roots()?;
    let template = BootCommandLine::parse(inputs.command_line_template.as_bytes())?;
    if template.render(&roots, inputs.plan.target, Some(inputs.plan.trial_id))
        != inputs.plan.trial_cmdline
        || template.render(&roots, inputs.plan.target, None) != inputs.plan.normal_cmdline
    {
        return Err(invalid(
            "bootstrap command lines differ from the actual root pair",
        ));
    }
    let trial_id = template.running_trial(
        &roots,
        current.layout.active,
        current.tryboot,
        &current.command_line,
    )?;
    if trial_id != current.tryboot.then_some(inputs.plan.trial_id) {
        return Err(invalid(
            "loaded bootstrap trial token differs from the staged plan",
        ));
    }
    Ok(BootObservation {
        binding: inputs.boot_binding(),
        boot_id: current.boot_id.clone(),
        uptime_ms: current.uptime_ms,
        actual_slot: Some(current.layout.active),
        actual_build_id: Some(current.image.build_id.clone()),
        recovery_protocol: current.image.recovery_protocol,
        default_slot: Some(if staged {
            inputs.plan.target
        } else {
            inputs.binding.original_slot
        }),
        tryboot: current.tryboot,
        trial_id,
    })
}

#[cfg(test)]
#[path = "bootstrap_trial_linux_tests.rs"]
mod tests;
