use super::*;
use crate::boot_linux::PhysicalIdentity;
use crate::native_install::admit_reboot_time;
use crate::recovery::{BootObservation, Phase, RecoveryCapability, RecoveryError, Slot, SlotImage};
use crate::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use crate::test_support::{self as feed, Fault};
use std::collections::BTreeMap;
use std::sync::Arc;

struct Fixture {
    temp: tempfile::TempDir,
    record: RecoveryRecord,
    boot: BootEvidence,
    setup: Status,
    candidate: VerifiedCandidate,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let feed = feed::Fixture::new().await;
        let mut trust = TrustStore::initialize(
            &temp.path().join("trust"),
            &feed.root_bytes,
            feed::location(),
        )
        .unwrap();
        let image = feed::descriptor(Channel::Release, 1);
        let transport = feed
            .publish(
                1,
                std::slice::from_ref(&image),
                feed::EXPIRES.parse().unwrap(),
                Fault::None,
            )
            .await;
        let candidate = feed::candidate(
            trust
                .refresh(
                    Arc::new(transport),
                    feed::time(),
                    Channel::Release,
                    &feed::device(),
                    &ImageHistory::default(),
                )
                .await
                .unwrap(),
        );
        let physical = PhysicalIdentity {
            model: "Raspberry Pi 5 Model B Rev 1.0".into(),
            serial: "a".repeat(16),
            bootloader_revision: "b".repeat(40),
            medium_sectors: 100_000_000,
            medium: BTreeMap::from([("ID_SERIAL".into(), "fixture-usb".into())]),
        };
        let layout = DiskLayout {
            disk_name: "sda".into(),
            disk_device: DeviceNumber { major: 8, minor: 0 },
            disk_sectors: 100_000_000,
            active: Slot::A,
            layout_id: Sha256::digest(b"layout"),
            partitions: std::array::from_fn(|i| Partition {
                number: i as u32 + 1,
                name: format!("sda{}", i + 1),
                device: DeviceNumber {
                    major: 8,
                    minor: i as u32 + 1,
                },
                start_sector: 8192 + i as u64 * 8_000_000,
                sectors: 6_000_000,
                partuuid: format!("076c4a2a-{:02}", i + 1),
            }),
        };
        let mut installed =
            ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json"))
                .unwrap();
        installed.boot_identity = image.boot_identity;
        let binding = BootBinding {
            device_id: physical.fingerprint(),
            layout_id: layout.layout_id,
            boot_identity: image.boot_identity,
            hardware_profile: Sha256::digest(b"hyperpixel"),
        };
        let observation = BootObservation {
            binding: binding.clone(),
            boot_id: "11111111-1111-1111-1111-111111111111".into(),
            uptime_ms: 1_000_000,
            actual_slot: Some(Slot::A),
            actual_build_id: Some(installed.build_id.clone()),
            recovery_protocol: crate::recovery::RECOVERY_PROTOCOL,
            default_slot: Some(Slot::A),
            tryboot: false,
            trial_id: None,
        };
        let record = RecoveryRecord::bootstrap(
            binding,
            SlotImage {
                slot: Slot::A,
                build_id: installed.build_id.clone(),
                generation: 0,
                payload: Sha256::digest(b"good compressed"),
                expanded: Sha256::digest(b"good expanded"),
                data: installed.data,
            },
            &observation,
        )
        .unwrap();
        let boot = BootEvidence {
            physical,
            image: installed,
            layout,
            kernel_build_id: "c".repeat(40),
            hardware_profile: "hyperpixel".into(),
            observation,
        };
        let setup = Status {
            schema: 1,
            bundle: Sha256::digest(b"public bundle"),
            initial_root: Sha256::digest(&feed.root_bytes),
            persistent_schema: 1,
        };
        Self {
            temp,
            record,
            boot,
            setup,
            candidate,
        }
    }

    fn selection(&self) -> Selection {
        Selection::new(&self.boot, &self.record, &self.setup, &self.candidate, None).unwrap()
    }
}

#[tokio::test]
async fn selection_rejects_changed_authority_history_boot_candidate_data_and_deadline() {
    let f = Fixture::new().await;
    let selection = f.selection();
    let check = |s: &Selection| s.check(&f.boot, &f.record, &f.setup, &f.candidate);
    check(&selection).unwrap();
    let bytes = serde_json::to_vec(&selection).unwrap();
    assert_eq!(
        Selection::parse(&bytes, selection.fingerprint()).unwrap(),
        selection
    );
    assert!(Selection::parse(&bytes, Sha256::digest(b"other approval")).is_err());
    type Change = fn(&mut Selection);
    let changes: &[Change] = &[
        |s| s.schema += 1,
        |s| s.purpose = "automatic".into(),
        |s| s.boot_id = "22222222-2222-2222-2222-222222222222".into(),
        |s| s.binding.device_id = Sha256::digest(b"different board"),
        |s| s.binding.layout_id = Sha256::digest(b"different disk"),
        |s| s.installed_image.build_id = "different-good".into(),
        |s| s.recovery = Sha256::digest(b"different journal"),
        |s| s.bundle = Sha256::digest(b"different bundle"),
        |s| s.initial_root = Sha256::digest(b"different root"),
        |s| s.persistent_schema += 1,
        |s| s.candidate.generation += 1,
        |s| s.retry_payload = Some(Sha256::digest(b"other failed payload")),
        |s| s.selected_ms += 1,
        |s| s.expires_ms += 1,
    ];
    for change in changes {
        let mut changed = selection.clone();
        change(&mut changed);
        assert!(check(&changed).is_err());
    }
    let mut boot = f.boot.clone();
    boot.observation.uptime_ms = selection.expires_ms;
    assert!(
        selection
            .check(&boot, &f.record, &f.setup, &f.candidate)
            .is_err()
    );
    boot.observation.uptime_ms = selection.selected_ms - 1;
    assert!(
        selection
            .check(&boot, &f.record, &f.setup, &f.candidate)
            .is_err()
    );
    boot = f.boot.clone();
    boot.observation.tryboot = true;
    assert!(
        selection
            .check(&boot, &f.record, &f.setup, &f.candidate)
            .is_err()
    );
    let mut json = serde_json::to_value(&selection).unwrap();
    json["qualified"] = true.into();
    assert!(
        Selection::parse(&serde_json::to_vec(&json).unwrap(), selection.fingerprint()).is_err()
    );
    assert!(
        Selection::parse(
            &vec![b' '; MAX_SELECTION_BYTES as usize + 1],
            selection.fingerprint()
        )
        .is_err()
    );
}

