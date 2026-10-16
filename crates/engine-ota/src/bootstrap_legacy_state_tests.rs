use super::*;
use crate::recovery::Slot;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use xattr::FileExt;

pub(crate) struct Fixture {
    pub(crate) temp: tempfile::TempDir,
    pub(crate) root: File,
    pub(crate) data: File,
    pub(crate) parent: File,
    pub(crate) owner: u32,
    pub(crate) binding: BootstrapBinding,
    pub(crate) inputs: Inputs,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let data_path = temp.path().join("data");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o755)).unwrap();
        fs::create_dir_all(data_path.join("spacewars-bootstrap")).unwrap();
        fs::set_permissions(
            data_path.join("spacewars-bootstrap"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        for (index, (path, mode)) in legacy_files::targets().into_iter().enumerate() {
            if ![0, 1, 3, 5, 7, 8, 13].contains(&index) {
                continue;
            }
            let target = root_path.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let mut parent = target.parent().unwrap();
            while parent != root_path {
                fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
                parent = parent.parent().unwrap();
            }
            fs::write(
                &target,
                if index == 3 {
                    Vec::new()
                } else {
                    format!("original fixture {path}\n").into_bytes()
                },
            )
            .unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(mode)).unwrap();
        }
        let manager = File::open(root_path.join(legacy_files::MANAGER)).unwrap();
        manager
            .set_xattr("user.fixture", b"binary\0attribute")
            .unwrap();
        manager
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_789),
            ))
            .unwrap();
        let root = File::open(root_path).unwrap();
        let data = File::open(&data_path).unwrap();
        let parent = File::open(data_path.join("spacewars-bootstrap")).unwrap();
        let owner = unsafe { libc::geteuid() };
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
                        name.into(),
                        Blob {
                            bytes: 4,
                            sha256: Sha256::digest(b"file"),
                        },
                    )
                })
                .collect(),
        };
        super::super::state::begin(&parent, owner, &binding).unwrap();
        let inputs = Inputs {
            binding_sha256: Sha256::digest(&serde_json::to_vec(&binding).unwrap()),
            raw_copies_sha256: Sha256::digest(b"independently checked raw copies"),
            data_copy_sha256: Sha256::digest(b"independently checked data copy"),
            replacements: legacy_files::targets()
                .iter()
                .map(|(path, mode)| Replacement {
                    path: (*path).into(),
                    mode: *mode,
                    contents: Blob {
                        bytes: path.len() as u64,
                        sha256: Sha256::digest(path.as_bytes()),
                    },
                })
                .collect(),
        };
        Self {
            temp,
            root,
            data,
            parent,
            owner,
            binding,
            inputs,
        }
    }

    pub(crate) fn active(&self) -> std::path::PathBuf {
        self.temp.path().join("data/spacewars-bootstrap/active")
    }
    pub(crate) fn manager(&self) -> std::path::PathBuf {
        self.temp.path().join("root").join(legacy_files::MANAGER)
    }
    pub(crate) fn run(&self) -> Result<LegacyPreparationReport> {
        prepare(
            &self.root,
            &self.data,
            &self.parent,
            self.owner,
            &self.binding,
            &self.inputs,
            0,
            |_| Ok(()),
            &mut |_| Ok(()),
        )
    }

    fn export(&self) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::copy(
            self.active().join("binding.json"),
            temp.path().join("binding.json"),
        )
        .unwrap();
        let destination = temp.path().join(DIRECTORY);
        fs::create_dir(&destination).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        for component in names() {
            fs::copy(
                self.active().join(DIRECTORY).join(&component),
                destination.join(component),
            )
            .unwrap();
        }
        temp
    }
}

