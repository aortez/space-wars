use crate::recovery::{Action, Plan, RecoveryError, RecoveryRecord};
use crate::store::{check_private, lock, private_open};
use crate::{Error, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(crate) const MAX_BYTES: u64 = 128 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: RecoveryRecord,
    checksum: Sha256,
}

pub(crate) fn initial_bytes(record: &RecoveryRecord) -> Result<Vec<u8>> {
    record.validate()?;
    let bytes = serde_json::to_vec(&Envelope {
        record: record.clone(),
        checksum: record.fingerprint(),
    })?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Store("recovery record exceeds size limit".into()));
    }
    Ok(bytes)
}

/// Fixed root-owned /data directory in production, opened by the service only.
/// The exclusive journal lock is distinct from the cross-tool installation lock
/// needed to exclude both the OTA writer and the supervised legacy helpers.
pub struct RecoveryStore {
    directory: PathBuf,
    record: RecoveryRecord,
    uncertain: bool,
    _lock: File,
}

impl RecoveryStore {
    /// Explicit supervised bootstrap; an existing directory is never replaced.
    pub fn initialize(directory: &Path, record: RecoveryRecord) -> Result<Self> {
        record.validate()?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        check_private(directory, true)?;
        let store = Self {
            directory: directory.into(),
            _lock: lock(directory)?,
            record,
            uncertain: false,
        };
        store.persist(&store.record)?;
        Ok(store)
    }

    pub fn open(directory: &Path) -> Result<Self> {
        check_private(directory, true)?;
        let lock = lock(directory)?;
        let path = directory.join("state.json");
        check_private(&path, false)?;
        let mut bytes = Vec::new();
        private_open(&path, false)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Store("recovery record exceeds size limit".into()));
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if envelope.record.fingerprint() != envelope.checksum {
            return Err(Error::Store("recovery record checksum mismatch".into()));
        }
        envelope.record.validate()?;
        Ok(Self {
            directory: directory.into(),
            record: envelope.record,
            _lock: lock,
            uncertain: false,
        })
    }

    pub fn record(&self) -> &RecoveryRecord {
        &self.record
    }

    #[cfg(any(target_os = "linux", test))]
    /// Use for cross-journal reconciliation. Unlike diagnostic `record()`, this
    /// rejects a handle whose last commit may have reached disk despite failure.
    pub fn durable_record(&self) -> Result<&RecoveryRecord> {
        if self.uncertain {
            return Err(Error::Store(
                "recovery persistence is uncertain; reopen required".into(),
            ));
        }
        Ok(&self.record)
    }

    /// A plan is opaque outside this crate. No action escapes before its record
    /// is durably stored. A failed flush poisons this handle because a rename
    /// may already have occurred; reopen and reconcile actual boot state.
    pub fn advance(
        &mut self,
        transition: impl FnOnce(&RecoveryRecord) -> std::result::Result<Plan, RecoveryError>,
    ) -> Result<Action> {
        if self.uncertain {
            return Err(Error::Store(
                "recovery persistence is uncertain; reopen required".into(),
            ));
        }
        let plan = transition(&self.record)?;
        if plan.previous != self.record.fingerprint() {
            return Err(RecoveryError::StaleCommand.into());
        }
        plan.record.validate()?;
        if plan.record != self.record {
            if let Err(error) = self.persist(&plan.record) {
                self.uncertain = true;
                return Err(error);
            }
            self.record = plan.record;
        }
        Ok(plan.action)
    }

    fn persist(&self, record: &RecoveryRecord) -> Result<()> {
        let bytes = initial_bytes(record)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(self.directory.join("state.json"))
            .map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
