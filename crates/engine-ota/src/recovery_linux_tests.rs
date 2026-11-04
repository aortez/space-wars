use super::*;

#[test]
fn service_writable_subtree_requires_the_same_data_partition_and_exact_bind_root() {
    let source = MountView {
        id: 22,
        device: DeviceNumber { major: 8, minor: 4 },
    };
    let target = MountView {
        id: 23,
        device: source.device,
    };
    for relative in ["spacewars-bootstrap/active/boot-trial", "spacewars-system"] {
        let base = format!(
            "22 20 8:4 / /data ro - ext4 /dev/sda4 rw\n23 22 8:4 /{relative} /data/{relative} rw - ext4 /dev/sda4 rw\n"
        );
        data_subtree(&parse_mountinfo(&base).unwrap(), source, target, relative).unwrap();
        for changed in [
            base.replace(&format!(" /{relative} "), " /another-directory "),
            base.replace(&format!(" /data/{relative} "), " /another-mount "),
            base.replace("23 22 8:4", "23 22 8:2"),
            base.replace("- ext4", "- tmpfs"),
            format!("{base}23 22 8:4 /{relative} /data/{relative} rw - ext4 /dev/sda4 rw\n"),
        ] {
            assert!(
                data_subtree(
                    &parse_mountinfo(&changed).unwrap(),
                    source,
                    target,
                    relative
                )
                .is_err()
            );
        }
    }
}

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
    validate_layout(
        "sda".into(),
        device(0),
        100_000,
        partitions,
        mounts,
        views(mounts),
    )
}

fn views(mounts: &[Mount]) -> [MountView; 3] {
    [20, 21, 22].map(|id| MountView {
        id,
        device: mounts
            .iter()
            .find(|mount| mount.id == id)
            .map_or(device(0), |mount| mount.device),
    })
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
        views(&mounts),
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
            &mounts,
            views(&mounts)
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
            &mounts,
            views(&mounts)
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

#[test]
fn visible_mount_id_handles_service_bind_mounts_without_hiding_inactive_roots() {
    let mut records = mounts(2);
    let mut observed = views(&records);
    let mut visible_boot = records[1].clone();
    visible_boot.id = 23;
    observed[1].id = 23;
    records.push(visible_boot);
    let good = validate_layout(
        "sda".into(),
        device(0),
        100_000,
        partitions("sda"),
        &records,
        observed,
    )
    .unwrap();
    assert_eq!(good.active, Slot::A);
    // Table order is not visibility evidence.
    records.reverse();
    validate_layout(
        "sda".into(),
        device(0),
        100_000,
        partitions("sda"),
        &records,
        observed,
    )
    .unwrap();
    let mut absent = observed;
    absent[1].id = 999;
    assert!(
        validate_layout(
            "sda".into(),
            device(0),
            100_000,
            partitions("sda"),
            &records,
            absent
        )
        .is_err()
    );
    // A covered mount still pins its block device and must prevent writing it.
    records
        .iter_mut()
        .find(|mount| mount.id == 21)
        .unwrap()
        .device = device(3);
    assert!(
        validate_layout(
            "sda".into(),
            device(0),
            100_000,
            partitions("sda"),
            &records,
            observed
        )
        .is_err()
    );
}

#[test]
fn opened_directory_mount_id_is_kernel_evidence_and_rejects_malformed_records() {
    let directory = tempfile::tempdir().unwrap();
    let opened = OpenedMount::open(directory.path()).unwrap();
    let records =
        parse_mountinfo(&bounded_text(Path::new("/proc/self/mountinfo"), 1024 * 1024).unwrap())
            .unwrap();
    let matched: Vec<_> = records
        .iter()
        .filter(|mount| mount.id == opened.view.id)
        .collect();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].device, opened.view.device);
    for invalid in [
        "",
        "mnt_id: 0\n",
        "mnt_id: -1\n",
        "mnt_id: 20\nmnt_id: 21\n",
        "mnt_id: 18446744073709551616\n",
    ] {
        assert!(fd_mount_id(invalid).is_err());
    }
    assert_eq!(
        fd_mount_id("pos:\t0\nflags:\t02300000\nmnt_id:\t42\nino:\t99\n").unwrap(),
        42
    );
}

#[test]
#[ignore = "requires unshare and mount with an unprivileged private user/mount namespace"]
fn real_service_style_bind_mount_selects_the_opened_visible_mount() {
    use std::process::Command;
    const TEST: &str =
        "recovery_linux::tests::real_service_style_bind_mount_selects_the_opened_visible_mount";
    const PARENT_NS: &str = "SPACEWARS_OTA_MOUNT_FIXTURE_PARENT_NS";
    let namespace = fs::read_link("/proc/self/ns/mnt").unwrap();
    let Some(parent_namespace) = std::env::var_os(PARENT_NS) else {
        let output = Command::new("unshare")
            .args(["-Ur", "-m", "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", TEST, "--nocapture"])
            .env(PARENT_NS, namespace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return;
    };
    // Mounts are allowed only in the newly created namespace. This fixture
    // never opens a block device or changes the caller's mount namespace.
    assert_ne!(namespace.as_os_str(), parent_namespace);
    assert!(
        Command::new("mount")
            .args(["--make-rprivate", "/"])
            .status()
            .unwrap()
            .success()
    );
    let directory = tempfile::tempdir().unwrap();
    struct Mounted<'a> {
        path: &'a Path,
        count: usize,
    }
    impl Drop for Mounted<'_> {
        fn drop(&mut self) {
            for _ in 0..self.count {
                let _ = Command::new("umount").arg(self.path).status();
            }
        }
    }
    let mut mounted = Mounted {
        path: directory.path(),
        count: 0,
    };
    assert!(
        Command::new("mount")
            .args(["-t", "tmpfs", "-o", "mode=0700", "tmpfs"])
            .arg(mounted.path)
            .status()
            .unwrap()
            .success()
    );
    mounted.count += 1;
    assert!(
        Command::new("mount")
            .arg("--bind")
            .arg(mounted.path)
            .arg(mounted.path)
            .status()
            .unwrap()
            .success()
    );
    mounted.count += 1;
    let opened = OpenedMount::open(mounted.path).unwrap();
    let records =
        parse_mountinfo(&bounded_text(Path::new("/proc/self/mountinfo"), 1024 * 1024).unwrap())
            .unwrap();
    let path = mounted.path.to_str().unwrap();
    let same_path: Vec<_> = records
        .iter()
        .filter(|mount| mount.path == path)
        .map(|mount| mount.id)
        .collect();
    assert_eq!(same_path.len(), 2);
    let selected = visible_mount(&records, opened.view, path, "tmpfs").unwrap();
    assert_eq!(selected.id, opened.view.id);
    println!(
        "{}",
        serde_json::json!({"stacked_mount_ids":same_path,"opened_visible_mount_id":opened.view.id})
    );
}
