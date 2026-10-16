use super::*;
use crate::Blob;
use crate::bootstrap::trial::{TrialInputs, TrialRecord};
use crate::bootstrap::{Board, BootstrapBinding, TrialPhase};
use crate::recovery::{AppInstance, BootObservation, HealthSample, Slot};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

pub(crate) struct Fixture {
    pub(in crate::bootstrap) temp: tempfile::TempDir,
    pub(in crate::bootstrap) inputs: TrialInputs,
    pub(in crate::bootstrap) boot_files: BTreeMap<String, Vec<u8>>,
    pub(in crate::bootstrap) owner: u32,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let assets = crate::bootstrap::tests::Assets::new();
        let plan = assets
            .plan(&crate::bootstrap::tests::request(
                Board::Pi5,
                "hyperpixel",
                Slot::A,
            ))
            .unwrap();
        let legacy_config = "retained legacy configuration\n".to_string();
        let mut original = assets.contents.clone();
        original.remove("tryboot.txt");
        original.insert("config.txt".into(), legacy_config.as_bytes().to_vec());
        original.insert(
            "cmdline.txt".into(),
            b"retained legacy command line\n".to_vec(),
        );
        original.insert(
            "spacewars-device.txt".into(),
            b"include spacewars-profiles/hyperpixel.txt\n".to_vec(),
        );
        original.insert(
            "kernel8.img".into(),
            b"old kernel; must remain available".to_vec(),
        );
        original.insert("hostname.txt".into(), b"spacewars-fixture\n".to_vec());
        original.insert("boot_slot".into(), b"B\n".to_vec());
        let origin = "de606400-dc75-446e-b26b-385592b0f562".to_string();
        let binding = BootstrapBinding {
            schema: 1,
            package_sha256: Sha256::digest(b"package"),
            device_id: Sha256::digest(b"device"),
            layout_id: Sha256::digest(b"layout"),
            original_boot_id: origin.clone(),
            original_slot: Slot::B,
            target: Slot::A,
            kernel_build_id: "a".repeat(40),
            hardware_profile: "hyperpixel".into(),
            boot_partition_bytes: 150 * 1024 * 1024,
            inactive_partition_bytes: 3 * 1024 * 1024 * 1024,
            original_boot_files: original
                .iter()
                .map(|(path, bytes)| {
                    (
                        path.clone(),
                        Blob {
                            bytes: bytes.len() as u64,
                            sha256: Sha256::digest(bytes),
                        },
                    )
                })
                .collect(),
        };
        let inputs = TrialInputs {
            binding,
            root_install_sha256: Sha256::digest(b"written root"),
            image: assets.image,
            kernel_build_id: "b".repeat(40),
            command_line_template: String::from_utf8(crate::bootstrap::tests::TEMPLATE.to_vec())
                .unwrap(),
            legacy_config,
            plan,
            origin_boot_id: origin,
        };
        let owner = unsafe { libc::geteuid() };
        fs::create_dir(temp.path().join("data")).unwrap();
        fs::set_permissions(temp.path().join("data"), fs::Permissions::from_mode(0o755)).unwrap();
        let parent = temp.path().join("data/spacewars-bootstrap");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        state::begin(&File::open(&parent).unwrap(), owner, &inputs.binding).unwrap();
        let directory = parent.join("active/boot-trial");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let record = TrialRecord::requested(inputs.clone()).unwrap();
        let bytes = serde_json::to_vec(&Envelope {
            checksum: Sha256::digest(&serde_json::to_vec(&record).unwrap()),
            record,
        })
        .unwrap();
        fs::write(directory.join(RECORD), bytes).unwrap();
        fs::set_permissions(directory.join(RECORD), fs::Permissions::from_mode(0o600)).unwrap();
        let mut boot_files = original;
        boot_files.insert(
            "tryboot.txt".into(),
            inputs.plan.trial_config.as_bytes().to_vec(),
        );
        for path in inputs.plan.staged_assets.keys() {
            boot_files.insert(
                format!("sw-bootstrap/{path}"),
                assets.contents[path].clone(),
            );
        }
        boot_files.insert(
            "sw-bootstrap/trial-cmdline.txt".into(),
            inputs.plan.trial_cmdline.as_bytes().to_vec(),
        );
        boot_files.insert(
            "sw-bootstrap/normal-cmdline.txt".into(),
            inputs.plan.normal_cmdline.as_bytes().to_vec(),
        );
        Self {
            temp,
            inputs,
            boot_files,
            owner,
        }
    }

    pub(crate) fn open(&self) -> TrialStore {
        self.try_open().unwrap().unwrap()
    }
    fn try_open(&self) -> Result<Option<TrialStore>> {
        TrialStore::under(
            File::open(self.temp.path())?,
            File::open(self.temp.path().join("data"))?,
            self.owner,
        )
    }
    pub(in crate::bootstrap) fn journal(&self) -> std::path::PathBuf {
        self.temp
            .path()
            .join("data/spacewars-bootstrap/active/boot-trial")
    }

    pub(crate) fn boot(&self) -> BootObservation {
        BootObservation {
            binding: self.inputs.boot_binding(),
            boot_id: "aa606400-dc75-446e-b26b-385592b0f562".into(),
            uptime_ms: 5_000,
            actual_slot: Some(Slot::A),
            actual_build_id: Some(self.inputs.image.build_id.clone()),
            recovery_protocol: self.inputs.image.recovery_protocol,
            default_slot: Some(Slot::B),
            tryboot: true,
            trial_id: Some(self.inputs.plan.trial_id),
        }
    }

    pub(crate) fn promoting(&self) -> (TrialStore, BootObservation) {
        let mut store = self.open();
        let mut boot = self.boot();
        store.advance(|record| record.reconcile(&boot)).unwrap();
        let mut monitor = store.durable().unwrap().monitor(&boot).unwrap();
        let mut proof = None;
        for number in 0..=30 {
            boot.uptime_ms = 5_000 + number * 2_000;
            proof = monitor.observe(Some(sample(&boot, number + 1))).unwrap();
        }
        store
            .advance(|record| record.healthy(proof.unwrap(), &boot))
            .unwrap();
        store.durable().unwrap().selection(&boot, true).unwrap();
        (store, boot)
    }

    pub(crate) fn promoted(&self) -> (TrialStore, BootObservation) {
        let (mut store, mut boot) = self.promoting();
        boot.default_slot = Some(Slot::A);
        store.advance(|record| record.reconcile(&boot)).unwrap();
        (store, boot)
    }
}

