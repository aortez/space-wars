//! One persistent, exclusively held image slot. A partial is never published
//! under the installer's content-addressed filename. Only newly checked TUF
//! evidence and complete compressed/expanded digests can publish that filename.
//!
//! These blocking filesystem operations belong on the updater's IO worker,
//! never the render thread. Cancellation/deadline guards run between bounded IO
//! chunks, including while hashing/decompressing an already downloaded file.
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file, unchanged};
use crate::recovery_linux::{DeviceNumber, DiskLayout};
use crate::{
    Blob, Channel, DeviceIdentity, Error, ImageDescriptor, ImageHistory, Result, Selection, Sha256,
    TrustStore, UsableTime, VerifiedCandidate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub const STAGING_RESERVE_BYTES: u64 = 256 * 1024 * 1024;
pub const TRANSFER_CHUNK_BYTES: usize = 64 * 1024;
const RECORD: &str = "candidate.json";
const NEXT: &str = "candidate.next";
const PARTIAL: &str = "payload.partial";
const LOCK: &str = "lock";
const RECORD_LIMIT: u64 = 8192;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    image: ImageDescriptor,
    // Private HTTP validator, never exposed in status or diagnostics.
    etag: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: Record,
    checksum: Sha256,
}

/// This describes retained bytes, not installation permission or current TUF
/// validity. Even a complete file must be reverified after a process restart.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum RetainedStatus {
    Empty,
    Unverified {
        candidate_id: Sha256,
        channel: Channel,
        generation: u64,
        bytes: u64,
        total: u64,
    },
}

pub struct StagingStore {
    directory: File,
    _lock: File,
    owner: u32,
    record: Option<Record>,
    #[cfg(test)]
    available_limit: Option<u64>,
}

/// No deserializer and no arbitrary path. This handle retains the staging lock
/// until dropped, so an installer cannot read a partially modified candidate.
pub struct Transfer<'a> {
    store: &'a mut StagingStore,
    candidate: VerifiedCandidate,
    file: File,
    bytes: u64,
    accepting: bool,
    failed: bool,
}

/// HTTP fields are untrusted. Keep their contents out of diagnostic output.
pub struct ResponseHeaders<'a> {
    pub status: u16,
    pub length: Option<u64>,
    pub range: Option<&'a str>,
    pub etag: Option<&'a str>,
    pub encoding: Option<&'a str>,
}

impl StagingStore {
    /// The device uses only this fixed root-owned path on its validated /data
    /// partition. Missing setup is an error, not an invitation to create a
    /// staging tree on an unvalidated filesystem.
    pub fn open(layout: &DiskLayout) -> Result<Self> {
        Self::under(staging_directory(layout)?, 0)
    }

    pub(crate) fn under(directory: File, owner: u32) -> Result<Self> {
        owned(&directory.metadata()?, true, owner, true)?;
        let lock = staging_lock(&directory, owner)?;
        let record = match optional(&directory, RECORD, libc::O_RDONLY)? {
            Some(file) => {
                owned(&file.metadata()?, false, owner, true)?;
                let stored: Envelope =
                    serde_json::from_slice(&read_file(file, owner, RECORD_LIMIT)?)
                        .map_err(|_| Error::Staging("invalid candidate record"))?;
                if stored.record.schema != 1
                    || Sha256::digest(&serde_json::to_vec(&stored.record)?) != stored.checksum
                    || stored.record.image.validate().is_err()
                    || stored
                        .record
                        .etag
                        .as_deref()
                        .is_some_and(|tag| !strong_etag(tag))
                {
                    return Err(Error::Staging("invalid candidate record"));
                }
                Some(stored.record)
            }
            None => None,
        };
        let store = Self {
            directory,
            _lock: lock,
            owner,
            record,
            #[cfg(test)]
            available_limit: None,
        };
        // Never sweep an arbitrary directory or silently delete unrelated data.
        // Unknown files indicate a layout that needs repair.
        let ready = store
            .record
            .as_ref()
            .map(|record| ready_name(&record.image));
        for (index, entry) in
            fs::read_dir(format!("/proc/self/fd/{}", store.directory.as_raw_fd()))?.enumerate()
        {
            let entry = entry?;
            let component = entry.file_name();
            let component = component
                .to_str()
                .ok_or(Error::Staging("unexpected staging entry"))?;
            if index >= 5
                || ![LOCK, RECORD, NEXT, PARTIAL].contains(&component)
                    && ready.as_deref() != Some(component)
            {
                return Err(Error::Staging("unexpected staging entry"));
            }
            let file = open_at(&store.directory, component, libc::O_RDONLY, 0)?;
            store.check_file(&file)?;
        }
        store.remove(NEXT)?;
        if store.record.is_none() {
            store.remove(PARTIAL)?;
        }
        Ok(store)
    }

