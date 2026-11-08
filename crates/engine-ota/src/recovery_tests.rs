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
        min_recovery_protocol: crate::recovery::RECOVERY_PROTOCOL,
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
        recovery_protocol: crate::recovery::RECOVERY_PROTOCOL,
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

#[test]
fn unpublished_bootstrap_baseline_allows_the_first_signed_generation_without_qualification_bypass()
{
    let (existing, mut boot, mut capability) = baseline();
    let mut good = existing.good().clone();
    good.generation = 0;
    good.payload = digest("unpublished bootstrap payload");
    good.expanded = digest("unpublished bootstrap expanded");
    good.build_id = "unpublished-bootstrap".into();
    boot.actual_build_id = Some(good.build_id.clone());
    let record = RecoveryRecord::bootstrap(boot.binding.clone(), good, &boot).unwrap();
    assert_eq!(record.image_history().highest_attempted_generation, 0);
    assert!(record.retained_good().is_none());
    capability.watchdog_qualified = false;
    assert_eq!(
        record
            .begin_descriptor(&descriptor(1), &boot, &capability, false)
            .unwrap_err(),
        RecoveryError::Unqualified
    );
    capability.watchdog_qualified = true;
    let first = record
        .begin_descriptor(&descriptor(1), &boot, &capability, false)
        .unwrap();
    first.record.validate().unwrap();
    assert_eq!(first.record.image_history().highest_attempted_generation, 1);
    assert_eq!(
        first.record.transaction().unwrap().previous_good.generation,
        0
    );
    assert_eq!(first.record.transaction().unwrap().candidate.generation, 1);
    assert!(
        record
            .begin_descriptor(&descriptor(0), &boot, &capability, false)
            .is_err()
    );
    let mut old = descriptor(1);
    old.min_recovery_protocol = 1;
    assert_eq!(
        record
            .begin_descriptor(&old, &boot, &capability, false)
            .unwrap_err(),
        RecoveryError::Ineligible
    );
    let mut old_record = serde_json::to_value(&record).unwrap();
    old_record["schema"] = 1.into();
    assert!(
        serde_json::from_value::<RecoveryRecord>(old_record)
            .unwrap()
            .validate()
            .is_err()
    );
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

pub(crate) fn writing(image: &ImageDescriptor) -> (RecoveryRecord, BootObservation) {
    let (record, boot, cap) = baseline();
    let plan = record.begin_descriptor(image, &boot, &cap, false).unwrap();
    (plan.record, boot)
}

fn requested() -> (RecoveryRecord, BootObservation) {
    let (record, boot) = prepared();
    let id = record.transaction().unwrap().id;
    let plan = record.trial_prepared(id, &boot).unwrap();
    assert_eq!(plan.action, Action::RebootTrial { transaction: id });
    (plan.record, boot)
}

#[cfg(target_os = "linux")]
#[test]
fn abandoned_manual_writer_settles_only_on_the_normal_good_boot() {
    for (record, boot) in [writing(&descriptor(2)), prepared(), requested()] {
        let id = record.transaction().unwrap().id;
        let settled = record.interrupted_normal(&boot).unwrap();
        assert_eq!(settled.action, Action::None);
        let tx = settled.record.transaction().unwrap();
        assert_eq!(tx.id, id);
        assert_eq!(tx.phase, Phase::RolledBack);
        assert_eq!(tx.failure, Some(FailureReason::InterruptedBeforeTrial));
        assert!(
            settled
                .record
                .image_history()
                .failed_images
                .contains(&tx.candidate.payload)
        );
        assert_eq!(settled.record.good(), record.good());
        assert_eq!(
            settled.record.interrupted_normal(&boot).unwrap().record,
            settled.record
        );
        let mut wrong = boot.clone();
        wrong.actual_slot = Some(Slot::B);
        assert!(record.interrupted_normal(&wrong).is_err());
        wrong = boot.clone();
        wrong.default_slot = Some(Slot::B);
        assert!(record.interrupted_normal(&wrong).is_err());
        wrong = boot.clone();
        wrong.tryboot = true;
        assert!(record.interrupted_normal(&wrong).is_err());
    }
    let (record, boot) = trial();
    assert!(record.interrupted_normal(&boot).is_err());
    assert_eq!(record.transaction().unwrap().phase, Phase::Observing);
}

pub(crate) fn trial() -> (RecoveryRecord, BootObservation) {
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

pub(crate) fn promoting() -> (RecoveryRecord, BootObservation) {
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
fn maintenance_requires_a_terminal_transaction_and_the_current_good_default() {
    let (record, mut boot, _) = baseline();
    record.maintenance(&boot).unwrap();
    boot.default_slot = Some(Slot::B);
    assert!(record.maintenance(&boot).is_err());
    for (record, boot) in [
        writing(&descriptor(2)),
        prepared(),
        requested(),
        trial(),
        promoting(),
    ] {
        assert!(record.maintenance(&boot).is_err());
    }
    let (record, mut boot) = promoting();
    boot.default_slot = Some(Slot::B);
    let committed = record.finish_promotion(&boot).unwrap().record;
    committed.maintenance(&boot).unwrap();
    boot.actual_slot = Some(Slot::A);
    assert!(committed.maintenance(&boot).is_err());
    let (record, boot) = prepared();
    let rolled_back = record
        .fail(FailureReason::InstallationFailed, &boot)
        .unwrap()
        .record;
    assert_eq!(rolled_back.transaction().unwrap().phase, Phase::RolledBack);
    rolled_back.maintenance(&boot).unwrap();
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
fn subsequent_update_works_from_the_committed_trial_without_an_extra_normal_reboot() {
    let (record, mut boot) = promoting();
    boot.default_slot = Some(Slot::B);
    let committed = record.finish_promotion(&boot).unwrap().record;
    let original_trial = boot.trial_id;
    boot.uptime_ms += 86_400_000;
    let (_, _, cap) = baseline();
    let next_image = descriptor(3);
    let writing = reopen(&committed)
        .begin_descriptor(&next_image, &boot, &cap, false)
        .unwrap()
        .record;
    let id = writing.transaction().unwrap().id;
    assert_ne!(Some(id), original_trial);
    let prepared = reopen(&writing)
        .written(
            id,
            next_image.compressed.sha256,
            next_image.expanded.sha256,
            &boot,
        )
        .unwrap();
    assert_eq!(
        prepared
            .record
            .selection_change(&prepared.action, &boot)
            .unwrap(),
        (Slot::A, Some(id))
    );
    let requested = prepared.record.trial_prepared(id, &boot).unwrap().record;
    // A service restart while still on the previous good boot quarantines the
    // interrupted attempt; it must not confuse the two distinct trial tokens.
    let interrupted = reopen(&requested).reconcile(&boot).unwrap().record;
    assert_eq!(interrupted.transaction().unwrap().phase, Phase::RolledBack);
    assert_eq!(interrupted.good().slot, Slot::B);
    interrupted.reconcile(&boot).unwrap();

    boot.boot_id = NEXT_BOOT.into();
    boot.uptime_ms = 20_000;
    boot.actual_slot = Some(Slot::A);
    boot.actual_build_id = Some("build-3".into());
    boot.trial_id = Some(id);
    let observing = reopen(&requested).reconcile(&boot).unwrap().record;
    let mut health = HealthMonitor::new(&observing, &boot).unwrap();
    let mut proof = None;
    for ms in (boot.uptime_ms..=boot.uptime_ms + HEALTH_WINDOW_MS).step_by(2000) {
        let mut reading = sample(ms);
        reading.build_id = "build-3".into();
        proof = health.observe(Some(reading)).unwrap();
    }
    boot.uptime_ms += HEALTH_WINDOW_MS;
    let promoting = observing
        .health_confirmed(proof.unwrap(), &boot)
        .unwrap()
        .record;
    boot.default_slot = Some(Slot::A);
    let committed = promoting.finish_promotion(&boot).unwrap().record;
    assert_eq!(committed.good().slot, Slot::A);
    assert_eq!(committed.retained_good().unwrap().slot, Slot::B);
    let third = reopen(&committed)
        .begin_descriptor(&descriptor(4), &boot, &cap, false)
        .unwrap();
    assert!(matches!(
        third.action,
        Action::WriteInactive { slot: Slot::B, .. }
    ));
}

#[test]
fn committed_trial_context_cannot_transfer_to_another_boot_token_or_payload() {
    let (record, mut boot) = promoting();
    boot.default_slot = Some(Slot::B);
    let committed = record.finish_promotion(&boot).unwrap().record;
    let (_, _, cap) = baseline();
    for fault in 0..3 {
        let mut changed = boot.clone();
        match fault {
            0 => changed.boot_id = NEXT_BOOT.into(),
            1 => changed.trial_id = Some(digest("unrelated trial")),
            _ => changed.actual_build_id = Some("another build".into()),
        }
        assert!(
            committed
                .begin_descriptor(&descriptor(3), &changed, &cap, false)
                .is_err()
        );
    }
    let mut corrupt = committed.clone();
    corrupt.good_boot.as_mut().unwrap().payload = digest("unrelated payload");
    assert!(corrupt.validate().is_err());
    let mut corrupt = committed.clone();
    corrupt.good_boot.as_mut().unwrap().trial_id = digest("unrelated trial");
    assert!(corrupt.validate().is_err());
    let mut corrupt = committed;
    corrupt.transaction = None;
    assert!(corrupt.validate().is_err());
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
    assert!(store.durable_record().is_err());
    // Even a no-op is denied once the on-disk outcome is uncertain.
    assert!(store.advance(|record| record.reconcile(&boot)).is_err());
}

#[test]
fn selector_writes_require_the_corresponding_durable_phase_and_live_context() {
    let (prepared, boot) = prepared();
    let id = prepared.transaction().unwrap().id;
    let prepare = Action::PrepareTrial {
        slot: Slot::B,
        transaction: id,
    };
    assert_eq!(
        prepared.selection_change(&prepare, &boot).unwrap(),
        (Slot::B, Some(id))
    );
    for action in [
        Action::PrepareTrial {
            slot: Slot::A,
            transaction: id,
        },
        Action::PromoteDefault {
            slot: Slot::B,
            transaction: id,
        },
        Action::RestoreDefault {
            slot: Slot::A,
            transaction: id,
        },
        Action::RebootTrial { transaction: id },
    ] {
        assert!(prepared.selection_change(&action, &boot).is_err());
    }
    let mut changed = boot.clone();
    changed.binding.device_id = digest("other physical combination");
    assert!(prepared.selection_change(&prepare, &changed).is_err());
    let requested = prepared.trial_prepared(id, &boot).unwrap().record;
    assert!(requested.selection_change(&prepare, &boot).is_err());

    let (promoting, boot) = promoting();
    let id = promoting.transaction().unwrap().id;
    let promote = Action::PromoteDefault {
        slot: Slot::B,
        transaction: id,
    };
    assert_eq!(
        promoting.selection_change(&promote, &boot).unwrap(),
        (Slot::B, None)
    );
    let mut delayed = boot.clone();
    delayed.uptime_ms += MAX_HEALTH_GAP_MS + 1;
    assert_eq!(
        promoting.selection_change(&promote, &delayed).unwrap_err(),
        RecoveryError::Unhealthy
    );
    let mut too_late = boot.clone();
    too_late.uptime_ms = TRIAL_DEADLINE_MS;
    assert!(promoting.selection_change(&promote, &too_late).is_err());
    let mut already_selected = boot.clone();
    already_selected.default_slot = Some(Slot::B);
    assert!(
        promoting
            .selection_change(&promote, &already_selected)
            .is_err()
    );
    let committed = promoting
        .finish_promotion(&already_selected)
        .unwrap()
        .record;
    assert!(
        committed
            .selection_change(&promote, &already_selected)
            .is_err()
    );

    let failed = promoting
        .fail(FailureReason::PersistentStateUnavailable, &already_selected)
        .unwrap();
    assert_eq!(
        failed
            .record
            .selection_change(&failed.action, &already_selected)
            .unwrap(),
        (Slot::A, None)
    );
    assert!(
        failed
            .record
            .selection_change(
                &Action::RestoreDefault {
                    slot: Slot::B,
                    transaction: id
                },
                &already_selected
            )
            .is_err()
    );
    assert!(
        failed
            .record
            .selection_change(&promote, &already_selected)
            .is_err()
    );
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

fn development(name: &str) -> DevelopmentImage {
    let official = descriptor(2);
    DevelopmentImage {
        schema: crate::development::DEVELOPMENT_SCHEMA,
        source_commit: "d".repeat(40),
        source_dirty: true,
        // The shared health samples report build-2.
        build_id: "build-2".into(),
        machine: official.machine,
        architecture: official.architecture,
        layout: official.layout,
        format: official.format,
        compressed: Blob {
            bytes: 40,
            sha256: digest(&format!("{name}-compressed")),
        },
        expanded: Blob {
            bytes: 400,
            sha256: digest(&format!("{name}-expanded")),
        },
        boot_identity: official.boot_identity,
        min_updater_protocol: 1,
        min_recovery_protocol: RECOVERY_PROTOCOL,
        data: official.data,
    }
}

/// Writes a development image from the baseline and requests its trial.
fn development_requested(image: &DevelopmentImage) -> (RecoveryRecord, BootObservation, Sha256) {
    let (record, boot, _) = baseline();
    let plan = record.begin_development(image, &boot).unwrap();
    let id = plan.record.transaction().unwrap().id;
    assert_eq!(
        plan.action,
        Action::WriteInactive {
            slot: Slot::B,
            transaction: id
        }
    );
    assert_eq!(
        plan.record
            .inactive_development_write(image, &boot)
            .unwrap(),
        id
    );
    let written = plan
        .record
        .written(id, image.compressed.sha256, image.expanded.sha256, &boot)
        .unwrap()
        .record;
    let requested = written.trial_prepared(id, &boot).unwrap().record;
    let mut trial = boot;
    trial.boot_id = TRIAL_BOOT.into();
    trial.uptime_ms = 20_000;
    trial.actual_slot = Some(Slot::B);
    trial.actual_build_id = Some("build-2".into());
    trial.tryboot = true;
    trial.trial_id = Some(id);
    (requested, trial, id)
}

#[test]
fn development_image_commits_without_a_generation_and_official_images_still_install() {
    let image = development("first");
    let (requested, mut trial, id) = development_requested(&image);
    assert_eq!(requested.schema, 3);
    assert!(requested.transaction().unwrap().development());
    assert_eq!(requested.image_history().highest_attempted_generation, 1);
    reopen(&requested);
    let observing = requested.reconcile(&trial).unwrap().record;
    let proof = healthy(&observing, &mut trial);
    let promoting = observing.health_confirmed(proof, &trial).unwrap().record;
    trial.default_slot = Some(Slot::B);
    let committed = promoting.finish_promotion(&trial).unwrap().record;
    assert_eq!(committed.good().slot, Slot::B);
    assert_eq!(committed.good().generation, 0);
    assert_eq!(committed.retained_good().unwrap().generation, 1);
    assert_eq!(committed.transaction().unwrap().id, id);
    let history = reopen(&committed).image_history();
    assert_eq!(history.installed_sha256, Some(image.compressed.sha256));
    assert_eq!(history.highest_attempted_generation, 1);
    assert!(history.failed_images.is_empty());

    // Another development image follows from the committed trial boot.
    let (_, _, cap) = baseline();
    let again = committed
        .begin_development(&development("second"), &trial)
        .unwrap()
        .record;
    assert_eq!(again.schema, 3);
    assert_eq!(again.transaction().unwrap().previous_good.generation, 0);

    // The next official generation installs over the development image and
    // returns the record to the schema that older images read.
    let official = committed
        .begin_descriptor(&descriptor(2), &trial, &cap, false)
        .unwrap()
        .record;
    assert_eq!(official.schema, 2);
    assert_eq!(official.transaction().unwrap().candidate.slot, Slot::A);
    assert_eq!(official.image_history().highest_attempted_generation, 2);
    reopen(&official);
}

#[test]
fn failed_development_trial_returns_without_quarantine_and_may_be_installed_again() {
    let image = development("broken");
    let (requested, trial, id) = development_requested(&image);
    let mut late = trial.clone();
    late.uptime_ms = TRIAL_DEADLINE_MS;
    let plan = requested.reconcile(&late).unwrap();
    assert_eq!(plan.action, Action::RebootPreviousGood { transaction: id });
    let pending = reopen(&plan.record);
    assert_eq!(pending.transaction().unwrap().phase, Phase::RollbackPending);
    assert!(pending.image_history().failed_images.is_empty());

    let (_, mut good, cap) = baseline();
    good.boot_id = NEXT_BOOT.into();
    let rolled = pending.reconcile(&good).unwrap().record;
    let tx = rolled.transaction().unwrap();
    assert_eq!(tx.phase, Phase::RolledBack);
    assert_eq!(tx.failure, Some(FailureReason::HealthTimeout));
    assert_eq!(rolled.schema, 3);
    let history = reopen(&rolled).image_history();
    assert!(history.failed_images.is_empty());
    assert_eq!(history.highest_attempted_generation, 1);
    assert_eq!(rolled.good().generation, 1);

    // The same bytes may be tried again, and official images are unaffected.
    rolled.begin_development(&image, &good).unwrap();
    let official = rolled
        .begin_descriptor(&descriptor(2), &good, &cap, false)
        .unwrap()
        .record;
    assert_eq!(official.schema, 2);
}

#[test]
fn development_images_need_this_units_boot_files_protocol_data_and_new_bytes() {
    let (record, boot, cap) = baseline();
    for fault in 0..5 {
        let mut image = development("refused");
        match fault {
            0 => image.boot_identity = digest("other boot"),
            1 => image.min_recovery_protocol += 1,
            2 => {
                image.data = DataCompatibility {
                    reads: SchemaRange { min: 2, max: 2 },
                    writes: 2,
                }
            }
            3 => image.compressed.sha256 = record.good().payload,
            _ => image.source_commit = "unknown".into(),
        }
        assert_eq!(
            record.begin_development(&image, &boot).unwrap_err(),
            RecoveryError::Ineligible,
            "fault {fault}"
        );
    }
    let writing = record
        .begin_descriptor(&descriptor(2), &boot, &cap, false)
        .unwrap()
        .record;
    assert_eq!(
        writing
            .begin_development(&development("busy"), &boot)
            .unwrap_err(),
        RecoveryError::Busy
    );
    let mut unknown = boot;
    unknown.actual_slot = Some(Slot::B);
    assert!(
        record
            .begin_development(&development("x"), &unknown)
            .is_err()
    );
}

#[test]
fn record_schema_follows_whether_the_transaction_is_a_development_install() {
    let (record, boot, cap) = baseline();
    let mut wrong = record.clone();
    wrong.schema = 3;
    assert!(wrong.validate().is_err());
    let image = development("schema");
    let development = record.begin_development(&image, &boot).unwrap().record;
    let mut wrong = development.clone();
    wrong.schema = 2;
    assert!(wrong.validate().is_err());
    let mut wrong = development.clone();
    wrong.transaction.as_mut().unwrap().attempt = 2;
    assert!(wrong.validate().is_err());
    // Writer checks are bound to one kind of transaction and exact bytes.
    assert!(development.inactive_write(&descriptor(2), &boot).is_err());
    assert!(
        development
            .inactive_development_write(&development_image_variant(&image), &boot)
            .is_err()
    );
    let official = record
        .begin_descriptor(&descriptor(2), &boot, &cap, false)
        .unwrap()
        .record;
    assert!(official.inactive_development_write(&image, &boot).is_err());
}

fn development_image_variant(image: &DevelopmentImage) -> DevelopmentImage {
    let mut changed = image.clone();
    changed.source_dirty = !changed.source_dirty;
    changed
}
