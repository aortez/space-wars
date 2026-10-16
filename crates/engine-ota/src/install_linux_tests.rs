use super::*;
use crate::{Channel, IMAGE_LAYOUT, ImageFormat, Sha256};
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;

struct Fixture {
    _directory: tempfile::TempDir,
    bytes: Vec<u8>,
    compressed: Vec<u8>,
    image: ImageDescriptor,
    identity: ImageIdentity,
}

impl Fixture {
    fn new() -> Self {
        Self::with_block_size(1024)
    }

    fn with_block_size(block_bytes: u32) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        fs::create_dir_all(root.join("etc/spacewars")).unwrap();
        let mut identity =
            ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json"))
                .unwrap();
        identity.boot_identity = Sha256::digest(b"boot");
        fs::write(
            root.join("etc/spacewars/image.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        let path = directory.path().join("fixture.ext4");
        let status = Command::new("/usr/sbin/mke2fs")
            .args([
                "-q",
                "-t",
                "ext4",
                "-b",
                &block_bytes.to_string(),
                "-E",
                "lazy_itable_init=0,lazy_journal_init=0",
                "-d",
            ])
            .arg(root)
            .arg(&path)
            .arg((16 * 1024 * 1024 / block_bytes).to_string())
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let bytes = fs::read(path).unwrap();
        let compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
        let image = descriptor(&identity, blob(&compressed), blob(&bytes));
        Self {
            _directory: directory,
            bytes,
            compressed,
            image,
            identity,
        }
    }
}

fn descriptor(identity: &ImageIdentity, compressed: Blob, expanded: Blob) -> ImageDescriptor {
    ImageDescriptor {
        schema: 1,
        generation: 2,
        source_commit: identity.source.commit.clone(),
        build_id: identity.build_id.clone(),
        channel: Channel::Release,
        machine: identity.machine.clone(),
        architecture: identity.architecture.clone(),
        layout: IMAGE_LAYOUT.into(),
        format: ImageFormat::RootfsExt4Zstd,
        compressed,
        expanded,
        boot_identity: identity.boot_identity,
        min_updater_protocol: 1,
        min_recovery_protocol: crate::recovery::RECOVERY_PROTOCOL,
        data: identity.data,
    }
}

fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}

