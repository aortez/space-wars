//! Actual normal-boot evidence during canonical copy and confirmation.
use super::canonical_files::CanonicalFiles;
use super::{CanonicalObservation, TrialStore, invalid};
use crate::Result;
use crate::boot_linux::{Runtime, check_mount};
use crate::install_lock::InstallLock;
use crate::owned_fs::{open_at, open_directory};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub struct CanonicalBoot<'a> {
    pub(super) lock: &'a InstallLock,
    pub(super) runtime: Runtime,
    pub(super) files: CanonicalFiles,
    uncertain: bool,
}

impl<'a> CanonicalBoot<'a> {
    pub fn open(lock: &'a InstallLock, store: &TrialStore) -> Result<Self> {
        lock.check()?;
        let runtime = Runtime::inspect()?;
        let root = open_directory(Path::new("/"), 0)?;
        let directory = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        check_mount(&directory, &runtime.layout, false)?;
        let files = CanonicalFiles::open(
            directory,
            0,
            store.durable()?.inputs.clone(),
            store.canonical()?,
        )?;
        let boot = Self {
            lock,
            runtime,
            files,
            uncertain: false,
        };
        boot.observe(store)?;
        Ok(boot)
    }

    pub fn observe(&self, store: &TrialStore) -> Result<CanonicalObservation> {
        if self.uncertain {
            return Err(invalid(
                "canonical selector durability is uncertain; reopen required",
            ));
        }
        self.inspect(store)
    }

    fn inspect(&self, store: &TrialStore) -> Result<CanonicalObservation> {
        self.lock.check()?;
        let current = Runtime::inspect()?;
        let record = store.durable()?;
        if !self.runtime.same_boot(&current) || record.inputs != self.files.inputs {
            return Err(invalid(
                "canonical migration runtime or immutable inputs changed",
            ));
        }
        let root = open_directory(Path::new("/"), 0)?;
        let named = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        if named.metadata()?.ino() != self.files.directory.metadata()?.ino()
            || named.metadata()?.dev() != self.files.directory.metadata()?.dev()
            || crate::recovery_linux::opened_mount_id(&named)?
                != crate::recovery_linux::opened_mount_id(&self.files.directory)?
        {
            return Err(invalid("canonical boot mount was replaced"));
        }
        check_mount(&self.files.directory, &current.layout, false)?;
        let boot = super::trial_linux::observation(&current, &record.inputs, true)?;
        record.check_boot(&boot)?;
        let observed = CanonicalObservation {
            boot,
            default: self.files.selected(store.canonical()?)?,
        };
        store.canonical()?.check_boot(&record.inputs, &observed)?;
        Ok(observed)
    }

    pub(super) fn guard(&self, store: &TrialStore) -> Result<()> {
        self.observe(store)?;
        self.files.verify(store.canonical()?)?;
        check_mount(&self.files.directory, &self.runtime.layout, true)
    }

    pub fn select(&mut self, store: &TrialStore, canonical: bool) -> Result<CanonicalObservation> {
        self.guard(store)?;
        self.uncertain = true;
        self.files.select(
            canonical,
            store,
            |store| {
                self.inspect(store)?;
                check_mount(&self.files.directory, &self.runtime.layout, true)
            },
            |_| Ok(()),
        )?;
        let after = self.inspect(store)?;
        self.uncertain = false;
        Ok(after)
    }

    pub fn authorize_reboot(&self, store: &TrialStore, fallback: bool) -> Result<()> {
        self.guard(store)?;
        let boot = self.observe(store)?;
        store
            .canonical()?
            .authorize_reboot(&store.durable()?.inputs, &boot, fallback)
    }
}
