//! Verified, fixed-target inactive-root writer. It cannot select a boot slot or
//! reboot. The service must revalidate metadata/time/admission before beginning
//! the journal transaction; this worker rechecks opened bytes and actual devices.
use crate::boot_linux::BootController;
use crate::image_tools::ImageTools;
use crate::owned_fs::{open_at, open_directory, owned, unchanged};
use crate::recovery::{Action, FailureReason};
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use crate::{
    Blob, Error, ImageDescriptor, ImageIdentity, RecoveryStore, Result, VerifiedCandidate,
    verify_blob,
};
use serde::Serialize;
use sha2::{Digest, Sha256 as Hasher};
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::time::{Duration, Instant};

const COPY_BYTES: usize = 64 * 1024;
const HEADER_BYTES: usize = 4096;
const ZSTD_WINDOW_LOG: u32 = 26;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteStage {
    CheckingPayload,
    Writing,
    Flushing,
    ReadingBack,
    CheckingFilesystem,
    Prepared,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct WriteProgress {
    pub stage: WriteStage,
    pub bytes: u64,
    pub total: u64,
}

/// An already-open private payload. Only its content hash determines its fixed
/// staging name, so a caller cannot redirect installation with a pathname.
pub struct StagedPayload {
    directory: File,
    file: File,
    original: Metadata,
    expected: Blob,
    name: String,
    owner: u32,
}

impl StagedPayload {
    pub fn open(candidate: &VerifiedCandidate, layout: &DiskLayout) -> Result<Self> {
        let root = open_directory(Path::new("/"), 0)?;
        let data = open_at(&root, "data", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&data.metadata()?, true, 0, false)?;
        if DeviceNumber::from_dev(data.metadata()?.dev()) != layout.partitions[3].device {
            return Err(invalid("staging is not on the validated data partition"));
        }
        let system = open_at(
            &data,
            "spacewars-system",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        owned(&system.metadata()?, true, 0, true)?;
        let staging = open_at(&system, "staging", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        if staging.metadata()?.dev() != data.metadata()?.dev() {
            return Err(invalid("staging filesystem differs from persistent data"));
        }
        Self::under(staging, 0, candidate.image().compressed.clone())
    }

    fn under(directory: File, owner: u32, expected: Blob) -> Result<Self> {
        owned(&directory.metadata()?, true, owner, true)?;
        let name = format!("{}.ext4.zst", expected.sha256);
        let file = open_at(&directory, &name, libc::O_RDONLY, 0)?;
        let original = file.metadata()?;
        owned(&original, false, owner, true)?;
        if original.len() != expected.bytes || original.dev() != directory.metadata()?.dev() {
            return Err(Error::ImageMismatch);
        }
        let staged = Self {
            directory,
            file,
            original,
            expected,
            name,
            owner,
        };
        staged.check()?;
        Ok(staged)
    }

    /// Borrowed by TrustStore::revalidate_image at final admission. Opening a
    /// payload alone neither verifies signatures nor authorizes a root write.
    pub fn reader(&mut self) -> Result<&mut File> {
        self.check()?;
        self.file.rewind()?;
        Ok(&mut self.file)
    }

    fn check(&self) -> Result<()> {
        owned(&self.directory.metadata()?, true, self.owner, true)?;
        let named = open_at(&self.directory, &self.name, libc::O_RDONLY, 0)?;
        let current = self.file.metadata()?;
        owned(&current, false, self.owner, true)?;
        if !unchanged(&self.original, &current) || !unchanged(&self.original, &named.metadata()?) {
            return Err(invalid("staged payload changed or was replaced"));
        }
        Ok(())
    }

    fn verify(&mut self, image: &ImageDescriptor) -> Result<()> {
        if self.expected != image.compressed {
            return Err(Error::CandidateChanged);
        }
        self.check()?;
        self.file.rewind()?;
        verify_blob(&mut self.file, &image.compressed)?;
        self.check()?;
        self.file.rewind()?;
        Ok(())
    }
}

/// Returns PrepareTrial only after readback, read-only fsck, embedded identity
/// checks and the durable Written transition. On failure the normal selector
/// remains untouched; quarantine is persisted if live evidence is still valid.
pub fn write_inactive(
    boot: &BootController<'_>,
    store: &mut RecoveryStore,
    candidate: &VerifiedCandidate,
    payload: &mut StagedPayload,
    mut report: impl FnMut(WriteProgress),
) -> Result<Action> {
    let image = candidate.image();
    // Do not quarantine some other transaction for a stale/wrong caller.
    let (transaction, initial) = boot.authorize_write(store, image)?;
    let result = (|| {
        report(progress(WriteStage::CheckingPayload, 0, image));
        payload.verify(image)?;
        let (_, current) = boot.authorize_write(store, image)?;
        if current.layout != initial.layout {
            return Err(invalid("layout changed before writing"));
        }
        let mut target = BlockTarget::open(&current.layout, image.expanded.bytes)?;
        boot.authorize_write(store, image)?;
        let mut tools = ImageTools::default();
        write_image(
            image,
            &mut payload.file,
            &mut target,
            |file| tools.check(file),
            &mut report,
        )?;
        payload.check()?;
        let (_, final_boot) = boot.authorize_write(store, image)?;
        target.check(&final_boot.layout)?;
        let action = store.advance(|record| {
            record.written(
                transaction,
                image.compressed.sha256,
                image.expanded.sha256,
                &final_boot.observation,
            )
        })?;
        report(progress(WriteStage::Prepared, image.expanded.bytes, image));
        Ok(action)
    })();
    if let Err(error) = &result {
        let failure = boot.observe().and_then(|actual| {
            store.advance(|record| {
                record.fail(FailureReason::InstallationFailed, &actual.observation)
            })
        });
        if let Err(journal_error) = failure {
            return Err(Error::Install(format!(
                "{error}; failure record requires reconciliation: {journal_error}"
            )));
        }
    }
    result
}

trait Target: Read + Write + Seek {
    fn capacity(&self) -> u64;
    fn flush_to_device(&mut self) -> Result<()>;
    fn file(&self) -> &File;
}

struct BlockTarget {
    file: File,
    partition: Partition,
    capacity: u64,
}
nix::ioctl_read!(block_size, 0x12, 114, u64);
nix::ioctl_read_bad!(
    block_read_only,
    nix::request_code_none!(0x12, 94),
    libc::c_int
);
nix::ioctl_none!(flush_block_cache, 0x12, 97);

impl BlockTarget {
    fn open(layout: &DiskLayout, expanded: u64) -> Result<Self> {
        let partition = layout.inactive_partition().clone();
        let capacity = partition
            .sectors
            .checked_mul(512)
            .ok_or_else(|| invalid("capacity overflow"))?;
        if expanded == 0
            || expanded > capacity
            || partition.device == layout.active_partition().device
        {
            return Err(invalid("image does not fit an inactive root"));
        }
        let root = open_directory(Path::new("/"), 0)?;
        let dev = open_at(&root, "dev", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&dev.metadata()?, true, 0, false)?;
        let node = open_at(&dev, &partition.name, libc::O_PATH, 0)?;
        Self::node(&node.metadata()?, &partition)?;
        // O_EXCL claims this kernel block device, including mounts/holders in
        // other mount namespaces. Keep it claimed through validation and commit.
        let file = open_at(&dev, &partition.name, libc::O_RDWR | libc::O_EXCL, 0)?;
        Self::node(&file.metadata()?, &partition)?;
        let target = Self {
            file,
            partition,
            capacity,
        };
        target.check(layout)?;
        Ok(target)
    }
    fn node(metadata: &Metadata, expected: &Partition) -> Result<()> {
        // The supported image gives disk group GID 6 raw access; the kiosk is
        // not a member. Both disk and root are operator-level privileges.
        if !metadata.file_type().is_block_device()
            || metadata.uid() != 0
            || ![0, 6].contains(&metadata.gid())
            || metadata.mode() & 0o007 != 0
            || DeviceNumber::from_dev(metadata.rdev()) != expected.device
        {
            return Err(invalid(
                "inactive node is not the validated root-owned block device",
            ));
        }
        Ok(())
    }
    fn check(&self, layout: &DiskLayout) -> Result<()> {
        if &self.partition != layout.inactive_partition() {
            return Err(invalid("inactive device changed"));
        }
        Self::node(&self.file.metadata()?, &self.partition)?;
        let (mut bytes, mut read_only) = (0u64, 1);
        // SAFETY: these live block-device ioctls fill correctly sized UAPI values.
        unsafe {
            block_size(self.file.as_raw_fd(), &mut bytes).map_err(|e| Error::Io(e.into()))?;
            block_read_only(self.file.as_raw_fd(), &mut read_only)
                .map_err(|e| Error::Io(e.into()))?;
        }
        if bytes != self.capacity || read_only != 0 {
            return Err(invalid("inactive device size or writeability changed"));
        }
        Ok(())
    }
}

impl Read for BlockTarget {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl Write for BlockTarget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.file.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for BlockTarget {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}
impl Target for BlockTarget {
    fn capacity(&self) -> u64 {
        self.capacity
    }
    fn file(&self) -> &File {
        &self.file
    }
    fn flush_to_device(&mut self) -> Result<()> {
        self.file.sync_all()?;
        // SAFETY: flush and invalidate only this still-exclusively-held device.
        unsafe { flush_block_cache(self.file.as_raw_fd()) }.map_err(|e| Error::Io(e.into()))?;
        Ok(())
    }
}

fn write_image(
    image: &ImageDescriptor,
    source: &mut (impl Read + Seek),
    target: &mut impl Target,
    inspect: impl FnOnce(&File) -> Result<ImageIdentity>,
    report: &mut impl FnMut(WriteProgress),
) -> Result<()> {
    image.validate()?;
    if image.expanded.bytes < HEADER_BYTES as u64 || image.expanded.bytes > target.capacity() {
        return Err(invalid("expanded image does not fit the target"));
    }
    source.rewind()?;
    // Rehash while decoding as well: the opening-time digest alone cannot
    // authorize bytes that changed after it was checked.
    let counted = HashReader {
        source,
        hash: Hasher::new(),
        bytes: 0,
        limit: image.compressed.bytes,
    };
    let mut input = BufReader::with_capacity(COPY_BYTES, counted);
    if !input.fill_buf()?.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        return Err(invalid("payload is not a standard zstd frame"));
    }
    let mut decoder = zstd::stream::read::Decoder::with_buffer(input)?.single_frame();
    decoder.window_log_max(ZSTD_WINDOW_LOG)?;
    let mut header = [0u8; HEADER_BYTES];
    decoder.read_exact(&mut header)?;
    check_ext4_header(&header, image.expanded.bytes)?;
    target.rewind()?;
    report(progress(WriteStage::Writing, 0, image));
    target.write_all(&header)?;
    let mut hash = Hasher::new();
    hash.update(header);
    let mut written = header.len() as u64;
    let mut last_report = Instant::now();
    let mut buffer = [0u8; COPY_BYTES];
    while written < image.expanded.bytes {
        let limit = (image.expanded.bytes - written).min(buffer.len() as u64) as usize;
        let count = decoder.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(Error::ImageMismatch);
        }
        target.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        written += count as u64;
        if last_report.elapsed() >= Duration::from_millis(250) || written == image.expanded.bytes {
            report(progress(WriteStage::Writing, written, image));
            last_report = Instant::now();
        }
    }
    if decoder.read(&mut buffer[..1])? != 0
        || hash.finalize().as_slice() != image.expanded.sha256.as_bytes()
    {
        return Err(Error::ImageMismatch);
    }
    let mut input = decoder.finish();
    if input.read(&mut buffer[..1])? != 0 {
        return Err(invalid("trailing bytes or concatenated zstd frames"));
    }
    let counted = input.into_inner();
    if counted.bytes != image.compressed.bytes
        || counted.hash.finalize().as_slice() != image.compressed.sha256.as_bytes()
    {
        return Err(Error::ImageMismatch);
    }
    report(progress(WriteStage::Flushing, written, image));
    target.flush_to_device()?;
    readback(target, image, report)?;
    report(progress(WriteStage::CheckingFilesystem, written, image));
    inspect(target.file())?.matches_descriptor(image)?;
    // The checker is read-only, but verify that promise before any selector is
    // eligible. The target can be larger than the filesystem; hash its extent.
    readback(target, image, report)?;
    Ok(())
}

fn readback(
    target: &mut impl Target,
    image: &ImageDescriptor,
    report: &mut impl FnMut(WriteProgress),
) -> Result<()> {
    report(progress(WriteStage::ReadingBack, 0, image));
    target.rewind()?;
    let mut limited = target.take(image.expanded.bytes);
    // verify_blob checks EOF on Take, which is exactly the signed image extent.
    verify_blob(&mut limited, &image.expanded)?;
    report(progress(
        WriteStage::ReadingBack,
        image.expanded.bytes,
        image,
    ));
    Ok(())
}

struct HashReader<R> {
    source: R,
    hash: Hasher,
    bytes: u64,
    limit: u64,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let remaining = self.limit - self.bytes;
        if remaining == 0 {
            if self.source.read(&mut [0])? != 0 {
                return Err(std::io::Error::other(
                    "compressed payload exceeds its signed length",
                ));
            }
            return Ok(0);
        }
        let length = remaining.min(buffer.len() as u64) as usize;
        let count = self.source.read(&mut buffer[..length])?;
        self.bytes += count as u64;
        self.hash.update(&buffer[..count]);
        Ok(count)
    }
}

fn check_ext4_header(header: &[u8], expanded: u64) -> Result<()> {
    if header.len() < 2048 {
        return Err(invalid("truncated filesystem header"));
    }
    let sb = &header[1024..2048];
    let u32_at = |offset| u32::from_le_bytes(sb[offset..offset + 4].try_into().unwrap());
    let incompatible = u32_at(96);
    // Yocto currently emits 1 KiB blocks. All ext4 block sizes up to the
    // supported kernel's 4 KiB page size are valid; 4 KiB is not mandatory.
    let block_bytes = 1024u64.checked_shl(u32_at(24)).unwrap_or(0);
    let blocks = u64::from(u32_at(4)) | (u64::from(u32_at(336)) << 32);
    if sb[56..58] != [0x53, 0xef]
        || sb[58..60] != [1, 0]
        || ![1024, 2048, 4096].contains(&block_bytes)
        || u32_at(20) != u32::from(block_bytes == 1024)
        || incompatible & 0x40 == 0
        || incompatible & (0x4 | 0x8) != 0
        || (incompatible & 0x80 == 0 && u32_at(336) != 0)
        || blocks.checked_mul(block_bytes) != Some(expanded)
    {
        return Err(invalid(
            "expected a clean, complete ext4 root with 1–4 KiB blocks and no external journal",
        ));
    }
    Ok(())
}

fn progress(stage: WriteStage, bytes: u64, image: &ImageDescriptor) -> WriteProgress {
    WriteProgress {
        stage,
        bytes,
        total: image.expanded.bytes,
    }
}
fn invalid(message: &str) -> Error {
    Error::Install(message.into())
}

#[cfg(test)]
#[path = "install_linux_tests.rs"]
mod tests;
