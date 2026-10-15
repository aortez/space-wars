use super::*;
use crate::recovery::Slot;
use std::collections::BTreeMap;
use std::io::{Cursor, SeekFrom};
use std::os::unix::fs::{PermissionsExt, symlink};

/// Optional size/format exercise using real build artifacts. The binding is
/// synthetic and both targets are temporary regular files, not live devices.
#[test]
#[ignore = "requires SPACEWARS_BOOTSTRAP_TEST_ARTIFACT_DIR containing boot.fat and rootfs.ext4"]
fn actual_build_artifacts_roundtrip_into_private_raw_recovery_copies() {
    use std::os::unix::fs::OpenOptionsExt;
    let directory = std::path::PathBuf::from(
        std::env::var_os("SPACEWARS_BOOTSTRAP_TEST_ARTIFACT_DIR").unwrap(),
    );
    let open = |name: &str| {
        let pinned = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW)
            .open(directory.join(name))
            .unwrap();
        let metadata = pinned.metadata().unwrap();
        assert!(
            metadata.is_file()
                && metadata.nlink() == 1
                && metadata.len() > 0
                && metadata.len() <= super::super::package::MAX_ROOT_BYTES
        );
        (
            File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd())).unwrap(),
            metadata,
        )
    };
    let (boot_source, boot_metadata) = open("boot.fat");
    let (root_source, root_metadata) = open("rootfs.ext4");
    let (temp, parent, owner, mut binding, _) = fixture();
    binding.boot_partition_bytes = boot_metadata.len();
    binding.inactive_partition_bytes = root_metadata.len();
    super::super::state::begin(&parent, owner, &binding).unwrap();
    let active = File::open(temp.path().join("active")).unwrap();
    let copies = super::directory(&active, DIRECTORY, owner).unwrap();
    let (boot, _) = copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::Boot,
        boot_source.try_clone().unwrap(),
        &mut |_| {},
        || {
            assert!(unchanged(&boot_metadata, &boot_source.metadata()?));
            Ok(())
        },
    )
    .unwrap();
    let (inactive, _) = copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::InactiveRoot,
        root_source.try_clone().unwrap(),
        &mut |_| {},
        || {
            assert!(unchanged(&root_metadata, &root_source.metadata()?));
            Ok(())
        },
    )
    .unwrap();
    let expected = copies_digest(&boot, &inactive).unwrap();
    let checked = verify_backup_export(&temp.path().join("active"), expected).unwrap();
    println!(
        "{}",
        serde_json::json!({"scope":"actual build artifacts with a synthetic device binding and temporary regular-file targets; no live device access", "boot":boot.image,"inactive_root":inactive.image,"export_check":checked})
    );
}