#[test]
fn originals_preserve_absence_empty_files_metadata_and_attributes_without_live_edits() {
    let fixture = Fixture::new();
    let before = legacy_files::inspect(&fixture.root, fixture.owner).unwrap();
    let result = fixture.run().unwrap();
    assert!(!result.reused && result.original_files_match && !result.ready_for_write);
    assert_eq!(
        before,
        legacy_files::inspect(&fixture.root, fixture.owner).unwrap()
    );
    let directory = File::open(fixture.active().join(DIRECTORY)).unwrap();
    let receipt = verify(&directory, fixture.owner, &fixture.binding).unwrap();
    assert_eq!(receipt.source, before);
    assert!(receipt.source.files[2].is_none());
    assert_eq!(receipt.source.files[3].as_ref().unwrap().contents.bytes, 0);
    let json = serde_json::to_value(&receipt).unwrap();
    assert_eq!(
        json["source"]["files"][0]["modified_nanoseconds"],
        123_456_789
    );
    assert_eq!(
        json["source"]["files"][0]["attributes"]["user.fixture"],
        serde_json::json!(b"binary\0attribute".to_vec())
    );
    for component in names() {
        assert_eq!(
            fs::metadata(fixture.active().join(DIRECTORY).join(component))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        fs::metadata(fixture.active().join(DIRECTORY))
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
    let exported = fixture.export();
    let checked = verify_legacy_export(exported.path(), result.receipt_sha256).unwrap();
    assert_eq!(checked.original_files, 7);
    assert_eq!(checked.original_bytes, result.original_bytes);
}

#[test]
fn repeated_preparation_keeps_originals_and_reports_later_operator_changes() {
    let fixture = Fixture::new();
    let first = fixture.run().unwrap();
    let original = fs::read(fixture.active().join(DIRECTORY).join("original-00.bin")).unwrap();
    let repeated = fixture.run().unwrap();
    assert!(repeated.reused && repeated.original_files_match);
    assert_eq!(first.receipt_sha256, repeated.receipt_sha256);
    fs::write(fixture.manager(), b"later operator edit").unwrap();
    let changed = fixture.run().unwrap();
    assert!(changed.reused && !changed.original_files_match);
    assert_eq!(first.receipt_sha256, changed.receipt_sha256);
    assert_eq!(
        fs::read(fixture.active().join(DIRECTORY).join("original-00.bin")).unwrap(),
        original
    );
}

#[test]
fn interruption_at_each_preservation_stage_can_resume_without_replacing_published_originals() {
    use LegacyPreparationStage::*;
    for stage in [Inspecting, Copying, Verifying, Publishing, Prepared] {
        let fixture = Fixture::new();
        let before = legacy_files::inspect(&fixture.root, fixture.owner).unwrap();
        let result = prepare(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            &fixture.inputs,
            0,
            |_| Ok(()),
            &mut |event| {
                if event.stage == stage && (stage != Copying || event.files_copied == 5) {
                    Err(invalid("fixture interruption"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err(), "{stage:?}");
        assert_eq!(
            before,
            legacy_files::inspect(&fixture.root, fixture.owner).unwrap()
        );
        assert_eq!(fixture.active().join(DIRECTORY).exists(), stage == Prepared);
        let resumed = fixture.run().unwrap();
        assert!(resumed.original_files_match);
        assert_eq!(resumed.reused, stage == Prepared);
        assert!(!fixture.active().join(PENDING).exists());
    }
}

#[test]
fn source_changes_during_copy_or_just_before_publication_never_publish_stale_originals() {
    for stage in [
        LegacyPreparationStage::Copying,
        LegacyPreparationStage::Publishing,
    ] {
        let fixture = Fixture::new();
        let mut changed = false;
        let result = prepare(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            &fixture.inputs,
            0,
            |_| Ok(()),
            &mut |event| {
                if event.stage == stage && !changed {
                    fs::write(fixture.manager(), b"changed during fixture capture").unwrap();
                    changed = true;
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!fixture.active().join(DIRECTORY).exists());
        assert!(fixture.active().join(PENDING).exists());
    }
}

#[test]
fn backup_guard_capacity_and_changed_inputs_do_not_grant_migration_authority() {
    let fixture = Fixture::new();
    assert!(
        prepare(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            &fixture.inputs,
            0,
            |initial| if initial {
                Err(invalid("fixture stale data backup"))
            } else {
                Ok(())
            },
            &mut |_| Ok(())
        )
        .is_err()
    );
    assert!(!fixture.active().join(PENDING).exists());
    assert!(
        prepare(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            &fixture.inputs,
            u64::MAX,
            |_| Ok(()),
            &mut |_| Ok(())
        )
        .is_err()
    );
    assert!(!fixture.active().join(DIRECTORY).exists());
    let original = fixture.run().unwrap();
    let mut changed = fixture.inputs.clone();
    changed.data_copy_sha256 = Sha256::digest(b"different data copy");
    assert!(
        prepare(
            &fixture.root,
            &fixture.data,
            &fixture.parent,
            fixture.owner,
            &fixture.binding,
            &changed,
            0,
            |_| Ok(()),
            &mut |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(
        fixture.run().unwrap().receipt_sha256,
        original.receipt_sha256
    );
}

#[test]
fn unknown_linked_or_unsafe_pending_files_are_rejected_before_any_truncation() {
    for kind in ["unknown", "symlink", "hardlink"] {
        let fixture = Fixture::new();
        let pending = fixture.active().join(PENDING);
        fs::create_dir(&pending).unwrap();
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap();
        let first = pending.join("original-00.bin");
        fs::write(&first, b"retain scratch").unwrap();
        fs::set_permissions(&first, fs::Permissions::from_mode(0o600)).unwrap();
        let outside = fixture.temp.path().join("outside");
        fs::write(&outside, b"retain outside").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
        match kind {
            "unknown" => fs::write(pending.join("unexpected"), b"retain").unwrap(),
            "symlink" => symlink(&outside, pending.join(RECORD)).unwrap(),
            _ => fs::hard_link(&outside, pending.join(RECORD)).unwrap(),
        }
        assert!(fixture.run().is_err());
        assert_eq!(fs::read(first).unwrap(), b"retain scratch");
        assert_eq!(fs::read(outside).unwrap(), b"retain outside");
    }
}

#[test]
fn changed_export_and_self_consistent_replacement_fail_the_separate_receipt_digest() {
    let fixture = Fixture::new();
    let result = fixture.run().unwrap();
    let exported = fixture.export();
    let file = exported.path().join(DIRECTORY).join("original-00.bin");
    let mut bytes = fs::read(&file).unwrap();
    bytes[0] ^= 1;
    fs::write(&file, &bytes).unwrap();
    assert!(verify_legacy_export(exported.path(), result.receipt_sha256).is_err());
    let path = exported.path().join(DIRECTORY).join(RECORD);
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope.receipt.source.files[0]
        .as_mut()
        .unwrap()
        .contents
        .sha256 = Sha256::digest(&bytes);
    envelope.checksum = digest(&envelope.receipt).unwrap();
    fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(verify_legacy_export(exported.path(), result.receipt_sha256).is_err());
    assert!(verify_legacy_export(&fixture.active(), result.receipt_sha256).is_ok());
}

#[test]
fn incomplete_or_unrecognized_published_records_are_never_reinitialized() {
    for missing in [false, true] {
        let fixture = Fixture::new();
        fixture.run().unwrap();
        let saved = fixture.active().join(DIRECTORY);
        let original_record = fs::read(saved.join(RECORD)).unwrap();
        if missing {
            // Even an absent original requires its checked empty placeholder.
            fs::remove_file(saved.join("original-02.bin")).unwrap();
        } else {
            fs::write(saved.join("unexpected"), b"retain for reconciliation").unwrap();
        }
        assert!(fixture.run().is_err());
        assert_eq!(fs::read(saved.join(RECORD)).unwrap(), original_record);
        assert!(!fixture.active().join(PENDING).exists());
    }
}

#[test]
#[ignore = "requires unshare and bind mounts in a private user/mount namespace"]
fn same_filesystem_directory_and_file_binds_are_rejected() {
    use std::process::Command;
    const CHILD: &str = "SPACEWARS_LEGACY_BIND_FIXTURE";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount", "--fork", "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "bootstrap::legacy_state::tests::same_filesystem_directory_and_file_binds_are_rejected", "--nocapture"])
            .env(CHILD, "1")
            .env("SPACEWARS_LEGACY_PARENT_USER_NS", fs::read_link("/proc/self/ns/user").unwrap())
            .env("SPACEWARS_LEGACY_PARENT_MOUNT_NS", fs::read_link("/proc/self/ns/mnt").unwrap())
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/user").unwrap(),
        std::path::PathBuf::from(std::env::var_os("SPACEWARS_LEGACY_PARENT_USER_NS").unwrap())
    );
    assert_ne!(
        fs::read_link("/proc/self/ns/mnt").unwrap(),
        std::path::PathBuf::from(std::env::var_os("SPACEWARS_LEGACY_PARENT_MOUNT_NS").unwrap())
    );
    assert!(
        Command::new("mount")
            .args(["--make-rprivate", "/"])
            .status()
            .unwrap()
            .success()
    );
    let fixture = Fixture::new();
    let manager = fixture.manager();
    for path in [manager.parent().unwrap(), manager.as_path()] {
        let original = fs::metadata(path).unwrap();
        assert!(
            Command::new("mount")
                .arg("--bind")
                .arg(path)
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(original.dev(), fs::metadata(path).unwrap().dev());
        assert_eq!(original.ino(), fs::metadata(path).unwrap().ino());
        assert!(fixture.run().is_err());
        assert!(!fixture.active().join(DIRECTORY).exists());
        assert!(Command::new("umount").arg(path).status().unwrap().success());
    }
    fixture.run().unwrap();
}

#[test]
fn source_links_writable_parents_and_special_files_are_not_captured() {
    for kind in ["file-link", "parent-link", "hardlink", "writable", "fifo"] {
        let fixture = Fixture::new();
        let source = fixture.manager();
        match kind {
            "file-link" => {
                fs::remove_file(&source).unwrap();
                symlink("/dev/zero", &source).unwrap();
            }
            "parent-link" => {
                let directory = source.parent().unwrap();
                let moved = fixture.temp.path().join("outside-bin");
                fs::rename(directory, &moved).unwrap();
                symlink(moved, directory).unwrap();
            }
            "hardlink" => {
                fs::hard_link(&source, fixture.temp.path().join("outside-link")).unwrap();
            }
            "writable" => {
                fs::set_permissions(source.parent().unwrap(), fs::Permissions::from_mode(0o777))
                    .unwrap();
            }
            _ => {
                use std::os::unix::ffi::OsStrExt;
                fs::remove_file(&source).unwrap();
                let name = std::ffi::CString::new(source.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
        }
        assert!(fixture.run().is_err(), "{kind}");
        assert!(!fixture.active().join(DIRECTORY).exists());
    }
}
