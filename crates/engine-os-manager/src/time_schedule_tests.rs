use super::*;
use crate::time::{ClockReceipt, SyncObservation, TimeReason, TimeRecord};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

const BOOT: &str = "10000000-0000-0000-0000-000000000000";
const NEW_BOOT: &str = "20000000-0000-0000-0000-000000000000";
const UTC: u64 = 1_800_000_000_000_000;

fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
fn provider(number: u32) -> ProviderIdentity {
    ProviderIdentity {
        bus_owner: format!(":1.{number}"),
        invocation_id: format!("{number:032x}"),
        pid: number,
        uid: 991,
    }
}
fn evidence(generation: u64, at_ms: u64, provider_number: u32) -> Evidence {
    Evidence {
        boot_id: BOOT.into(),
        success: Success {
            generation,
            provider: provider(provider_number),
            at_ms,
        },
    }
}
fn open(temp: &tempfile::TempDir, ms: u64) -> Schedule {
    Schedule::open(&temp.path().join("schedule"), &clock(ms)).unwrap()
}
fn active(status: Status) -> Active {
    status.active.expect("live check")
}

#[test]
fn default_off_and_read_only_status_never_create_work_or_disk_writes() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let path = temp.path().join("schedule/state.json");
    let before = fs::read(&path).unwrap();
    for ms in [100, 200, DAILY_MS * 10] {
        assert!(schedule.status(&clock(ms)).unwrap().active.is_none());
        assert!(
            schedule
                .observe(&clock(ms), false, None)
                .unwrap()
                .active
                .is_none()
        );
    }
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn boot_enable_and_restart_preserve_the_original_attempt_and_consumed_query() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 1_000);
    let start = active(schedule.observe(&clock(1_000), true, None).unwrap());
    assert_eq!(start.deadline_ms, 121_000);
    assert!(
        schedule
            .connected(&clock(2_000), start.id, Connection::Ethernet)
            .unwrap()
    );
    assert!(
        schedule
            .claim_query(&clock(3_000), start.id, provider(1))
            .unwrap()
    );
    drop(schedule);
    for ms in [4_000, 20_000, 120_000] {
        let mut reopened = open(&temp, ms);
        let attempt = active(reopened.observe(&clock(ms), true, None).unwrap());
        assert_eq!(attempt.id, start.id);
        assert_eq!(attempt.deadline_ms, start.deadline_ms);
        assert!(attempt.query_requested);
        assert!(
            !reopened
                .claim_query(&clock(ms), start.id, provider(1))
                .unwrap()
        );
    }
    let mut schedule = open(&temp, 121_000);
    let ended = schedule.observe(&clock(121_000), true, None).unwrap();
    assert_eq!(ended.completion, Some(Completion::AttemptTimeout));
    assert_eq!(ended.next_ms, Some(121_000 + RETRY_MS[0]));
    assert_eq!(ended.failure_stage, 1);
    drop(schedule);
    let mut reopened = open(&temp, 122_000);
    assert_eq!(
        reopened.observe(&clock(122_000), true, None).unwrap(),
        ended
    );
}

#[test]
fn recent_sync_fast_path_uses_receipt_age_and_daily_boundary_uses_actual_success() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 300_100);
    let proof = evidence(7, 100, 1);
    let status = schedule
        .observe(&clock(300_100), true, Some(&proof))
        .unwrap();
    assert!(status.active.is_none());
    assert_eq!(status.completion, Some(Completion::RecentSuccess));
    assert_eq!(status.next_ms, Some(100 + DAILY_MS));
    assert!(
        schedule
            .observe(&clock(99 + DAILY_MS), true, Some(&proof))
            .unwrap()
            .active
            .is_none()
    );
    let attempt = active(
        schedule
            .observe(&clock(100 + DAILY_MS), true, Some(&proof))
            .unwrap(),
    );
    assert_eq!(attempt.started_ms, 100 + DAILY_MS);
    assert!(!attempt.query_requested);

    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 300_101);
    assert!(
        schedule
            .observe(&clock(300_101), true, Some(&proof))
            .unwrap()
            .active
            .is_some()
    );
}

