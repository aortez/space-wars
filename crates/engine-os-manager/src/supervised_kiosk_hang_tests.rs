use super::*;
use engine_ota::boot_linux::PhysicalIdentity;
use engine_ota::recovery::{BootObservation, Slot};
use engine_ota::recovery_linux::{DeviceNumber, DiskLayout, Partition};
use std::cell::RefCell;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::{Command, Stdio};

fn fixture() -> (Expectation, BootEvidence, Envelope, Early) {
    let image = ImageIdentity::parse(include_bytes!(
        "../../engine-ota/tests/fixtures/image-identity-v1.json"
    ))
    .unwrap();
    let binding = BootBinding {
        device_id: Sha256::digest(b"synthetic-board"),
        layout_id: Sha256::digest(b"synthetic-layout"),
        boot_identity: image.boot_identity,
        hardware_profile: Sha256::digest(b"synthetic-profile"),
    };
    let previous_good = SlotImage {
        slot: Slot::B,
        build_id: "old-good".into(),
        generation: 12,
        payload: Sha256::digest(b"old"),
        expanded: Sha256::digest(b"old-root"),
        data: image.data,
    };
    let candidate = SlotImage {
        slot: Slot::A,
        build_id: image.build_id.clone(),
        generation: 13,
        payload: Sha256::digest(b"new"),
        expanded: Sha256::digest(b"new-root"),
        data: image.data,
    };
    let origin = "11111111-1111-1111-1111-111111111111";
    let trial = "22222222-2222-2222-2222-222222222222";
    let candidate_id = Sha256::digest(b"candidate");
    // Construct a synthetic durable journal, using the documented transaction
    // encoding. This grants no native installation capability or approval.
    let transaction =
        Sha256::digest(&serde_json::to_vec(&(9, candidate_id, origin, &binding)).unwrap());
    let record: RecoveryRecord = serde_json::from_value(json!({
        "schema":2, "binding":binding, "good":previous_good,
        "retained_good":null, "good_boot":null, "highest_attempted_generation":13,
        "sequence":9, "failures":{},
        "transaction":{"id":transaction, "candidate_id":candidate_id,
            "previous_good":previous_good,"candidate":candidate,"attempt":1,
            "origin_boot_id":origin,"trial_boot_id":trial,"phase":"observing",
            "health":null,"failure":null}
    }))
    .unwrap();
    record.validate().unwrap();
    let expected = Expectation {
        schema: 1,
        binding: binding.clone(),
        kernel_build_id: "a".repeat(40),
        image: image.clone(),
        transaction,
        origin_boot_id: origin.into(),
        trial_boot_id: trial.into(),
        previous_good,
        candidate,
    };
    let boot = BootEvidence {
        physical: PhysicalIdentity {
            model: "synthetic".into(),
            serial: "synthetic".into(),
            bootloader_revision: "a".repeat(40),
            medium_sectors: 100_000_000,
            medium: Default::default(),
        },
        layout: DiskLayout {
            disk_name: "fixture".into(),
            disk_device: DeviceNumber { major: 8, minor: 0 },
            disk_sectors: 100_000_000,
            active: Slot::A,
            layout_id: binding.layout_id,
            partitions: std::array::from_fn(|i| Partition {
                number: i as u32 + 1,
                name: format!("fixture{i}"),
                device: DeviceNumber {
                    major: 8,
                    minor: i as u32 + 1,
                },
                start_sector: 8192 + i as u64 * 6291456,
                sectors: 6291456,
                partuuid: format!("076c4a2a-{:02}", i + 1),
            }),
        },
        kernel_build_id: expected.kernel_build_id.clone(),
        hardware_profile: "picade".into(),
        image,
        observation: BootObservation {
            binding,
            boot_id: trial.into(),
            uptime_ms: 30_000,
            actual_slot: Some(Slot::A),
            actual_build_id: Some(expected.image.build_id.clone()),
            recovery_protocol: 2,
            default_slot: Some(Slot::B),
            tryboot: true,
            trial_id: Some(transaction),
        },
    };
    let early = Early {
        schema: 1,
        boot_id: trial.into(),
        trial_id: transaction,
        armed_boottime_ms: 500,
        timeout_seconds: 210,
    };
    let journal = Envelope {
        checksum: record.fingerprint(),
        record,
    };
    (expected, boot, journal, early)
}
fn plan() -> Plan {
    let (expectation, _, _, _) = fixture();
    Plan {
        health: KioskHealth {
            boot_id: expectation.trial_boot_id.clone(),
            uptime_ms: 29_000,
            process_id: 1234,
            start_ticks: 987,
            invocation_id: "b".repeat(32),
            build_id: expectation.image.build_id.clone(),
            control_response: 10,
            presented_frames: 100,
        },
        expectation,
        selected_ms: 30_000,
    }
}