struct FileTarget {
    file: File,
    capacity: u64,
    written: u64,
    fail_write_at: Option<u64>,
    fail_flush: bool,
    corrupt_flush: bool,
}
impl FileTarget {
    fn new(capacity: u64) -> Self {
        let file = tempfile::tempfile().unwrap();
        file.set_len(capacity).unwrap();
        Self {
            file,
            capacity,
            written: 0,
            fail_write_at: None,
            fail_flush: false,
            corrupt_flush: false,
        }
    }
}
impl Read for FileTarget {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl Write for FileTarget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.fail_write_at.is_some_and(|n| self.written >= n) {
            return Err(std::io::Error::other("injected full device"));
        }
        let limit = self.fail_write_at.map_or(bytes.len(), |n| {
            (n - self.written).min(bytes.len() as u64) as usize
        });
        let count = self.file.write(&bytes[..limit])?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for FileTarget {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}
impl Target for FileTarget {
    fn file(&self) -> &File {
        &self.file
    }
    fn capacity(&self) -> u64 {
        self.capacity
    }
    fn flush_to_device(&mut self) -> Result<()> {
        if self.fail_flush {
            return Err(invalid("injected flush failure"));
        }
        if self.corrupt_flush {
            self.file.seek(SeekFrom::Start(8192))?;
            self.file.write_all(b"damaged")?;
        }
        self.file.sync_all()?;
        Ok(())
    }
}

#[test]
fn real_ext4_round_trip_checks_readback_fsck_identity_and_preserves_unused_tail() {
    for block_bytes in [1024, 2048, 4096] {
        round_trip(Fixture::with_block_size(block_bytes));
    }
}

#[test]
fn shared_writer_accepts_only_the_checked_embedded_identity_without_release_metadata() {
    let fixture = Fixture::new();
    let compressed = blob(&fixture.compressed);
    let expanded = blob(&fixture.bytes);
    for case in ["matching", "different-image", "checker-mutated"] {
        let mut expected = fixture.identity.clone();
        if case == "different-image" {
            expected.boot_identity = Sha256::digest(b"another boot set");
        }
        expected.validate().unwrap();
        let mut target = FileTarget::new(expanded.bytes + 4096);
        let mut tools = ImageTools::default();
        let mut stages = Vec::new();
        let result = write_checked_ext4(
            &compressed,
            &expanded,
            &mut Cursor::new(&fixture.compressed),
            &mut target,
            |file| {
                let actual = tools.check(file)?;
                if actual != expected {
                    return Err(invalid(
                        "bootstrap filesystem identity differs from its package",
                    ));
                }
                if case == "checker-mutated" {
                    let mut file = file.try_clone()?;
                    file.seek(SeekFrom::Start(9000))?;
                    file.write_all(b"changed")?;
                }
                Ok(())
            },
            &mut |progress| {
                assert_eq!(progress.total, expanded.bytes);
                stages.push(progress.stage);
            },
        );
        assert_eq!(result.is_ok(), case == "matching", "{case}: {result:?}");
        assert!(stages.contains(&WriteStage::CheckingFilesystem));
        // Success here is only a verified byte copy. No journal transition or
        // externally visible Prepared status occurs in the shared mechanism.
        assert!(!stages.contains(&WriteStage::Prepared));
    }
}

#[test]
fn shared_writer_rejects_unbounded_or_unfitting_inputs_before_writing() {
    let fixture = Fixture::new();
    for case in [
        "empty-compressed",
        "oversized-compressed",
        "small-expanded",
        "small-target",
    ] {
        let mut compressed = blob(&fixture.compressed);
        let mut expanded = blob(&fixture.bytes);
        match case {
            "empty-compressed" => compressed.bytes = 0,
            "oversized-compressed" => compressed.bytes = crate::MAX_COMPRESSED_BYTES + 1,
            "small-expanded" => expanded.bytes = HEADER_BYTES as u64 - 1,
            _ => (),
        }
        let mut target = FileTarget::new(if case == "small-target" {
            4096
        } else {
            fixture.bytes.len() as u64
        });
        let mut checked = false;
        assert!(
            write_checked_ext4(
                &compressed,
                &expanded,
                &mut Cursor::new(&fixture.compressed),
                &mut target,
                |_| {
                    checked = true;
                    Ok(())
                },
                &mut |_| {},
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(target.written, 0, "{case}");
        assert!(!checked, "{case}");
    }
}

fn round_trip(fixture: Fixture) {
    let mut target = FileTarget::new(fixture.image.expanded.bytes + 4096);
    target
        .file
        .seek(SeekFrom::Start(fixture.image.expanded.bytes))
        .unwrap();
    target.file.write_all(b"untouched tail").unwrap();
    let mut tools = ImageTools::default();
    let mut stages = Vec::new();
    write_image(
        &fixture.image,
        &mut Cursor::new(&fixture.compressed),
        &mut target,
        |file| tools.check(file),
        &mut |p| stages.push(p.stage),
    )
    .unwrap();
    assert!(stages.contains(&WriteStage::CheckingFilesystem));
    target.file.rewind().unwrap();
    let mut bytes = vec![0; fixture.bytes.len()];
    target.file.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, fixture.bytes);
    let mut tail = [0; 14];
    target.file.read_exact(&mut tail).unwrap();
    assert_eq!(&tail, b"untouched tail");
}

/// Optional build-artifact check: the input is opened read-only and every write
/// goes to an anonymous temporary file. No production device path is accepted.
#[test]
#[ignore = "requires SPACEWARS_OTA_TEST_EXT4 pointing to an expanded Yocto rootfs artifact"]
fn yocto_artifact_checks_actual_format_and_retains_dirty_source_rejection() {
    let mut root = File::open(std::env::var_os("SPACEWARS_OTA_TEST_EXT4").unwrap()).unwrap();
    let metadata = root.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 3 * 1024 * 1024 * 1024);
    let identity = ImageTools::default().check(&root).unwrap();
    let mut compressed = tempfile::tempfile().unwrap();
    let mut encoder = zstd::stream::write::Encoder::new(&mut compressed, 3).unwrap();
    std::io::copy(&mut root, &mut encoder).unwrap();
    encoder.finish().unwrap();
    let image = descriptor(&identity, file_blob(&mut compressed), file_blob(&mut root));
    let mut target = FileTarget::new(image.expanded.bytes + 4096);
    let mut tools = ImageTools::default();
    let mut stages = Vec::new();
    let result = write_image(
        &image,
        &mut compressed,
        &mut target,
        |file| tools.check(file),
        &mut |p| stages.push(p.stage),
    );
    assert!(stages.contains(&WriteStage::CheckingFilesystem));
    if identity.source.dirty {
        assert!(
            matches!(&result, Err(Error::ImageIdentity(message)) if message.contains("dirty")),
            "{result:?}"
        );
    } else {
        result.as_ref().unwrap();
    }
    println!(
        "{}",
        serde_json::json!({
            "descriptor": image,
            "source": identity.source,
            "result": result.err().map(|error| error.to_string()),
            "stages": stages,
            "target_is_temporary_regular_file": true,
        })
    );
}

fn file_blob(file: &mut File) -> Blob {
    file.rewind().unwrap();
    let mut hash = Hasher::new();
    let mut buffer = [0u8; COPY_BYTES];
    let mut bytes = 0;
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        bytes += count as u64;
    }
    file.rewind().unwrap();
    Blob {
        bytes,
        sha256: format!("{:x}", hash.finalize()).try_into().unwrap(),
    }
}

