use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

const BOOT: &str = "11111111-2222-3333-4444-555555555555";
const OTHER_BOOT: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";
const THIRD_BOOT: &str = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff";

fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
fn caller(pid: u32) -> Caller {
    Caller {
        uid: 1000,
        pid,
        process_start_ticks: 1234,
    }
}
fn token(n: u8) -> HoldToken {
    HoldToken::parse(format!("{n:064x}")).unwrap()
}

#[test]
fn download_reservation_requires_owned_opportunity_and_preserves_its_absolute_cap() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    enable(&mut policy);
    let root = Caller {
        uid: 0,
        ..caller(1)
    };
    let mode = policy.status(&clock(0)).unwrap().mode_generation;
    let mut request = DownloadReservation {
        deadline: clock(DOWNLOAD_MS),
        admission_deadline_ms: CHECK_MS,
        mode_generation: Some(mode),
        time_hold: None,
        handoff_deadline_ms: None,
    };
    assert!(
        policy
            .reserve_download(&token(2), caller(1), &clock(1), &request, true)
            .is_err()
    );
    assert!(
        policy
            .reserve_download(&token(2), root, &clock(1), &request, false)
            .is_err()
    );
    policy
        .reserve(&token(1), root, HoldKind::TimeCheck, &clock(1))
        .unwrap();
    request.time_hold = Some(token(1));
    request.handoff_deadline_ms = Some(100_000);
    assert!(
        policy
            .reserve_download(
                &token(2),
                Caller { pid: 2, ..root },
                &clock(1),
                &request,
                false
            )
            .is_err()
    );
    policy
        .reserve_download(&token(2), root, &clock(1), &request, false)
        .unwrap();
    policy.release(&token(1), root, &clock(2)).unwrap();
    // A lost admission reply is idempotent after the parent has released.
    policy
        .reserve_download(&token(2), root, &clock(2), &request, false)
        .unwrap();
    assert_eq!(policy.status(&clock(2)).unwrap().holds.len(), 1);
    // Another token cannot reuse the completed parent even on an available link.
    assert!(
        policy
            .reserve_download(&token(3), root, &clock(2), &request, true)
            .is_err()
    );
    request.deadline.ms += 1;
    request.admission_deadline_ms += 1;
    assert!(
        policy
            .reserve_download(&token(2), root, &clock(2), &request, false)
            .is_err()
    );
    for ms in (60_000..DOWNLOAD_MS).step_by(60_000) {
        policy.renew(&token(2), root, &clock(ms)).unwrap();
    }
    assert_eq!(
        policy.status(&clock(DOWNLOAD_MS - 1)).unwrap().holds[0].expires_in_ms,
        1
    );
    assert!(policy.renew(&token(2), root, &clock(DOWNLOAD_MS)).is_err());
}

#[test]
fn download_manual_admission_and_automatic_mode_guards_reject_delayed_requests() {
    for case in ["manual", "mode", "boot", "late", "future", "parent-kind"] {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
        enable(&mut policy);
        let root = Caller {
            uid: 0,
            ..caller(1)
        };
        let mut request = DownloadReservation {
            deadline: clock(DOWNLOAD_MS),
            admission_deadline_ms: CHECK_MS,
            mode_generation: Some(policy.status(&clock(0)).unwrap().mode_generation),
            time_hold: None,
            handoff_deadline_ms: None,
        };
        let at = match case {
            "manual" => {
                request.mode_generation = None;
                request.admission_deadline_ms = 30_000;
                policy
                    .reserve_download(&token(1), root, &clock(29_999), &request, false)
                    .unwrap();
                30_000
            }
            "mode" => {
                request.mode_generation = Some(0);
                1
            }
            "boot" => {
                request.deadline.boot_id = OTHER_BOOT.into();
                1
            }
            "late" => CHECK_MS,
            "future" => {
                request.deadline.ms += 100;
                request.admission_deadline_ms += 100;
                1
            }
            _ => {
                policy
                    .reserve(&token(1), root, HoldKind::Setup, &clock(0))
                    .unwrap();
                request.time_hold = Some(token(1));
                request.handoff_deadline_ms = Some(100_000);
                1
            }
        };
        assert!(
            policy
                .reserve_download(&token(2), root, &clock(at), &request, true)
                .is_err(),
            "{case}"
        );
    }
}

