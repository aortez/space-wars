use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

const BOOT: &str = "11111111-2222-3333-4444-555555555555";
fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
fn open(temp: &tempfile::TempDir, ms: u64) -> Schedule {
    Schedule::open(&temp.path().join("updates"), &clock(ms)).unwrap()
}
fn active(status: Status) -> Active {
    status.active.expect("live update attempt")
}
fn manual(schedule: &mut Schedule, ms: u64) -> Active {
    let selection = schedule.status(&clock(ms)).unwrap().selection_generation;
    active(
        schedule
            .check_now(&clock(ms), &clock(ms), selection)
            .unwrap(),
    )
}

#[test]
fn passive_status_and_offline_due_work_never_start_a_session_or_write() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let path = temp.path().join("updates/state.json");
    let before = fs::read(&path).unwrap();
    for ms in [100, 1000, DAILY_MS * 4] {
        let status = schedule.status(&clock(ms)).unwrap();
        assert_eq!(status.channel, Channel::Release);
        assert!(status.due);
        assert!(status.active.is_none());
        assert!(
            schedule
                .automatic(&clock(ms), true, Opportunity::None)
                .unwrap()
                .active
                .is_none()
        );
    }
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn normal_network_startup_and_daily_checks_do_not_follow_ntp_retries() {
    let temp = tempfile::tempdir().unwrap();
    let mut updates = open(&temp, 100);
    let first = active(
        updates
            .automatic(&clock(100), false, Opportunity::None)
            .unwrap(),
    );
    assert_eq!(first.trigger, Trigger::Automatic);
    assert!(
        updates
            .finish(&clock(200), first.id, Completion::Failed)
            .unwrap()
    );
    let due = updates.status(&clock(200)).unwrap().next_ms;
    assert_eq!(due, 100 + DAILY_MS);
    let mut time =
        crate::time_schedule::Schedule::open(&temp.path().join("time"), &clock(200)).unwrap();
    // The actual NTP scheduler can consume retries without changing OTA's day.
    for ms in [200, 900_500, 4_501_000, 26_102_000, 47_703_000, 69_304_000] {
        let attempt = time
            .observe(&clock(ms), true, None)
            .unwrap()
            .active
            .unwrap();
        time.fail(
            &clock(ms),
            attempt.id,
            crate::time_schedule::Completion::ProviderUnavailable,
        )
        .unwrap();
        let status = updates
            .automatic(&clock(ms), true, Opportunity::TimeCheck)
            .unwrap();
        assert!(status.active.is_none());
        assert_eq!(status.next_ms, due);
    }
    // A policy toggle is not another update allowance either.
    assert!(
        updates
            .automatic(&clock(due - 1), false, Opportunity::Connected)
            .unwrap()
            .active
            .is_none()
    );
    let second = active(
        updates
            .automatic(&clock(due), true, Opportunity::TimeCheck)
            .unwrap(),
    );
    assert_eq!(second.id, first.id + 1);
    assert_eq!(second.started_ms, due);
}

#[test]
fn due_partial_offline_checks_join_existing_connections_or_time_work() {
    for opportunity in [Opportunity::Connected, Opportunity::TimeCheck] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        assert!(
            schedule
                .automatic(&clock(100), true, Opportunity::None)
                .unwrap()
                .active
                .is_none()
        );
        let attempt = active(schedule.automatic(&clock(1000), true, opportunity).unwrap());
        assert_eq!(attempt.started_ms, 1000);
        assert_eq!(attempt.admission_deadline_ms, 1000 + PREPARATION_MS);
        assert_eq!(attempt.deadline_ms, 1000 + SESSION_MS);
        for ms in [1001, 5000, 10000] {
            assert_eq!(
                active(schedule.automatic(&clock(ms), true, opportunity).unwrap()),
                attempt
            );
        }
    }
}

