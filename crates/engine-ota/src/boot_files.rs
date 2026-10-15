//! Boot filesystem operations beneath one pinned, verified directory handle.
use crate::boot::{
    BootCommandLine, BootManifest, MUTABLE_BOOT_FILES, SlotRoots, TrialSelection, asset_path,
    hardware_profile,
};
use crate::owned_fs::{io_result, open_at, owned, read_file, unchanged};
use crate::recovery::Slot;
use crate::{Blob, Error, Result, Sha256};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Selections {
    pub default: Slot,
    pub trial: Option<TrialSelection>,
    pub profile: String,
}

pub(crate) struct BootFiles {
    directory: File,
    owner: u32,
    device: u64,
    manifest: BootManifest,
    template: BootCommandLine,
    uncertain: bool,
    #[cfg(test)]
    fault: Option<FaultPoint>,
}

impl BootFiles {
    pub(crate) fn open(directory: File, owner: u32, expected: Sha256) -> Result<Self> {
        let metadata = directory.metadata()?;
        owned(&metadata, true, owner, false)?;
        let manifest = BootManifest::parse(
            &read_file(
                open_at(
                    &directory,
                    "spacewars-boot-manifest.json",
                    libc::O_RDONLY,
                    0,
                )?,
                owner,
                crate::boot::MAX_BOOT_MANIFEST_BYTES as u64,
            )?,
            expected,
        )?;
        let template_bytes = read_file(
            open_at(&directory, "spacewars-cmdline.txt", libc::O_RDONLY, 0)?,
            owner,
            4096,
        )?;
        check_bytes(&manifest, "spacewars-cmdline.txt", &template_bytes)?;
        let files = Self {
            directory,
            owner,
            device: metadata.dev(),
            manifest,
            template: BootCommandLine::parse(&template_bytes)?,
            uncertain: false,
            #[cfg(test)]
            fault: None,
        };
        files.verify()?;
        Ok(files)
    }

    pub(crate) fn directory(&self) -> &File {
        &self.directory
    }

    pub(crate) fn identity(&self) -> Sha256 {
        self.manifest.identity()
    }

    pub(crate) fn template(&self) -> &BootCommandLine {
        &self.template
    }

    pub(crate) fn asset(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        self.certain()?;
        let bytes = self.read(path, limit)?;
        check_bytes(&self.manifest, path, &bytes)?;
        Ok(bytes)
    }

    pub(crate) fn selections(&self, roots: &SlotRoots) -> Result<Selections> {
        self.certain()?;
        Ok(Selections {
            default: self
                .template
                .default_slot(roots, &self.read("cmdline.txt", 4096)?)?,
            trial: self
                .template
                .trial(roots, &self.read("spacewars-trial.txt", 8192)?)?,
            profile: hardware_profile(
                std::str::from_utf8(&self.read("spacewars-device.txt", 4096)?)
                    .map_err(|_| invalid("non-UTF8 hardware selection"))?,
            )?
            .into(),
        })
    }

    /// Before any mutation the coordinator repeats this under InstallLock.
    pub(crate) fn verify(&self) -> Result<()> {
        self.certain()?;
        BootManifest::parse(
            &self.read(
                "spacewars-boot-manifest.json",
                crate::boot::MAX_BOOT_MANIFEST_BYTES as u64,
            )?,
            self.identity(),
        )?;
        if self.read("spacewars-boot-id", 65)? != format!("{}\n", self.identity()).as_bytes() {
            return Err(invalid("boot identity file disagrees with installed image"));
        }
        let mut expected: BTreeSet<String> = self.manifest.files().keys().cloned().collect();
        expected.extend(MUTABLE_BOOT_FILES.iter().map(|path| (*path).into()));
        let mut found = BTreeSet::new();
        let mut temporary = 0;
        self.inventory(&self.directory, "", &expected, &mut found, &mut temporary)?;
        if found != expected {
            return Err(invalid("boot filesystem is missing required files"));
        }
        for (path, expected) in self.manifest.files() {
            self.verify_blob(path, expected)?;
        }
        Ok(())
    }

    fn inventory(
        &self,
        directory: &File,
        prefix: &str,
        expected: &BTreeSet<String>,
        found: &mut BTreeSet<String>,
        temporary: &mut usize,
    ) -> Result<()> {
        let before = directory.metadata()?;
        self.owned(&before, true)?;
        // The kernel's descriptor link pins this exact directory. Names from
        // read_dir are opened relative to its descriptor, never followed as paths.
        for (index, entry) in fs::read_dir(fd_path(directory))?.enumerate() {
            if index >= 2048 {
                return Err(invalid("too many boot directory entries"));
            }
            let entry = entry?;
            let component = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("invalid boot filename"))?;
            let path = format!("{prefix}{component}");
            if entry.file_type()?.is_dir() {
                let sub = format!("{path}/");
                if !asset_path(&path) || !expected.iter().any(|name| name.starts_with(&sub)) {
                    return Err(invalid("unexpected boot directory"));
                }
                let child = open_at(directory, &component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                self.inventory(&child, &sub, expected, found, temporary)?;
            } else {
                let leftover = prefix.is_empty() && temporary_name(&component);
                if !expected.contains(&path) && !leftover {
                    return Err(invalid(format!("unexpected boot file: {path}")));
                }
                let file = open_at(directory, &component, libc::O_RDONLY, 0)?;
                let metadata = file.metadata()?;
                self.owned(&metadata, false)?;
                if leftover {
                    *temporary += 1;
                    if *temporary > 16 || metadata.len() > 8192 {
                        return Err(invalid("excessive or oversized interrupted selector files"));
                    }
                } else {
                    found.insert(path);
                }
            }
        }
        if !unchanged(&before, &directory.metadata()?) {
            return Err(invalid("boot directory changed during inspection"));
        }
        Ok(())
    }