#[test]
fn wireless_fallback_preserves_the_time_hold_budget_and_owner() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    policy
        .reserve(&token(1), caller(1), HoldKind::TimeCheck, &clock(0))
        .unwrap();
    policy
        .reserve(&token(2), caller(1), HoldKind::Download, &clock(0))
        .unwrap();
    assert!(
        policy
            .require_wireless(&token(1), caller(2), &clock(15_000))
            .is_err()
    );
    assert!(
        policy
            .require_wireless(&token(2), caller(1), &clock(15_000))
            .is_err()
    );
    assert!(
        policy
            .require_wireless(&token(3), caller(1), &clock(15_000))
            .is_err()
    );
    policy
        .require_wireless(&token(1), caller(1), &clock(15_000))
        .unwrap();
    let status = policy.status(&clock(15_000)).unwrap();
    let hold = status
        .holds
        .iter()
        .find(|h| h.kind == HoldKind::TimeCheck)
        .unwrap();
    assert!(hold.wireless_required);
    assert_eq!(hold.expires_in_ms, 105_000);
    policy
        .require_wireless(&token(1), caller(1), &clock(16_000))
        .unwrap();
    assert_eq!(
        policy.status(&clock(16_000)).unwrap().generation,
        status.generation
    );
    policy.renew(&token(1), caller(1), &clock(119_999)).unwrap();
    assert!(
        policy
            .require_wireless(&token(1), caller(1), &clock(120_000))
            .is_err()
    );
    policy
        .release(&token(1), caller(1), &clock(120_000))
        .unwrap();
    assert_eq!(policy.status(&clock(120_000)).unwrap().holds.len(), 1);
}

#[test]
fn scheduler_reservations_bind_boot_admission_deadline_mode_and_original_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    enable(&mut policy);
    let root = Caller {
        uid: 0,
        ..caller(1)
    };
    let mode = policy.status(&clock(0)).unwrap().mode_generation;
    for (who, end, admit, expected) in [
        (caller(1), clock(120_000), 30_000, Some(mode)),
        (
            root,
            Clock {
                boot_id: OTHER_BOOT.into(),
                ms: 120_000,
            },
            30_000,
            Some(mode),
        ),
        (root, clock(120_000), 0, Some(mode)),
        (root, clock(120_001), 30_000, Some(mode)),
        (root, clock(120_000), 30_000, Some(mode + 1)),
    ] {
        assert!(
            policy
                .reserve_time(&token(1), who, &clock(0), &end, admit, expected)
                .is_err()
        );
    }
    policy
        .reserve_time(
            &token(1),
            root,
            &clock(20_000),
            &clock(120_000),
            30_000,
            Some(mode),
        )
        .unwrap();
    assert_eq!(
        policy.status(&clock(20_000)).unwrap().holds[0].expires_in_ms,
        100_000
    );
    policy
        .reserve_time(
            &token(1),
            root,
            &clock(29_999),
            &clock(120_000),
            30_000,
            Some(mode),
        )
        .unwrap();
    assert!(
        policy
            .reserve_time(
                &token(2),
                root,
                &clock(30_000),
                &clock(120_000),
                30_000,
                Some(mode)
            )
            .is_err()
    );
    assert!(
        policy
            .reserve_time(
                &token(1),
                root,
                &clock(30_000),
                &clock(150_000),
                150_000,
                Some(mode)
            )
            .is_err()
    );
    policy
        .reserve_time(
            &token(1),
            root,
            &clock(30_000),
            &clock(120_000),
            120_000,
            Some(mode),
        )
        .unwrap();
    assert_eq!(
        policy.status(&clock(30_000)).unwrap().holds[0].expires_in_ms,
        90_000
    );
}

