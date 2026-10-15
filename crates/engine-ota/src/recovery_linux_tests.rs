use super::*;

fn device(minor: u32) -> DeviceNumber {
    DeviceNumber { major: 8, minor }
}

fn partitions(prefix: &str) -> [Partition; 4] {
    std::array::from_fn(|i| Partition {
        number: i as u32 + 1,
        name: format!("{prefix}{}", i + 1),
        device: device(i as u32 + 1),
        start_sector: 2048 + i as u64 * 8192,
        sectors: 8192,
        partuuid: format!("076c4a2a-0{}", i + 1),
    })
}

fn mounts(root: u32) -> Vec<Mount> {
    parse_mountinfo(&format!(
        "20 0 8:{root} / / rw,relatime shared:1 - ext4 /dev/root rw\n21 20 8:1 / /boot rw - vfat /dev/sda1 rw\n22 20 8:4 / /data rw - ext4 /dev/sda4 rw\n"
    )).unwrap()
}

fn layout(partitions: [Partition; 4], mounts: &[Mount]) -> Result<DiskLayout> {
    validate_layout("sda".into(), device(0), 100_000, partitions, mounts)
}

#[test]
fn mounted_device_numbers_choose_both_slots_without_parsing_name_suffixes() {
    for prefix in ["sda", "mmcblk0p", "nvme0n1p"] {
        for root in [2, 3] {
            let layout = layout(partitions(prefix), &mounts(root)).unwrap();
            assert_eq!(layout.active_partition().device, device(root));
            assert_eq!(
                layout.inactive_partition().device,
                device(if root == 2 { 3 } else { 2 })
            );
            assert_eq!(layout.active_partition().name, format!("{prefix}{root}"));
        }
    }
}

#[test]
fn different_disk_data_bind_mounts_duplicates_and_unknown_root_fail_closed() {
    for fault in 0..6 {
        let mut mounts = mounts(2);
        match fault {
            0 => mounts[0].device = device(8),
            1 => mounts[2].device = device(20),
            2 => mounts[2].root = "/data-subdirectory".into(),
            3 => mounts.push(mounts[1].clone()),
            4 => mounts[0].filesystem = "overlay".into(),
            _ => {
                mounts.pop();
            }
        }
        assert!(layout(partitions("sda"), &mounts).is_err());
    }
}

#[test]
fn even_a_read_only_inactive_mount_prevents_installation() {
    let mut mounts = mounts(2);
    mounts.extend(parse_mountinfo("23 20 8:3 / /mnt/inspection ro - ext4 /dev/sda3 ro\n").unwrap());
    assert!(layout(partitions("sda"), &mounts).is_err());
}

#[test]
fn overlapping_out_of_disk_duplicate_and_unequal_partitions_are_rejected() {
    for fault in 0..7 {
        let mut parts = partitions("sda");
        match fault {
            0 => parts[2].start_sector = parts[1].start_sector,
            1 => parts[3].sectors = 100_000,
            2 => parts[3].start_sector = u64::MAX,
            3 => parts[2].device = parts[1].device,
            4 => parts[2].partuuid = parts[1].partuuid.clone(),
            5 => parts[2].sectors -= 1,
            _ => parts[0].number = 5,
        }
        assert!(layout(parts, &mounts(2)).is_err());
    }
}

#[test]
fn layout_identity_survives_device_enumeration_but_changes_with_geometry_or_uuid() {
    let original = layout(partitions("sda"), &mounts(2)).unwrap().layout_id;
    let mut parts = partitions("nvme0n1p");
    let mut mounts = mounts(3);
    for part in &mut parts {
        part.device.major = 259;
    }
    for mount in &mut mounts {
        mount.device.major = 259;
    }
    let changed = validate_layout(
        "nvme0n1".into(),
        DeviceNumber {
            major: 259,
            minor: 0,
        },
        100_000,
        parts.clone(),
        &mounts,
    )
    .unwrap();
    assert_eq!(original, changed.layout_id);
    parts[3].sectors -= 1;
    assert_ne!(
        original,
        validate_layout(
            "nvme0n1".into(),
            DeviceNumber {
                major: 259,
                minor: 0
            },
            100_000,
            parts.clone(),
            &mounts
        )
        .unwrap()
        .layout_id
    );
    parts[3].sectors += 1;
    parts[3].partuuid = "aabbccdd-04".into();
    assert_ne!(
        original,
        validate_layout(
            "nvme0n1".into(),
            DeviceNumber {
                major: 259,
                minor: 0
            },
            100_000,
            parts,
            &mounts
        )
        .unwrap()
        .layout_id
    );
}

#[test]
fn root_selectors_are_strict_and_duplicate_root_arguments_are_rejected() {
    for value in [
        "/dev/sda2",
        "/dev/mmcblk0p2",
        "/dev/nvme0n1p3",
        "PARTUUID=076c4a2a-02",
        "UUID=11111111-1111-1111-1111-111111111111",
    ] {
        assert!(
            root_node(&root_argument(&format!("quiet root={value} rootwait")).unwrap()).is_ok()
        );
    }
    for value in [
        "",
        "quiet",
        "root=/dev/sda2 root=/dev/sda2",
        "root= rootwait",
    ] {
        assert!(root_argument(value).is_err());
    }
    for value in [
        "/etc/passwd",
        "/dev/../sda2",
        "PARTUUID=../../etc/passwd",
        "PARTUUID=076c4a2a-02/PARTNROFF=1",
        "LABEL=rootfs-a",
        "UUID=invalid",
    ] {
        assert!(root_node(value).is_err());
    }
}

#[test]
fn malformed_or_oversized_kernel_evidence_is_rejected() {
    for value in ["", "-1:2", "8:2:3", "4294967296:1"] {
        assert!(DeviceNumber::parse(value).is_err());
    }
    assert!(parse_mountinfo("20 0 8:2 / / rw ext4 /dev/root rw").is_err());
    assert!(parse_mountinfo("20 0 8:2 / / rw - ext4").is_err());
    assert!(parse_mountinfo(&"20 0 8:2 / / rw - ext4 /dev/root rw\n".repeat(4097)).is_err());
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("size");
    for value in ["-1", "18446744073709551616", "1 2"] {
        fs::write(&path, value).unwrap();
        assert!(number(&path).is_err());
    }
    fs::write(&path, vec![b'x'; 65]).unwrap();
    assert!(bounded_text(&path, 64).is_err());
}

#[test]
fn cloned_disk_uuid_cannot_hide_behind_the_single_udev_alias() {
    let parts = partitions("sda");
    let records: Vec<_> = parts
        .iter()
        .map(|part| (part.device, part.partuuid.clone()))
        .collect();
    check_uuid_records(&parts, &records).unwrap();
    let mut duplicate = records.clone();
    duplicate.push((device(18), parts[1].partuuid.clone()));
    assert!(check_uuid_records(&parts, &duplicate).is_err());
    assert!(check_uuid_records(&parts, &records[..3]).is_err());
    let mut wrong_device = records;
    wrong_device[1].0 = device(18);
    assert!(check_uuid_records(&parts, &wrong_device).is_err());
}
