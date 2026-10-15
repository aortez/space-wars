use super::*;
use crate::bootstrap::tests::linux::Fixture;
use crate::recovery::Slot;
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use std::os::unix::fs::{PermissionsExt, symlink};

fn open(fixture: &Fixture) -> Result<OperatorPackage> {
    let bytes = fs::read(fixture.output().join("bundle.json"))?;
    OperatorPackage::under(
        File::open(fixture.output())?,
        unsafe { libc::geteuid() },
        Sha256::digest(&bytes),
    )
}

fn legacy(package: &BootstrapPackage) -> LegacyBootEvidence {
    let partition = |number, start, sectors| Partition {
        number,
        name: format!("sda{number}"),
        device: DeviceNumber {
            major: 8,
            minor: number,
        },
        start_sector: start,
        sectors,
        partuuid: format!("076c4a2a-{number:02}"),
    };
    let mut boot_files: BTreeMap<_, _> = package
        .files
        .iter()
        .filter_map(|(p, b)| p.strip_prefix("boot/").map(|p| (p.to_owned(), b.clone())))
        .collect();
    for name in [
        "spacewars-recovery.cpio.gz",
        "spacewars-boot-manifest.json",
        "spacewars-trial.txt",
        "tryboot.txt",
        "spacewars-cmdline.txt",
        "spacewars-kernel-build-id",
    ] {
        boot_files.remove(name);
    }
    LegacyBootEvidence {
        schema: 1,
        purpose: "test-only-observed-legacy-boot",
        physical: crate::boot_linux::PhysicalIdentity {
            model: "Raspberry Pi 5 Model B Rev 1.1".into(),
            serial: "123456789abcdef0".into(),
            bootloader_revision: "a".repeat(40),
            medium_sectors: 200_000_000,
            medium: BTreeMap::new(),
        },
        boot_id: "de606400-dc75-446e-b26b-385592b0f562".into(),
        layout: DiskLayout {
            disk_name: "sda".into(),
            disk_device: DeviceNumber { major: 8, minor: 0 },
            disk_sectors: 200_000_000,
            active: Slot::B,
            partitions: [
                partition(1, 8192, 307200),
                partition(2, 319488, 6291456),
                partition(3, 6610944, 6291456),
                partition(4, 12902400, 100_000_000),
            ],
            layout_id: Sha256::digest(b"fixture layout"),
        },
        kernel_build_id: "b".repeat(40),
        hardware_profile: "hyperpixel".into(),
        hardware_configuration: vec!["[all]".into(), "dtoverlay=vc4-kms-dpi-hyperpixel4".into()],
        hostname: Some("spacewars".into()),
        boot_files,
        boot_free_bytes: 128 * 1024 * 1024,
        boot_allocation_bytes: 4096,
        data_free_bytes: 8 * 1024 * 1024 * 1024,
        root_preflight_complete: false,
    }
}

fn rewrite_record(fixture: &Fixture, change: impl FnOnce(&mut BootstrapPackage)) {
    let path = fixture.output().join("bundle.json");
    let mut record: BootstrapPackage = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    change(&mut record);
    let bytes = serde_json::to_vec_pretty(&record).unwrap();
    fs::write(path, &bytes).unwrap();
    let mut checksums = record
        .files
        .iter()
        .map(|(p, b)| format!("{}  {p}\n", b.sha256))
        .collect::<String>();
    checksums.push_str(&format!("{}  bundle.json\n", Sha256::digest(&bytes)));
    fs::write(fixture.output().join("SHA256SUMS"), checksums).unwrap();
}

#[test]
fn private_package_checks_assets_and_rederives_the_live_inactive_target() {
    let fixture = Fixture::new();
    let record = fixture.build().unwrap();
    let mut package = open(&fixture).unwrap();
    let mut device = legacy(&record);
    let report = package.check_device(&device).unwrap();
    assert!(!report.ready_for_write);
    assert_eq!(report.plan.target, Slot::A);
    assert!(report.boot_space.required_bytes <= device.boot_free_bytes);
    assert!(report.data_required_bytes < device.data_free_bytes);
    device.layout.active = Slot::A;
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.layout.partitions[1].partuuid = "aabbccdd-02".into();
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.hardware_profile = "hdmi".into();
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.physical.model = "Raspberry Pi 4 Model B Rev 1.4".into();
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.boot_files.get_mut("start4.elf").unwrap().sha256 = Sha256::digest(b"other firmware");
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device
        .hardware_configuration
        .push("dtparam=audio=off".into());
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.boot_free_bytes = report.boot_space.required_bytes - 1;
    assert!(package.check_device(&device).is_err());
    device = legacy(&record);
    device.data_free_bytes = report.data_required_bytes - 1;
    assert!(package.check_device(&device).is_err());
}

#[test]
fn verified_backup_credit_keeps_resume_space_checks_without_demanding_duplicate_copies() {
    let fixture = Fixture::new();
    let record = fixture.build().unwrap();
    let mut package = open(&fixture).unwrap();
    let mut device = legacy(&record);
    let original = package.check_device(&device).unwrap();
    let raw =
        (device.layout.partitions[0].sectors + device.layout.inactive_partition().sectors) * 512;
    device.data_free_bytes = original.data_required_bytes - raw;
    assert!(package.check_device(&device).is_err());
    let resumed = package.check_device_with_credit(&device, raw).unwrap();
    resumed.require_data_space().unwrap();
    assert_eq!(resumed.data_required_bytes, device.data_free_bytes);
    device.data_free_bytes -= 1;
    assert!(
        package
            .check_device_with_credit(&device, raw)
            .unwrap()
            .require_data_space()
            .is_err()
    );
    assert!(package.check_device_with_credit(&device, raw + 1).is_err());
}