#[test]
fn mode_generation_distinguishes_off_on_from_hold_updates_and_stay_connected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    enable(&mut policy);
    let first = policy.status(&clock(0)).unwrap();
    policy
        .set_policy(
            first.generation,
            Policy {
                stay_connected: true,
                ..enabled()
            },
            &clock(0),
        )
        .unwrap();
    policy
        .reserve(&token(1), caller(1), HoldKind::Setup, &clock(0))
        .unwrap();
    assert_eq!(
        policy.status(&clock(0)).unwrap().mode_generation,
        first.mode_generation
    );
    for partial_offline in [false, true] {
        let generation = policy.status(&clock(0)).unwrap().generation;
        policy
            .set_policy(
                generation,
                Policy {
                    partial_offline,
                    stay_connected: true,
                },
                &clock(0),
            )
            .unwrap();
    }
    let mode = policy.status(&clock(0)).unwrap().mode_generation;
    assert_eq!(mode, first.mode_generation + 2);
    assert!(
        policy
            .reserve_time(
                &token(2),
                Caller {
                    uid: 0,
                    ..caller(1)
                },
                &clock(0),
                &clock(120_000),
                30_000,
                Some(first.mode_generation)
            )
            .is_err()
    );
    drop(policy);
    assert_eq!(
        NetworkPolicy::open(&path, clock(1))
            .unwrap()
            .status(&clock(1))
            .unwrap()
            .mode_generation,
        mode
    );
}

#[test]
fn legacy_network_record_without_mode_generation_retains_saved_policy_and_checksum() {
    use sha2::Digest;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    drop(NetworkPolicy::open(&path, clock(0)).unwrap());
    let mut record = Record::initial();
    record.generation = 8;
    record.policy = enabled();
    let bytes = serde_json::to_string(&record).unwrap();
    assert!(!bytes.contains("mode_generation"));
    let checksum = format!("{:x}", sha2::Sha256::digest(bytes.as_bytes()));
    fs::write(
        path.join("state.json"),
        format!("{{\"record\":{bytes},\"checksum\":\"{checksum}\"}}"),
    )
    .unwrap();
    let mut policy = NetworkPolicy::open(&path, clock(1)).unwrap();
    let status = policy.status(&clock(1)).unwrap();
    assert_eq!(status.policy, enabled());
    assert_eq!(status.mode_generation, 0);
    policy
        .set_policy(status.generation, Policy::default(), &clock(1))
        .unwrap();
    assert_eq!(policy.status(&clock(1)).unwrap().mode_generation, 1);
}
fn idle() -> Activity {
    Activity {
        ssh_sessions: Some(false),
        checkpoints: Some(false),
        install_lock: Some(false),
        operations: Some(false),
    }
}
fn enabled() -> Policy {
    Policy {
        partial_offline: true,
        stay_connected: false,
    }
}
fn enable(policy: &mut NetworkPolicy) {
    let generation = policy.status(&clock(0)).unwrap().generation;
    policy.set_policy(generation, enabled(), &clock(0)).unwrap();
}
fn observe_through(policy: &mut NetworkPolicy, begin: u64, end: u64, activity: Activity) {
    for ms in (begin..=end).step_by(1000) {
        policy.observe(activity, &clock(ms)).unwrap();
    }
}

#[test]
fn ssh_fallback_preserves_saved_bytes_and_excludes_an_active_coordinator() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    protect_ssh_without_service(&path).unwrap();
    assert!(!path.exists());
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    enable(&mut policy);
    policy
        .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
        .unwrap();
    assert!(protect_ssh_without_service(&path).is_err());
    let bytes = fs::read(path.join("state.json")).unwrap();
    drop(policy);
    let guard = protect_ssh_without_service(&path).unwrap();
    assert!(NetworkPolicy::open(&path, clock(0)).is_err());
    assert_eq!(fs::read(path.join("state.json")).unwrap(), bytes);
    drop(guard);
    NetworkPolicy::open(&path, clock(0)).unwrap();
    fs::write(path.join("state.json"), b"damaged").unwrap();
    assert!(protect_ssh_without_service(&path).is_err());
    fs::remove_file(path.join("state.json")).unwrap();
    assert!(protect_ssh_without_service(&path).is_err());
    assert!(!path.join("state.json").exists());
}