pub(in crate::bootstrap) fn sample(boot: &BootObservation, number: u64) -> HealthSample {
    HealthSample {
        uptime_ms: boot.uptime_ms,
        instance: AppInstance {
            pid: 123,
            start_ticks: 1,
            invocation_id: "a".repeat(32),
        },
        build_id: boot.actual_build_id.clone().unwrap(),
        executable_verified: true,
        state_writable: true,
        control_response: number,
        presented_frames: number * 10,
    }
}

#[test]
fn staging_record_publication_resumes_but_never_authorizes_an_unrequested_trial() {
    for point in [
        "trial-record-written",
        "trial-record-synced",
        "trial-record-published",
        "trial-record-directory-synced",
    ] {
        let fixture = Fixture::new();
        fs::remove_dir_all(fixture.journal()).unwrap();
        let active = File::open(fixture.journal().parent().unwrap()).unwrap();
        assert!(
            prepare(&active, fixture.owner, &fixture.inputs, |current| {
                if current == point {
                    return Err(invalid("interrupted staging record"));
                }
                Ok(())
            })
            .is_err(),
            "{point}"
        );
        if !fixture.journal().exists() {
            assert!(fixture.try_open().is_err());
            assert!(
                TrialStore::under_mode(
                    File::open(fixture.temp.path()).unwrap(),
                    File::open(fixture.temp.path().join("data")).unwrap(),
                    fixture.owner,
                    true
                )
                .unwrap()
                .is_none()
            );
            prepare(&active, fixture.owner, &fixture.inputs, |_| Ok(())).unwrap();
        }
        let mut store = fixture.open();
        assert_eq!(store.phase(), TrialPhase::Staging);
        assert!(
            store
                .advance(|record| record.reconcile(&fixture.boot()))
                .is_err()
        );
        store
            .advance(|record| {
                record.phase = TrialPhase::TrialReady;
                Ok(())
            })
            .unwrap();
        assert!(
            store
                .advance(|record| record.reconcile(&fixture.boot()))
                .is_err()
        );
        assert!(store.watchdog_token(&fixture.boot()).is_err());
        assert!(prepare(&active, fixture.owner, &fixture.inputs, |_| Ok(())).is_err());
        assert_eq!(store.phase(), TrialPhase::TrialReady);
    }
}