    fn open_file(&self, path: &str) -> Result<File> {
        if !asset_path(path) {
            return Err(invalid("invalid boot asset path"));
        }
        let mut directory = self.directory.try_clone()?;
        let mut components = path.split('/').peekable();
        while let Some(component) = components.next() {
            let is_directory = components.peek().is_some();
            let file = open_at(
                &directory,
                component,
                libc::O_RDONLY | if is_directory { libc::O_DIRECTORY } else { 0 },
                0,
            )?;
            self.owned(&file.metadata()?, is_directory)?;
            if !is_directory {
                return Ok(file);
            }
            directory = file;
        }
        Err(invalid("empty boot asset path"))
    }

    fn owned(&self, metadata: &fs::Metadata, directory: bool) -> Result<()> {
        owned(metadata, directory, self.owner, false)?;
        if metadata.dev() != self.device {
            return Err(invalid("boot asset crosses into another filesystem"));
        }
        Ok(())
    }

    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        read_file(self.open_file(path)?, self.owner, limit)
    }

    fn verify_blob(&self, path: &str, expected: &Blob) -> Result<()> {
        let mut file = self.open_file(path)?;
        let before = file.metadata()?;
        if before.len() != expected.bytes {
            return Err(invalid(format!("boot asset length mismatch: {path}")));
        }
        let mut hash = Hasher::new();
        let mut count = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let bytes = file.read(&mut buffer)?;
            if bytes == 0 {
                break;
            }
            count += bytes as u64;
            if count > expected.bytes {
                return Err(invalid("boot asset grew during inspection"));
            }
            hash.update(&buffer[..bytes]);
        }
        let digest: Sha256 = format!("{:x}", hash.finalize())
            .try_into()
            .expect("SHA-256 hexadecimal");
        if count != expected.bytes
            || digest != expected.sha256
            || !unchanged(&before, &file.metadata()?)
        {
            return Err(invalid(format!(
                "boot asset hash mismatch or changed file: {path}"
            )));
        }
        Ok(())
    }

    /// Only the journal-aware controller can request these fixed selectors.
    /// Files are flushed before rename, then the directory and filesystem are
    /// flushed and the result is read back. An error poisons this handle.
    pub(crate) fn select(
        &mut self,
        roots: &SlotRoots,
        slot: Slot,
        trial: Option<Sha256>,
        authorize: impl FnOnce(&Self) -> Result<()>,
    ) -> Result<()> {
        self.verify()?;
        self.selections(roots)?;
        // Recheck the durable journal and boot deadline after hashing USB
        // contents, immediately before creating any selector temporary.
        authorize(self)?;
        let destination = if trial.is_some() {
            "spacewars-trial.txt"
        } else {
            "cmdline.txt"
        };
        let value = self.template.render(roots, slot, trial);
        let directory = fd_path(&self.directory);
        // Any unsuccessful mutation may have reached rename or a failed flush.
        // Only reopening and reconciling live selectors can resolve that doubt.
        self.uncertain = true;
        let mut temporary = tempfile::Builder::new()
            .prefix(".spacewars-boot-")
            .suffix(".tmp")
            .rand_bytes(16)
            .tempfile_in(&directory)?;
        self.owned(&temporary.as_file().metadata()?, false)?;
        #[cfg(test)]
        self.inject(FaultPoint::Created)?;
        temporary.write_all(value.as_bytes())?;
        #[cfg(test)]
        self.inject(FaultPoint::Written)?;
        temporary.as_file().sync_all()?;
        #[cfg(test)]
        self.inject(FaultPoint::FileSynced)?;
        temporary
            .persist(directory.join(destination))
            .map_err(|error| error.error)?;
        #[cfg(test)]
        self.inject(FaultPoint::Renamed)?;
        self.directory.sync_all()?;
        #[cfg(test)]
        self.inject(FaultPoint::DirectorySynced)?;
        // SAFETY: directory is an open descriptor on the boot filesystem.
        io_result(unsafe { libc::syncfs(self.directory.as_raw_fd()) })?;
        #[cfg(test)]
        self.inject(FaultPoint::FilesystemSynced)?;
        if self.read(destination, 8192)? != value.as_bytes() {
            return Err(invalid(
                "selector readback disagrees with the requested root",
            ));
        }
        self.uncertain = false;
        Ok(())
    }

    fn certain(&self) -> Result<()> {
        if self.uncertain {
            Err(invalid(
                "selector durability is uncertain; reopen and reconcile",
            ))
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn inject(&self, point: FaultPoint) -> Result<()> {
        if self.fault == Some(point) {
            Err(std::io::Error::other("injected selector persistence failure").into())
        } else {
            Ok(())
        }
    }
}

fn fd_path(directory: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

fn temporary_name(name: &str) -> bool {
    name.strip_prefix(".spacewars-boot-")
        .and_then(|name| name.strip_suffix(".tmp"))
        .is_some_and(|random| {
            random.len() == 16 && random.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

fn check_bytes(manifest: &BootManifest, path: &str, bytes: &[u8]) -> Result<()> {
    if !manifest.files().get(path).is_some_and(|blob| {
        blob.bytes == bytes.len() as u64 && blob.sha256 == Sha256::digest(bytes)
    }) {
        return Err(invalid(format!("immutable boot asset mismatch: {path}")));
    }
    Ok(())
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::Boot(reason.into())
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultPoint {
    Created,
    Written,
    FileSynced,
    Renamed,
    DirectorySynced,
    FilesystemSynced,
}

#[cfg(test)]
#[path = "boot_files_tests.rs"]
pub(crate) mod tests;