#[test]
fn defaults_leave_networking_alone_and_saved_override_remains_separate() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    assert_eq!(policy.status(&clock(0)).unwrap().policy, Policy::default());
    policy
        .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
        .unwrap();
    assert_eq!(policy.status(&clock(0)).unwrap().intent, Intent::Unmanaged);
    let generation = policy.status(&clock(0)).unwrap().generation;
    let saved = Policy {
        partial_offline: true,
        stay_connected: true,
    };
    policy.set_policy(generation, saved, &clock(0)).unwrap();
    assert!(
        policy
            .set_policy(generation, Policy::default(), &clock(0))
            .is_err()
    );
    policy.release(&token(1), caller(2), &clock(1)).unwrap();
    drop(policy);
    let mut policy = NetworkPolicy::open(&path, clock(2)).unwrap();
    assert_eq!(policy.status(&clock(2)).unwrap().policy, saved);
    let generation = policy.status(&clock(2)).unwrap().generation;
    policy
        .set_policy(
            generation,
            Policy {
                partial_offline: false,
                ..saved
            },
            &clock(2),
        )
        .unwrap();
    assert_eq!(policy.status(&clock(2)).unwrap().intent, Intent::Unmanaged);
    assert!(policy.status(&clock(2)).unwrap().policy.stay_connected);
}

#[test]
fn temporary_holds_require_the_same_process_and_hide_capabilities_in_status() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    enable(&mut policy);
    let secret = HoldToken::generate().unwrap();
    policy
        .reserve(&secret, caller(1), HoldKind::Setup, &clock(0))
        .unwrap();
    policy
        .reserve(&token(2), caller(2), HoldKind::Download, &clock(0))
        .unwrap();
    let status = policy.status(&clock(0)).unwrap();
    assert_eq!(status.intent, Intent::Available);
    assert_eq!(status.holds.len(), 2);
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains(secret.expose())
    );
    assert!(!format!("{secret:?}").contains(secret.expose()));
    assert!(
        !fs::read_to_string(path.join("state.json"))
            .unwrap()
            .contains(secret.expose())
    );
    assert!(policy.release(&secret, caller(2), &clock(1)).is_err());
    assert!(
        policy
            .renew(
                &secret,
                Caller {
                    process_start_ticks: 1235,
                    ..caller(1)
                },
                &clock(1)
            )
            .is_err()
    );
    assert!(
        policy
            .reserve(&secret, caller(1), HoldKind::Download, &clock(1))
            .is_err()
    );
    policy.release(&secret, caller(1), &clock(1)).unwrap();
    policy.release(&secret, caller(1), &clock(1)).unwrap();
    assert_eq!(policy.status(&clock(1)).unwrap().holds.len(), 1);
    drop(policy);
    let mut policy = NetworkPolicy::open(&path, clock(2)).unwrap();
    assert!(policy.status(&clock(2)).unwrap().holds.is_empty());
    assert!(policy.renew(&token(2), caller(2), &clock(2)).is_err());
}

#[test]
fn retry_is_idempotent_and_time_checks_and_downloads_have_absolute_limits() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    for (id, kind) in [
        (1, HoldKind::TimeCheck),
        (2, HoldKind::Download),
        (3, HoldKind::Setup),
    ] {
        policy
            .reserve(&token(id), caller(1), kind, &clock(0))
            .unwrap();
    }
    let generation = policy.status(&clock(0)).unwrap().generation;
    policy
        .reserve(&token(2), caller(1), HoldKind::Download, &clock(30_000))
        .unwrap();
    assert_eq!(
        policy.status(&clock(30_000)).unwrap().generation,
        generation
    );
    policy.renew(&token(1), caller(1), &clock(30_000)).unwrap();
    assert_eq!(policy.temporary[&token(1).id()].expires_ms, CHECK_MS);
    for ms in (30_000..DOWNLOAD_MS).step_by(30_000) {
        policy.renew(&token(2), caller(1), &clock(ms)).unwrap();
        policy.renew(&token(3), caller(1), &clock(ms)).unwrap();
        assert!(policy.temporary[&token(2).id()].expires_ms <= DOWNLOAD_MS);
    }
    assert!(
        policy
            .renew(&token(2), caller(1), &clock(DOWNLOAD_MS))
            .is_err()
    );
    assert!(
        policy
            .reserve(
                &token(2),
                caller(1),
                HoldKind::Download,
                &clock(DOWNLOAD_MS)
            )
            .is_err()
    );
    assert!(policy.temporary[&token(3).id()].expires_ms > DOWNLOAD_MS);
}

