use super::*;
use crate::boot_linux::PhysicalIdentity;
use crate::owned_fs::open_directory;
use crate::recovery::{Action, BootObservation, RecoveryError, Slot, SlotImage};
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use crate::{Channel, ImageIdentity, RecoveryStore, TrustStore, test_support as feed};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::Arc;

fn owner() -> u32 {
    unsafe { libc::geteuid() }
}
fn private_write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn boot() -> BootEvidence {
    let image =
        ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json")).unwrap();
    let physical = PhysicalIdentity {
        model: "Raspberry Pi 4 Model B Rev 1.4".into(),
        serial: "1234567890abcdef".into(),
        bootloader_revision: "b".repeat(40),
        medium_sectors: 100_000_000,
        medium: [("ID_SERIAL".into(), "test-medium".into())].into(),
    };
    let binding = BootBinding {
        device_id: physical.fingerprint(),
        layout_id: Sha256::digest(b"fixture-layout"),
        boot_identity: image.boot_identity,
        hardware_profile: Sha256::digest(b"picade-profile"),
    };
    BootEvidence {
        physical,
        layout: DiskLayout {
            disk_name: "fixture".into(),
            disk_device: DeviceNumber { major: 8, minor: 0 },
            disk_sectors: 100_000_000,
            active: Slot::A,
            partitions: std::array::from_fn(|i| Partition {
                number: i as u32 + 1,
                name: format!("fixture{}", i + 1),
                device: DeviceNumber {
                    major: 8,
                    minor: i as u32 + 1,
                },
                start_sector: 8192 + i as u64 * 6291456,
                sectors: 6291456,
                partuuid: format!("076c4a2a-{:02}", i + 1),
            }),
            layout_id: binding.layout_id,
        },
        kernel_build_id: "a".repeat(40),
        hardware_profile: "picade".into(),
        observation: BootObservation {
            binding,
            boot_id: "11111111-1111-1111-1111-111111111111".into(),
            uptime_ms: 100_000,
            actual_slot: Some(Slot::A),
            actual_build_id: Some(image.build_id.clone()),
            recovery_protocol: RECOVERY_PROTOCOL,
            default_slot: Some(Slot::A),
            tryboot: false,
            trial_id: None,
        },
        image,
    }
}

struct Fixture {
    root: tempfile::TempDir,
    boot: BootEvidence,
    approval: Approval,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let boot = boot();
        let directory = root.path().join(DIRECTORY);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        // Synthetic reports are private fixtures, never installed on a device.
        let passed = CASES
            .into_iter()
            .map(|case| {
                let bytes =
                    serde_json::to_vec(&serde_json::json!({"synthetic": true, "case": case}))
                        .unwrap();
                private_write(&directory.join(case.filename()), &bytes);
                Evidence {
                    case,
                    report: Blob {
                        bytes: bytes.len() as u64,
                        sha256: Sha256::digest(&bytes),
                    },
                }
            })
            .collect();
        let approval = Approval {
            schema: 1,
            purpose: "physical-recovery-admission".into(),
            binding: boot.observation.binding.clone(),
            kernel_build_id: boot.kernel_build_id.clone(),
            recovery_protocol: RECOVERY_PROTOCOL,
            passed,
        };
        let fixture = Self {
            root,
            boot,
            approval,
        };
        fixture.save();
        fixture.open().unwrap();
        fixture
    }
    fn save(&self) {
        private_write(
            &self.path(APPROVAL),
            &serde_json::to_vec(&self.approval).unwrap(),
        );
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(DIRECTORY).join(name)
    }
    fn open(&self) -> Result<Qualification> {
        Qualification::under(
            open_directory(self.root.path(), owner())?,
            owner(),
            &self.boot,
        )
    }
}

#[test]
fn missing_approval_and_incomplete_or_duplicated_cases_never_qualify() {
    let mut f = Fixture::new();
    fs::remove_file(f.path(APPROVAL)).unwrap();
    assert!(f.open().is_err());
    let original = f.approval.clone();
    for omitted in 0..CASES.len() {
        f.approval = original.clone();
        f.approval.passed.remove(omitted);
        f.save();
        assert!(f.open().is_err(), "omitted {omitted}");
    }
    f.approval = original;
    f.approval.passed[1] = f.approval.passed[0].clone();
    f.save();
    assert!(f.open().is_err());
}