#[test]
fn invented_plans_cannot_override_checked_assets_even_with_an_operator_hash() {
    let fixture = Fixture::new();
    let record = fixture.build().unwrap();
    rewrite_record(&fixture, |package| package.plan.staged_asset_bytes += 1);
    let mut package = open(&fixture).unwrap();
    let error = package.check_device(&legacy(&record)).unwrap_err();
    assert!(error.to_string().contains("serialized bootstrap plan"));
    rewrite_record(&fixture, |package| package.plan.staged_asset_bytes -= 1);
    rewrite_record(&fixture, |package| {
        package
            .files
            .insert(
                "boot/hostname.txt".into(),
                Blob {
                    bytes: 2,
                    sha256: Sha256::digest(b"x\n"),
                },
            )
            .map(drop)
            .unwrap_or(())
    });
    fs::write(fixture.output().join("boot/hostname.txt"), b"x\n").unwrap();
    fs::set_permissions(
        fixture.output().join("boot/hostname.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(open(&fixture).is_err());
}

#[test]
fn changed_files_metadata_links_and_unlisted_content_invalidate_the_package() {
    let fixture = Fixture::new();
    fixture.build().unwrap();
    let mut package = open(&fixture).unwrap();
    let path = fixture.output().join("rootfs.ext4.zst");
    let original = fs::read(&path).unwrap();
    fs::write(&path, vec![b'x'; original.len()]).unwrap();
    assert!(package.check().is_err());
    assert!(open(&fixture).is_err());
    fs::write(&path, &original).unwrap();
    package = open(&fixture).unwrap();
    fs::rename(&path, fixture.output().join("old-rootfs")).unwrap();
    fs::write(&path, &original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(package.check().is_err());
    assert!(open(&fixture).is_err());
    fs::remove_file(fixture.output().join("old-rootfs")).unwrap();
    package = open(&fixture).unwrap();
    fs::set_permissions(
        fixture.output().join("boot/overlays"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(package.check().is_err());
    fs::set_permissions(
        fixture.output().join("boot/overlays"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let kernel = fixture.output().join("boot/kernel8.img");
    fs::remove_file(&kernel).unwrap();
    symlink("kernel_2712.img", &kernel).unwrap();
    assert!(open(&fixture).is_err());
}

#[test]
fn operator_digest_and_unambiguous_bounded_records_are_required() {
    let fixture = Fixture::new();
    let record = fixture.build().unwrap();
    let owner = unsafe { libc::geteuid() };
    assert!(
        OperatorPackage::under(
            File::open(fixture.output()).unwrap(),
            owner,
            Sha256::digest(b"not this package")
        )
        .is_err()
    );
    assert!(
        OperatorPackage::under(
            File::open(fixture.output()).unwrap(),
            owner.wrapping_add(1),
            Sha256::digest(&fs::read(fixture.output().join("bundle.json")).unwrap())
        )
        .is_err()
    );
    let bytes = serde_json::to_string(&record).unwrap();
    let blob = serde_json::to_string(&record.compressed).unwrap();
    let duplicate = bytes.replacen(
        "\"files\":{",
        &format!("\"files\":{{\"rootfs.ext4.zst\":{blob},"),
        1,
    );
    assert!(serde_json::from_str::<BootstrapPackage>(&duplicate).is_err());
    rewrite_record(&fixture, |record| {
        record.purpose = "signed-ota-candidate".into()
    });
    assert!(open(&fixture).is_err());
    rewrite_record(&fixture, |record| {
        record.purpose = "supervised-bootstrap".into()
    });
    fs::write(fixture.output().join("SHA256SUMS"), b"incorrect\n").unwrap();
    assert!(open(&fixture).is_err());
    fs::write(
        fixture.output().join("bundle.json"),
        vec![b' '; MAX_RECORD_BYTES as usize + 1],
    )
    .unwrap();
    assert!(open(&fixture).is_err());
}

#[test]
fn profile_marker_is_the_only_normalized_hardware_change() {
    assert_eq!(
        normalized_profile(&["[all]".into()], "hdmi"),
        normalized_profile(
            &["[all]".into(), "dtoverlay=spacewars-profile-hdmi".into()],
            "hdmi"
        )
    );
    assert_ne!(
        normalized_profile(&["[all]".into(), "dtparam=button1=304".into()], "picade"),
        normalized_profile(&["[all]".into(), "dtparam=button1=305".into()], "picade")
    );
}

/// Read-only build-artifact validation. The device context is an explicit
/// fixture, not a claim of a privileged live-device preflight.
#[test]
#[ignore = "requires SPACEWARS_BOOTSTRAP_TEST_PACKAGE pointing to a private workstation package"]
fn real_private_package_is_checked_without_device_writes() {
    let path = std::env::var_os("SPACEWARS_BOOTSTRAP_TEST_PACKAGE").unwrap();
    let path = std::path::Path::new(&path);
    let bytes = fs::read(path.join("bundle.json")).unwrap();
    let mut package = OperatorPackage::under(
        File::open(path).unwrap(),
        unsafe { libc::geteuid() },
        Sha256::digest(&bytes),
    )
    .unwrap();
    let mut device = legacy(&package.record);
    device.boot_free_bytes = 79_437_824;
    let report = package.check_device(&device).unwrap();
    assert!(!report.ready_for_write);
    println!(
        "{}",
        serde_json::json!({"package_sha256":report.package_sha256,"image_build":report.image.build_id,
        "boot_space":report.boot_space,"scope":"read-only workstation package with a synthetic Pi 5 context; no device access"})
    );
}