fn binding() -> BootstrapBinding {
    BootstrapBinding {
        schema: 1,
        package_sha256: Sha256::digest(b"package"),
        device_id: Sha256::digest(b"device"),
        layout_id: Sha256::digest(b"layout"),
        original_boot_id: "de606400-dc75-446e-b26b-385592b0f562".into(),
        original_slot: Slot::B,
        target: Slot::A,
        kernel_build_id: "a".repeat(40),
        hardware_profile: "hyperpixel".into(),
        boot_partition_bytes: 128 * 1024,
        inactive_partition_bytes: 256 * 1024,
        original_boot_files: ["config.txt", "cmdline.txt", "spacewars-device.txt"]
            .into_iter()
            .map(|p| {
                (
                    p.into(),
                    Blob {
                        bytes: 4,
                        sha256: Sha256::digest(b"file"),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    }
}

fn fixture() -> (tempfile::TempDir, File, u32, BootstrapBinding, Vec<u8>) {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let file = File::open(temp.path()).unwrap();
    let owner = unsafe { libc::geteuid() };
    let binding = binding();
    let bytes = (0..binding.boot_partition_bytes)
        .map(|i| (i % 251) as u8)
        .collect();
    (temp, file, owner, binding, bytes)
}

#[test]
fn completed_copies_are_verified_immutable_and_bound_to_the_original_attempt() {
    let (temp, parent, owner, binding, bytes) = fixture();
    let (receipt, reused) = copy_one(
        &parent,
        owner,
        &binding,
        BackupPartition::Boot,
        Cursor::new(&bytes),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    assert!(!reused);
    assert_eq!(
        receipt.image,
        Blob {
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes)
        }
    );
    let path = temp.path().join("boot/partition.img");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert!(!temp.path().join(".boot.pending").exists());
    // The saved original must not be refreshed from different live bytes.
    let (again, reused) = copy_one(
        &parent,
        owner,
        &binding,
        BackupPartition::Boot,
        Cursor::new(b"different source"),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    assert!(reused);
    assert_eq!(again, receipt);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut other = binding.clone();
    other.package_sha256 = Sha256::digest(b"other package");
    assert!(
        copy_one(
            &parent,
            owner,
            &other,
            BackupPartition::Boot,
            Cursor::new(&bytes),
            &mut |_| {},
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn interruptions_before_and_after_publication_resume_without_replacing_completed_copies() {
    for boundary in ["partial-copy", "before-publish", "after-publish"] {
        let (temp, parent, owner, binding, bytes) = fixture();
        let mut checks = 0;
        let source = if boundary == "partial-copy" {
            &bytes[..70_000]
        } else {
            bytes.as_slice()
        };
        let result = copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            Cursor::new(source),
            &mut |_| {},
            || {
                checks += 1;
                if checks == 3 {
                    Err(invalid("injected interruption before publish"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err(), "{boundary}");
        assert!(!temp.path().join("boot").exists());
        assert!(temp.path().join(".boot.pending/partition.img").is_file());
        if boundary == "after-publish" {
            // Model process loss after the atomic directory rename, before
            // the caller observes the final directory flush/result.
            fs::rename(temp.path().join(".boot.pending"), temp.path().join("boot")).unwrap();
        }
        let (_, reused) = copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            Cursor::new(&bytes),
            &mut |_| {},
            || Ok(()),
        )
        .unwrap();
        assert_eq!(reused, boundary == "after-publish");
        assert_eq!(
            fs::read(temp.path().join("boot/partition.img")).unwrap(),
            bytes
        );
        assert!(!temp.path().join(".boot.pending").exists());
    }
}

#[test]
fn damaged_completed_copies_are_never_overwritten_or_credited() {
    for failure in [
        "bytes",
        "record",
        "mode",
        "hard-link",
        "missing-record",
        "extra",
    ] {
        let (temp, parent, owner, binding, bytes) = fixture();
        copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            Cursor::new(&bytes),
            &mut |_| {},
            || Ok(()),
        )
        .unwrap();
        let image = temp.path().join("boot/partition.img");
        let record = temp.path().join("boot/record.json");
        match failure {
            "bytes" => fs::write(&image, vec![0; bytes.len()]).unwrap(),
            "record" => fs::write(&record, b"{}\n").unwrap(),
            "mode" => fs::set_permissions(&image, fs::Permissions::from_mode(0o644)).unwrap(),
            "hard-link" => fs::hard_link(&image, temp.path().join("alias")).unwrap(),
            "missing-record" => fs::remove_file(&record).unwrap(),
            "extra" => fs::write(temp.path().join("boot/unknown"), b"unknown").unwrap(),
            _ => unreachable!(),
        }
        let before = fs::read(&image).unwrap();
        assert!(
            copy_one(
                &parent,
                owner,
                &binding,
                BackupPartition::Boot,
                Cursor::new(&bytes),
                &mut |_| {},
                || Ok(())
            )
            .is_err(),
            "{failure}"
        );
        assert_eq!(fs::read(image).unwrap(), before);
        assert!(!temp.path().join(".boot.pending").exists());
    }
}

#[test]
fn pending_links_and_unknown_files_do_not_redirect_or_erase_other_content() {
    for failure in [
        "image-link",
        "record-link",
        "directory-link",
        "unknown",
        "oversized",
    ] {
        let (temp, parent, owner, binding, bytes) = fixture();
        let victim = temp.path().join("untouched");
        fs::write(&victim, b"keep").unwrap();
        let pending = temp.path().join(".boot.pending");
        if failure == "directory-link" {
            symlink(&victim, &pending).unwrap();
        } else {
            fs::create_dir(&pending).unwrap();
            fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap();
            match failure {
                "image-link" => symlink(&victim, pending.join(IMAGE)).unwrap(),
                "record-link" => symlink(&victim, pending.join(RECORD)).unwrap(),
                "unknown" => fs::write(pending.join("unknown"), b"keep too").unwrap(),
                "oversized" => {
                    fs::write(pending.join(IMAGE), vec![0; bytes.len() + 1]).unwrap();
                    fs::set_permissions(pending.join(IMAGE), fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                _ => unreachable!(),
            }
        }
        assert!(
            copy_one(
                &parent,
                owner,
                &binding,
                BackupPartition::Boot,
                Cursor::new(&bytes),
                &mut |_| {},
                || Ok(())
            )
            .is_err(),
            "{failure}"
        );
        assert_eq!(fs::read(victim).unwrap(), b"keep");
        assert!(!temp.path().join("boot").exists());
    }
}

struct ChangingSource {
    cursor: Cursor<Vec<u8>>,
    rewinds: u32,
}
impl Read for ChangingSource {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(bytes)
    }
}
impl Seek for ChangingSource {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        if position == SeekFrom::Start(0) {
            self.rewinds += 1;
            if self.rewinds > 1 {
                self.cursor.get_mut()[0] ^= 1;
            }
        }
        self.cursor.seek(position)
    }
}

#[test]
fn changed_or_unbounded_source_and_readback_damage_never_publish_a_copy() {
    let (temp, parent, owner, binding, bytes) = fixture();
    let source = ChangingSource {
        cursor: Cursor::new(bytes.clone()),
        rewinds: 0,
    };
    assert!(
        copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            source,
            &mut |_| {},
            || Ok(())
        )
        .is_err()
    );
    assert!(!temp.path().join("boot").exists());
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(
        copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            Cursor::new(extra),
            &mut |_| {},
            || Ok(())
        )
        .is_err()
    );
    assert!(!temp.path().join("boot").exists());
    let path = temp.path().join(".boot.pending/partition.img");
    assert!(
        copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::Boot,
            Cursor::new(&bytes),
            &mut |progress| {
                if matches!(progress.stage, BackupStage::CheckingSource) && progress.bytes == 0 {
                    fs::write(&path, vec![0; bytes.len()]).unwrap();
                }
            },
            || Ok(())
        )
        .is_err()
    );
    assert!(!temp.path().join("boot").exists());
    let mut output = std::io::sink();
    for length in [0, super::super::package::MAX_ROOT_BYTES + 1] {
        assert!(
            stream(
                &mut Cursor::new(&bytes),
                &mut output,
                length,
                BackupPartition::Boot,
                BackupStage::Copying,
                &mut |_| {}
            )
            .is_err()
        );
    }
}

#[test]
fn only_completed_verified_copies_reduce_the_resume_space_requirement() {
    let (temp, parent, owner, binding, bytes) = fixture();
    super::super::state::begin(&parent, owner, &binding).unwrap();
    assert_eq!(verified_bytes(&parent, owner, &binding).unwrap(), 0);
    let active = File::open(temp.path().join("active")).unwrap();
    let copies = directory(&active, DIRECTORY, owner).unwrap();
    copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::Boot,
        Cursor::new(&bytes),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        verified_bytes(&parent, owner, &binding).unwrap(),
        binding.boot_partition_bytes
    );
    let inactive = vec![5; binding.inactive_partition_bytes as usize];
    copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::InactiveRoot,
        Cursor::new(&inactive),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        verified_bytes(&parent, owner, &binding).unwrap(),
        binding.boot_partition_bytes + binding.inactive_partition_bytes
    );
    fs::write(
        temp.path()
            .join("active/recovery-copies/boot/partition.img"),
        b"damaged",
    )
    .unwrap();
    assert!(verified_bytes(&parent, owner, &binding).is_err());
}

#[test]
fn reclaiming_pending_space_preserves_completed_copies_and_refuses_unsafe_scratch() {
    let (temp, parent, owner, binding, bytes) = fixture();
    copy_one(
        &parent,
        owner,
        &binding,
        BackupPartition::Boot,
        Cursor::new(&bytes),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    let mut checks = 0;
    let inactive = vec![7; binding.inactive_partition_bytes as usize];
    assert!(
        copy_one(
            &parent,
            owner,
            &binding,
            BackupPartition::InactiveRoot,
            Cursor::new(&inactive),
            &mut |_| {},
            || {
                checks += 1;
                if checks == 3 {
                    Err(invalid("interrupted"))
                } else {
                    Ok(())
                }
            }
        )
        .is_err()
    );
    let path = temp.path().join(".inactive-root.pending/partition.img");
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        binding.inactive_partition_bytes
    );
    reset_pending(&parent, owner, &binding).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    assert_eq!(
        fs::read(temp.path().join("boot/partition.img")).unwrap(),
        bytes
    );
    fs::write(&path, b"partial").unwrap();
    symlink(
        "missing",
        temp.path().join(".inactive-root.pending/record-alias"),
    )
    .unwrap();
    assert!(reset_pending(&parent, owner, &binding).is_err());
    assert_eq!(fs::read(path).unwrap(), b"partial");
}

#[test]
fn independent_export_must_match_the_device_digest_even_after_self_consistent_tampering() {
    let (temp, parent, owner, binding, bytes) = fixture();
    super::super::state::begin(&parent, owner, &binding).unwrap();
    let active = File::open(temp.path().join("active")).unwrap();
    let copies = directory(&active, DIRECTORY, owner).unwrap();
    let (boot, _) = copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::Boot,
        Cursor::new(&bytes),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    let (inactive, _) = copy_one(
        &copies,
        owner,
        &binding,
        BackupPartition::InactiveRoot,
        Cursor::new(vec![3; binding.inactive_partition_bytes as usize]),
        &mut |_| {},
        || Ok(()),
    )
    .unwrap();
    let expected = copies_digest(&boot, &inactive).unwrap();
    let export = temp.path().join("independent-export");
    for name in [
        "",
        "recovery-copies",
        "recovery-copies/boot",
        "recovery-copies/inactive-root",
    ] {
        fs::create_dir(export.join(name)).unwrap();
        fs::set_permissions(export.join(name), fs::Permissions::from_mode(0o700)).unwrap();
    }
    for name in [
        "binding.json",
        "recovery-copies/boot/partition.img",
        "recovery-copies/boot/record.json",
        "recovery-copies/inactive-root/partition.img",
        "recovery-copies/inactive-root/record.json",
    ] {
        fs::copy(temp.path().join("active").join(name), export.join(name)).unwrap();
    }
    let result = verify_backup_export(&export, expected).unwrap();
    assert_eq!(result.package_sha256, binding.package_sha256);
    assert_eq!(result.boot_bytes, binding.boot_partition_bytes);
    assert!(verify_backup_export(&export, Sha256::digest(b"wrong report")).is_err());
    let changed = vec![4; bytes.len()];
    fs::write(export.join("recovery-copies/boot/partition.img"), &changed).unwrap();
    let path = export.join("recovery-copies/boot/record.json");
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope.receipt.image.sha256 = Sha256::digest(&changed);
    envelope.checksum = Sha256::digest(&serde_json::to_vec(&envelope.receipt).unwrap());
    fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(
        verify_backup_export(&export, expected)
            .unwrap_err()
            .to_string()
            .contains("independently selected")
    );
    assert_eq!(
        fs::read(
            temp.path()
                .join("active/recovery-copies/boot/partition.img")
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn a_readonly_view_of_a_writable_superblock_is_not_a_backup_window() {
    let device = DeviceNumber { major: 8, minor: 1 };
    let good = "24 1 8:1 / /boot ro,nosuid - vfat /dev/sda1 ro,fmask=0022\n";
    read_only_mounts(good, device).unwrap();
    for bad in [
        good.replace("ro,fmask", "rw,fmask"),
        good.replace("ro,nosuid", "rw,nosuid"),
        good.replace("ro,nosuid", "ro,rw,nosuid"),
        good.replace("vfat", "ext4"),
        good.replace("8:1", "8:9"),
        "incomplete".into(),
        format!("{good}25 1 8:1 / /another rw - vfat /dev/sda1 rw\n"),
    ] {
        assert!(read_only_mounts(&bad, device).is_err(), "{bad}");
    }
    let (_temp, file, _owner, _binding, _bytes) = fixture();
    assert!(
        RawSource::node(
            &file.metadata().unwrap(),
            &Partition {
                number: 1,
                name: "sda1".into(),
                device,
                start_sector: 8192,
                sectors: 307200,
                partuuid: "076c4a2a-01".into()
            }
        )
        .is_err()
    );
}