    pub fn status(&self) -> Result<RetainedStatus> {
        let Some(record) = &self.record else {
            return Ok(RetainedStatus::Empty);
        };
        let partial = optional(&self.directory, PARTIAL, libc::O_RDONLY)?;
        let ready = optional(&self.directory, &ready_name(&record.image), libc::O_RDONLY)?;
        if partial.is_some() && ready.is_some() {
            return Err(Error::Staging("conflicting staged files"));
        }
        let bytes = match partial.or(ready) {
            Some(file) => {
                self.check_file(&file)?;
                file.metadata()?.len()
            }
            None => 0,
        };
        if bytes > record.image.compressed.bytes {
            return Err(Error::Staging("staged file exceeds signed length"));
        }
        Ok(RetainedStatus::Unverified {
            candidate_id: record.image.candidate_id(),
            channel: record.image.channel,
            generation: record.image.generation,
            bytes,
            total: record.image.compressed.bytes,
        })
    }

    /// Invalidate a changed/absent channel selection. Only known update-owned
    /// names are removed; no ROM, settings or partition is ever touched.
    pub fn clear(&mut self) -> Result<()> {
        self.remove(PARTIAL)?;
        if let Some(record) = &self.record {
            self.remove(&ready_name(&record.image))?;
        }
        self.remove(NEXT)?;
        self.remove(RECORD)?;
        self.record = None;
        Ok(())
    }

    pub fn prepare(&mut self, candidate: &VerifiedCandidate) -> Result<Transfer<'_>> {
        candidate.image().validate()?;
        if self
            .record
            .as_ref()
            .is_none_or(|record| record.image != *candidate.image())
        {
            self.clear()?;
            self.save(Record {
                schema: 1,
                image: candidate.image().clone(),
                etag: None,
            })?;
        }
        self.status()?;
        let ready = ready_name(candidate.image());
        if let Some(file) = optional(&self.directory, &ready, libc::O_RDONLY)? {
            self.check_file(&file)?;
            self.rename(&ready, PARTIAL)?;
        }
        let mut file = open_at(
            &self.directory,
            PARTIAL,
            libc::O_RDWR | libc::O_CREAT,
            0o600,
        )?;
        self.check_file(&file)?;
        let mut bytes = file.metadata()?.len();
        if bytes > candidate.image().compressed.bytes {
            return Err(Error::Staging("staged file exceeds signed length"));
        }
        // Complete retained bytes can be hashed without an HTTP validator.
        // An interrupted representation without a strong validator starts over.
        if bytes < candidate.image().compressed.bytes
            && self.record.as_ref().unwrap().etag.is_none()
        {
            file.set_len(0)?;
            file.sync_all()?;
            bytes = 0;
        }
        self.check_space(candidate.image().compressed.bytes - bytes)?;
        file.seek(std::io::SeekFrom::Start(bytes))?;
        Ok(Transfer {
            store: self,
            candidate: candidate.clone(),
            file,
            bytes,
            accepting: false,
            failed: false,
        })
    }

    fn check_file(&self, file: &File) -> Result<()> {
        let metadata = file.metadata()?;
        owned(&metadata, false, self.owner, true)?;
        if metadata.mode() & 0o111 != 0 {
            return Err(Error::Staging("staged files must not be executable"));
        }
        if metadata.dev() != self.directory.metadata()?.dev() {
            return Err(Error::Staging("staged file is on another filesystem"));
        }
        Ok(())
    }

    fn check_space(&self, remaining: u64) -> Result<()> {
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: the live directory fd is valid; success initializes stat.
        io_result(unsafe { libc::fstatvfs(self.directory.as_raw_fd(), stat.as_mut_ptr()) })?;
        let stat = unsafe { stat.assume_init() };
        let block = stat.f_frsize;
        let available = stat
            .f_bavail
            .checked_mul(block)
            .ok_or(Error::StagingSpace)?;
        #[cfg(test)]
        let available = available.min(self.available_limit.unwrap_or(u64::MAX));
        let remaining = remaining
            .checked_add(block.checked_sub(1).ok_or(Error::StagingSpace)?)
            .and_then(|bytes| bytes.checked_div(block))
            .and_then(|blocks| blocks.checked_mul(block))
            .ok_or(Error::StagingSpace)?;
        if available
            < remaining
                .checked_add(STAGING_RESERVE_BYTES)
                .ok_or(Error::StagingSpace)?
        {
            return Err(Error::StagingSpace);
        }
        Ok(())
    }

    fn save(&mut self, record: Record) -> Result<()> {
        let checksum = Sha256::digest(&serde_json::to_vec(&record)?);
        let bytes = serde_json::to_vec(&Envelope {
            record: record.clone(),
            checksum,
        })?;
        if bytes.len() as u64 > RECORD_LIMIT {
            return Err(Error::Staging("candidate record too large"));
        }
        self.remove(NEXT)?;
        let mut file = open_at(
            &self.directory,
            NEXT,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        self.rename(NEXT, RECORD)?;
        self.record = Some(record);
        Ok(())
    }

    fn remove(&self, component: &str) -> Result<()> {
        let Some(file) = optional(&self.directory, component, libc::O_RDONLY)? else {
            return Ok(());
        };
        self.check_file(&file)?;
        let component = name(component)?;
        // SAFETY: the validated single component and directory fd remain live.
        io_result(unsafe { libc::unlinkat(self.directory.as_raw_fd(), component.as_ptr(), 0) })?;
        self.directory.sync_all()?;
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        let from = name(from)?;
        let to = name(to)?;
        // SAFETY: both fixed components are relative to the held directory.
        io_result(unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                from.as_ptr(),
                self.directory.as_raw_fd(),
                to.as_ptr(),
            )
        })?;
        self.directory.sync_all()?;
        Ok(())
    }
}

