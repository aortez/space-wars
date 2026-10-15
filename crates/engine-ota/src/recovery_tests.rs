use super::*;
use crate::{Blob, Channel, IMAGE_LAYOUT, ImageFormat, SchemaRange};

const OLD_BOOT: &str = "11111111-1111-1111-1111-111111111111";
const TRIAL_BOOT: &str = "22222222-2222-2222-2222-222222222222";
const NEXT_BOOT: &str = "33333333-3333-3333-3333-333333333333";

fn digest(text: &str) -> Sha256 {
    Sha256::digest(text.as_bytes())
}

fn descriptor(generation: u64) -> ImageDescriptor {
    ImageDescriptor {
        schema: 1,
        generation,
        source_commit: "a".repeat(40),
        build_id: format!("build-{generation}"),
        channel: Channel::Release,
        machine: "raspberrypi-spacewars".into(),
        architecture: "aarch64".into(),
        layout: IMAGE_LAYOUT.into(),
        format: ImageFormat::RootfsExt4Zstd,
        compressed: Blob {
            bytes: 40,
            sha256: digest(&format!("compressed-{generation}")),
        },
        expanded: Blob {
            bytes: 400,
            sha256: digest(&format!("expanded-{generation}")),
        },
        boot_identity: digest("boot"),
        min_updater_protocol: 1,
        min_recovery_protocol: 1,
        data: DataCompatibility {
            reads: SchemaRange { min: 1, max: 1 },
            writes: 1,
        },
    }
}

fn baseline() -> (RecoveryRecord, BootObservation, RecoveryCapability) {
    let binding = BootBinding {
        device_id: digest("board"),
        layout_id: digest("layout"),
        boot_identity: digest("boot"),
        hardware_profile: digest("picade"),
    };
    let image = SlotImage::from_descriptor(Slot::A, &descriptor(1));
    let boot = BootObservation {
        binding: binding.clone(),
        boot_id: OLD_BOOT.into(),
        uptime_ms: 1_000_000,
        actual_slot: Some(Slot::A),
        actual_build_id: Some(image.build_id.clone()),
        recovery_protocol: 1,
        default_slot: Some(Slot::A),
        tryboot: false,
        trial_id: None,
    };
    let cap = RecoveryCapability {
        binding: binding.clone(),
        tryboot_qualified: true,
        watchdog_qualified: true,
    };
    (
        RecoveryRecord::bootstrap(binding, image, &boot).unwrap(),
        boot,
        cap,
    )
}

fn prepared() -> (RecoveryRecord, BootObservation) {
    let (record, boot, cap) = baseline();
    let image = descriptor(2);
    let plan = record.begin_descriptor(&image, &boot, &cap, false).unwrap();
    let id = plan.record.transaction().unwrap().id;
    assert_eq!(
        plan.action,
        Action::WriteInactive {
            slot: Slot::B,
            transaction: id
        }
    );
    let plan = plan
        .record
        .written(id, image.compressed.sha256, image.expanded.sha256, &boot)
        .unwrap();
    assert_eq!(
        plan.action,
        Action::PrepareTrial {
            slot: Slot::B,
            transaction: id
        }
    );
    (plan.record, boot)
}

fn requested() -> (RecoveryRecord, BootObservation) {
    let (record, boot) = prepared();
    let id = record.transaction().unwrap().id;
    let plan = record.trial_prepared(id, &boot).unwrap();
    assert_eq!(plan.action, Action::RebootTrial { transaction: id });
    (plan.record, boot)
}

fn trial() -> (RecoveryRecord, BootObservation) {
    let (record, mut boot) = requested();
    boot.boot_id = TRIAL_BOOT.into();
    boot.uptime_ms = 20_000;
    boot.actual_slot = Some(Slot::B);
    boot.actual_build_id = Some("build-2".into());
    boot.tryboot = true;
    boot.trial_id = Some(record.transaction().unwrap().id);
    let plan = record.reconcile(&boot).unwrap();
    assert_eq!(
        plan.action,
        Action::ObserveTrial {
            transaction: boot.trial_id.unwrap()
        }
    );
    (plan.record, boot)
}

fn sample(ms: u64) -> HealthSample {
    HealthSample {
        uptime_ms: ms,
        instance: AppInstance {
            pid: 100,
            start_ticks: 10,
            invocation_id: "a".repeat(32),
        },
        build_id: "build-2".into(),
        executable_verified: true,
        state_writable: true,
        control_response: ms,
        presented_frames: ms / 16,
    }
}