#[test]
fn deployment_survives_command_gaps_and_reboot_grace_cannot_reset_on_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    policy
        .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(10))
        .unwrap();
    let before = policy.record.deployments[0].expires_ms;
    drop(policy);
    let mut policy = NetworkPolicy::open(&path, clock(100)).unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, before);
    assert!(
        policy
            .release(
                &token(1),
                Caller {
                    uid: 0,
                    ..caller(1)
                },
                &clock(100)
            )
            .is_err()
    );
    policy
        .reserve(&token(1), caller(222), HoldKind::Deployment, &clock(100))
        .unwrap();
    drop(policy);
    let next = Clock {
        boot_id: OTHER_BOOT.into(),
        ms: 500,
    };
    let policy = NetworkPolicy::open(&path, next.clone()).unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, 500 + RECONNECT_MS);
    assert!(policy.record.deployments[0].reboot_grace_used);
    drop(policy);
    let policy = NetworkPolicy::open(
        &path,
        Clock {
            ms: 50_000,
            ..next.clone()
        },
    )
    .unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, 500 + RECONNECT_MS);
    drop(policy);
    let third = Clock {
        boot_id: THIRD_BOOT.into(),
        ms: 200,
    };
    let mut policy = NetworkPolicy::open(&path, third.clone()).unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, 200);
    assert!(policy.renew(&token(1), caller(3), &third).is_err());
    policy.release(&token(1), caller(3), &third).unwrap();
    policy
        .reserve(&token(2), caller(3), HoldKind::Deployment, &third)
        .unwrap();
    assert!(!policy.record.deployments[0].reboot_grace_used);
}

#[test]
fn explicit_deployment_renewal_restores_a_bounded_reconnect_allowance() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    policy
        .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
        .unwrap();
    drop(policy);
    let next = Clock {
        boot_id: OTHER_BOOT.into(),
        ms: 10,
    };
    let mut policy = NetworkPolicy::open(&path, next.clone()).unwrap();
    policy.renew(&token(1), caller(99), &next).unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, 10 + DEPLOYMENT_MS);
    drop(policy);
    let policy = NetworkPolicy::open(
        &path,
        Clock {
            boot_id: THIRD_BOOT.into(),
            ms: 20,
        },
    )
    .unwrap();
    assert_eq!(policy.record.deployments[0].expires_ms, 20 + RECONNECT_MS);
}

#[test]
fn unknown_or_live_activity_never_reaps_expired_holds_or_allows_shutdown() {
    for activity in [
        Activity::default(),
        Activity {
            ssh_sessions: Some(true),
            ..idle()
        },
        Activity {
            checkpoints: Some(true),
            ..idle()
        },
        Activity {
            install_lock: Some(true),
            ..idle()
        },
        Activity {
            operations: Some(true),
            ..idle()
        },
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
        enable(&mut policy);
        policy
            .reserve(&token(1), caller(1), HoldKind::Setup, &clock(0))
            .unwrap();
        observe_through(&mut policy, 0, 300_000, activity);
        assert_eq!(policy.status(&clock(300_000)).unwrap().holds.len(), 1);
        observe_through(&mut policy, 301_000, 310_000, idle());
        assert_eq!(policy.status(&clock(310_000)).unwrap().holds.len(), 1);
        policy.observe(idle(), &clock(311_000)).unwrap();
        let status = policy.status(&clock(311_000)).unwrap();
        assert!(status.holds.is_empty());
        assert_eq!(status.intent, Intent::MayDisable);
        assert_eq!(
            policy.status(&clock(317_000)).unwrap().intent,
            Intent::ObserveActivity
        );
    }
}

#[test]
fn setup_expiration_and_service_restart_allow_checkpoint_recovery_and_quiet_grace() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
    enable(&mut policy);
    policy
        .reserve(&token(1), caller(1), HoldKind::Setup, &clock(0))
        .unwrap();
    observe_through(&mut policy, 0, SETUP_MS + CHECKPOINT_MS - 1000, idle());
    assert_eq!(
        policy
            .status(&clock(SETUP_MS + CHECKPOINT_MS - 1000))
            .unwrap()
            .holds
            .len(),
        1
    );
    policy
        .observe(idle(), &clock(SETUP_MS + CHECKPOINT_MS))
        .unwrap();
    assert!(
        policy
            .status(&clock(SETUP_MS + CHECKPOINT_MS))
            .unwrap()
            .holds
            .is_empty()
    );
    drop(policy);
    let mut policy = NetworkPolicy::open(&path, clock(300_000)).unwrap();
    observe_through(&mut policy, 300_000, 409_000, idle());
    assert_eq!(
        policy.status(&clock(409_000)).unwrap().intent,
        Intent::RecoverOperations
    );
    policy.observe(idle(), &clock(410_000)).unwrap();
    assert_eq!(
        policy.status(&clock(410_000)).unwrap().intent,
        Intent::MayDisable
    );
}