#[test]
fn corrupt_truncated_trailing_and_concatenated_streams_never_validate() {
    let fixture = Fixture::new();
    for kind in [
        "truncated",
        "corrupt",
        "trailing",
        "concatenated",
        "wrong-hash",
        "expanded-too-short",
        "expanded-too-long",
        "output-truncated",
        "output-extra",
        "invalid-block-size",
        "dirty-filesystem",
        "needs-journal-recovery",
        "external-journal",
        "small-target",
        "gzip",
    ] {
        let mut compressed = fixture.compressed.clone();
        let mut image = fixture.image.clone();
        match kind {
            "truncated" => {
                compressed.truncate(compressed.len() - 4);
                image.compressed = blob(&compressed);
            }
            "corrupt" => {
                let index = compressed.len() / 2;
                compressed[index] ^= 1;
                image.compressed = blob(&compressed);
            }
            "trailing" => {
                compressed.push(0);
                image.compressed = blob(&compressed);
            }
            "concatenated" => {
                compressed.extend(zstd::stream::encode_all(&b""[..], 1).unwrap());
                image.compressed = blob(&compressed);
            }
            "wrong-hash" => image.compressed.sha256 = Sha256::digest(b"different"),
            "expanded-too-short" => image.expanded.bytes -= 4096,
            "expanded-too-long" => image.expanded.bytes += 4096,
            "output-truncated"
            | "output-extra"
            | "invalid-block-size"
            | "dirty-filesystem"
            | "needs-journal-recovery"
            | "external-journal" => {
                let mut bytes = fixture.bytes.clone();
                match kind {
                    "output-truncated" => {
                        bytes.pop();
                    }
                    "output-extra" => bytes.push(0),
                    "invalid-block-size" => {
                        bytes[1024 + 24..1024 + 28].copy_from_slice(&32u32.to_le_bytes())
                    }
                    "dirty-filesystem" => bytes[1024 + 58] = 0,
                    "needs-journal-recovery" => bytes[1024 + 96] |= 0x4,
                    "external-journal" => bytes[1024 + 96] |= 0x8,
                    _ => unreachable!(),
                }
                compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
                image.compressed = blob(&compressed);
            }
            "gzip" => {
                compressed[..4].copy_from_slice(&[0x1f, 0x8b, 8, 0]);
                image.compressed = blob(&compressed);
            }
            _ => {}
        }
        let mut target = FileTarget::new(if kind == "small-target" {
            4096
        } else {
            image.expanded.bytes
        });
        let mut inspected = false;
        let result = write_image(
            &image,
            &mut Cursor::new(&compressed),
            &mut target,
            |_| {
                inspected = true;
                Ok(fixture.identity.clone())
            },
            &mut |_| {},
        );
        assert!(result.is_err(), "{kind}");
        assert!(!inspected, "{kind}");
        if [
            "small-target",
            "expanded-too-long",
            "expanded-too-short",
            "invalid-block-size",
            "dirty-filesystem",
            "needs-journal-recovery",
            "external-journal",
            "gzip",
        ]
        .contains(&kind)
        {
            assert_eq!(target.written, 0);
        }
    }
}

#[test]
fn write_flush_and_readback_failures_stop_before_filesystem_acceptance() {
    let fixture = Fixture::new();
    for kind in ["write", "flush", "readback"] {
        let mut target = FileTarget::new(fixture.image.expanded.bytes);
        target.fail_write_at = (kind == "write").then_some(10_000);
        target.fail_flush = kind == "flush";
        target.corrupt_flush = kind == "readback";
        let mut inspected = false;
        assert!(
            write_image(
                &fixture.image,
                &mut Cursor::new(&fixture.compressed),
                &mut target,
                |_| {
                    inspected = true;
                    Ok(fixture.identity.clone())
                },
                &mut |_| {}
            )
            .is_err()
        );
        assert!(!inspected);
    }
}