fn healthy(record: &RecoveryRecord, boot: &mut BootObservation) -> HealthProof {
    let mut monitor = HealthMonitor::new(record, boot).unwrap();
    for ms in (boot.uptime_ms..boot.uptime_ms + HEALTH_WINDOW_MS).step_by(2_000) {
        assert!(monitor.observe(Some(sample(ms))).unwrap().is_none());
    }
    boot.uptime_ms += HEALTH_WINDOW_MS;
    monitor
        .observe(Some(sample(boot.uptime_ms)))
        .unwrap()
        .unwrap()
}

fn promoting() -> (RecoveryRecord, BootObservation) {
    let (record, mut boot) = trial();
    let proof = healthy(&record, &mut boot);
    let plan = record.health_confirmed(proof, &boot).unwrap();
    assert_eq!(
        plan.action,
        Action::PromoteDefault {
            slot: Slot::B,
            transaction: boot.trial_id.unwrap()
        }
    );
    (plan.record, boot)
}

fn reopen(record: &RecoveryRecord) -> RecoveryRecord {
    let reopened: RecoveryRecord =
        serde_json::from_slice(&serde_json::to_vec(record).unwrap()).unwrap();
    reopened.validate().unwrap();
    reopened
}

#[test]
fn default_changes_only_after_continuous_health_and_old_root_is_retained() {
    let (record, mut boot) = promoting();
    assert_eq!(record.good().slot, Slot::A);
    assert!(record.retained_good().is_none());
    assert_eq!(
        record.finish_promotion(&boot).unwrap_err(),
        RecoveryError::BootMismatch
    );
    boot.default_slot = Some(Slot::B);
    let committed = record.finish_promotion(&boot).unwrap().record;
    assert_eq!(committed.good().slot, Slot::B);
    assert_eq!(committed.retained_good().unwrap().slot, Slot::A);
    assert_eq!(
        reopen(&committed).reconcile(&boot).unwrap().action,
        Action::None
    );
    boot.boot_id = NEXT_BOOT.into();
    boot.tryboot = false;
    boot.trial_id = None;
    assert_eq!(
        reopen(&committed).reconcile(&boot).unwrap().action,
        Action::None
    );
}

#[test]
fn unknown_roots_wrong_devices_and_unqualified_recovery_never_grant_writes() {
    let (record, boot, cap) = baseline();
    for invalid in 0..6 {
        let mut changed = boot.clone();
        let mut cap = cap.clone();
        match invalid {
            0 => changed.actual_slot = None,
            1 => changed.binding.device_id = digest("different board"),
            2 => changed.actual_build_id = None,
            3 => changed.default_slot = Some(Slot::B),
            4 => cap.watchdog_qualified = false,
            _ => cap.tryboot_qualified = false,
        }
        assert!(
            record
                .begin_descriptor(&descriptor(2), &changed, &cap, false)
                .is_err()
        );
    }
}

#[test]
fn generation_floor_commits_before_writing_and_corrupt_write_never_arms_trial() {
    let (record, boot, cap) = baseline();
    let record = record
        .begin_descriptor(&descriptor(2), &boot, &cap, false)
        .unwrap()
        .record;
    assert_eq!(
        reopen(&record).image_history().highest_attempted_generation,
        2
    );
    let id = record.transaction().unwrap().id;
    assert_eq!(
        record
            .written(
                id,
                descriptor(2).compressed.sha256,
                digest("corrupt"),
                &boot
            )
            .unwrap_err(),
        RecoveryError::WriteMismatch
    );
    assert_eq!(
        record.trial_prepared(id, &boot).unwrap_err(),
        RecoveryError::StaleCommand
    );
}

#[test]
fn reset_or_service_restart_before_trial_preserves_old_root_and_quarantines_attempt() {
    let (record, boot, cap) = baseline();
    let writing = record
        .begin_descriptor(&descriptor(2), &boot, &cap, false)
        .unwrap()
        .record;
    for record in [writing, prepared().0, requested().0] {
        for boot_id in [OLD_BOOT, NEXT_BOOT] {
            let mut boot = boot.clone();
            boot.boot_id = boot_id.into();
            let plan = reopen(&record).reconcile(&boot).unwrap();
            assert_eq!(plan.action, Action::None);
            assert_eq!(plan.record.transaction().unwrap().phase, Phase::RolledBack);
            assert_eq!(plan.record.good().slot, Slot::A);
            assert!(
                plan.record
                    .image_history()
                    .failed_images
                    .contains(&descriptor(2).compressed.sha256)
            );
        }
    }
}