#[test]
fn observation_gaps_stale_settings_and_clock_regression_revoke_the_decision() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    enable(&mut policy);
    policy.observe(idle(), &clock(0)).unwrap();
    policy.observe(idle(), &clock(200_000)).unwrap();
    assert_eq!(
        policy.status(&clock(200_000)).unwrap().intent,
        Intent::QuietGrace
    );
    observe_through(&mut policy, 201_000, 210_000, idle());
    assert_eq!(
        policy.status(&clock(210_000)).unwrap().intent,
        Intent::MayDisable
    );
    policy
        .release(&token(255), caller(1), &clock(210_000))
        .unwrap();
    assert_eq!(
        policy.status(&clock(210_000)).unwrap().intent,
        Intent::MayDisable
    );
    let generation = policy.status(&clock(210_000)).unwrap().generation;
    policy
        .reserve(&token(1), caller(1), HoldKind::TimeCheck, &clock(210_000))
        .unwrap();
    assert!(
        policy
            .set_policy(generation, Policy::default(), &clock(210_000))
            .is_err()
    );
    assert!(policy.status(&clock(209_999)).is_err());
    assert!(
        policy
            .status(&Clock {
                boot_id: OTHER_BOOT.into(),
                ms: 210_000
            })
            .is_err()
    );
    assert_eq!(
        policy.status(&clock(210_000)).unwrap().intent,
        Intent::Available
    );
}

#[test]
fn new_ssh_admission_replaces_expired_ssh_holds_without_requiring_idle() {
    for activity in [
        Activity::default(),
        Activity {
            ssh_sessions: Some(true),
            ..idle()
        },
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
        enable(&mut policy);
        let root = Caller {
            uid: 0,
            ..caller(1)
        };
        policy
            .reserve(&token(0), caller(1), HoldKind::Setup, &clock(0))
            .unwrap();
        policy
            .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
            .unwrap();
        for n in 2..MAX_HOLDS as u8 {
            policy
                .reserve(&token(n), root, HoldKind::Ssh, &clock(u64::from(n == 31)))
                .unwrap();
        }
        observe_through(&mut policy, 1, SETUP_MS, activity);
        assert_eq!(
            policy.status(&clock(SETUP_MS)).unwrap().holds.len(),
            MAX_HOLDS
        );
        assert!(
            policy
                .reserve(&token(40), caller(2), HoldKind::Ssh, &clock(SETUP_MS))
                .is_err()
        );
        assert!(
            policy
                .reserve(&token(40), caller(2), HoldKind::Setup, &clock(SETUP_MS))
                .is_err()
        );
        assert!(
            policy
                .reserve(&token(2), root, HoldKind::Ssh, &clock(SETUP_MS))
                .is_err()
        );
        policy
            .reserve(&token(40), root, HoldKind::Ssh, &clock(SETUP_MS))
            .unwrap();
        let status = policy.status(&clock(SETUP_MS)).unwrap();
        assert_eq!(status.holds.len(), 4);
        assert_eq!(status.intent, Intent::Available);
        assert!(policy.temporary.contains_key(&token(0).id()));
        assert!(policy.temporary.contains_key(&token(31).id()));
        assert!(policy.temporary.contains_key(&token(40).id()));
        assert_eq!(policy.record.deployments.len(), 1);
        assert_eq!(policy.temporary[&token(40).id()].expires_ms, SETUP_MS * 2);
        // A lost reply rejoins the new reservation without extending its cap.
        policy
            .reserve(&token(40), root, HoldKind::Ssh, &clock(SETUP_MS + 1))
            .unwrap();
        assert_eq!(policy.temporary[&token(40).id()].expires_ms, SETUP_MS * 2);
    }
}