#[test]
fn repeated_check_now_joins_work_and_explicit_retry_retains_daily_limits() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let first = manual(&mut schedule, 100);
    let initial = schedule.status(&clock(100)).unwrap();
    for ms in [101, 1000, ADMISSION_MS] {
        assert_eq!(manual(&mut schedule, ms), first);
        assert_eq!(
            schedule.status(&clock(ms)).unwrap().generation,
            initial.generation
        );
    }
    schedule
        .finish(&clock(ADMISSION_MS), first.id, Completion::Failed)
        .unwrap();
    assert!(
        schedule
            .automatic(&clock(ADMISSION_MS), false, Opportunity::Connected)
            .unwrap()
            .active
            .is_none()
    );
    let retry = manual(&mut schedule, ADMISSION_MS + 1);
    assert_eq!(retry.id, first.id + 1);
    assert_eq!(retry.deadline_ms, ADMISSION_MS + 1 + SESSION_MS);
    assert!(
        !schedule
            .finish(&clock(ADMISSION_MS + 2), first.id, Completion::Ready)
            .unwrap()
    );
    assert_eq!(
        active(schedule.status(&clock(ADMISSION_MS + 2)).unwrap()),
        retry
    );
}

#[test]
fn queue_delay_counts_toward_admission_and_association_has_no_fresh_window() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let a = active(schedule.check_now(&clock(20_000), &clock(100), 0).unwrap());
    assert_eq!(a.admission_deadline_ms, 30_100);
    assert!(schedule.admitted(&clock(30_099), a.id).unwrap());
    // Admission is separate from association/feed preparation.
    assert!(
        schedule
            .payload_started(&clock(90_000), a.id, &clock(90_000))
            .unwrap()
    );
    assert_eq!(
        active(schedule.status(&clock(90_000)).unwrap()).deadline_ms,
        100 + SESSION_MS
    );

    let late = tempfile::tempdir().unwrap();
    let mut schedule = open(&late, 100);
    let status = schedule.check_now(&clock(30_100), &clock(100), 0).unwrap();
    assert!(status.active.is_none());
    assert_eq!(status.completion, Some(Completion::AdmissionTimeout));
    assert!(!schedule.admitted(&clock(30_101), 1).unwrap());
    assert!(
        schedule
            .automatic(&clock(30_101), true, Opportunity::TimeCheck)
            .unwrap()
            .active
            .is_none()
    );
}

#[test]
fn exact_preparation_and_total_deadlines_reject_late_progress_and_ready() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let a = manual(&mut schedule, 100);
    schedule.admitted(&clock(200), a.id).unwrap();
    assert!(
        !schedule
            .payload_started(
                &clock(a.preparation_deadline_ms),
                a.id,
                &clock(a.preparation_deadline_ms)
            )
            .unwrap()
    );
    assert_eq!(
        schedule
            .status(&clock(a.preparation_deadline_ms))
            .unwrap()
            .completion,
        Some(Completion::PreparationTimeout)
    );

    let b = manual(&mut schedule, a.preparation_deadline_ms + 1);
    assert!(
        schedule
            .payload_started(&clock(b.started_ms), b.id, &clock(b.started_ms))
            .is_err()
    );
    schedule.admitted(&clock(b.started_ms + 1), b.id).unwrap();
    assert!(
        schedule
            .finish(&clock(b.started_ms + 2), b.id, Completion::Ready)
            .is_err()
    );
    schedule
        .payload_started(&clock(b.started_ms + 2), b.id, &clock(b.started_ms + 2))
        .unwrap();
    assert!(
        !schedule
            .finish(&clock(b.deadline_ms), b.id, Completion::Ready)
            .unwrap()
    );
    assert_eq!(
        schedule.status(&clock(b.deadline_ms)).unwrap().completion,
        Some(Completion::SessionTimeout)
    );
}

#[test]
fn payload_progress_uses_its_actual_receipt_without_extending_total_time() {
    for received_offset in [PREPARATION_MS - 1, PREPARATION_MS, SESSION_MS - 1] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        let a = manual(&mut schedule, 100);
        schedule.admitted(&clock(101), a.id).unwrap();
        let observed = clock(a.started_ms + received_offset + 1000);
        let received = clock(a.started_ms + received_offset);
        let accepted = schedule
            .payload_started(&observed, a.id, &received)
            .unwrap();
        assert_eq!(accepted, received_offset < PREPARATION_MS);
        if accepted {
            assert_eq!(
                active(schedule.status(&observed).unwrap()).deadline_ms,
                a.deadline_ms
            );
            assert!(
                !schedule
                    .finish(&clock(a.deadline_ms), a.id, Completion::Ready)
                    .unwrap()
            );
        } else {
            assert!(schedule.status(&observed).unwrap().active.is_none());
        }
    }
}

