use super::*;
use crate::recovery::Slot;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    temp: tempfile::TempDir,
    root: File,
    data: File,
    parent: File,
    owner: u32,
    binding: BootstrapBinding,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for path in ["data/spacewars/config", "data/spacewars-bootstrap", "etc"] {
            fs::create_dir_all(temp.path().join(path)).unwrap();
        }
        fs::set_permissions(
            temp.path().join("data/spacewars-bootstrap"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::write(
            temp.path().join("data/spacewars/config/settings.json"),
            b"synthetic saved settings",
        )
        .unwrap();
        fs::write(temp.path().join("etc/hostname"), b"fixture\n").unwrap();
        let owner = unsafe { libc::geteuid() };
        let root = File::open(temp.path()).unwrap();
        let data = File::open(temp.path().join("data")).unwrap();
        let parent = File::open(temp.path().join("data/spacewars-bootstrap")).unwrap();
        let binding = BootstrapBinding {
            schema: 1,
            package_sha256: Sha256::digest(b"package"),
            device_id: Sha256::digest(b"device"),
            layout_id: Sha256::digest(b"layout"),
            original_boot_id: "de606400-dc75-446e-b26b-385592b0f562".into(),
            original_slot: Slot::B,
            target: Slot::A,
            kernel_build_id: "a".repeat(40),
            hardware_profile: "hyperpixel".into(),
            boot_partition_bytes: 150 * 1024 * 1024,
            inactive_partition_bytes: 3 * 1024 * 1024 * 1024,
            original_boot_files: ["config.txt", "cmdline.txt", "spacewars-device.txt"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        Blob {
                            bytes: 4,
                            sha256: Sha256::digest(b"file"),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        };
        super::super::state::begin(&parent, owner, &binding).unwrap();
        Self {
            temp,
            root,
            data,
            parent,
            owner,
            binding,
        }
    }
    fn active(&self) -> std::path::PathBuf {
        self.temp.path().join("data/spacewars-bootstrap/active")
    }
    fn run(&self) -> Result<DataBackupReport> {
        create(
            &self.root,
            &self.data,
            &self.parent,
            self.owner,
            &self.binding,
            256 * 1024 * 1024,
            || Ok(()),
            &mut |_| {},
        )
    }
    fn export(&self) -> tempfile::TempDir {
        let output = tempfile::tempdir().unwrap();
        fs::set_permissions(output.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::copy(
            self.active().join("binding.json"),
            output.path().join("binding.json"),
        )
        .unwrap();
        let directory = output.path().join(DIRECTORY);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        for component in [ARCHIVE, RECORD] {
            fs::copy(
                self.active().join(DIRECTORY).join(component),
                directory.join(component),
            )
            .unwrap();
        }
        output
    }
}

#[test]
fn durable_data_copy_reuses_original_and_reports_later_source_changes() {
    let fixture = Fixture::new();
    let result = fixture.run().unwrap();
    assert!(!result.reused && result.source_matches && !result.ready_for_write);
    let archive = fixture.active().join(DIRECTORY).join(ARCHIVE);
    let original = fs::read(&archive).unwrap();
    assert_eq!(fs::metadata(&archive).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(archive.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    let exported = fixture.export();
    let checked = verify_data_backup_export(exported.path(), result.receipt_sha256).unwrap();
    assert_eq!(checked.archive_bytes, original.len() as u64);
    let again = fixture.run().unwrap();
    assert!(again.reused && again.source_matches);
    assert_eq!(again.receipt, result.receipt);
    fs::write(
        fixture
            .temp
            .path()
            .join("data/spacewars/config/settings.json"),
        b"later user data",
    )
    .unwrap();
    let changed = fixture.run().unwrap();
    assert!(changed.reused && !changed.source_matches);
    assert_eq!(fs::read(&archive).unwrap(), original);
}

#[test]
fn source_change_during_copy_leaves_only_unpublished_scratch_and_can_retry() {
    let fixture = Fixture::new();
    let mut changed = false;
    let result = create(
        &fixture.root,
        &fixture.data,
        &fixture.parent,
        fixture.owner,
        &fixture.binding,
        0,
        || Ok(()),
        &mut |progress| {
            if matches!(progress.stage, DataBackupStage::Copying) && progress.bytes > 0 && !changed
            {
                changed = true;
                fs::write(
                    fixture.temp.path().join("etc/hostname"),
                    b"changed fixture\n",
                )
                .unwrap();
            }
        },
    );
    assert!(result.is_err());
    assert!(!fixture.active().join(DIRECTORY).exists());
    assert!(fixture.active().join(PENDING).exists());
    let retry = fixture.run().unwrap();
    assert!(!retry.reused && retry.source_matches);
    assert!(!fixture.active().join(PENDING).exists());
}

#[test]
fn source_guard_and_capacity_failure_never_publish_a_data_copy() {
    let fixture = Fixture::new();
    assert!(
        create(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            0,
            || Err(invalid("fixture writer active")),
            &mut |_| {}
        )
        .is_err()
    );
    assert!(!fixture.active().join(PENDING).exists());
    assert!(
        create(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            u64::MAX,
            || Ok(()),
            &mut |_| {}
        )
        .is_err()
    );
    assert!(!fixture.active().join(DIRECTORY).exists());
    fixture.run().unwrap();
}

#[test]
fn changed_or_self_consistently_replaced_export_cannot_match_separate_digest() {
    let fixture = Fixture::new();
    let result = fixture.run().unwrap();
    let exported = fixture.export();
    assert!(verify_data_backup_export(exported.path(), Sha256::digest(b"wrong digest")).is_err());
    let archive_path = exported.path().join(DIRECTORY).join(ARCHIVE);
    let mut bytes = fs::read(&archive_path).unwrap();
    bytes[0] ^= 1;
    fs::write(&archive_path, &bytes).unwrap();
    assert!(verify_data_backup_export(exported.path(), result.receipt_sha256).is_err());
    let record_path = exported.path().join(DIRECTORY).join(RECORD);
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    envelope.receipt.archive.sha256 = Sha256::digest(&bytes);
    envelope.checksum = receipt_digest(&envelope.receipt).unwrap();
    fs::write(&record_path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(verify_data_backup_export(exported.path(), result.receipt_sha256).is_err());
    assert!(verify_data_backup_export(&fixture.active(), result.receipt_sha256).is_ok());
}

#[test]
fn unknown_or_unsafe_receipt_prevents_truncating_unpublished_archive() {
    for unsafe_record in [false, true] {
        let fixture = Fixture::new();
        let pending = fixture.active().join(PENDING);
        fs::create_dir(&pending).unwrap();
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap();
        let archive = pending.join(ARCHIVE);
        fs::write(&archive, b"old scratch bytes").unwrap();
        fs::set_permissions(&archive, fs::Permissions::from_mode(0o600)).unwrap();
        if unsafe_record {
            symlink(&archive, pending.join(RECORD)).unwrap();
        } else {
            fs::write(pending.join("unrecognized"), b"retain").unwrap();
        }
        assert!(fixture.run().is_err());
        assert_eq!(fs::read(archive).unwrap(), b"old scratch bytes");
    }
}

#[test]
fn unsafe_pending_and_completed_paths_are_never_followed_or_overwritten() {
    for completed in [false, true] {
        let fixture = Fixture::new();
        let outside = fixture.temp.path().join("outside");
        fs::write(&outside, b"retain me").unwrap();
        if completed {
            fixture.run().unwrap();
            fs::remove_file(fixture.active().join(DIRECTORY).join(ARCHIVE)).unwrap();
        } else {
            fs::create_dir(fixture.active().join(PENDING)).unwrap();
            fs::set_permissions(
                fixture.active().join(PENDING),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        symlink(
            &outside,
            fixture
                .active()
                .join(if completed { DIRECTORY } else { PENDING })
                .join(ARCHIVE),
        )
        .unwrap();
        assert!(fixture.run().is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"retain me");
    }
}
