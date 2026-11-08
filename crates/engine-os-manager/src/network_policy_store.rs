use super::{Record, invalid};
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(super) struct SavePause {
    pub point: &'static str,
    pub entered: std::sync::mpsc::SyncSender<()>,
    pub resume: std::sync::mpsc::Receiver<()>,
}

const LIMIT: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: Record,
    checksum: String,
}

pub(super) struct Store {
    pub(super) access: Option<crate::network_access::Access>,
    path: PathBuf,
    directory: File,
    lock: File,
    bytes: Option<Vec<u8>>,
    generation: Option<u64>,
    uncertain: bool,
    #[cfg(test)]
    pub(super) fail_at: Option<&'static str>,
    #[cfg(test)]
    pub(super) pause: std::sync::Mutex<Option<SavePause>>,
}

impl Store {
    pub fn open(path: &Path) -> Result<(Self, Record)> {
        Ok(Self::open_inner(path, true)?.expect("initializing open returns a store"))
    }

    /// Inspect under the same exclusive owner lock without creating state or
    /// advancing its generation. Only an absent directory means uninitialized.
    pub fn existing(path: &Path) -> Result<Option<(Self, Record)>> {
        Self::open_inner(path, false)
    }

    fn open_inner(path: &Path, initialize: bool) -> Result<Option<(Self, Record)>> {
        let created = if initialize {
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(error) => return Err(error.into()),
            }
        } else {
            false
        };
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(path);
        let directory = match directory {
            Ok(directory) => directory,
            Err(error) if !initialize && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let metadata = directory.metadata()?;
        if !metadata.is_dir() || !private(&metadata) {
            return Err(invalid("unsafe network policy directory"));
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        if created {
            options.create_new(true);
        }
        let lock = options.open(path.join("lock"))?;
        check_file(&lock.metadata()?)?;
        if lock.metadata()?.len() != 0 {
            return Err(invalid("nonempty network policy lock"));
        }
        lock.try_lock()
            .map_err(|_| invalid("network policy already has an owner"))?;
        let mut store = Self {
            access: None,
            path: path.into(),
            directory,
            lock,
            bytes: None,
            generation: None,
            uncertain: false,
            #[cfg(test)]
            fail_at: None,
            #[cfg(test)]
            pause: std::sync::Mutex::new(None),
        };
        let record = if created {
            let record = Record::initial();
            store.save(&record)?;
            // Publish the initial directory entry, not only its state file.
            File::open(
                path.parent()
                    .ok_or_else(|| invalid("network policy has no parent"))?,
            )?
            .sync_all()?;
            record
        } else {
            // Missing/corrupt state in an existing directory is not a fresh
            // device: it may contain the only saved deployment protection.
            let bytes = read(path)?;
            let envelope: Envelope = serde_json::from_slice(&bytes)?;
            envelope.record.validate()?;
            if checksum(&envelope.record)? != envelope.checksum {
                return Err(invalid("damaged network policy record"));
            }
            store.generation = Some(envelope.record.generation);
            store.bytes = Some(bytes);
            envelope.record
        };
        store.check()?;
        Ok(Some((store, record)))
    }

    pub fn check(&self) -> Result<()> {
        let result = self.check_inner();
        if result.is_err()
            && let Some(access) = &self.access
        {
            access.fail();
        }
        result
    }

    fn check_inner(&self) -> Result<()> {
        if self.uncertain {
            return Err(invalid(
                "network policy durability is uncertain; reopen before acting",
            ));
        }
        self.check_storage()
    }

    fn check_storage(&self) -> Result<()> {
        let directory = fs::symlink_metadata(&self.path)?;
        if !directory.is_dir()
            || !private(&directory)
            || !same(&directory, &self.directory.metadata()?)
        {
            return Err(invalid("network policy directory changed"));
        }
        let lock = fs::symlink_metadata(self.path.join("lock"))?;
        check_file(&lock)?;
        if !same(&lock, &self.lock.metadata()?) || lock.len() != 0 {
            return Err(invalid("network policy lock changed"));
        }
        if let Some(expected) = &self.bytes {
            if read(&self.path)? != *expected {
                return Err(invalid("network policy changed outside its owner"));
            }
        } else {
            match fs::symlink_metadata(self.path.join("state.json")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => return Err(invalid("initial network policy state already exists")),
            }
        }
        Ok(())
    }

    pub fn save(&mut self, record: &Record) -> Result<()> {
        self.save_inner(record, false)
    }

    pub fn reserve_revisions(&mut self, record: &Record) -> Result<()> {
        self.save_inner(record, true)
    }

    fn save_inner(&mut self, record: &Record, reserve: bool) -> Result<()> {
        let result = self.write_record(record, reserve);
        if result.is_err()
            && let Some(access) = &self.access
        {
            access.fail();
        }
        result
    }

    fn write_record(&mut self, record: &Record, reserve: bool) -> Result<()> {
        self.check()?;
        record.validate()?;
        if self.generation.is_some_and(|old| {
            old.checked_add(if reserve { 1 << 32 } else { 1 }) != Some(record.generation)
        }) || (self.generation.is_none() && *record != Record::initial())
        {
            return Err(invalid(
                "network policy revision is not the next durable generation",
            ));
        }
        let bytes = serde_json::to_vec(&Envelope {
            record: record.clone(),
            checksum: checksum(record)?,
        })?;
        if bytes.len() as u64 > LIMIT {
            return Err(invalid("network policy record exceeds limit"));
        }
        self.uncertain = true;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.path)?;
        temporary.write_all(&bytes)?;
        self.point("written")?;
        temporary.as_file().sync_all()?;
        self.point("file-synced")?;
        self.check_storage()?;
        temporary
            .persist(self.path.join("state.json"))
            .map_err(|e| e.error)?;
        self.point("renamed")?;
        self.directory.sync_all()?;
        self.point("directory-synced")?;
        self.bytes = Some(bytes);
        self.generation = Some(record.generation);
        self.check_storage()?;
        self.uncertain = false;
        Ok(())
    }

    fn point(&self, _name: &str) -> Result<()> {
        #[cfg(test)]
        {
            let mut pause = self.pause.lock().unwrap();
            let pause = if pause.as_ref().is_some_and(|pause| pause.point == _name) {
                pause.take()
            } else {
                None
            };
            if let Some(pause) = pause {
                pause.entered.send(()).unwrap();
                pause
                    .resume
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            }
        }
        #[cfg(test)]
        if self.fail_at == Some(_name) {
            return Err(invalid("injected network policy flush failure"));
        }
        Ok(())
    }
}

fn read(directory: &Path) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join("state.json"))?;
    check_file(&file.metadata()?)?;
    if file.metadata()?.len() > LIMIT {
        return Err(invalid("network policy record exceeds limit"));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid("network policy record exceeds limit"));
    }
    Ok(bytes)
}

fn private(metadata: &fs::Metadata) -> bool {
    metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0
}

fn check_file(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_file() || !private(metadata) || metadata.nlink() != 1 {
        return Err(invalid("unsafe network policy file"));
    }
    Ok(())
}

fn same(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn checksum(record: &Record) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(record)?)))
}