#[test]
fn approval_is_strict_bounded_and_does_not_accept_qualification_booleans_or_paths() {
    let f = Fixture::new();
    let original = serde_json::to_value(&f.approval).unwrap();
    for case in 0..9 {
        let mut value = original.clone();
        match case {
            0 => value["qualified"] = true.into(),
            1 => value["schema"] = 2.into(),
            2 => value["purpose"] = "supervised-signed-trial".into(),
            3 => value["passed"][0]["case"] = "../../approval".into(),
            4 => value["passed"][0]["report"]["path"] = "/etc/passwd".into(),
            5 => value["passed"][0]["report"]["bytes"] = 0.into(),
            6 => value["passed"][0]["report"]["bytes"] = (REPORT_BYTES + 1).into(),
            7 => value["passed"][0]["report"]["sha256"] = "0".repeat(64).into(),
            _ => value["binding"]["device_id"] = "0".repeat(64).into(),
        }
        private_write(&f.path(APPROVAL), &serde_json::to_vec(&value).unwrap());
        assert!(f.open().is_err(), "alteration {case}");
    }
    for bytes in [b"{".to_vec(), vec![b' '; APPROVAL_BYTES as usize + 1]] {
        private_write(&f.path(APPROVAL), &bytes);
        assert!(f.open().is_err());
    }
}

#[test]
fn physical_boot_kernel_profile_and_protocol_changes_require_new_approval() {
    let f = Fixture::new();
    for case in 0..9 {
        let mut boot = f.boot.clone();
        match case {
            0 => boot.physical.serial = "abcdef0123456789".into(),
            1 => boot.physical.bootloader_revision = "c".repeat(40),
            2 => {
                boot.physical
                    .medium
                    .insert("ID_SERIAL".into(), "replacement".into());
            }
            3 => boot.physical.medium_sectors += 1,
            4 => boot.observation.binding.layout_id = Sha256::digest(b"new layout"),
            5 => boot.observation.binding.boot_identity = Sha256::digest(b"new boot assets"),
            6 => boot.observation.binding.hardware_profile = Sha256::digest(b"new profile"),
            7 => boot.kernel_build_id = "c".repeat(40),
            _ => boot.observation.recovery_protocol += 1,
        }
        boot.observation.binding.device_id = boot.physical.fingerprint();
        assert!(f.approval.validate(&boot).is_err(), "alteration {case}");
    }
    // A normal reboot or moving between the qualified root slots is not new hardware.
    let mut boot = f.boot.clone();
    boot.observation.boot_id = "22222222-2222-2222-2222-222222222222".into();
    boot.observation.actual_slot = Some(Slot::B);
    f.approval.validate(&boot).unwrap();
}

#[test]
fn missing_changed_or_oversized_retained_reports_fail_closed() {
    for case in 0..4 {
        let f = Fixture::new();
        let path = f.path(&CASES[0].filename());
        match case {
            0 => fs::remove_file(path).unwrap(),
            1 => private_write(&path, b"different report"),
            2 => private_write(&path, &vec![b'x'; REPORT_BYTES as usize + 1]),
            _ => {
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                private_write(&path, &bytes);
            }
        }
        assert!(f.open().is_err(), "alteration {case}");
    }
}

