use crate::{Error, FeedLocation, Result, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedFeed {
    pub metadata: BTreeMap<String, String>,
    pub trusted_fingerprint: Sha256,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImageRecord {
    pub payload: Sha256,
    pub descriptor: Sha256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    pub schema: u32,
    pub location: FeedLocation,
    pub trusted: BTreeMap<String, String>,
    pub feed: Option<RetainedFeed>,
    pub images: BTreeMap<u64, ImageRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredState {
    state: State,
    checksum: Sha256,
}

/// Exclusively owned verifier state. The device service opens this as root in
/// its fixed persistent directory; tests and publisher tools use private stores.
/// It must never be opened at a path supplied through the kiosk socket.
pub struct TrustStore {
    pub(crate) path: PathBuf,
    pub(crate) state: State,
    _lock: File,
}

impl TrustStore {
    /// Explicit bootstrap only. Never silently replace missing/corrupt state
    /// with the root bundled in a newer or rolled-back application image.
    pub fn initialize(path: &Path, root: &[u8], location: FeedLocation) -> Result<Self> {
        location.validate()?;
        if root.len() > crate::MAX_METADATA_BYTES {
            return Err(Error::Store("bootstrap root exceeds size limit".into()));
        }
        let signed: tough::schema::Signed<tough::schema::Root> = serde_json::from_slice(root)?;
        signed
            .signed
            .verify_role(&signed)
            .map_err(|error| Error::Store(error.to_string()))?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        // Reinitialization must be a separate, supervised operation, not a way
        // to erase replay, revocation, or image-generation history.
        builder.create(path)?;
        check_private(path, true)?;
        let lock = lock(path)?;
        let state = State {
            schema: 1,
            location,
            trusted: BTreeMap::from([(
                "root.json".into(),
                String::from_utf8(root.to_vec())
                    .map_err(|_| Error::Store("root is not UTF-8 JSON".into()))?,
            )]),
            feed: None,
            images: BTreeMap::new(),
        };
        let store = Self {
            path: path.into(),
            state,
            _lock: lock,
        };
        store.persist()?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<Self> {
        check_private(path, true)?;
        let lock = lock(path)?;
        let state_path = path.join("state.json");
        check_private(&state_path, false)?;
        let mut bytes = Vec::new();
        private_open(&state_path, false)?
            .take(MAX_STATE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            return Err(Error::Store("trust state exceeds size limit".into()));
        }
        let stored: StoredState = serde_json::from_slice(&bytes)?;
        if Sha256::digest(&serde_json::to_vec(&stored.state)?) != stored.checksum {
            return Err(Error::Store("trust state checksum mismatch".into()));
        }
        let state = stored.state;
        if state.schema != 1 || !state.trusted.contains_key("root.json") {
            return Err(Error::Store("unsupported or missing trust state".into()));
        }
        state.location.validate()?;
        validate_trusted(&state.trusted)?;
        if state
            .feed
            .as_ref()
            .is_some_and(|feed| feed.trusted_fingerprint != trusted_fingerprint(&state.trusted))
        {
            return Err(Error::Store(
                "retained feed does not match trust state".into(),
            ));
        }
        Ok(Self {
            path: path.into(),
            state,
            _lock: lock,
        })
    }

    pub(crate) fn persist(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&StoredState {
            state: self.state.clone(),
            checksum: Sha256::digest(&serde_json::to_vec(&self.state)?),
        })?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            return Err(Error::Store("trust state exceeds size limit".into()));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.path)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.path.join("state.json"))
            .map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(&self.path)?.sync_all()?;
        Ok(())
    }

    pub(crate) fn working_datastore(&self) -> Result<tempfile::TempDir> {
        validate_trusted(&self.state.trusted)?;
        let directory = tempfile::tempdir_in(&self.path)?;
        for (name, data) in &self.state.trusted {
            fs::write(directory.path().join(name), data)?;
        }
        Ok(directory)
    }

    pub(crate) fn retain_datastore(&mut self, directory: &Path) -> Result<()> {
        // TUF's rollback floor lives in root/timestamp/snapshot; the latter
        // includes all delegated role versions. Keep no growing set of old
        // versioned delegation files in persistent storage.
        let mut trusted = BTreeMap::new();
        for name in [
            "root.json",
            "timestamp.json",
            "snapshot.json",
            "targets.json",
            "latest_known_time.json",
        ] {
            let path = directory.join(name);
            match fs::read_to_string(&path) {
                Ok(data) => {
                    trusted.insert(name.to_owned(), data);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        validate_trusted(&trusted)?;
        self.state.trusted = trusted;
        if self.state.feed.as_ref().is_some_and(|feed| {
            feed.trusted_fingerprint != trusted_fingerprint(&self.state.trusted)
        }) {
            self.state.feed = None;
        }
        Ok(())
    }
}

pub(crate) fn trusted_fingerprint(trusted: &BTreeMap<String, String>) -> Sha256 {
    let metadata: BTreeMap<_, _> = trusted
        .iter()
        .filter(|(name, _)| name.as_str() != "latest_known_time.json")
        .map(|(name, data)| {
            (
                name,
                serde_json::from_str::<serde_json::Value>(data).expect("validated metadata"),
            )
        })
        .collect();
    let mut bytes = Vec::new();
    metadata
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut bytes,
            olpc_cjson::CanonicalFormatter::new(),
        ))
        .expect("metadata map serializes");
    Sha256::digest(&bytes)
}

fn validate_trusted(trusted: &BTreeMap<String, String>) -> Result<()> {
    for (name, data) in trusted {
        if ![
            "root.json",
            "timestamp.json",
            "snapshot.json",
            "targets.json",
            "latest_known_time.json",
        ]
        .contains(&name.as_str())
            || data.len() > crate::MAX_METADATA_BYTES
        {
            return Err(Error::Store("invalid retained metadata entry".into()));
        }
        // tough otherwise treats some corrupt prior metadata as absent. A
        // corrupt private trust record requires repair, never a reset floor.
        match name.as_str() {
            "root.json" => {
                let _: tough::schema::Signed<tough::schema::Root> = serde_json::from_str(data)?;
            }
            "timestamp.json" => {
                let _: tough::schema::Signed<tough::schema::Timestamp> =
                    serde_json::from_str(data)?;
            }
            "snapshot.json" => {
                let _: tough::schema::Signed<tough::schema::Snapshot> = serde_json::from_str(data)?;
            }
            "targets.json" => {
                let _: tough::schema::Signed<tough::schema::Targets> = serde_json::from_str(data)?;
            }
            "latest_known_time.json" => {
                let _: jiff::Timestamp = serde_json::from_str(data)?;
            }
            _ => unreachable!(),
        }
    }
    if !trusted.contains_key("root.json") {
        return Err(Error::Store("missing trusted root".into()));
    }
    Ok(())
}

fn lock(path: &Path) -> Result<File> {
    let file = private_open(&path.join("lock"), true)?;
    check_private(&path.join("lock"), false)?;
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => Error::Busy,
        std::fs::TryLockError::Error(error) => Error::Io(error),
    })?;
    Ok(file)
}

fn private_open(path: &Path, create: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn check_private(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(Error::Store(
            "expected private regular file or directory".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments, preconditions, or side effects.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(Error::Store(
                "trust store has unsafe ownership or permissions".into(),
            ));
        }
    }
    Ok(())
}