#[test]
fn staging_record_rejects_unsafe_pending_state_without_touching_it() {
    for case in ["unknown", "link", "oversized", "binding"] {
        let fixture = Fixture::new();
        fs::remove_dir_all(fixture.journal()).unwrap();
        let active_path = fixture.journal().parent().unwrap().to_owned();
        let active = File::open(&active_path).unwrap();
        let pending = backup::directory(&active, PREPARING, fixture.owner).unwrap();
        match case {
            "unknown" => fs::write(active_path.join(PREPARING).join("unknown"), b"keep").unwrap(),
            "link" => symlink("/dev/zero", active_path.join(PREPARING).join(RECORD)).unwrap(),
            "oversized" => {
                let file = backup::writable(&pending, RECORD, fixture.owner).unwrap();
                file.set_len(LIMIT + 1).unwrap();
            }
            "binding" => fs::write(active_path.join("binding.json"), b"corrupt").unwrap(),
            _ => unreachable!(),
        }
        assert!(
            prepare(&active, fixture.owner, &fixture.inputs, |_| Ok(())).is_err(),
            "{case}"
        );
        assert!(!fixture.journal().exists());
    }
}

#[test]
fn interrupted_publication_never_releases_an_action_and_reopen_uses_only_published_state() {
    for point in ["write", "file-sync", "rename", "directory-sync"] {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let boot = fixture.boot();
        store.fault = Some(point);
        assert!(
            store.advance(|record| record.reconcile(&boot)).is_err(),
            "{point}"
        );
        assert!(store.durable().is_err(), "{point}");
        assert!(store.advance(|record| record.reconcile(&boot)).is_err());
        drop(store);
        let mut reopened = fixture.open();
        let expected = if matches!(point, "rename" | "directory-sync") {
            TrialPhase::ObservingTrial
        } else {
            TrialPhase::TrialRequested
        };
        assert_eq!(reopened.phase(), expected, "{point}");
        assert_eq!(
            reopened.advance(|record| record.reconcile(&boot)).unwrap(),
            TrialAction::Observe
        );
    }
}

#[test]
fn missing_corrupt_conflicting_and_unpublished_state_cannot_initialize_a_trial() {
    for case in [
        "missing",
        "corrupt",
        "pending",
        "unknown",
        "link",
        "production",
    ] {
        let fixture = Fixture::new();
        let path = fixture.journal().join(RECORD);
        match case {
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => fs::write(&path, b"corrupt").unwrap(),
            "pending" => fs::rename(&path, fixture.journal().join(PENDING)).unwrap(),
            "unknown" => fs::write(fixture.journal().join("surprise"), b"unknown").unwrap(),
            "link" => {
                fs::remove_file(&path).unwrap();
                symlink("/dev/zero", &path).unwrap();
            }
            "production" => {
                let system = fixture.temp.path().join("data/spacewars-system");
                fs::create_dir(&system).unwrap();
                fs::set_permissions(&system, fs::Permissions::from_mode(0o700)).unwrap();
                symlink("missing", system.join("recovery")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(fixture.try_open().is_err(), "{case}");
    }
}

#[test]
fn journal_excludes_concurrent_openers_and_rejects_named_directory_or_binding_replacement() {
    let fixture = Fixture::new();
    let store = fixture.open();
    assert!(matches!(fixture.try_open(), Err(Error::Busy)));
    let path = fixture.journal();
    fs::rename(&path, path.with_file_name("saved-boot-trial")).unwrap();
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(store.durable().is_err());
    drop(store);
    let fixture = Fixture::new();
    let store = fixture.open();
    fs::write(
        fixture.journal().parent().unwrap().join("binding.json"),
        b"changed",
    )
    .unwrap();
    assert!(store.durable().is_err());
}

#[test]
fn a_trial_proof_does_not_confirm_normal_boot_or_another_context() {
    let fixture = Fixture::new();
    let (mut store, mut boot) = fixture.promoted();
    let json = serde_json::to_value(&store.record).unwrap();
    let proof = serde_json::from_value(json["trial_health"].clone()).unwrap();
    boot.tryboot = false;
    boot.trial_id = None;
    boot.boot_id = "bb606400-dc75-446e-b26b-385592b0f562".into();
    boot.uptime_ms = 65_000;
    store.advance(|record| record.reconcile(&boot)).unwrap();
    assert!(
        store
            .advance(|record| record.healthy(proof, &boot))
            .is_err()
    );
    let mut changed = json;
    changed["inputs"]["root_install_sha256"] =
        serde_json::to_value(Sha256::digest(b"another write")).unwrap();
    assert!(
        serde_json::from_value::<TrialRecord>(changed)
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        store
            .advance(|record| {
                record.inputs.root_install_sha256 = Sha256::digest(b"different");
                Ok(TrialAction::Observe)
            })
            .is_err()
    );
}