#[test]
fn ssh_replacement_preserves_capacity_and_reservations_if_the_commit_fails() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
        let root = Caller {
            uid: 0,
            ..caller(1)
        };
        for n in 0..MAX_HOLDS as u8 {
            policy
                .reserve(&token(n), root, HoldKind::Ssh, &clock(0))
                .unwrap();
        }
        assert!(
            policy
                .reserve(&token(40), root, HoldKind::Ssh, &clock(SETUP_MS - 1))
                .is_err()
        );
        policy.fail_save_at(point);
        assert!(
            policy
                .reserve(&token(40), root, HoldKind::Ssh, &clock(SETUP_MS))
                .is_err()
        );
        assert_eq!(policy.temporary.len(), MAX_HOLDS);
        assert!(!policy.temporary.contains_key(&token(40).id()));
        assert!(policy.status(&clock(SETUP_MS)).is_err());
    }
}

#[test]
fn bounded_ownership_cannot_be_exhausted_by_idempotent_retries() {
    let temp = tempfile::tempdir().unwrap();
    let mut policy = NetworkPolicy::open(&temp.path().join("network"), clock(0)).unwrap();
    for n in 0..MAX_HOLDS as u8 {
        policy
            .reserve(&token(n), caller(1), HoldKind::Setup, &clock(0))
            .unwrap();
    }
    assert!(
        policy
            .reserve(&token(40), caller(1), HoldKind::Setup, &clock(0))
            .is_err()
    );
    policy
        .reserve(&token(1), caller(1), HoldKind::Setup, &clock(0))
        .unwrap();
    assert_eq!(policy.status(&clock(0)).unwrap().holds.len(), MAX_HOLDS);
    assert!(HoldToken::parse("../path".into()).is_err());
    assert!(HoldToken::parse("A".repeat(64)).is_err());
}

#[test]
fn corrupt_missing_linked_or_changed_state_cannot_reset_policy_or_deployments() {
    for change in [
        "corrupt",
        "missing",
        "symlink",
        "hardlink",
        "public",
        "directory",
        "lock",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
        policy
            .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
            .unwrap();
        let state = path.join("state.json");
        let original = fs::read(&state).unwrap();
        match change {
            "corrupt" => fs::write(&state, b"{}").unwrap(),
            "missing" => fs::remove_file(&state).unwrap(),
            "symlink" => {
                fs::rename(&state, temp.path().join("foreign")).unwrap();
                symlink(temp.path().join("foreign"), &state).unwrap();
            }
            "hardlink" => fs::hard_link(&state, temp.path().join("alias")).unwrap(),
            "public" => fs::set_permissions(&state, fs::Permissions::from_mode(0o644)).unwrap(),
            "directory" => fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap(),
            "lock" => fs::remove_file(path.join("lock")).unwrap(),
            _ => unreachable!(),
        }
        assert!(policy.status(&clock(0)).is_err(), "{change}");
        assert!(
            policy.release(&token(1), caller(1), &clock(0)).is_err(),
            "{change}"
        );
        drop(policy);
        assert!(NetworkPolicy::open(&path, clock(1)).is_err(), "{change}");
        if change == "symlink" {
            assert_eq!(fs::read(temp.path().join("foreign")).unwrap(), original);
        }
    }
}

#[test]
fn flush_failures_release_no_usable_status_and_reopen_preserves_whichever_record_committed() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let mut policy = NetworkPolicy::open(&path, clock(0)).unwrap();
        let generation = policy.status(&clock(0)).unwrap().generation;
        policy
            .set_policy(
                generation,
                Policy {
                    partial_offline: true,
                    stay_connected: true,
                },
                &clock(0),
            )
            .unwrap();
        policy.store.fail_at = Some(point);
        assert!(
            policy
                .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(0))
                .is_err()
        );
        assert!(policy.status(&clock(0)).is_err());
        assert!(
            policy
                .reserve(&token(2), caller(1), HoldKind::Deployment, &clock(0))
                .is_err()
        );
        drop(policy);
        let mut policy = NetworkPolicy::open(&path, clock(1)).unwrap();
        assert!(policy.status(&clock(1)).unwrap().policy.stay_connected);
        let committed = matches!(point, "renamed" | "directory-synced");
        assert_eq!(policy.record.deployments.len(), usize::from(committed));
        policy
            .reserve(&token(1), caller(1), HoldKind::Deployment, &clock(1))
            .unwrap();
        assert_eq!(policy.record.deployments.len(), 1);
        assert!(NetworkPolicy::open(&path, clock(1)).is_err());
    }
}