#[test]
fn manual_requests_coalesce_without_extending_admission_or_clearing_failure_history() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let first = active(schedule.sync_now(&clock(100), None).unwrap());
    assert_eq!(first.admission_deadline_ms, 30_100);
    for ms in [101, 10_000, 30_099] {
        assert_eq!(active(schedule.sync_now(&clock(ms), None).unwrap()), first);
    }
    let timed_out = schedule.observe(&clock(30_100), false, None).unwrap();
    assert_eq!(timed_out.completion, Some(Completion::AdmissionTimeout));
    assert_eq!(timed_out.failure_stage, 1);
    assert_eq!(timed_out.next_ms, None);
    assert!(
        !schedule
            .connected(&clock(30_100), first.id, Connection::Wireless)
            .unwrap()
    );
    let next = schedule.sync_now(&clock(31_000), None).unwrap();
    assert_eq!(next.failure_stage, 1);
    let next = active(next);
    assert_ne!(next.id, first.id);
    assert_eq!(next.admission_deadline_ms, 61_000);
    assert!(
        schedule
            .connected(&clock(60_999), next.id, Connection::Wireless)
            .unwrap()
    );
    assert!(
        schedule
            .observe(&clock(61_000), false, None)
            .unwrap()
            .active
            .is_some()
    );
    assert_eq!(
        schedule
            .observe(&clock(151_000), false, None)
            .unwrap()
            .completion,
        Some(Completion::AttemptTimeout)
    );
}

#[test]
fn manual_at_an_expired_attempt_gets_a_new_bounded_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 0);
    let a = active(schedule.observe(&clock(0), true, None).unwrap());
    let b = schedule.sync_now(&clock(ATTEMPT_MS), None).unwrap();
    assert_eq!(b.failure_stage, 1);
    let b = active(b);
    assert_ne!(a.id, b.id);
    assert_eq!(b.trigger, Trigger::Manual);
    assert_eq!(b.deadline_ms, ATTEMPT_MS * 2);
}

#[test]
fn ethernet_fallback_is_fifteen_seconds_without_another_query_or_window() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 0);
    let id = active(schedule.observe(&clock(0), true, None).unwrap()).id;
    assert!(!schedule.claim_query(&clock(0), id, provider(1)).unwrap());
    schedule
        .connected(&clock(1000), id, Connection::Ethernet)
        .unwrap();
    assert!(schedule.claim_query(&clock(1000), id, provider(1)).unwrap());
    schedule
        .connected(&clock(15_999), id, Connection::Ethernet)
        .unwrap();
    assert!(!active(schedule.status(&clock(15_999)).unwrap()).wireless_required);
    drop(schedule);
    let mut schedule = open(&temp, 16_000);
    assert!(active(schedule.status(&clock(16_000)).unwrap()).wireless_required);
    schedule
        .connected(&clock(16_000), id, Connection::Wireless)
        .unwrap();
    schedule
        .connected(&clock(16_001), id, Connection::Ethernet)
        .unwrap();
    assert!(
        !schedule
            .claim_query(&clock(16_001), id, provider(2))
            .unwrap()
    );
    let a = active(schedule.status(&clock(16_001)).unwrap());
    assert_eq!(a.connection, Some(Connection::Wireless));
    assert_eq!(a.deadline_ms, ATTEMPT_MS);
}

