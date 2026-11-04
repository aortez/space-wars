//! Bounded volatile progress shared with the independent status service.
//! No recovery decision trusts this file; the private journal is authoritative.
use crate::{Error, Result};
use engine_ota::coordinator::{RecoveryProgress, RecoveryStage};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const DIRECTORY: &str = "/run/spacewars-updater";
const MAX_BYTES: u64 = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryReport {
    pub schema: u32,
    pub boot_id: String,
    pub uptime_ms: u64,
    pub progress: RecoveryProgress,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryStatus {
    pub report: Option<RecoveryReport>,
    pub fresh: bool,
    pub error: Option<String>,
}

pub fn read(boot_id: &str, uptime_ms: u64) -> RecoveryStatus {
    read_at(Path::new(DIRECTORY), 0, boot_id, uptime_ms)
}

fn read_at(directory: &Path, owner: u32, boot_id: &str, uptime_ms: u64) -> RecoveryStatus {
    let result = (|| -> Result<RecoveryReport> {
        check(&fs::symlink_metadata(directory)?, owner, true)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join("status.json"))?;
        check(&file.metadata()?, owner, false)?;
        if file.metadata()?.len() > MAX_BYTES {
            return Err(invalid("oversized recovery status"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("oversized recovery status"));
        }
        let report: RecoveryReport = serde_json::from_slice(&bytes)?;
        if report.schema != 1 || report.boot_id != boot_id || report.uptime_ms > uptime_ms {
            return Err(invalid("recovery status does not describe this boot"));
        }
        Ok(report)
    })();
    match result {
        Ok(report) => {
            let terminal = report.progress.stage.finished()
                && report.progress.stage != RecoveryStage::RebootRequested;
            RecoveryStatus {
                fresh: terminal || uptime_ms - report.uptime_ms < 15_000,
                report: Some(report),
                error: None,
            }
        }
        Err(error) => RecoveryStatus {
            report: None,
            fresh: false,
            error: Some(error.to_string().chars().take(256).collect()),
        },
    }
}

pub(crate) struct Reporter {
    directory: PathBuf,
    owner: u32,
    boot_id: String,
}

impl Reporter {
    pub(crate) fn open(boot_id: String) -> Result<Self> {
        Self::at(Path::new(DIRECTORY), 0, boot_id)
    }
    fn at(directory: &Path, owner: u32, boot_id: String) -> Result<Self> {
        check(&fs::symlink_metadata(directory)?, owner, true)?;
        Ok(Self {
            directory: directory.into(),
            owner,
            boot_id,
        })
    }
    pub(crate) fn publish(&self, progress: RecoveryProgress) -> Result<()> {
        self.write(progress, crate::linux::clock_reading()?.boottime_us / 1000)
    }
    fn write(&self, progress: RecoveryProgress, uptime_ms: u64) -> Result<()> {
        check(&fs::symlink_metadata(&self.directory)?, self.owner, true)?;
        let report = RecoveryReport {
            schema: 1,
            boot_id: self.boot_id.clone(),
            uptime_ms,
            progress,
        };
        let bytes = serde_json::to_vec(&report)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("oversized recovery status"));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join("status.json"))
            .map_err(|e| e.error)?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

fn check(metadata: &fs::Metadata, owner: u32, directory: bool) -> Result<()> {
    if metadata.uid() != owner
        || metadata.mode() & 0o022 != 0
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file() || metadata.nlink() != 1
        }
    {
        return Err(invalid(
            "unsafe recovery status ownership, type or permissions",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn status_is_boot_bound_and_stalled_worker_does_not_claim_fresh_progress() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let writer = Reporter::at(directory.path(), owner, "boot-a".into()).unwrap();
        writer
            .write(RecoveryProgress::new(RecoveryStage::Observing), 10_000)
            .unwrap();
        assert!(read_at(directory.path(), owner, "boot-a", 12_000).fresh);
        assert!(!read_at(directory.path(), owner, "boot-a", 25_000).fresh);
        assert!(
            read_at(directory.path(), owner, "boot-b", 12_000)
                .report
                .is_none()
        );
        assert!(
            read_at(directory.path(), owner, "boot-a", 9_000)
                .report
                .is_none()
        );
        writer
            .write(RecoveryProgress::new(RecoveryStage::Healthy), 80_000)
            .unwrap();
        assert!(read_at(directory.path(), owner, "boot-a", 86_400_000).fresh);
        writer
            .write(
                RecoveryProgress::new(RecoveryStage::RebootRequested),
                80_000,
            )
            .unwrap();
        assert!(!read_at(directory.path(), owner, "boot-a", 100_000).fresh);
    }

    #[test]
    fn bad_report_is_isolated_from_time_status_and_has_bounded_errors() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let path = directory.path().join("status.json");
        for bytes in [b"broken".to_vec(), vec![b' '; MAX_BYTES as usize + 1]] {
            fs::write(&path, bytes).unwrap();
            let status = read_at(directory.path(), owner, "boot-a", 10_000);
            assert!(status.report.is_none());
            assert!(status.error.unwrap().len() <= 256);
        }
        fs::remove_file(&path).unwrap();
        symlink("/dev/zero", &path).unwrap();
        assert!(
            read_at(directory.path(), owner, "boot-a", 10_000)
                .report
                .is_none()
        );
        fs::remove_file(&path).unwrap();
        let writer = Reporter::at(directory.path(), owner, "boot-a".into()).unwrap();
        writer
            .write(RecoveryProgress::new(RecoveryStage::Starting), 1)
            .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(
            read_at(directory.path(), owner, "boot-a", 10_000)
                .report
                .is_none()
        );
    }
}