#[test]
fn exact_early_trial_is_required() {
    let (expected, boot, journal, early) = fixture();
    expected.matches(&boot, &journal, &early).unwrap();
    for case in 0..12 {
        let (_, mut b, mut j, mut e) = fixture();
        match case {
            0 => b.observation.tryboot = false,
            1 => b.observation.boot_id = expected.origin_boot_id.clone(),
            2 => b.observation.actual_slot = Some(Slot::B),
            3 => b.observation.default_slot = Some(Slot::A),
            4 => b.observation.uptime_ms = 45_000,
            5 => b.kernel_build_id = "b".repeat(40),
            6 => b.observation.binding.device_id = Sha256::digest(b"another-board"),
            7 => j.checksum = Sha256::digest(b"bad-checksum"),
            8 => e.trial_id = Sha256::digest(b"old-transaction"),
            9 => e.boot_id = expected.origin_boot_id.clone(),
            10 => e.armed_boottime_ms = 30_001,
            11 => e.timeout_seconds = 211,
            _ => unreachable!(),
        }
        assert!(expected.matches(&b, &j, &e).is_err(), "case {case}");
    }
    for field in ["candidate", "previous_good"] {
        let mut value = serde_json::to_value(&expected).unwrap();
        value[field]["payload"] = json!(Sha256::digest(b"wrong-payload"));
        let changed: Expectation = serde_json::from_value(value).unwrap();
        assert!(changed.matches(&boot, &journal, &early).is_err());
    }
    let mut value = serde_json::to_value(&journal.record).unwrap();
    value["transaction"]["phase"] = json!("trial-requested");
    value["transaction"]["trial_boot_id"] = Value::Null;
    let record: RecoveryRecord = serde_json::from_value(value).unwrap();
    let old = Envelope {
        checksum: record.fingerprint(),
        record,
    };
    assert!(expected.matches(&boot, &old, &early).is_err());
}

#[test]
fn plan_digest_and_deadlines_bind_transport() {
    let plan = plan();
    plan_time(&plan, 30_000).unwrap();
    for now in [29_999, 40_000, 45_000] {
        assert!(plan_time(&plan, now).is_err());
    }
    let serialized = serde_json::to_string_pretty(&plan).unwrap();
    let parsed: Plan = serde_json::from_str(&serialized).unwrap();
    assert_eq!(plan.fingerprint().unwrap(), parsed.fingerprint().unwrap());
    let mut changed = parsed.clone();
    changed.health.start_ticks += 1;
    assert_ne!(plan.fingerprint().unwrap(), changed.fingerprint().unwrap());
    changed = parsed.clone();
    changed.health.boot_id = plan.expectation.origin_boot_id.clone();
    assert!(plan_time(&changed, 30_000).is_err());
    let mut value = serde_json::to_value(&plan).unwrap();
    value["extra"] = json!(true);
    assert!(serde_json::from_value::<Plan>(value).is_err());
}

#[test]
fn signal_requires_durable_consumption_and_final_recheck() {
    for fail in ["consume", "recheck", "stop", "postcheck", "none"] {
        let events = RefCell::new(Vec::new());
        let result = one_stop(
            || {
                events.borrow_mut().push("consume");
                require(fail != "consume", "consume")
            },
            || {
                events.borrow_mut().push("recheck");
                require(fail != "recheck", "recheck")
            },
            |signal| {
                let name = if signal == libc::SIGSTOP {
                    "stop"
                } else {
                    "resume"
                };
                events.borrow_mut().push(name);
                require(fail != "stop", "signal")
            },
            || {
                events.borrow_mut().push("postcheck");
                require(fail != "postcheck", "late")
            },
        );
        let expected = match fail {
            "consume" => vec!["consume"],
            "recheck" => vec!["consume", "recheck"],
            "stop" => vec!["consume", "recheck", "stop"],
            "postcheck" => vec!["consume", "recheck", "stop", "postcheck", "resume"],
            _ => vec!["consume", "recheck", "stop", "postcheck"],
        };
        assert_eq!(*events.borrow(), expected);
        assert_eq!(result.is_ok(), fail == "none");
    }
}

#[test]
fn consumed_or_partial_record_cannot_be_replayed_or_linked() {
    let dir = tempfile::tempdir().unwrap();
    let parent = File::open(dir.path()).unwrap();
    let receipt = json!({"consumed":true});
    create_record(&parent, "transaction.json", &receipt).unwrap();
    assert!(create_record(&parent, "transaction.json", &receipt).is_err());
    std::fs::write(dir.path().join("partial.json"), b"").unwrap();
    assert!(create_record(&parent, "partial.json", &receipt).is_err());
    symlink(dir.path().join("elsewhere"), dir.path().join("linked.json")).unwrap();
    assert!(create_record(&parent, "linked.json", &receipt).is_err());
    assert!(!dir.path().join("elsewhere").exists());
    std::fs::create_dir(dir.path().join("public")).unwrap();
    std::fs::set_permissions(
        dir.path().join("public"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(directory(&parent, "public", unsafe { libc::geteuid() }, true).is_err());
}

#[test]
fn actual_pidfd_targets_only_its_child_and_does_not_retarget_after_exit() {
    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    // Reap even if an assertion fails; never leave a stopped child behind.
    struct ChildGuard<'a>(&'a mut std::process::Child);
    impl Drop for ChildGuard<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let guard = ChildGuard(&mut child);
    let pinned = Pinned::open(guard.0.id()).unwrap();
    pinned.signal(libc::SIGSTOP).unwrap();
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", guard.0.id())).unwrap();
        if stat.rsplit_once(')').unwrap().1.split_whitespace().next() == Some("T") {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    pinned.signal(libc::SIGCONT).unwrap();
    guard.0.kill().unwrap();
    guard.0.wait().unwrap();
    assert!(pinned.signal(libc::SIGSTOP).is_err());
}