#[test]
fn stale_provider_or_attempt_receipts_do_not_complete_a_requested_query() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 1_000_000);
    let old = evidence(1, 100, 1);
    let id = active(
        schedule
            .observe(&clock(1_000_000), true, Some(&old))
            .unwrap(),
    )
    .id;
    schedule
        .connected(&clock(1_000_000), id, Connection::Ethernet)
        .unwrap();
    schedule
        .claim_query(&clock(1_000_000), id, provider(1))
        .unwrap();
    for proof in [evidence(2, 999_999, 2), evidence(3, 1_000_001, 1)] {
        assert!(
            schedule
                .observe(&clock(1_000_010), true, Some(&proof))
                .unwrap()
                .active
                .is_some()
        );
    }
    let proof = evidence(4, 1_000_020, 2);
    let status = schedule
        .observe(&clock(1_000_020), true, Some(&proof))
        .unwrap();
    assert!(status.active.is_none());
    assert_eq!(status.completion, Some(Completion::Synchronized));
    assert_eq!(status.next_ms, Some(1_000_020 + DAILY_MS));
    assert!(
        !schedule
            .fail(&clock(1_000_021), id, Completion::ProviderRejected)
            .unwrap()
    );
    assert!(
        !schedule
            .claim_query(&clock(1_000_021), id, provider(3))
            .unwrap()
    );
}

#[test]
fn early_natural_sync_finishes_before_any_provider_query() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let id = active(schedule.observe(&clock(100), true, None).unwrap()).id;
    let status = schedule
        .observe(&clock(101), true, Some(&evidence(1, 101, 1)))
        .unwrap();
    assert_eq!(status.completion, Some(Completion::Synchronized));
    assert!(status.active.is_none());
    assert!(!schedule.claim_query(&clock(102), id, provider(1)).unwrap());
}

#[test]
fn retries_are_fifteen_minutes_one_hour_then_six_hours_and_success_resets_them() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 0);
    let mut ms = 0;
    for delay in [RETRY_MS[0], RETRY_MS[1], RETRY_MS[2], RETRY_MS[2]] {
        let id = active(schedule.observe(&clock(ms), true, None).unwrap()).id;
        ms += 100;
        schedule
            .fail(&clock(ms), id, Completion::ProviderUnavailable)
            .unwrap();
        let status = schedule.status(&clock(ms)).unwrap();
        assert_eq!(status.next_ms, Some(ms + delay));
        assert!(
            schedule
                .observe(&clock(ms + delay - 1), true, None)
                .unwrap()
                .active
                .is_none()
        );
        ms += delay;
        drop(schedule);
        schedule = open(&temp, ms);
    }
    // A genuine sync while a deployment/other connection keeps the network up
    // resets the independent failure sequence without inventing another query.
    let proof = evidence(1, ms, 2);
    let status = schedule.observe(&clock(ms), true, Some(&proof)).unwrap();
    assert_eq!(status.failure_stage, 0);
    assert_eq!(status.next_ms, Some(ms + DAILY_MS));
    assert!(status.active.is_none());
    let id = active(
        schedule
            .sync_now(&clock(ms + RECENT_MS + 1), Some(&proof))
            .unwrap(),
    )
    .id;
    schedule
        .fail(
            &clock(ms + RECENT_MS + 1),
            id,
            Completion::NetworkUnavailable,
        )
        .unwrap();
    assert_eq!(
        schedule.status(&clock(ms + RECENT_MS + 1)).unwrap().next_ms,
        Some(ms + RECENT_MS + 1 + RETRY_MS[0])
    );
}

#[test]
fn mode_off_cancels_automatic_work_but_not_explicit_manual_work() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let id = active(schedule.observe(&clock(100), true, None).unwrap()).id;
    let off = schedule.observe(&clock(101), false, None).unwrap();
    assert_eq!(off.completion, Some(Completion::Cancelled));
    assert!(off.active.is_none());
    assert_eq!(off.next_ms, None);
    assert!(
        !schedule
            .connected(&clock(102), id, Connection::Wireless)
            .unwrap()
    );
    assert!(
        !schedule
            .fail(&clock(102), id, Completion::NetworkUnavailable)
            .unwrap()
    );
    let manual = active(schedule.sync_now(&clock(103), None).unwrap());
    assert_eq!(
        active(schedule.observe(&clock(104), true, None).unwrap()).id,
        manual.id
    );
    assert_eq!(
        active(schedule.observe(&clock(105), false, None).unwrap()).id,
        manual.id
    );
    let status = schedule
        .observe(&clock(106), false, Some(&evidence(1, 106, 1)))
        .unwrap();
    assert_eq!(status.next_ms, None);
    assert!(status.active.is_none());
}

