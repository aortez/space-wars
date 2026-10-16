//! Workstation-only packaging of an operator-selected, expanded ext4 image.
//! Only supplied regular artifact files are read and a new output directory is
//! created. This has no device authority, network, journal or reboot operation.
use super::{BootPlan, PlanRequest, STAGE_DIRECTORY, invalid};
use crate::boot::{BootCommandLine, BootManifest, MAX_BOOT_MANIFEST_BYTES, UNPREPARED_TRIAL};
use crate::file_command::FileCommand;
use crate::image_tools::ImageTools;
use crate::owned_fs::{name, open_at, unchanged};
use crate::{Blob, ImageFormat, ImageIdentity, MAX_COMPRESSED_BYTES, Result, Sha256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub(super) const MAX_ROOT_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const NOTICE: &str = "Supervised bootstrap artifact, not a signed OTA candidate.\n\
This directory does not install itself or enable automatic updates.\n\
The board/profile/target/PARTUUIDs are operator-supplied planning inputs.\n\
Device apply must re-observe them under the installation lock, verify all\n\
retained firmware and capacity, preserve access/data, and retain recovery copies.\n\
Keep the original default boot configuration throughout the first one-shot trial.\n\
Do not copy the canonical boot tree over a running legacy device in one step.\n\
Do not create a production qualification record from this package.\n\
See docs/ota-updates.md for the remaining supervised migration procedure.\n";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapPackage {
    pub schema: u32,
    pub bootstrap_protocol: u32,
    pub purpose: String,
    pub image: ImageIdentity,
    pub format: ImageFormat,
    pub compressed: Blob,
    pub expanded: Blob,
    pub plan: BootPlan,
    /// Exact package files, excluding this record and its checksum list.
    /// Checksums detect accidental changes; they are not feed signatures.
    #[serde(deserialize_with = "crate::boot::unique_files")]
    pub files: BTreeMap<String, Blob>,
}

/// Build a new private directory on the workstation. The inputs are regular
/// artifact files, not live partitions. Never expose this through a device
/// socket or use its output as VerifiedCandidate authority.
pub fn build_package(
    rootfs: &Path,
    boot_directory: &Path,
    output: &Path,
    request: &PlanRequest,
) -> Result<BootstrapPackage> {
    let root = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW)
        .open(rootfs)?;
    let mut root = regular_reader(root)?;
    let before = root.metadata()?;
    if !before.is_file() || !(2048..=MAX_ROOT_BYTES).contains(&before.len()) {
        return Err(invalid(
            "bootstrap input must be a bounded regular ext4 image",
        ));
    }
    let mut header = [0; 2048];
    root.read_exact(&mut header)?;
    crate::install_linux::check_ext4_header(&header, before.len())?;
    root.rewind()?;
    let expanded = transfer(&mut root, &mut std::io::sink(), before.len())?;
    let image = ImageTools::default().check_bootstrap(&root)?;
    if image.source.dirty
        || image.machine != "raspberrypi-spacewars"
        || image.architecture != "aarch64"
        || image.updater_protocol != crate::UPDATER_PROTOCOL
        || image.recovery_protocol != crate::recovery::RECOVERY_PROTOCOL
    {
        return Err(invalid("bootstrap requires a clean, supported Pi image"));
    }
    let boot = directory(boot_directory)?;
    let manifest_bytes = read_asset(
        &boot,
        "spacewars-boot-manifest.json",
        MAX_BOOT_MANIFEST_BYTES as u64,
    )?;
    let manifest = BootManifest::parse(&manifest_bytes, image.boot_identity)?;
    let boot_id = read_asset(&boot, "spacewars-boot-id", 65)?;
    if boot_id != format!("{}\n", image.boot_identity).as_bytes() {
        return Err(invalid("boot identity file does not match the rootfs"));
    }

    let parent_path = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent_path = fs::canonicalize(parent_path)?;
    let parent = directory(&parent_path)?;
    let output_name = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("invalid package directory name"))?;
    let output_name_c = name(output_name)?;
    match fs::symlink_metadata(parent_path.join(output_name)) {
        Ok(_) => return Err(invalid("package output already exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let pending = tempfile::Builder::new()
        .prefix(".sw-bootstrap-")
        .tempdir_in(&parent_path)?;
    fs::set_permissions(pending.path(), fs::Permissions::from_mode(0o700))?;
    let mut files = BTreeMap::new();
    let canonical_path = pending.path().join("boot");
    fs::create_dir(&canonical_path)?;
    for (path, expected) in manifest.files() {
        let mut source = asset(&boot, path)?;
        let metadata = source.metadata()?;
        if metadata.len() != expected.bytes {
            return Err(invalid("boot asset length differs from the manifest"));
        }
        let destination = canonical_path.join(path);
        fs::create_dir_all(destination.parent().expect("asset parent"))?;
        let mut target = new_file(&destination)?;
        let actual = transfer(&mut source, &mut target, expected.bytes)?;
        if &actual != expected || !unchanged(&metadata, &source.metadata()?) {
            return Err(invalid("boot asset changed or differs from its manifest"));
        }
        target.sync_all()?;
        files.insert(format!("boot/{path}"), actual);
    }
    // Render only from the snapshot already checked against this rootfs's
    // manifest, never a second read of the caller's mutable input directory.
    let config = fs::read_to_string(canonical_path.join("config.txt"))?;
    if !matches!(request.profile.as_str(), "hdmi" | "hyperpixel" | "picade") {
        return Err(invalid("unsupported bootstrap hardware profile"));
    }
    let profile = fs::read_to_string(
        canonical_path.join(format!("spacewars-profiles/{}.txt", request.profile)),
    )?;
    let template =
        BootCommandLine::parse(&fs::read(canonical_path.join("spacewars-cmdline.txt"))?)?;
    let plan = BootPlan::create(&image, &manifest, &config, &profile, &template, request)?;
    write(
        pending.path(),
        &mut files,
        "boot/spacewars-boot-manifest.json",
        &manifest_bytes,
    )?;
    write(
        pending.path(),
        &mut files,
        "boot/spacewars-boot-id",
        &boot_id,
    )?;
    write(
        pending.path(),
        &mut files,
        "boot/cmdline.txt",
        plan.normal_cmdline.as_bytes(),
    )?;
    write(
        pending.path(),
        &mut files,
        "boot/spacewars-trial.txt",
        UNPREPARED_TRIAL.as_bytes(),
    )?;
    write(
        pending.path(),
        &mut files,
        "boot/spacewars-device.txt",
        format!(
            "[all]\ninclude spacewars-profiles/{}.txt\n[all]\n",
            request.profile
        )
        .as_bytes(),
    )?;
    for (path, expected) in &plan.staged_assets {
        let mut source = File::open(canonical_path.join(path))?;
        let name = format!("{STAGE_DIRECTORY}/{path}");
        let target_path = pending.path().join(&name);
        fs::create_dir_all(target_path.parent().expect("asset parent"))?;
        let mut target = new_file(&target_path)?;
        let actual = transfer(&mut source, &mut target, expected.bytes)?;
        if &actual != expected {
            return Err(invalid("staged boot snapshot changed"));
        }
        target.sync_all()?;
        files.insert(name, actual);
    }
    for (name, contents) in [
        ("trial-config.txt".into(), &plan.trial_config),
        ("normal-config.txt".into(), &plan.normal_config),
        (
            format!("{STAGE_DIRECTORY}/trial-cmdline.txt"),
            &plan.trial_cmdline,
        ),
        (
            format!("{STAGE_DIRECTORY}/normal-cmdline.txt"),
            &plan.normal_cmdline,
        ),
    ] {
        write(pending.path(), &mut files, &name, contents.as_bytes())?;
    }
    write(
        pending.path(),
        &mut files,
        "image.json",
        &serde_json::to_vec_pretty(&image)?,
    )?;
    write(pending.path(), &mut files, "README.txt", NOTICE.as_bytes())?;

    // Verify both executable identities against actual ext4 content. Retain
    // the exact packaged manager for later, separately authorized root use.
    let mut checker = FileCommand::default();
    for (path, expected) in [
        ("/usr/bin/engine-client", &image.binaries.client),
        ("/usr/bin/engine-os-manager", &image.binaries.os_manager),
    ] {
        let mut command = Command::new("/usr/sbin/debugfs");
        command.args([
            "-R",
            &format!("cat {path}"),
            &format!("/proc/self/fd/{}", root.as_raw_fd()),
        ]);
        let bytes = checker.run(
            command,
            &root,
            Duration::from_secs(30),
            expected.bytes as usize,
        )?;
        if blob(&bytes) != *expected {
            return Err(invalid("packaged executable differs from image identity"));
        }
        if path == "/usr/bin/engine-os-manager" {
            write(pending.path(), &mut files, "engine-os-manager", &bytes)?;
        }
    }

    // The fallback root needs the same access/network and maintenance guards.
    // Carry their exact image bytes, not files from the builder's checkout.
    super::legacy_assets::extract(&root, |path, bytes| {
        fs::create_dir_all(
            pending
                .path()
                .join(path)
                .parent()
                .expect("fixed asset parent"),
        )?;
        write(pending.path(), &mut files, path, bytes)
    })?;

    root.rewind()?;
    let compressed_path = pending.path().join("rootfs.ext4.zst");
    let compressed_file = new_file(&compressed_path)?;
    let mut encoder = zstd::stream::write::Encoder::new(compressed_file, 3)?;
    encoder.window_log(26)?;
    encoder.include_checksum(true)?;
    encoder.set_pledged_src_size(Some(expanded.bytes))?;
    let actual = transfer(&mut root, &mut encoder, expanded.bytes)?;
    encoder.finish()?.sync_all()?;
    if actual != expanded || !unchanged(&before, &root.metadata()?) {
        return Err(invalid("rootfs changed during bootstrap packaging"));
    }
    let mut compressed_file = File::open(&compressed_path)?;
    let length = compressed_file.metadata()?.len();
    if length == 0 || length > MAX_COMPRESSED_BYTES {
        return Err(invalid("compressed root exceeds its bound"));
    }
    let compressed = transfer(&mut compressed_file, &mut std::io::sink(), length)?;
    files.insert("rootfs.ext4.zst".into(), compressed.clone());
    let package = BootstrapPackage {
        schema: 3,
        bootstrap_protocol: super::BOOTSTRAP_PROTOCOL,
        purpose: "supervised-bootstrap".into(),
        image,
        format: ImageFormat::RootfsExt4Zstd,
        compressed,
        expanded,
        plan,
        files,
    };
    let record = serde_json::to_vec_pretty(&package)?;
    let mut checksums = package
        .files
        .iter()
        .map(|(name, blob)| format!("{}  {name}\n", blob.sha256))
        .collect::<String>();
    checksums.push_str(&format!("{}  bundle.json\n", Sha256::digest(&record)));
    for (name, contents) in [
        ("bundle.json", record.as_slice()),
        ("SHA256SUMS", checksums.as_bytes()),
    ] {
        let mut file = new_file(&pending.path().join(name))?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    sync_directories(pending.path())?;
    let pending_name = name(
        pending
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .expect("temporary directory name"),
    )?;
    // SAFETY: both C names and the parent directory stay live. A new directory
    // is published without replacing any previous kit, including a symlink.
    if unsafe {
        libc::renameat2(
            parent.as_raw_fd(),
            pending_name.as_ptr(),
            parent.as_raw_fd(),
            output_name_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let _ = pending.keep();
    parent.sync_all()?;
    Ok(package)
}

fn directory(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?)
}

fn asset(directory: &File, path: &str) -> Result<File> {
    if !crate::boot::asset_path(path) {
        return Err(invalid("invalid artifact path"));
    }
    let mut parent = directory.try_clone()?;
    let mut parts = path.split('/').peekable();
    while let Some(component) = parts.next() {
        if parts.peek().is_none() {
            return regular_reader(open_at(&parent, component, libc::O_PATH, 0)?);
        }
        parent = open_at(&parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    }
    Err(invalid("empty artifact path"))
}

fn regular_reader(path: File) -> Result<File> {
    // O_PATH pins the inode without activating a device supplied by mistake.
    // Reopen that proven regular inode, not the caller's replaceable name.
    if !path.metadata()?.is_file() {
        return Err(invalid("bootstrap input must be a regular artifact file"));
    }
    Ok(File::open(format!("/proc/self/fd/{}", path.as_raw_fd()))?)
}

fn read_asset(directory: &File, path: &str, limit: u64) -> Result<Vec<u8>> {
    let mut file = asset(directory, path)?;
    let before = file.metadata()?;
    if before.len() > limit {
        return Err(invalid("bootstrap metadata exceeds its bound"));
    }
    let mut bytes = Vec::new();
    transfer(&mut file, &mut bytes, before.len())?;
    if !unchanged(&before, &file.metadata()?) {
        return Err(invalid("bootstrap metadata changed"));
    }
    Ok(bytes)
}

fn new_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

fn write(root: &Path, files: &mut BTreeMap<String, Blob>, name: &str, bytes: &[u8]) -> Result<()> {
    let mut file = new_file(&root.join(name))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    files.insert(name.into(), blob(bytes));
    Ok(())
}

fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}

pub(super) fn transfer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected: u64,
) -> Result<Blob> {
    let mut hash = Hasher::new();
    let mut remaining = expected;
    let mut buffer = [0; 64 * 1024];
    while remaining != 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = reader.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(invalid("bootstrap artifact is truncated"));
        }
        writer.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if reader.read(&mut buffer[..1])? != 0 {
        return Err(invalid("bootstrap artifact grew"));
    }
    Ok(Blob {
        bytes: expected,
        sha256: format!("{:x}", hash.finalize())
            .try_into()
            .expect("SHA-256 hex"),
    })
}

fn sync_directories(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_directories(&entry.path())?;
        }
    }
    // Packages are transferred to a private root-owned data directory. Keep
    // every generated directory private regardless of the workstation umask.
    let directory = directory(path)?;
    directory.set_permissions(fs::Permissions::from_mode(0o700))?;
    directory.sync_all()?;
    Ok(())
}