impl Transfer<'_> {
    pub fn offset(&self) -> u64 {
        self.bytes
    }
    pub fn total(&self) -> u64 {
        self.candidate.image().compressed.bytes
    }
    pub fn etag(&self) -> Option<&str> {
        self.store
            .record
            .as_ref()
            .and_then(|record| record.etag.as_deref())
    }

    /// A server that ignores Range may return a full 200 response. That response
    /// replaces the prefix; it is never appended to it. A 206 must account for
    /// the exact remaining suffix and match the retained strong ETag.
    pub fn accept(&mut self, headers: ResponseHeaders<'_>) -> Result<()> {
        self.accepting = false;
        self.failed = true;
        if headers.encoding.is_some_and(|value| value != "identity") {
            return Err(Error::DownloadResponse);
        }
        match headers.status {
            200 if headers.range.is_none() && headers.length == Some(self.total()) => {
                self.restart()?
            }
            206 if self.bytes > 0
                && self.bytes < self.total()
                && headers.length == Some(self.total() - self.bytes)
                && headers.range
                    == Some(
                        format!("bytes {}-{}/{}", self.bytes, self.total() - 1, self.total())
                            .as_str(),
                    )
                && self.etag().is_some()
                && headers.etag == self.etag() => {}
            _ => {
                return Err(Error::DownloadResponse);
            }
        }
        self.failed = true;
        let mut record = self.store.record.clone().unwrap();
        record.etag = headers
            .etag
            .filter(|tag| strong_etag(tag))
            .map(str::to_owned);
        self.store.save(record)?;
        self.store.check_space(self.total() - self.bytes)?;
        self.accepting = true;
        self.failed = false;
        Ok(())
    }

    pub fn restart(&mut self) -> Result<()> {
        self.accepting = false;
        self.failed = true;
        self.file.set_len(0)?;
        self.file.rewind()?;
        self.file.sync_all()?;
        self.bytes = 0;
        let mut record = self.store.record.clone().unwrap();
        record.etag = None;
        self.store.save(record)?;
        self.failed = false;
        Ok(())
    }

    pub fn append(&mut self, bytes: &[u8]) -> Result<()> {
        if self.failed
            || !self.accepting
            || bytes.is_empty()
            || bytes.len() > TRANSFER_CHUNK_BYTES
            || bytes.len() as u64 > self.total() - self.bytes
        {
            self.failed = true;
            return Err(Error::Staging("invalid download chunk or excess payload"));
        }
        self.failed = true;
        self.store.check_space(self.total() - self.bytes)?;
        self.file.write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        self.failed = false;
        Ok(())
    }

    /// Persist a useful prefix on normal cancellation/interruption. A hard
    /// process death can lose recent bytes; reopening uses actual file length
    /// and the final whole-file digest still detects any damaged prefix.
    pub fn checkpoint(&self) -> Result<()> {
        self.file.sync_all().map_err(Error::from)
    }

    /// Rehash compressed bytes, decode just one bounded zstd frame and check
    /// expanded length/hash without storing a second image. The callback supplies
    /// the service's current usable time and cancellation/channel/deadline guard.
    /// It is called again after hashing and before the atomic publication.
    pub async fn finish(
        mut self,
        trust: &mut TrustStore,
        device: &DeviceIdentity,
        history: &ImageHistory,
        mut guard: impl FnMut() -> Result<Option<UsableTime>>,
    ) -> Result<VerifiedCandidate> {
        if self.failed {
            return Err(Error::Staging("download must be restarted after an error"));
        }
        let initial_time = guard()?.ok_or(Error::WaitingForTime)?;
        trust.admit_time(initial_time)?;
        if self.bytes != self.total()
            || self.candidate.image().expanded.bytes > device.inactive_slot_bytes
        {
            return Err(Error::ImageMismatch);
        }
        self.file.sync_all()?;
        let before = self.file.metadata()?;
        let verification = self.verify(&mut guard);
        if let Err(error) = verification {
            if matches!(error, Error::ImageMismatch) {
                self.restart()?;
            }
            return Err(error);
        }
        self.store.check_file(&self.file)?;
        let named = open_at(&self.store.directory, PARTIAL, libc::O_RDONLY, 0)?;
        if !unchanged(&before, &self.file.metadata()?) || !unchanged(&before, &named.metadata()?) {
            return Err(Error::Staging("payload changed during verification"));
        }
        let time = guard()?.ok_or(Error::WaitingForTime)?;
        let verified = match trust
            .cached(Some(time), self.candidate.image().channel, device, history)
            .await?
        {
            Selection::Available { candidate } if candidate.id() == self.candidate.id() => {
                candidate
            }
            _ => return Err(Error::CandidateChanged),
        };
        let current = guard()?.ok_or(Error::WaitingForTime)?;
        if current.0 < time.0 {
            return Err(Error::TimeMovedBackwards);
        }
        // A late observation of expiry remains a durable clock floor even
        // when this transfer cannot publish a candidate.
        trust.admit_time(current)?;
        if current.0 >= verified.expires() {
            return Err(Error::Staging("metadata expired before publication"));
        }
        self.store.check_space(0)?;
        let named = open_at(&self.store.directory, PARTIAL, libc::O_RDONLY, 0)?;
        if !unchanged(&before, &self.file.metadata()?) || !unchanged(&before, &named.metadata()?) {
            return Err(Error::Staging("payload changed before publication"));
        }
        self.store.rename(PARTIAL, &ready_name(verified.image()))?;
        Ok(verified)
    }

    fn verify(&mut self, guard: &mut impl FnMut() -> Result<Option<UsableTime>>) -> Result<()> {
        self.file.rewind()?;
        verify_guarded(&mut self.file, &self.candidate.image().compressed, guard)?;
        self.file.rewind()?;
        let guard = RefCell::new(guard);
        // Also guard compressed reads: a decoder may consume many input blocks
        // before producing the next output block, especially for malformed data.
        let input = GuardedRead {
            reader: &mut self.file,
            check: || {
                guard.borrow_mut()().and_then(|time| time.ok_or(Error::WaitingForTime).map(|_| ()))
            },
        };
        let mut input = BufReader::with_capacity(TRANSFER_CHUNK_BYTES, input);
        if !input
            .fill_buf()
            .map_err(verification_error)?
            .starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
        {
            return Err(Error::ImageMismatch);
        }
        let mut decoder = zstd::stream::read::Decoder::with_buffer(input)
            .map_err(|_| Error::ImageMismatch)?
            .single_frame();
        decoder
            .window_log_max(26)
            .map_err(|_| Error::ImageMismatch)?;
        verify_guarded(&mut decoder, &self.candidate.image().expanded, &mut || {
            guard.borrow_mut()()
        })?;
        if decoder
            .finish()
            .read(&mut [0u8; 1])
            .map_err(verification_error)?
            != 0
        {
            return Err(Error::ImageMismatch);
        }
        Ok(())
    }
}