#[test]
fn enabling_during_manual_work_uses_the_success_time_without_an_extra_boot_check() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let id = active(schedule.sync_now(&clock(100), None).unwrap()).id;
    assert_eq!(
        active(schedule.observe(&clock(101), true, None).unwrap()).id,
        id
    );
    let proof = evidence(1, 102, 1);
    let completed = schedule.observe(&clock(103), true, Some(&proof)).unwrap();
    assert_eq!(completed.completion, Some(Completion::Synchronized));
    assert_eq!(completed.next_ms, Some(102 + DAILY_MS));
    assert!(completed.active.is_none());
    assert_eq!(
        schedule.observe(&clock(104), true, Some(&proof)).unwrap(),
        completed
    );
}

#[test]
fn new_boot_has_one_new_check_and_late_old_boot_callbacks_cannot_touch_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 5_000);
    let old = active(schedule.observe(&clock(5_000), true, None).unwrap());
    drop(schedule);
    let new_clock = Clock {
        boot_id: NEW_BOOT.into(),
        ms: 100,
    };
    let mut schedule = Schedule::open(&temp.path().join("schedule"), &new_clock).unwrap();
    let new = active(
        schedule
            .observe(&new_clock, true, Some(&evidence(1, 50, 1)))
            .unwrap(),
    );
    assert_ne!(new.id, old.id);
    assert_eq!(new.deadline_ms, 120_100);
    assert!(
        !schedule
            .fail(&new_clock, old.id, Completion::ProviderUnavailable)
            .unwrap()
    );
    assert!(
        schedule
            .fail(&clock(5_001), new.id, Completion::ProviderUnavailable)
            .is_err()
    );
    drop(schedule);
    let mut schedule = Schedule::open(&temp.path().join("schedule"), &new_clock).unwrap();
    assert_eq!(
        active(schedule.observe(&new_clock, true, None).unwrap()),
        new
    );
}

#[test]
fn long_suspend_collapses_overdue_work_and_does_not_replay_retry_windows() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let a = active(schedule.observe(&clock(100), true, None).unwrap());
    let future = DAILY_MS * 100;
    let status = schedule.observe(&clock(future), true, None).unwrap();
    assert_eq!(status.failure_stage, 1);
    let b = active(status);
    assert_eq!(b.id, a.id + 1);
    assert_eq!(b.started_ms, future);
    assert_eq!(b.deadline_ms, future + ATTEMPT_MS);
    assert_eq!(
        active(schedule.observe(&clock(future), true, None).unwrap()),
        b
    );
}

#[test]
fn persistence_failure_never_authorizes_a_query_and_reopening_preserves_any_committed_claim() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        let id = active(schedule.observe(&clock(100), true, None).unwrap()).id;
        schedule
            .connected(&clock(100), id, Connection::Wireless)
            .unwrap();
        schedule.store.fail_at = Some(point);
        assert!(schedule.claim_query(&clock(101), id, provider(1)).is_err());
        assert!(schedule.claim_query(&clock(102), id, provider(1)).is_err());
        assert!(schedule.status(&clock(102)).is_err());
        drop(schedule);
        let mut schedule = open(&temp, 103);
        let committed = active(schedule.status(&clock(103)).unwrap()).query_requested;
        assert_eq!(committed, matches!(point, "renamed" | "directory-synced"));
        assert_eq!(
            schedule.claim_query(&clock(104), id, provider(1)).unwrap(),
            !committed
        );
    }
}