#[test]
fn all_download_outcomes_wait_for_the_next_day_without_retaining_connection_work() {
    for completion in [
        Completion::Ready,
        Completion::NoCandidate,
        Completion::Ineligible,
        Completion::Failed,
        Completion::Cancelled,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        let a = manual(&mut schedule, 100);
        schedule.admitted(&clock(101), a.id).unwrap();
        schedule
            .payload_started(&clock(102), a.id, &clock(102))
            .unwrap();
        assert!(schedule.finish(&clock(500), a.id, completion).unwrap());
        let status = schedule
            .automatic(&clock(1000), false, Opportunity::Connected)
            .unwrap();
        assert_eq!(status.completion, Some(completion));
        assert!(status.active.is_none());
        assert_eq!(status.next_ms, 100 + DAILY_MS);
    }
}

#[test]
fn process_restart_never_replays_a_session_or_refreshes_download_limits() {
    for progressed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        let a = manual(&mut schedule, 100);
        if progressed {
            schedule.admitted(&clock(101), a.id).unwrap();
            schedule
                .payload_started(&clock(102), a.id, &clock(102))
                .unwrap();
        }
        drop(schedule);
        for ms in [500, 1000, 5000, 100_000] {
            let mut schedule = open(&temp, ms);
            let status = schedule
                .automatic(&clock(ms), true, Opportunity::TimeCheck)
                .unwrap();
            assert!(status.active.is_none());
            assert_eq!(status.completion, Some(Completion::Interrupted));
            assert_eq!(status.next_ms, 100 + DAILY_MS);
            assert!(!schedule.admitted(&clock(ms), a.id).unwrap());
        }
        let mut schedule = open(&temp, 100_000);
        assert_eq!(manual(&mut schedule, 100_000).id, a.id + 1);
    }
}

#[test]
fn channel_changes_invalidate_old_requests_and_do_not_modify_installed_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let original = manual(&mut schedule, 100);
    let nightly = schedule.select(&clock(101), Channel::Nightly).unwrap();
    assert!(nightly.active.is_none());
    assert!(nightly.due);
    assert_eq!(nightly.selection_generation, 1);
    assert_eq!(nightly.completion, Some(Completion::ChannelChanged));
    assert!(schedule.check_now(&clock(102), &clock(100), 0).is_err());
    assert!(
        !schedule
            .finish(&clock(102), original.id, Completion::Ready)
            .unwrap()
    );
    let new = active(
        schedule
            .automatic(&clock(103), true, Opportunity::Connected)
            .unwrap(),
    );
    assert_eq!(new.id, original.id + 1);
    let before = schedule.status(&clock(103)).unwrap();
    assert_eq!(
        schedule.select(&clock(104), Channel::Nightly).unwrap(),
        before
    );
    let release = schedule.select(&clock(105), Channel::Release).unwrap();
    assert_eq!(release.selection_generation, 2);
    assert!(release.due);
    assert!(
        schedule
            .automatic(&clock(106), true, Opportunity::None)
            .unwrap()
            .active
            .is_none()
    );
    // Only a schedule lives here; selection does not write image/trust history.
    let mut entries: Vec<_> = fs::read_dir(temp.path().join("updates"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    entries.sort();
    assert_eq!(entries, ["lock", "state.json"]);
}

#[test]
fn new_boot_offers_one_check_and_rejects_old_boot_callbacks() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let old = manual(&mut schedule, 100);
    schedule.select(&clock(101), Channel::Nightly).unwrap();
    let old = manual(&mut schedule, old.started_ms + 2);
    drop(schedule);
    let new_clock = Clock {
        boot_id: "99999999-2222-3333-4444-555555555555".into(),
        ms: 1,
    };
    let mut schedule = Schedule::open(&temp.path().join("updates"), &new_clock).unwrap();
    let status = schedule.status(&new_clock).unwrap();
    assert_eq!(status.channel, Channel::Nightly);
    assert_eq!(status.selection_generation, 1);
    assert_eq!(status.completion, Some(Completion::InterruptedByBoot));
    assert!(status.due);
    assert!(
        schedule
            .finish(&clock(102), old.id, Completion::Failed)
            .is_err()
    );
    let new = active(
        schedule
            .automatic(&new_clock, true, Opportunity::TimeCheck)
            .unwrap(),
    );
    assert_eq!(new.id, old.id + 1);
    assert_eq!(new.started_ms, 1);
}

