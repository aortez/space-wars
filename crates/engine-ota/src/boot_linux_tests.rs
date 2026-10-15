use super::*;
use crate::boot_files::tests::{Fixture, roots};
use crate::recovery::Slot;
use crate::recovery_linux::Partition;
use std::fs;

const MEDIUM: &str = "E:ID_BUS=usb\nE:ID_TYPE=disk\nE:ID_SERIAL=fixture-unique-usb-serial\nE:ID_VENDOR=Fixture\nE:ID_MODEL=Storage\nE:ID_REVISION=1.00\n";

fn physical(record: &str) -> Result<PhysicalIdentity> {
    PhysicalIdentity::parse(
        "Raspberry Pi 5 Model B Rev 1.1".into(),
        "abcdef0123456789".into(),
        "b".repeat(40),
        100_000_000,
        record,
    )
}

fn runtime(fixture: &Fixture, files: &BootFiles) -> Runtime {
    let mut image =
        ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json")).unwrap();
    image.boot_identity = fixture.identity;
    let geometry = [
        (8192, 307200),
        (319488, 6291456),
        (6610944, 6291456),
        (12902400, 2097152),
    ];
    let partitions = std::array::from_fn(|index| Partition {
        number: index as u32 + 1,
        name: format!("sda{}", index + 1),
        device: DeviceNumber {
            major: 8,
            minor: index as u32 + 1,
        },
        start_sector: geometry[index].0,
        sectors: geometry[index].1,
        partuuid: format!("076c4a2a-{:02}", index + 1),
    });
    Runtime {
        physical: physical(MEDIUM).unwrap(),
        image,
        layout: DiskLayout {
            disk_name: "sda".into(),
            disk_device: DeviceNumber { major: 8, minor: 0 },
            disk_sectors: 100_000_000,
            active: Slot::A,
            partitions,
            layout_id: Sha256::digest(b"fixture layout"),
        },
        kernel: "a".repeat(40),
        profile: "hdmi".into(),
        command_line: format!(
            "reboot=w {}",
            files.template().render(&roots(), Slot::A, None)
        ),
        boot_id: "11111111-1111-1111-1111-111111111111".into(),
        uptime_ms: 20_000,
        tryboot: false,
    }
}

#[test]
fn qualification_identity_changes_with_board_firmware_or_usb_medium() {
    let original = physical(MEDIUM).unwrap();
    let expected = original.fingerprint();
    let mut changed = original.clone();
    changed.serial = "fedcba9876543210".into();
    assert_ne!(changed.fingerprint(), expected);
    let mut changed = original.clone();
    changed.bootloader_revision = "c".repeat(40);
    assert_ne!(changed.fingerprint(), expected);
    let mut changed = original.clone();
    changed
        .medium
        .insert("ID_SERIAL".into(), "other-medium".into());
    assert_ne!(changed.fingerprint(), expected);
    let mut changed = original.clone();
    changed.medium_sectors += 1;
    assert_ne!(changed.fingerprint(), expected);
    let mut changed = original.clone();
    changed.model = "Raspberry Pi 4 Model B Rev 1.4".into();
    assert_ne!(changed.fingerprint(), expected);
    assert_eq!(
        physical(&format!("{MEDIUM}E:ID_PATH=enumeration-may-change\n"))
            .unwrap()
            .fingerprint(),
        expected
    );
    for invalid in [
        MEDIUM.replace("E:ID_SERIAL=fixture-unique-usb-serial\n", ""),
        MEDIUM.replace("ID_BUS=usb", "ID_BUS=pci"),
        format!("{MEDIUM}E:ID_SERIAL=duplicate\n"),
        MEDIUM.replace("Storage", "bad\0model"),
    ] {
        assert!(physical(&invalid).is_err());
    }
    assert!(
        PhysicalIdentity::parse(
            original.model,
            "0".repeat(16),
            original.bootloader_revision,
            100_000_000,
            MEDIUM
        )
        .is_err()
    );
}

#[test]
fn disk_files_cannot_substitute_for_loaded_kernel_profile_or_firmware_trial_evidence() {
    let fixture = Fixture::new();
    let mut files = fixture.open().unwrap();
    let original = runtime(&fixture, &files);
    let normal = original.evidence(&files).unwrap();
    assert_eq!(normal.observation.actual_slot, Some(Slot::A));
    assert!(!normal.observation.tryboot);
    let mut changed = original.clone();
    changed.kernel = "c".repeat(40);
    assert!(changed.evidence(&files).is_err());
    let mut changed = original.clone();
    changed.profile = "hyperpixel".into();
    assert!(changed.evidence(&files).is_err());
    let mut changed = original.clone();
    changed.image.boot_identity = Sha256::digest(b"different root's required boot set");
    assert!(changed.evidence(&files).is_err());
    let mut changed = original.clone();
    changed.tryboot = true;
    assert!(changed.evidence(&files).is_err());
    let id = Sha256::digest(b"durable trial");
    files
        .select(&roots(), Slot::B, Some(id), |_| Ok(()))
        .unwrap();
    // A stale prepared trial file is harmless during a normal boot.
    original.evidence(&files).unwrap();
    assert!(changed.evidence(&files).is_err());
    changed.layout.active = Slot::B;
    changed.command_line = files.template().render(&roots(), Slot::B, Some(id));
    let trial = changed.evidence(&files).unwrap();
    assert_eq!(trial.observation.actual_slot, Some(Slot::B));
    assert_eq!(trial.observation.default_slot, Some(Slot::A));
    assert_eq!(trial.observation.trial_id, Some(id));
    changed.tryboot = false;
    assert!(changed.evidence(&files).is_err());
    fs::write(
        fixture.directory.path().join("spacewars-device.txt"),
        "[all]\ninclude spacewars-profiles/picade.txt\n",
    )
    .unwrap();
    let mut pi5_picade = original;
    pi5_picade.profile = "picade".into();
    assert!(pi5_picade.evidence(&files).is_err());
}

#[test]
fn boot_context_is_stable_across_time_but_not_boot_or_image_changes() {
    let fixture = Fixture::new();
    let files = fixture.open().unwrap();
    let original = runtime(&fixture, &files);
    let mut later = original.clone();
    later.uptime_ms += 10;
    assert!(original.same_boot(&later));
    later.boot_id = "22222222-2222-2222-2222-222222222222".into();
    assert!(!original.same_boot(&later));
    let mut changed = original.clone();
    changed.image.build_id.push_str("-changed");
    assert!(!original.same_boot(&changed));
    let mut earlier = original.clone();
    earlier.uptime_ms -= 1;
    assert!(!original.same_boot(&earlier));
    assert!(!parse_tryboot(&[0, 0, 0, 0]).unwrap());
    assert!(parse_tryboot(&[0, 0, 0, 1]).unwrap());
    for invalid in [&[][..], &[1][..], &[1, 0, 0, 0][..], &[0, 0, 0, 2][..]] {
        assert!(parse_tryboot(invalid).is_err());
    }
}