fn verify_guarded(
    reader: &mut impl Read,
    blob: &Blob,
    guard: &mut impl FnMut() -> Result<Option<UsableTime>>,
) -> Result<()> {
    let mut remaining = blob.bytes;
    let mut hash = Hasher::new();
    let mut buffer = [0u8; TRANSFER_CHUNK_BYTES];
    while remaining > 0 {
        guard()?.ok_or(Error::WaitingForTime)?;
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = reader
            .read(&mut buffer[..limit])
            .map_err(verification_error)?;
        if count == 0 {
            return Err(Error::ImageMismatch);
        }
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    guard()?.ok_or(Error::WaitingForTime)?;
    if reader.read(&mut buffer[..1]).map_err(verification_error)? != 0
        || hash.finalize().as_slice() != blob.sha256.as_bytes()
    {
        return Err(Error::ImageMismatch);
    }
    Ok(())
}

struct GuardedRead<R, F> {
    reader: R,
    check: F,
}

impl<R: Read, F: FnMut() -> Result<()>> Read for GuardedRead<R, F> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        (self.check)().map_err(std::io::Error::other)?;
        let limit = buffer.len().min(TRANSFER_CHUNK_BYTES);
        self.reader.read(&mut buffer[..limit])
    }
}

fn verification_error(error: std::io::Error) -> Error {
    // Preserve a deadline/cancellation from the guarded compressed reader; a
    // decoder or underlying read failure is never successful verification.
    error
        .into_inner()
        .and_then(|error| error.downcast::<Error>().ok())
        .map_or(Error::ImageMismatch, |error| *error)
}