#[test]
fn duplicate_trial_request_and_stale_transaction_cannot_repeat_reboot() {
    let (record, boot) = requested();
    let id = record.transaction().unwrap().id;
    assert_eq!(
        record.trial_prepared(id, &boot).unwrap_err(),
        RecoveryError::StaleCommand
    );
    let (prepared, boot) = prepared();
    assert_eq!(
        prepared.trial_prepared(digest("stale"), &boot).unwrap_err(),
        RecoveryError::StaleCommand
    );
}

#[test]
fn trial_timeout_uses_boot_uptime_and_cannot_be_extended_by_service_restart() {
    let (record, mut boot) = trial();
    boot.uptime_ms = TRIAL_DEADLINE_MS - 1;
    assert!(matches!(
        reopen(&record).reconcile(&boot).unwrap().action,
        Action::ObserveTrial { .. }
    ));
    boot.uptime_ms += 1;
    let failed = reopen(&record).reconcile(&boot).unwrap();
    assert!(matches!(failed.action, Action::RebootPreviousGood { .. }));
    assert_eq!(
        failed.record.transaction().unwrap().failure,
        Some(FailureReason::HealthTimeout)
    );
    boot.boot_id = NEXT_BOOT.into();
    boot.uptime_ms = 20_000;
    boot.actual_slot = Some(Slot::A);
    boot.actual_build_id = Some("build-1".into());
    boot.tryboot = false;
    boot.trial_id = None;
    let rolled_back = reopen(&failed.record).reconcile(&boot).unwrap().record;
    assert_eq!(rolled_back.image_history().highest_attempted_generation, 2);
    assert_eq!(rolled_back.transaction().unwrap().phase, Phase::RolledBack);
}

#[test]
fn trial_requires_matching_firmware_token_new_boot_and_actual_candidate() {
    let (record, boot) = trial();
    for invalid in 0..5 {
        let mut changed = boot.clone();
        match invalid {
            0 => changed.tryboot = false,
            1 => changed.trial_id = Some(digest("another trial")),
            2 => changed.boot_id = OLD_BOOT.into(),
            3 => changed.default_slot = Some(Slot::B),
            _ => changed.actual_slot = None,
        }
        assert_eq!(
            record.reconcile(&changed).unwrap_err(),
            RecoveryError::BootMismatch
        );
    }
    let mut changed = boot;
    changed.actual_build_id = Some("unexpected-image".into());
    assert_eq!(
        record
            .reconcile(&changed)
            .unwrap()
            .record
            .transaction()
            .unwrap()
            .failure,
        Some(FailureReason::WrongImage)
    );
}

#[test]
fn promotion_interruption_reconciles_both_selector_choices() {
    let (record, mut boot) = promoting();
    // Selector committed, journal not yet finalized, same or subsequent boot.
    boot.default_slot = Some(Slot::B);
    for restarted in [false, true] {
        let mut boot = boot.clone();
        if restarted {
            boot.boot_id = NEXT_BOOT.into();
            boot.tryboot = false;
            boot.trial_id = None;
        }
        let committed = reopen(&record).reconcile(&boot).unwrap().record;
        assert_eq!(committed.transaction().unwrap().phase, Phase::Committed);
        assert_eq!(committed.retained_good().unwrap().slot, Slot::A);
    }
    // Power lost before changing default: the old root remains authoritative.
    let (_, boot, _) = baseline();
    let rolled_back = reopen(&record)
        .reconcile(&BootObservation {
            boot_id: NEXT_BOOT.into(),
            ..boot
        })
        .unwrap()
        .record;
    assert_eq!(rolled_back.good().slot, Slot::A);
    assert_eq!(
        rolled_back.transaction().unwrap().failure,
        Some(FailureReason::PromotionInterrupted)
    );
}

