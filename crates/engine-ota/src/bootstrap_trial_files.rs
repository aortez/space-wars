//! Verify the retained legacy tree plus the temporary boot tree. Only config.txt
//! can change here, between two configurations bound by the private journal.
use super::trial::TrialInputs;
use super::{STAGE_DIRECTORY, inspect, invalid};
use crate::owned_fs::{io_result, open_at, owned, read_file};
use crate::{Blob, Result, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub(super) struct TrialBootFiles {
    pub directory: File,
    pub inputs: TrialInputs,
    owner: u32,
    #[cfg(test)]
    pub fault: Option<&'static str>,
}

impl TrialBootFiles {
    pub fn open(directory: File, owner: u32, inputs: TrialInputs) -> Result<Self> {
        inputs.validate()?;
        owned(&directory.metadata()?, true, owner, false)?;
        let files = Self {
            directory,
            inputs,
            owner,
            #[cfg(test)]
            fault: None,
        };
        files.verify()?;
        Ok(files)
    }

    pub fn staged_default(&self) -> Result<bool> {
        let bytes = read_file(
            open_at(&self.directory, "config.txt", libc::O_RDONLY, 0)?,
            self.owner,
            16 * 1024,
        )?;
        if bytes == self.inputs.legacy_config.as_bytes() {
            return Ok(false);
        }
        if bytes == self.inputs.plan.normal_config.as_bytes() {
            return Ok(true);
        }
        Err(invalid(
            "bootstrap default is neither retained legacy nor checked staged config",
        ))
    }

    pub fn verify(&self) -> Result<()> {
        let expected = expected(&self.inputs, self.staged_default()?);
        let mut actual = BTreeMap::new();
        inspect::inventory(
            &self.directory,
            "",
            self.directory.metadata()?.dev(),
            self.owner,
            &mut actual,
            &mut 0,
        )?;
        let temporaries: Vec<_> = actual
            .keys()
            .filter(|path| temporary(path))
            .cloned()
            .collect();
        if temporaries.len() > 16 {
            return Err(invalid("too many interrupted bootstrap selector files"));
        }
        for path in temporaries {
            if actual.remove(&path).unwrap().bytes > 16 * 1024 {
                return Err(invalid("oversized bootstrap selector temporary"));
            }
        }
        if actual != expected {
            return Err(invalid("retained or staged bootstrap boot assets changed"));
        }
        Ok(())
    }

    pub fn select(
        &self,
        staged: bool,
        mut authorize: impl FnMut(&Self) -> Result<()>,
    ) -> Result<()> {
        self.verify()?;
        authorize(self)?;
        let value = if staged {
            &self.inputs.plan.normal_config
        } else {
            &self.inputs.legacy_config
        };
        let directory = PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()));
        let mut temporary = tempfile::Builder::new()
            .prefix(".spacewars-bootstrap-")
            .suffix(".tmp")
            .rand_bytes(16)
            .tempfile_in(&directory)?;
        owned(&temporary.as_file().metadata()?, false, self.owner, false)?;
        self.inject("created")?;
        temporary.write_all(value.as_bytes())?;
        self.inject("written")?;
        temporary.as_file().sync_all()?;
        self.inject("file-sync")?;
        // Recheck the durable authority and the boot-relative health deadline
        // after hashing/preparing, immediately before the selector replacement.
        authorize(self)?;
        temporary
            .persist(directory.join("config.txt"))
            .map_err(|error| error.error)?;
        self.inject("rename")?;
        self.directory.sync_all()?;
        self.inject("directory-sync")?;
        io_result(unsafe { libc::syncfs(self.directory.as_raw_fd()) })?;
        self.inject("filesystem-sync")?;
        if self.staged_default()? != staged {
            return Err(invalid("bootstrap selector readback differs"));
        }
        Ok(())
    }

    fn inject(&self, point: &str) -> Result<()> {
        #[cfg(test)]
        if self.fault == Some(point) {
            return Err(std::io::Error::other("bootstrap selector interruption fixture").into());
        }
        let _ = point;
        Ok(())
    }
}

pub(super) fn expected(inputs: &TrialInputs, staged: bool) -> BTreeMap<String, Blob> {
    let mut expected = inputs.binding.original_boot_files.clone();
    if staged {
        expected.insert(
            "config.txt".into(),
            blob(inputs.plan.normal_config.as_bytes()),
        );
    }
    expected.insert(
        "tryboot.txt".into(),
        blob(inputs.plan.trial_config.as_bytes()),
    );
    for (path, contents) in &inputs.plan.staged_assets {
        expected.insert(format!("{STAGE_DIRECTORY}/{path}"), contents.clone());
    }
    expected.insert(
        format!("{STAGE_DIRECTORY}/trial-cmdline.txt"),
        blob(inputs.plan.trial_cmdline.as_bytes()),
    );
    expected.insert(
        format!("{STAGE_DIRECTORY}/normal-cmdline.txt"),
        blob(inputs.plan.normal_cmdline.as_bytes()),
    );
    expected
}

fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}
pub(super) fn temporary(path: &str) -> bool {
    path.strip_prefix(".spacewars-bootstrap-")
        .and_then(|name| name.strip_suffix(".tmp"))
        .is_some_and(|random| {
            random.len() == 16 && random.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
#[path = "bootstrap_trial_files_tests.rs"]
mod tests;
