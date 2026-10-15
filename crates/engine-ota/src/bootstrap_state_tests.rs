use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

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
    }
}

fn fixture() -> (tempfile::TempDir, File, File, u32) {
    let temp = tempfile::tempdir().unwrap();
    let data = File::open(temp.path()).unwrap();
    let path = temp.path().join(super::super::device::DIRECTORY);
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    (temp, data, File::open(path).unwrap(), unsafe {
        libc::geteuid()
    })
}

#[test]
fn preparation_is_durable_and_idempotent_across_a_legacy_reboot() {
    let (temp, data, parent, owner) = fixture();
    require_no_active_under(&data, owner).unwrap();
    let expected = binding();
    assert!(existing(&parent, owner, &expected).unwrap().is_none());
    let (record, resumed) = begin(&parent, owner, &expected).unwrap();
    assert!(!resumed);
    assert_eq!(record, expected);
    assert!(require_no_active_under(&data, owner).is_err());
    let path = temp.path().join("spacewars-bootstrap/active/binding.json");
    let original = fs::read(&path).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    let mut current = expected.clone();
    current.original_boot_id = "aa606400-dc75-446e-b26b-385592b0f562".into();
    drop(parent);
    let reopened = File::open(temp.path().join("spacewars-bootstrap")).unwrap();
    let (record, resumed) = begin(&reopened, owner, &current).unwrap();
    assert!(resumed);
    assert_eq!(record, expected);
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn changed_package_device_layout_slot_or_boot_is_not_a_new_attempt() {
    let (temp, _, parent, owner) = fixture();
    let expected = binding();
    begin(&parent, owner, &expected).unwrap();
    let path = temp.path().join("spacewars-bootstrap/active/binding.json");
    let original = fs::read(&path).unwrap();
    let mut changes = Vec::new();
    for field in 0..7 {
        let mut current = expected.clone();
        match field {
            0 => current.package_sha256 = Sha256::digest(b"other package"),
            1 => current.device_id = Sha256::digest(b"other device"),
            2 => current.layout_id = Sha256::digest(b"other layout"),
            3 => {
                current.original_slot = Slot::A;
                current.target = Slot::B;
            }
            4 => {
                current
                    .original_boot_files
                    .get_mut("config.txt")
                    .unwrap()
                    .sha256 = Sha256::digest(b"other config")
            }
            5 => current.kernel_build_id = "b".repeat(40),
            6 => current.inactive_partition_bytes -= 512,
            _ => unreachable!(),
        }
        changes.push(current);
    }
    for current in changes {
        assert!(begin(&parent, owner, &current).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}

#[test]
fn incomplete_or_corrupt_reservations_never_reinitialize_or_release_maintenance() {
    for corruption in 0..7 {
        let (temp, data, parent, owner) = fixture();
        let expected = binding();
        begin(&parent, owner, &expected).unwrap();
        let active = temp.path().join("spacewars-bootstrap/active");
        let path = active.join(RECORD);
        match corruption {
            0 => fs::remove_file(&path).unwrap(),
            1 => fs::write(&path, b"{\"binding\":").unwrap(),
            2 => {
                let mut record: Envelope =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                record.checksum = Sha256::digest(b"corrupt");
                fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            }
            3 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            4 => {
                fs::remove_file(&path).unwrap();
                symlink("missing", &path).unwrap();
            }
            5 => fs::write(active.join("unexpected"), b"state").unwrap(),
            6 => fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap(),
            _ => unreachable!(),
        }
        let before = fs::read(&path).ok();
        assert!(begin(&parent, owner, &expected).is_err());
        assert!(require_no_active_under(&data, owner).is_err());
        assert_eq!(fs::read(path).ok(), before);
    }
}

#[test]
fn unsafe_state_types_or_ownership_block_both_prepare_and_maintenance() {
    for variant in 0..4 {
        let (temp, data, parent, owner) = fixture();
        let path = temp.path().join("spacewars-bootstrap/active");
        match variant {
            0 => symlink("missing", &path).unwrap(),
            1 => fs::write(&path, b"not a directory").unwrap(),
            2 => {
                fs::create_dir(&path).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            3 => {
                begin(&parent, owner, &binding()).unwrap();
                assert!(existing(&parent, owner.wrapping_add(1), &binding()).is_err());
                fs::set_permissions(
                    temp.path().join("spacewars-bootstrap"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(begin(&parent, owner, &binding()).is_err());
        assert!(require_no_active_under(&data, owner).is_err());
    }
}