#[test]
fn service_restart_before_selector_commit_requires_fresh_health_window() {
    let (record, boot) = promoting();
    let resumed = reopen(&record).reconcile(&boot).unwrap().record;
    assert_eq!(resumed.transaction().unwrap().phase, Phase::Observing);
    assert!(resumed.transaction().unwrap().health.is_none());
    let mut observer = HealthMonitor::new(&resumed, &boot).unwrap();
    assert!(
        observer
            .observe(Some(sample(boot.uptime_ms)))
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_controls_frames_missing_requests_gaps_and_process_restarts_break_health() {
    let (record, boot) = trial();
    for fault in 0..6 {
        let mut observer = HealthMonitor::new(&record, &boot).unwrap();
        for ms in (20_000..78_000).step_by(2_000) {
            assert!(observer.observe(Some(sample(ms))).unwrap().is_none());
        }
        let mut bad = sample(78_000);
        match fault {
            0 => bad.control_response = 76_000,
            1 => bad.presented_frames = 76_000 / 16,
            2 => {
                observer.observe(None).unwrap();
            }
            3 => bad.uptime_ms = 90_000,
            4 => bad.instance.pid += 1,
            _ => bad.instance.start_ticks += 1,
        }
        assert!(observer.observe(Some(bad)).unwrap().is_none());
        assert!(observer.observe(Some(sample(80_000))).unwrap().is_none());
    }
}

#[test]
fn wrong_executable_build_or_unwritable_state_cannot_produce_health_proof() {
    let (record, boot) = trial();
    for fault in 0..5 {
        let mut observer = HealthMonitor::new(&record, &boot).unwrap();
        let mut bad = sample(20_000);
        match fault {
            0 => bad.executable_verified = false,
            1 => bad.state_writable = false,
            2 => bad.build_id = "different-build".into(),
            3 => bad.instance.invocation_id.clear(),
            _ => bad.uptime_ms = TRIAL_DEADLINE_MS,
        }
        assert_eq!(
            observer.observe(Some(bad)).unwrap_err(),
            RecoveryError::Unhealthy
        );
    }
}

#[test]
fn health_proofs_cannot_be_reused_after_a_gap_or_from_a_different_trial() {
    let (record, mut boot) = trial();
    let proof = healthy(&record, &mut boot);
    boot.uptime_ms += MAX_HEALTH_GAP_MS + 1;
    assert_eq!(
        record.health_confirmed(proof.clone(), &boot).unwrap_err(),
        RecoveryError::Unhealthy
    );
    boot.uptime_ms = 80_000;
    let mut proof = proof;
    proof.transaction = digest("stale");
    assert_eq!(
        record.health_confirmed(proof, &boot).unwrap_err(),
        RecoveryError::StaleCommand
    );
}

#[test]
fn explicit_retry_is_bounded_and_never_lowers_persisted_history() {
    let (record, boot) = requested();
    let record = record.reconcile(&boot).unwrap().record;
    let (_, _, cap) = baseline();
    let image = descriptor(2);
    assert_eq!(
        record
            .begin_descriptor(&image, &boot, &cap, false)
            .unwrap_err(),
        RecoveryError::Quarantined
    );
    let view = record
        .retry_verification_history(image.compressed.sha256)
        .unwrap();
    assert_eq!(view.highest_attempted_generation, 1);
    assert_eq!(record.image_history().highest_attempted_generation, 2);
    let second = record
        .begin_descriptor(&image, &boot, &cap, true)
        .unwrap()
        .record;
    assert_eq!(second.transaction().unwrap().attempt, 2);
    assert_ne!(
        second.transaction().unwrap().id,
        record.transaction().unwrap().id
    );
    let failed = second.reconcile(&boot).unwrap().record;
    assert_eq!(
        reopen(&failed)
            .retry_verification_history(image.compressed.sha256)
            .unwrap_err(),
        RecoveryError::Quarantined
    );
    assert_eq!(failed.image_history().highest_attempted_generation, 2);
}

#[test]
fn next_update_stops_advertising_retained_slot_before_overwriting_it() {
    let (record, mut boot) = promoting();
    boot.default_slot = Some(Slot::B);
    let record = record.finish_promotion(&boot).unwrap().record;
    boot.boot_id = NEXT_BOOT.into();
    boot.tryboot = false;
    boot.trial_id = None;
    let (_, _, cap) = baseline();
    let plan = record
        .begin_descriptor(&descriptor(3), &boot, &cap, false)
        .unwrap();
    assert!(plan.record.retained_good().is_none());
    assert_eq!(plan.record.good().slot, Slot::B);
    assert!(matches!(
        plan.action,
        Action::WriteInactive { slot: Slot::A, .. }
    ));
}

#[test]
fn corrupted_or_cross_phase_records_fail_validation() {
    let (record, _) = promoting();
    for fault in 0..7 {
        let mut bad = record.clone();
        match fault {
            0 => bad.schema += 1,
            1 => bad.highest_attempted_generation = 0,
            2 => bad.transaction.as_mut().unwrap().health = None,
            3 => bad.transaction.as_mut().unwrap().candidate.slot = Slot::A,
            4 => bad.transaction.as_mut().unwrap().trial_boot_id = Some(OLD_BOOT.into()),
            5 => {
                bad.transaction
                    .as_mut()
                    .unwrap()
                    .health
                    .as_mut()
                    .unwrap()
                    .finished_ms = TRIAL_DEADLINE_MS
            }
            _ => bad.transaction.as_mut().unwrap().attempt = 0,
        }
        assert_eq!(bad.validate(), Err(RecoveryError::InvalidState));
    }
}

#[test]
fn journal_reopen_keeps_attempt_floor_and_lock_excludes_a_second_owner() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovery");
    let (record, boot, cap) = baseline();
    assert!(crate::RecoveryStore::open(&path).is_err());
    let mut store = crate::RecoveryStore::initialize(&path, record).unwrap();
    assert!(matches!(
        crate::RecoveryStore::open(&path),
        Err(crate::Error::Busy)
    ));
    let action = store
        .advance(|record| record.begin_descriptor(&descriptor(2), &boot, &cap, false))
        .unwrap();
    assert!(matches!(
        action,
        Action::WriteInactive { slot: Slot::B, .. }
    ));
    drop(store);
    let mut store = crate::RecoveryStore::open(&path).unwrap();
    assert_eq!(
        store.record().image_history().highest_attempted_generation,
        2
    );
    assert_eq!(
        store.advance(|record| record.reconcile(&boot)).unwrap(),
        Action::None
    );
    assert!(
        store
            .record()
            .image_history()
            .failed_images
            .contains(&descriptor(2).compressed.sha256)
    );
}

#[test]
fn journal_rejects_stale_plans_and_never_replaces_corruption_with_bootstrap() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovery");
    let (record, boot, cap) = baseline();
    let stale = record
        .begin_descriptor(&descriptor(3), &boot, &cap, false)
        .unwrap();
    let mut store = crate::RecoveryStore::initialize(&path, record.clone()).unwrap();
    store
        .advance(|record| record.begin_descriptor(&descriptor(2), &boot, &cap, false))
        .unwrap();
    assert!(matches!(
        store.advance(|_| Ok(stale)),
        Err(crate::Error::Recovery(RecoveryError::StaleCommand))
    ));
    drop(store);
    std::fs::write(path.join("state.json"), b"{broken").unwrap();
    assert!(crate::RecoveryStore::open(&path).is_err());
    assert!(crate::RecoveryStore::initialize(&path, record).is_err());
    assert_eq!(std::fs::read(path.join("state.json")).unwrap(), b"{broken");
}