#[test]
fn ownership_permissions_links_and_special_files_cannot_supply_approval() {
    for target in [APPROVAL.to_owned(), CASES[0].filename()] {
        for case in 0..5 {
            let f = Fixture::new();
            let path = f.path(&target);
            match case {
                0 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                1 => fs::hard_link(&path, f.root.path().join("alias")).unwrap(),
                2 => {
                    let moved = f.root.path().join("moved");
                    fs::rename(&path, &moved).unwrap();
                    symlink(moved, &path).unwrap();
                }
                3 => {
                    fs::remove_file(&path).unwrap();
                    fs::create_dir(&path).unwrap();
                }
                _ => {
                    fs::remove_file(&path).unwrap();
                    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
            }
            assert!(f.open().is_err(), "{target} alteration {case}");
        }
    }
    let f = Fixture::new();
    assert!(
        Qualification::under(
            open_directory(f.root.path(), owner()).unwrap(),
            owner().wrapping_add(1),
            &f.boot
        )
        .is_err()
    );
    fs::set_permissions(
        f.root.path().join(DIRECTORY),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    assert!(f.open().is_err());
}

#[test]
fn revocation_or_replacement_after_read_cannot_reuse_the_open_proof() {
    for target in [APPROVAL.to_owned(), CASES[0].filename()] {
        for case in 0..4 {
            let f = Fixture::new();
            let proof = f.open().unwrap();
            let path = f.path(&target);
            match case {
                0 => fs::remove_file(&path).unwrap(),
                1 => {
                    let bytes = fs::read(&path).unwrap();
                    let next = f.path("next");
                    private_write(&next, &bytes);
                    fs::rename(next, &path).unwrap();
                }
                2 => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                _ => {
                    let mut bytes = fs::read(&path).unwrap();
                    bytes[0] ^= 1;
                    private_write(&path, &bytes);
                }
            }
            assert!(proof.check().is_err(), "{target} alteration {case}");
        }
    }
    let f = Fixture::new();
    let proof = f.open().unwrap();
    let directory = f.root.path().join(DIRECTORY);
    fs::rename(&directory, f.root.path().join("old")).unwrap();
    fs::create_dir(&directory).unwrap();
    assert!(proof.check().is_err());
}

#[tokio::test]
async fn signed_candidate_needs_current_evidence_and_durable_commit_before_write_action() {
    let mut f = Fixture::new();
    let descriptor = feed::descriptor(Channel::Release, 2);
    f.boot.observation.binding.boot_identity = descriptor.boot_identity;
    f.boot.image.boot_identity = descriptor.boot_identity;
    f.approval.binding = f.boot.observation.binding.clone();
    f.save();
    let baseline = RecoveryRecord::bootstrap(
        f.boot.observation.binding.clone(),
        SlotImage {
            slot: Slot::A,
            build_id: f.boot.image.build_id.clone(),
            generation: 1,
            payload: Sha256::digest(b"previous good payload"),
            expanded: Sha256::digest(b"previous good root"),
            data: descriptor.data,
        },
        &f.boot.observation,
    )
    .unwrap();
    let path = f.root.path().join("recovery");
    let mut journal = RecoveryStore::initialize(&path, baseline).unwrap();
    let signed = feed::Fixture::new().await;
    let mut trust = TrustStore::initialize(
        &f.root.path().join("trust"),
        &signed.root_bytes,
        feed::location(),
    )
    .unwrap();
    let transport = signed
        .publish(
            1,
            &[descriptor],
            feed::EXPIRES.parse().unwrap(),
            feed::Fault::None,
        )
        .await;
    let candidate = feed::candidate(
        trust
            .refresh(
                Arc::new(transport),
                feed::time(),
                Channel::Release,
                &feed::device(),
                &journal.record().image_history(),
            )
            .await
            .unwrap(),
    );
    let proof = f.open().unwrap();
    let before = fs::read(path.join("state.json")).unwrap();
    let retained = f.root.path().join("revoked-approval");
    fs::rename(f.path(APPROVAL), &retained).unwrap();
    assert!(proof.plan(journal.record(), &f.boot, &candidate).is_err());
    fs::rename(retained, f.path(APPROVAL)).unwrap();
    let proof = f.open().unwrap();
    let mut trial = f.boot.clone();
    trial.observation.tryboot = true;
    assert!(proof.plan(journal.record(), &trial, &candidate).is_err());
    let mut wrong = f.boot.clone();
    wrong.observation.actual_slot = Some(Slot::B);
    assert!(proof.plan(journal.record(), &wrong, &candidate).is_err());
    let plan = proof.plan(journal.record(), &f.boot, &candidate).unwrap();
    let id = plan.installation().unwrap().1.id;
    assert_eq!(fs::read(path.join("state.json")).unwrap(), before);
    assert!(journal.record().transaction().is_none());
    assert!(
        matches!(journal.advance(|_| Ok(plan)).unwrap(), Action::WriteInactive { transaction, .. } if transaction == id)
    );
    assert!(matches!(
        proof.plan(journal.record(), &f.boot, &candidate),
        Err(Error::Recovery(RecoveryError::Busy))
    ));
    drop(journal);
    let recovered = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovered.record().transaction().unwrap().id, id);
    assert_ne!(fs::read(path.join("state.json")).unwrap(), before);
}