#[tokio::test]
async fn supervised_attempt_is_durable_single_use_and_does_not_qualify_ordinary_admission() {
    let f = Fixture::new().await;
    let selection = f.selection();
    let boot = &f.boot.observation;
    let cap = RecoveryCapability {
        binding: boot.binding.clone(),
        tryboot_qualified: false,
        watchdog_qualified: false,
    };
    let path = f.temp.path().join("recovery");
    let mut store = RecoveryStore::initialize(&path, f.record.clone()).unwrap();
    assert!(matches!(
        store.advance(|r| r.begin(&f.candidate, boot, &cap)),
        Err(Error::Recovery(RecoveryError::Unqualified))
    ));
    assert!(
        store
            .advance(|r| r.begin_supervised(&f.candidate, boot, Sha256::digest(b"stale"), false))
            .is_err()
    );
    assert_eq!(store.record(), &f.record);
    let action = store
        .advance(|r| r.begin_supervised(&f.candidate, boot, selection.recovery, false))
        .unwrap();
    let Action::WriteInactive {
        transaction: id,
        slot: Slot::B,
    } = action
    else {
        panic!("wrong action")
    };
    drop(store);
    let mut store = RecoveryStore::open(&path).unwrap();
    assert_eq!(
        store.record().image_history().highest_attempted_generation,
        1
    );
    assert!(
        store
            .advance(|r| r.begin_supervised(&f.candidate, boot, selection.recovery, false))
            .is_err()
    );
    assert!(store.record().trial_reboot(id, boot, false).is_err());
    let image = f.candidate.image();
    store
        .advance(|r| r.written(id, image.compressed.sha256, image.expanded.sha256, boot))
        .unwrap();
    assert_eq!(
        store.record().trial_reboot(id, boot, false).unwrap(),
        Slot::B
    );
    assert!(store.record().trial_reboot(id, boot, true).is_err());
    store.advance(|r| r.trial_prepared(id, boot)).unwrap();
    assert!(store.advance(|r| r.trial_prepared(id, boot)).is_err());
    assert_eq!(
        store.record().trial_reboot(id, boot, true).unwrap(),
        Slot::B
    );
    let mut changed = boot.clone();
    changed.boot_id = "22222222-2221-1111-1111-111111111111".into();
    assert!(store.record().trial_reboot(id, &changed, true).is_err());
    store
        .advance(|r| r.fail(FailureReason::InstallationFailed, boot))
        .unwrap();
    assert_eq!(
        store.record().transaction().unwrap().phase,
        Phase::RolledBack
    );
    assert!(store.record().trial_reboot(id, boot, true).is_err());
    // A fresh plan cannot erase quarantine. Only the existing explicit retry
    // view may admit attempt two, and its generation floor remains persisted.
    let current = store.record().fingerprint();
    assert!(
        store
            .advance(|r| r.begin_supervised(&f.candidate, boot, current, false))
            .is_err()
    );
    store
        .advance(|r| r.begin_supervised(&f.candidate, boot, current, true))
        .unwrap();
    assert_eq!(store.record().transaction().unwrap().attempt, 2);
    store
        .advance(|r| r.fail(FailureReason::InstallationFailed, boot))
        .unwrap();
    let current = store.record().fingerprint();
    assert!(
        store
            .advance(|r| r.begin_supervised(&f.candidate, boot, current, true))
            .is_err()
    );
    assert!(matches!(
        store.advance(|r| r.begin(&f.candidate, boot, &cap)),
        Err(Error::Recovery(RecoveryError::Unqualified))
    ));
    assert_eq!(
        store.record().image_history().highest_attempted_generation,
        1
    );
}

#[tokio::test]
async fn expiration_at_reboot_stays_rejected_after_clock_rollback_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let f = feed::Fixture::new().await;
    let path = directory.path().join("trust");
    let mut trust = TrustStore::initialize(&path, &f.root_bytes, feed::location()).unwrap();
    let expires: jiff::Timestamp = feed::EXPIRES.parse().unwrap();
    assert!(matches!(
        admit_reboot_time(&mut trust, Err(Error::WaitingForTime), expires),
        Err(Error::WaitingForTime)
    ));
    admit_reboot_time(&mut trust, Ok(feed::time().unwrap()), expires).unwrap();
    assert!(admit_reboot_time(&mut trust, Ok(UsableTime(expires)), expires).is_err());
    drop(trust);
    let mut trust = TrustStore::open(&path).unwrap();
    assert!(matches!(
        admit_reboot_time(&mut trust, Ok(feed::time().unwrap()), expires),
        Err(Error::TimeMovedBackwards)
    ));
}
