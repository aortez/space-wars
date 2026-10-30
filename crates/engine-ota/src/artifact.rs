//! Workstation image qualification. This does not authorize signing or install.
//! Run pinned tools without signing keys; policy must be selected independently
//! of the candidate. Only fixed read-only filesystem tools inspect image bytes.
use crate::boot::BootManifest;
use crate::development::DevelopmentImage;
use crate::image_tools::{ImageReader, ImageTools};
use crate::install_linux::{Target, check_ext4_header, write_checked_ext4_guarded};
use crate::{Blob, BuildSource, Error, ImageFormat, ImageIdentity, Result, Sha256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub const MAX_ROOT_BYTES: u64 = 3 * 1024 * 1024 * 1024;
pub const MAX_POLICY_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    pub source_commit: String,
    pub build_inputs_sha256: Sha256,
    pub rust_version: String,
    pub cargo_version: String,
    pub cc_version_line: String,
}

impl Policy {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_POLICY_BYTES {
            return Err(invalid("image policy exceeds its size bound"));
        }
        let policy: Self = serde_json::from_slice(bytes)?;
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !commit(&self.source_commit)
            || self.build_inputs_sha256.as_bytes() == &[0; 32]
            || [&self.rust_version, &self.cargo_version].iter().any(|v| {
                v.is_empty() || v.len() > 32 || !v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            })
            || self.cc_version_line.is_empty()
            || self.cc_version_line.len() > 256
            || !self
                .cc_version_line
                .bytes()
                .all(|b| b.is_ascii_graphic() || b == b' ')
        {
            return Err(invalid("invalid independently selected image policy"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub schema: u32,
    pub format: ImageFormat,
    pub compressed: Blob,
    pub expanded: Blob,
}

impl Candidate {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 4096 {
            return Err(invalid("candidate record exceeds its size bound"));
        }
        let candidate: Self = serde_json::from_slice(bytes)?;
        candidate.validate()?;
        Ok(candidate)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.format != ImageFormat::RootfsExt4Zstd
            || !(1..=crate::MAX_COMPRESSED_BYTES).contains(&self.compressed.bytes)
            || !(4096..=MAX_ROOT_BYTES).contains(&self.expanded.bytes)
            || self.compressed.sha256.as_bytes() == &[0; 32]
            || self.expanded.sha256.as_bytes() == &[0; 32]
        {
            return Err(invalid("invalid rootfs candidate format or size"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    url: String,
    commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocalLayer {
    path: String,
    source_commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildInputs {
    schema: u32,
    source_commit: String,
    kas_version: String,
    machine: String,
    target: String,
    layers: BTreeMap<String, Layer>,
    local_layer: LocalLayer,
    configuration: BTreeMap<String, Blob>,
    expanded_kas_sha256: Sha256,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Version {
    version: String,
    details: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CCompiler {
    details: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Tools {
    rustc: Version,
    cargo: Version,
    cc: CCompiler,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CompilerRecord {
    schema: u32,
    source: BuildSource,
    rust_target: String,
    profile: String,
    kiosk_feature: String,
    tools: Tools,
}

/// Evidence returned only after checking actual bytes. Not a signing capability.
#[derive(Debug, Serialize)]
pub struct Qualification {
    schema: u32,
    image: ImageIdentity,
    build_inputs_sha256: Sha256,
    build_inputs: BuildInputs,
    compiler: CompilerRecord,
    expanded: Blob,
}

impl Qualification {
    pub fn image(&self) -> &ImageIdentity {
        &self.image
    }
}

fn inspect(root: &mut File, policy: &Policy) -> Result<Qualification> {
    policy.validate()?;
    let (before, expanded) = measure(root)?;
    let image = ImageTools::default().check_bootstrap(root)?;
    if image.source.dirty
        || image.source.commit != policy.source_commit
        || image.machine != "raspberrypi-spacewars"
        || image.architecture != "aarch64"
        || image.updater_protocol != crate::UPDATER_PROTOCOL
        || image.recovery_protocol != crate::recovery::RECOVERY_PROTOCOL
        || image.data.reads.min != 1
        || image.data.reads.max != 1
        || image.data.writes != 1
    {
        return Err(invalid(
            "image source, architecture or compatibility differs from policy",
        ));
    }
    let mut reader = ImageReader::new(root);
    reader.file(
        "/etc/spacewars/image.json",
        0o644,
        crate::MAX_IMAGE_IDENTITY_BYTES,
    )?;
    reader.file("/etc/spacewars/bootstrap-runtime.json", 0o644, 1024)?;
    check_contents(&mut reader, &image)?;
    let input_bytes = reader.file("/usr/share/spacewars/build-inputs.json", 0o644, 16 * 1024)?;
    if Sha256::digest(&input_bytes) != policy.build_inputs_sha256 {
        return Err(invalid(
            "embedded build inputs differ from independently selected digest",
        ));
    }
    let inputs: BuildInputs = serde_json::from_slice(&input_bytes)?;
    let expected_layers = [
        "meta-openembedded",
        "meta-pi-base",
        "meta-raspberrypi",
        "meta-rust-bin",
        "poky",
    ];
    let expected_configuration = [
        "Cargo.lock",
        "yocto/kas-spacewars.yml",
        "yocto/meta-spacewars/conf/machine/raspberrypi-spacewars.conf",
    ];
    if inputs.schema != 1
        || inputs.source_commit != policy.source_commit
        || inputs.kas_version != "5.4"
        || inputs.machine != image.machine
        || inputs.target != "spacewars-image"
        || inputs.local_layer.path != "yocto/meta-spacewars"
        || inputs.local_layer.source_commit != policy.source_commit
        || inputs.layers.keys().map(String::as_str).ne(expected_layers)
        || inputs
            .configuration
            .keys()
            .map(String::as_str)
            .ne(expected_configuration)
        || inputs.layers.values().any(|layer| !commit(&layer.commit))
        || inputs.configuration.values().any(|blob| {
            blob.bytes == 0 || blob.bytes > 1024 * 1024 || blob.sha256.as_bytes() == &[0; 32]
        })
        || inputs.expanded_kas_sha256.as_bytes() == &[0; 32]
    {
        return Err(invalid(
            "embedded layer/configuration provenance is incomplete",
        ));
    }
    let compiler: CompilerRecord = serde_json::from_slice(&reader.file(
        "/usr/share/spacewars/build.json",
        0o644,
        32 * 1024,
    )?)?;
    if compiler.schema != 1
        || compiler.source != image.source
        || compiler.rust_target != "aarch64-unknown-linux-gnu"
        || compiler.profile != "release"
        || compiler.kiosk_feature != "pi-kiosk-gpu"
        || compiler.tools.rustc.version != policy.rust_version
        || compiler.tools.cargo.version != policy.cargo_version
        || !compiler
            .tools
            .rustc
            .details
            .lines()
            .any(|line| line == format!("release: {}", policy.rust_version))
        || !compiler
            .tools
            .cargo
            .details
            .starts_with(&format!("cargo {} ", policy.cargo_version))
        || compiler.tools.cc.details.lines().next() != Some(policy.cc_version_line.as_str())
        || [
            &compiler.tools.rustc.details,
            &compiler.tools.cargo.details,
            &compiler.tools.cc.details,
        ]
        .iter()
        .any(|s| s.is_empty() || s.len() > 8192 || s.contains('\0'))
    {
        return Err(invalid(
            "packaged compiler/configuration record differs from policy",
        ));
    }
    if hash(root, before.len())? != expanded || !unchanged(&before, &root.metadata()?) {
        return Err(invalid("image changed during qualification"));
    }
    Ok(Qualification {
        schema: 1,
        image,
        build_inputs_sha256: policy.build_inputs_sha256,
        build_inputs: inputs,
        compiler,
        expanded,
    })
}

/// A development image skips the release policy: any source state, builder
/// and provenance. The unit still checks its compatibility before writing.
fn inspect_development(root: &mut File) -> Result<(ImageIdentity, Blob)> {
    let (before, expanded) = measure(root)?;
    let image = ImageTools::default().check_development(root)?;
    let mut reader = ImageReader::new(root);
    reader.file(
        "/etc/spacewars/image.json",
        0o644,
        crate::MAX_IMAGE_IDENTITY_BYTES,
    )?;
    reader.file("/etc/spacewars/development-runtime.json", 0o644, 1024)?;
    check_contents(&mut reader, &image)?;
    if hash(root, before.len())? != expanded || !unchanged(&before, &root.metadata()?) {
        return Err(invalid("image changed during qualification"));
    }
    Ok((image, expanded))
}

fn measure(root: &mut File) -> Result<(Metadata, Blob)> {
    let before = root.metadata()?;
    if !before.is_file() || !(4096..=MAX_ROOT_BYTES).contains(&before.len()) {
        return Err(invalid(
            "qualification requires a bounded regular ext4 image",
        ));
    }
    let expanded = hash(root, before.len())?;
    root.rewind()?;
    let mut header = [0u8; 4096];
    root.read_exact(&mut header)?;
    check_ext4_header(&header, expanded.bytes)?;
    Ok((before, expanded))
}

/// Structural checks shared by release and development images.
fn check_contents(reader: &mut ImageReader<'_>, image: &ImageIdentity) -> Result<()> {
    let source: BuildSource =
        serde_json::from_slice(&reader.file("/usr/share/spacewars/source.json", 0o644, 4096)?)?;
    if source != image.source {
        return Err(invalid(
            "packaged source record differs from image identity",
        ));
    }
    let manifest = reader.file(
        "/etc/spacewars/boot-manifest.json",
        0o644,
        crate::boot::MAX_BOOT_MANIFEST_BYTES,
    )?;
    BootManifest::parse(&manifest, image.boot_identity)?;
    if reader.file("/etc/spacewars/boot-id", 0o644, 65)?
        != format!("{}\n", image.boot_identity).as_bytes()
    {
        return Err(invalid("image boot identity records disagree"));
    }
    for (path, expected) in [
        ("/usr/bin/engine-client", &image.binaries.client),
        ("/usr/bin/engine-os-manager", &image.binaries.os_manager),
    ] {
        let bytes = reader.file(path, 0o755, expected.bytes as usize)?;
        if bytes.len() as u64 != expected.bytes
            || Sha256::digest(&bytes) != expected.sha256
            || bytes.len() < 64
            || &bytes[..6] != b"\x7fELF\x02\x01"
            || u16::from_le_bytes([bytes[18], bytes[19]]) != 183
        {
            return Err(invalid(
                "packaged executable hash/architecture differs from image identity",
            ));
        }
    }
    Ok(())
}

/// Verify one bounded Zstandard frame using the same decoder/readback checks as
/// the installer, but with an anonymous regular file instead of a block device.
pub fn verify(
    policy: &Policy,
    candidate: &Candidate,
    payload: &Path,
    scratch: &Path,
) -> Result<Qualification> {
    policy.validate()?;
    decode(candidate, payload, scratch, |root| inspect(root, policy))
}

fn decode<T>(
    candidate: &Candidate,
    payload: &Path,
    scratch: &Path,
    check: impl FnOnce(&mut File) -> Result<T>,
) -> Result<T> {
    candidate.validate()?;
    let mut payload = regular(payload)?;
    let original = payload.metadata()?;
    if original.len() != candidate.compressed.bytes {
        return Err(Error::ImageMismatch);
    }
    let file = tempfile::tempfile_in(scratch)?;
    file.set_len(candidate.expanded.bytes)?;
    let mut target = TemporaryTarget {
        file,
        bytes: candidate.expanded.bytes,
    };
    let mut result = None;
    write_checked_ext4_guarded(
        &candidate.compressed,
        &candidate.expanded,
        &mut payload,
        &mut target,
        |file| {
            result = Some(check(&mut file.try_clone()?)?);
            Ok(())
        },
        &mut |_| Ok(()),
    )?;
    if !unchanged(&original, &payload.metadata()?) {
        return Err(invalid("compressed image changed during qualification"));
    }
    result.ok_or_else(|| invalid("filesystem qualification did not execute"))
}

/// Create a flat candidate, then independently decode/check its actual bytes.
/// Generation/channel assignment and signing belong to a different stage.
pub fn pack(policy: &Policy, rootfs: &Path, output: &Path) -> Result<Candidate> {
    policy.validate()?;
    package(
        rootfs,
        output,
        |root| Ok(inspect(root, policy)?.expanded),
        |candidate, payload, scratch| {
            let verified = verify(policy, candidate, payload, scratch)?;
            Ok(vec![
                ("candidate.json", serde_json::to_vec_pretty(candidate)?),
                ("qualification.json", serde_json::to_vec_pretty(&verified)?),
            ])
        },
    )
}

/// The same single-frame payload for `maintenance development install`, with
/// a development descriptor taken from the decoded image instead of a policy.
pub fn pack_development(rootfs: &Path, output: &Path) -> Result<DevelopmentImage> {
    let mut descriptor = None;
    package(
        rootfs,
        output,
        |root| Ok(inspect_development(root)?.1),
        |candidate, payload, scratch| {
            let (image, expanded) = decode(candidate, payload, scratch, inspect_development)?;
            let development =
                DevelopmentImage::from_identity(&image, candidate.compressed.clone(), expanded);
            development.validate()?;
            let bytes = serde_json::to_vec_pretty(&development)?;
            descriptor = Some(development);
            Ok(vec![("development.json", bytes)])
        },
    )?;
    descriptor.ok_or_else(|| invalid("development descriptor was not written"))
}

type Records = Vec<(&'static str, Vec<u8>)>;

fn package(
    rootfs: &Path,
    output: &Path,
    qualify: impl FnOnce(&mut File) -> Result<Blob>,
    finish: impl FnOnce(&Candidate, &Path, &Path) -> Result<Records>,
) -> Result<Candidate> {
    let parent = fs::canonicalize(
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let output_name = output
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("invalid candidate directory"))?;
    match fs::symlink_metadata(parent.join(output_name)) {
        Ok(_) => return Err(invalid("candidate output already exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let output_name =
        CString::new(output_name).map_err(|_| invalid("invalid candidate directory"))?;
    let mut root = regular(rootfs)?;
    let expanded = qualify(&mut root)?;
    let original = root.metadata()?;
    let pending = tempfile::Builder::new()
        .prefix(".ota-candidate-")
        .tempdir_in(&parent)?;
    let payload_path = pending.path().join("image.ext4.zst");
    let payload = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&payload_path)?;
    let mut encoder = zstd::stream::write::Encoder::new(payload, 3)?;
    encoder.window_log(26)?;
    encoder.include_checksum(true)?;
    encoder.set_pledged_src_size(Some(expanded.bytes))?;
    root.rewind()?;
    let mut copied = (&mut root).take(expanded.bytes + 1);
    if std::io::copy(&mut copied, &mut encoder)? != expanded.bytes {
        return Err(invalid("expanded image length changed during packaging"));
    }
    encoder.finish()?.sync_all()?;
    if !unchanged(&original, &root.metadata()?) || hash(&mut root, original.len())? != expanded {
        return Err(invalid("expanded image changed during packaging"));
    }
    let mut payload = regular(&payload_path)?;
    let length = payload.metadata()?.len();
    let candidate = Candidate {
        schema: 1,
        format: ImageFormat::RootfsExt4Zstd,
        compressed: hash(&mut payload, length)?,
        expanded,
    };
    candidate.validate()?;
    for (name, bytes) in finish(&candidate, &payload_path, pending.path())? {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(pending.path().join(name))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    let directory = File::open(&parent)?;
    File::open(pending.path())?.sync_all()?;
    let from = CString::new(
        pending
            .path()
            .file_name()
            .expect("temporary directory name")
            .as_encoded_bytes(),
    )
    .map_err(|_| invalid("invalid temporary directory"))?;
    // SAFETY: both names and the parent descriptor remain live. Existing output
    // directories, files or links are never replaced, including a racing writer.
    if unsafe {
        libc::renameat2(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            output_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let _ = pending.keep();
    directory.sync_all()?;
    Ok(candidate)
}

fn regular(path: &Path) -> Result<File> {
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !handle.metadata()?.is_file() {
        return Err(invalid(
            "artifact input must be a regular file, not a link or device",
        ));
    }
    Ok(File::open(format!("/proc/self/fd/{}", handle.as_raw_fd()))?)
}

fn hash(file: &mut File, length: u64) -> Result<Blob> {
    file.rewind()?;
    let mut hasher = Hasher::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        count += size as u64;
        if count > length {
            return Err(invalid("artifact grew while hashing"));
        }
        hasher.update(&buffer[..size]);
    }
    if count != length {
        return Err(invalid("artifact shrank while hashing"));
    }
    Ok(Blob {
        bytes: count,
        sha256: format!("{:x}", hasher.finalize())
            .try_into()
            .expect("SHA-256 hexadecimal"),
    })
}

fn unchanged(a: &Metadata, b: &Metadata) -> bool {
    (
        a.dev(),
        a.ino(),
        a.len(),
        a.mtime(),
        a.mtime_nsec(),
        a.ctime(),
        a.ctime_nsec(),
    ) == (
        b.dev(),
        b.ino(),
        b.len(),
        b.mtime(),
        b.mtime_nsec(),
        b.ctime(),
        b.ctime_nsec(),
    )
}

fn commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn invalid(message: &str) -> Error {
    Error::Install(message.into())
}

struct TemporaryTarget {
    file: File,
    bytes: u64,
}
impl Read for TemporaryTarget {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(b)
    }
}
impl Write for TemporaryTarget {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.file.write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for TemporaryTarget {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(p)
    }
}
impl Target for TemporaryTarget {
    fn capacity(&self) -> u64 {
        self.bytes
    }
    fn flush_to_device(&mut self) -> Result<()> {
        Ok(self.file.sync_all()?)
    }
    fn file(&self) -> &File {
        &self.file
    }
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;