#[test]
fn failed_journal_replace_releases_no_action_and_requires_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovery");
    let (record, boot, cap) = baseline();
    let mut store = crate::RecoveryStore::initialize(&path, record.clone()).unwrap();
    // An unreplaceable target stands in for a failed persistence operation.
    std::fs::remove_file(path.join("state.json")).unwrap();
    std::fs::create_dir(path.join("state.json")).unwrap();
    assert!(
        store
            .advance(|record| record.begin_descriptor(&descriptor(2), &boot, &cap, false))
            .is_err()
    );
    assert_eq!(store.record(), &record);
    // Even a no-op is denied once the on-disk outcome is uncertain.
    assert!(store.advance(|record| record.reconcile(&boot)).is_err());
}

#[test]
fn uncommitted_temporary_file_never_supersedes_the_last_complete_journal() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovery");
    let (record, boot, cap) = baseline();
    drop(crate::RecoveryStore::initialize(&path, record.clone()).unwrap());
    let pending = record
        .begin_descriptor(&descriptor(2), &boot, &cap, false)
        .unwrap();
    std::fs::write(
        path.join(".unfinished-replacement"),
        serde_json::to_vec(&pending.record).unwrap(),
    )
    .unwrap();
    assert_eq!(crate::RecoveryStore::open(&path).unwrap().record(), &record);
}

#[cfg(unix)]
#[test]
fn journal_rejects_world_readable_state_and_symlinked_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovery");
    let (record, _, _) = baseline();
    drop(crate::RecoveryStore::initialize(&path, record).unwrap());
    let state = path.join("state.json");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(crate::RecoveryStore::open(&path).is_err());
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::rename(&state, path.join("real.json")).unwrap();
    symlink("real.json", &state).unwrap();
    assert!(crate::RecoveryStore::open(&path).is_err());
}