#[test]
fn long_sleep_collapses_overdue_days_without_replaying_checks() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let first = manual(&mut schedule, 100);
    let later = 20 * DAILY_MS;
    let next = active(
        schedule
            .automatic(&clock(later), true, Opportunity::Connected)
            .unwrap(),
    );
    assert_eq!(next.id, first.id + 1);
    assert_eq!(next.started_ms, later);
    assert_eq!(
        schedule.status(&clock(later)).unwrap().next_ms,
        later + DAILY_MS
    );
    assert!(
        !schedule
            .finish(&clock(later), first.id, Completion::Ready)
            .unwrap()
    );
}

#[test]
fn uncertain_flush_does_not_authorize_work_and_restart_honors_the_committed_allowance() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        schedule.store.fail_at = Some(point);
        assert!(
            schedule
                .automatic(&clock(100), false, Opportunity::None)
                .is_err()
        );
        assert!(schedule.status(&clock(100)).is_err());
        assert!(schedule.check_now(&clock(101), &clock(101), 0).is_err());
        drop(schedule);
        let mut schedule = open(&temp, 200);
        let status = schedule.status(&clock(200)).unwrap();
        if matches!(point, "renamed" | "directory-synced") {
            assert_eq!(status.completion, Some(Completion::Interrupted));
            assert_eq!(status.next_ms, 100 + DAILY_MS);
            assert!(!status.due);
        } else {
            assert!(status.due);
        }
    }
}

#[test]
fn missing_corrupt_or_replaced_state_cannot_reset_the_automatic_allowance() {
    for fault in ["missing", "corrupt", "symlink", "public", "hardlink"] {
        let temp = tempfile::tempdir().unwrap();
        let mut schedule = open(&temp, 100);
        manual(&mut schedule, 100);
        assert!(Schedule::open(&temp.path().join("updates"), &clock(101)).is_err());
        let path = temp.path().join("updates/state.json");
        match fault {
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => fs::write(&path, b"{}").unwrap(),
            "symlink" => {
                fs::rename(&path, temp.path().join("saved")).unwrap();
                symlink(temp.path().join("saved"), &path).unwrap();
            }
            "public" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            _ => fs::hard_link(&path, temp.path().join("saved")).unwrap(),
        }
        assert!(schedule.status(&clock(101)).is_err());
        drop(schedule);
        assert!(
            Schedule::open(&temp.path().join("updates"), &clock(102)).is_err(),
            "{fault}"
        );
    }
}

#[test]
fn bad_clock_receipt_overflow_and_invalid_completion_leave_state_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = open(&temp, 100);
    let before = fs::read(temp.path().join("updates/state.json")).unwrap();
    assert!(schedule.check_now(&clock(101), &clock(102), 0).is_err());
    assert!(
        schedule
            .finish(&clock(101), 1, Completion::Interrupted)
            .is_err()
    );
    assert!(schedule.status(&clock(100)).is_err());
    assert!(
        schedule
            .automatic(&clock(u64::MAX), false, Opportunity::Connected)
            .is_err()
    );
    assert_eq!(
        fs::read(temp.path().join("updates/state.json")).unwrap(),
        before
    );
    drop(schedule);
    assert!(Schedule::open(&temp.path().join("updates"), &clock(99)).is_err());
}