fn strong_etag(tag: &str) -> bool {
    (2..=256).contains(&tag.len())
        && tag.starts_with('"')
        && tag.ends_with('"')
        && tag.as_bytes()[1..tag.len() - 1]
            .iter()
            .all(|byte| *byte == 0x21 || (0x23..=0x7e).contains(byte))
}

fn ready_name(image: &ImageDescriptor) -> String {
    format!("{}.ext4.zst", image.compressed.sha256)
}

fn optional(directory: &File, component: &str, flags: i32) -> Result<Option<File>> {
    match open_at(directory, component, flags, 0) {
        Ok(file) => Ok(Some(file)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn staging_lock(directory: &File, owner: u32) -> Result<File> {
    owned(&directory.metadata()?, true, owner, true)?;
    let file = open_at(directory, LOCK, libc::O_RDWR | libc::O_CREAT, 0o600)?;
    owned(&file.metadata()?, false, owner, true)?;
    if file.metadata()?.dev() != directory.metadata()?.dev() {
        return Err(Error::Staging("lock is on another filesystem"));
    }
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => Error::Busy,
        fs::TryLockError::Error(error) => Error::Io(error),
    })?;
    Ok(file)
}

pub(crate) fn staging_directory(layout: &DiskLayout) -> Result<File> {
    let root = open_directory(Path::new("/"), 0)?;
    let data = open_at(&root, "data", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&data.metadata()?, true, 0, false)?;
    if DeviceNumber::from_dev(data.metadata()?.dev()) != layout.partitions[3].device {
        return Err(Error::Staging(
            "staging is not on the validated data partition",
        ));
    }
    let system = open_at(
        &data,
        "spacewars-system",
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    owned(&system.metadata()?, true, 0, true)?;
    let staging = open_at(&system, "staging", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&staging.metadata()?, true, 0, true)?;
    if staging.metadata()?.dev() != data.metadata()?.dev() {
        return Err(Error::Staging(
            "staging filesystem differs from persistent data",
        ));
    }
    Ok(staging)
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