#[test]
fn bad_filesystem_identity_or_checker_mutation_never_produces_success() {
    let fixture = Fixture::new();
    for kind in ["checker-error", "wrong-build", "dirty", "checker-wrote"] {
        let mut target = FileTarget::new(fixture.image.expanded.bytes);
        assert!(
            write_image(
                &fixture.image,
                &mut Cursor::new(&fixture.compressed),
                &mut target,
                |file| {
                    let mut identity = fixture.identity.clone();
                    match kind {
                        "checker-error" => return Err(invalid("fixture fsck failed")),
                        "wrong-build" => {
                            identity.rootfs_tree = Sha256::digest(b"another");
                            identity.build_id = format!("rootfs-{}", identity.rootfs_tree);
                        }
                        "dirty" => {
                            identity.source.dirty = true;
                            identity.source.revision.push_str("-dirty");
                        }
                        "checker-wrote" => {
                            let mut file = file.try_clone().unwrap();
                            file.seek(SeekFrom::Start(9000)).unwrap();
                            file.write_all(b"oops").unwrap();
                        }
                        _ => unreachable!(),
                    }
                    Ok(identity)
                },
                &mut |_| {}
            )
            .is_err(),
            "{kind}"
        );
    }
    // The real checker must also reject filesystem damage, independently of
    // a matching signed byte hash or the presence of the embedded JSON.
    let mut corrupt = FileTarget::new(fixture.image.expanded.bytes);
    corrupt.file.write_all(&fixture.bytes).unwrap();
    corrupt.file.seek(SeekFrom::Start(2048)).unwrap();
    corrupt.file.write_all(&[0; 4096]).unwrap();
    assert!(ImageTools::default().check(&corrupt.file).is_err());
}

#[test]
fn staging_is_private_named_by_hash_and_rechecks_the_open_file_and_name() {
    let fixture = Fixture::new();
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let uid = unsafe { libc::geteuid() };
    let path = directory
        .path()
        .join(format!("{}.ext4.zst", fixture.image.compressed.sha256));
    let open = || {
        StagedPayload::under(
            open_directory(directory.path(), uid).unwrap(),
            uid,
            fixture.image.compressed.clone(),
        )
    };
    assert!(open().is_err());
    fs::write(&path, &fixture.compressed).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut staged = open().unwrap();
    staged.verify(&fixture.image).unwrap();
    fs::rename(&path, directory.path().join("old")).unwrap();
    fs::write(&path, &fixture.compressed).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(staged.reader().is_err());
    let mut staged = open().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(b"nope")
        .unwrap();
    assert!(staged.verify(&fixture.image).is_err());
    assert!(open().unwrap().verify(&fixture.image).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(open().is_err());
    fs::remove_file(&path).unwrap();
    symlink(directory.path().join("old"), &path).unwrap();
    assert!(open().is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(directory.path().join("old"), &path).unwrap();
    assert!(open().is_err());
}

#[test]
fn raw_write_requires_current_durable_writing_phase_and_matching_candidate() {
    let fixture = Fixture::new();
    let (record, mut boot) = crate::recovery::tests::writing(&fixture.image);
    let directory = tempfile::tempdir().unwrap();
    let mut store = RecoveryStore::initialize(&directory.path().join("journal"), record).unwrap();
    let id = store
        .durable_record()
        .unwrap()
        .inactive_write(&fixture.image, &boot)
        .unwrap();
    let mut changed = fixture.image.clone();
    changed.compressed.bytes += 1;
    assert!(store.record().inactive_write(&changed, &boot).is_err());
    boot.actual_slot = Some(crate::recovery::Slot::B);
    assert!(
        store
            .record()
            .inactive_write(&fixture.image, &boot)
            .is_err()
    );
    boot.actual_slot = Some(crate::recovery::Slot::A);
    store
        .advance(|r| {
            r.written(
                id,
                fixture.image.compressed.sha256,
                fixture.image.expanded.sha256,
                &boot,
            )
        })
        .unwrap();
    assert!(
        store
            .record()
            .inactive_write(&fixture.image, &boot)
            .is_err()
    );
}
