use crate::time::{TimeReason, TimeRecord};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const LIMIT: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: TimeRecord,
    checksum: String,
}

/// A root-owned, separate /data subtree in production. UI requests never
/// supply this path. Private same-user directories support isolated tests.
pub struct TimeStore {
    directory: PathBuf,
    _lock: File,
}

impl TimeStore {
    pub fn open(directory: &Path) -> Result<Self> {
        match fs::DirBuilder::new().mode(0o700).create(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() || !private(&metadata) {
            return Err(Error::Invalid(
                "time directory must be private and owned by the service".into(),
            ));
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join("lock"))?;
        if !private(&lock.metadata()?) || !lock.metadata()?.is_file() {
            return Err(Error::Invalid("unsafe time lock".into()));
        }
        lock.try_lock()
            .map_err(|error| Error::Invalid(format!("time store is already owned: {error}")))?;
        Ok(Self {
            directory: directory.into(),
            _lock: lock,
        })
    }

    /// Missing/corrupt time metadata yields unknown time. Unlike TUF trust
    /// state, time evidence can be re-established by a new provider receipt.
    /// Preserve the corrupt bytes until a successful observation is committed.
    pub fn load(&self) -> (TimeRecord, Option<TimeReason>) {
        let path = self.directory.join("state.json");
        let result = (|| -> Result<TimeRecord> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || !private(&metadata) || metadata.len() > LIMIT {
                return Err(Error::Invalid("unsafe time metadata".into()));
            }
            let mut bytes = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > LIMIT {
                return Err(Error::Invalid("time metadata exceeds limit".into()));
            }
            let envelope: Envelope = serde_json::from_slice(&bytes)?;
            if envelope.record.schema != 1 || checksum(&envelope.record)? != envelope.checksum {
                return Err(Error::Invalid(
                    "unsupported or damaged time metadata".into(),
                ));
            }
            if let Some(sync) = &envelope.record.last_sync
                && (envelope.record.success_generation == 0
                    || sync.receipt.packet_count == 0
                    || sync.receipt.boottime_us < sync.receipt.monotonic_us
                    || sync.boot_id.len() > 64
                    || sync.provider.invocation_id.len() != 32
                    || sync.provider.bus_owner.len() > 255)
            {
                return Err(Error::Invalid("invalid confirmed time record".into()));
            }
            if envelope.record.rtc.as_ref().is_some_and(|rtc| {
                rtc.sync_generation == 0 || rtc.sync_generation > envelope.record.success_generation
            }) {
                return Err(Error::Invalid("invalid RTC association".into()));
            }
            Ok(envelope.record)
        })();
        match result {
            Ok(record) => (record, None),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                (TimeRecord::default(), Some(TimeReason::MissingState))
            }
            Err(_) => (TimeRecord::default(), Some(TimeReason::CorruptState)),
        }
    }

    pub fn save(&self, record: &TimeRecord) -> Result<()> {
        let bytes = serde_json::to_vec(&Envelope {
            record: record.clone(),
            checksum: checksum(record)?,
        })?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::Invalid("time metadata exceeds limit".into()));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join("state.json"))
            .map_err(|error| error.error)?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

fn private(metadata: &fs::Metadata) -> bool {
    // SAFETY: geteuid has no preconditions or side effects.
    metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0
}

fn checksum(record: &TimeRecord) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(record)?)))
}