#[test]
fn corrupt_missing_or_replaced_state_does_not_reset_a_schedule() {
    for fault in ["missing", "corrupt", "symlink", "public"] {
        let temp = tempfile::tempdir().unwrap();
        let schedule = open(&temp, 100);
        assert!(Schedule::open(&temp.path().join("schedule"), &clock(100)).is_err());
        drop(schedule);
        let path = temp.path().join("schedule/state.json");
        match fault {
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => fs::write(&path, b"{}").unwrap(),
            "symlink" => {
                fs::rename(&path, temp.path().join("saved")).unwrap();
                symlink(temp.path().join("saved"), &path).unwrap();
            }
            _ => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
        }
        assert!(
            Schedule::open(&temp.path().join("schedule"), &clock(101)).is_err(),
            "{fault}"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    assert!(
        store::Store::existing(&temp.path().join("missing"))
            .unwrap()
            .is_none()
    );
    assert!(!temp.path().join("missing").exists());
}

#[test]
fn schedule_detects_clock_and_accepted_generation_regressions_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 1_000);
    let proof = evidence(5, 500, 1);
    schedule.observe(&clock(1_000), true, Some(&proof)).unwrap();
    let expected = fs::read(temp.path().join("schedule/state.json")).unwrap();
    for proof in [
        evidence(4, 501, 2),
        evidence(5, 501, 1),
        evidence(6, 499, 2),
        evidence(6, 2_000, 2),
    ] {
        assert!(schedule.observe(&clock(1_001), true, Some(&proof)).is_err());
    }
    assert!(schedule.status(&clock(1_000)).is_err());
    assert_eq!(
        fs::read(temp.path().join("schedule/state.json")).unwrap(),
        expected
    );
    drop(schedule);
    assert!(Schedule::open(&temp.path().join("schedule"), &clock(999)).is_err());
}

#[test]
fn only_accepted_clock_evidence_can_satisfy_the_fast_path() {
    let now = ClockReading {
        realtime_us: UTC,
        monotonic_us: 1_000_000,
        boottime_us: 1_000_000,
    };
    let mut tracker = TimeTracker::new(
        BOOT.into(),
        TimeRecord::default(),
        TimeReason::WaitingForNetworkSync,
        now,
        None,
    );
    let mut observation = SyncObservation {
        provider: provider(1),
        receipt: ClockReceipt {
            packet_count: 1,
            realtime_us: UTC,
            monotonic_us: 1_000_000,
            boottime_us: 1_000_000,
        },
        ntp_packet_count: 1,
        ntp_spike: true,
        ntp_valid: true,
        notification_realtime_us: Some(UTC),
        kernel_synchronized: true,
        now,
    };
    assert!(!tracker.observe(&observation));
    assert!(Evidence::accepted(&tracker, now).is_none());
    observation.ntp_spike = false;
    assert!(tracker.observe(&observation));
    assert_eq!(
        Evidence::accepted(&tracker, now).unwrap().success.at_ms,
        1_000
    );
    for step in [10_000_000i64, -10_000_000] {
        let changed = ClockReading {
            realtime_us: UTC.checked_add_signed(step).unwrap(),
            ..now
        };
        assert!(Evidence::accepted(&tracker, changed).is_none());
    }
    // Accepted age advances by boot time, including sleep; it is not reset by
    // reading status or by restarting the independent service.
    let later = ClockReading {
        realtime_us: UTC + 600_000_000,
        boottime_us: 601_000_000,
        monotonic_us: 1_000_001,
    };
    let accepted = Evidence::accepted(&tracker, later).unwrap();
    assert_eq!(accepted.success.at_ms, 1_000);
}

#[test]
fn queued_manual_admission_keeps_its_receipt_deadline_and_allows_later_association() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 1000);
    let a = active(
        schedule
            .sync_received(&clock(20_000), &clock(1000), None)
            .unwrap(),
    );
    assert_eq!(a.admission_deadline_ms, 31_000);
    assert!(schedule.admitted(&clock(30_999), a.id).unwrap());
    assert!(
        schedule
            .observe(&clock(40_000), false, None)
            .unwrap()
            .active
            .is_some()
    );
    assert!(
        schedule
            .connected(&clock(40_000), a.id, Connection::Wireless)
            .unwrap()
    );
    assert_eq!(
        active(schedule.status(&clock(40_000)).unwrap()).deadline_ms,
        121_000
    );
}
