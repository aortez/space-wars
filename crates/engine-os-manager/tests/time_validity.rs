use engine_os_manager::time::*;

const SECOND: u64 = 1_000_000;
const UTC: u64 = 1_800_000_000 * SECOND;

fn reading(seconds: u64) -> ClockReading {
    ClockReading {
        realtime_us: UTC + seconds * SECOND,
        monotonic_us: seconds * SECOND,
        boottime_us: seconds * SECOND,
    }
}

fn tracker() -> TimeTracker {
    TimeTracker::new(
        "boot-a".into(),
        TimeRecord::default(),
        TimeReason::MissingState,
        reading(100),
        None,
    )
}

fn observed(seconds: u64, packet_count: u64) -> SyncObservation {
    SyncObservation {
        provider: ProviderIdentity {
            bus_owner: ":1.12".into(),
            invocation_id: "ab".repeat(16),
            pid: 41,
            uid: 992,
        },
        receipt: ClockReceipt {
            packet_count,
            realtime_us: UTC + seconds * SECOND,
            monotonic_us: seconds * SECOND,
            boottime_us: seconds * SECOND,
        },
        ntp_packet_count: packet_count,
        ntp_spike: false,
        ntp_valid: true,
        notification_realtime_us: Some(UTC + seconds * SECOND + 100),
        kernel_synchronized: true,
        now: reading(seconds + 1),
    }
}

#[test]
fn fresh_applied_non_spike_completion_is_usable_and_deduplicated() {
    let mut state = tracker();
    let sample = observed(100, 1);
    assert!(state.observe(&sample));
    assert!(!state.observe(&sample));
    let status = state.status(reading(111));
    assert!(status.usable);
    assert_eq!(status.source, Some(TimeSource::NetworkSync));
    assert_eq!(status.success_generation, 1);
    assert_eq!(
        status.sync_age,
        SyncAge::Known {
            seconds: 11,
            overdue: false
        }
    );
}

#[test]
fn marker_checkpoint_link_and_sticky_kernel_state_are_not_completion() {
    let mut samples = Vec::new();
    let mut zero = observed(100, 1);
    zero.receipt.packet_count = 0;
    samples.push(zero);
    let mut rejected = observed(100, 2);
    rejected.receipt.packet_count = 1; // failed adjustment kept the older receipt
    samples.push(rejected);
    let mut spike = observed(100, 1);
    spike.ntp_spike = true;
    samples.push(spike);
    let mut invalid = observed(100, 1);
    invalid.ntp_valid = false;
    samples.push(invalid);
    let mut failed_clock = observed(100, 1);
    failed_clock.kernel_synchronized = false;
    samples.push(failed_clock);
    let mut missing_marker = observed(100, 1);
    missing_marker.notification_realtime_us = None;
    samples.push(missing_marker);
    let mut old_marker = observed(100, 1);
    old_marker.notification_realtime_us = Some(UTC);
    samples.push(old_marker);
    let mut checkpoint_save = observed(100, 1);
    checkpoint_save.notification_realtime_us = Some(UTC + 150 * SECOND);
    samples.push(checkpoint_save);
    let mut inconsistent_clock = observed(100, 1);
    inconsistent_clock.now.realtime_us += 3600 * SECOND;
    samples.push(inconsistent_clock);
    for sample in samples {
        let mut state = tracker();
        assert!(!state.observe(&sample), "{sample:?}");
        assert!(!state.status(sample.now).usable);
        assert_eq!(state.record().success_generation, 0);
    }
}

#[test]
fn service_restart_preserves_age_and_does_not_recount_cached_packet() {
    let mut state = tracker();
    let sample = observed(100, 1);
    assert!(state.observe(&sample));
    let record: TimeRecord =
        serde_json::from_slice(&serde_json::to_vec(state.record()).unwrap()).unwrap();
    let mut restarted = TimeTracker::new(
        "boot-a".into(),
        record,
        TimeReason::WaitingForNetworkSync,
        reading(500),
        None,
    );
    assert!(restarted.status(reading(500)).usable);
    let mut cached = sample;
    cached.now = reading(500);
    assert!(!restarted.observe(&cached));
    assert_eq!(
        restarted.status(reading(500)).sync_age,
        SyncAge::Known {
            seconds: 400,
            overdue: false
        }
    );
    assert_eq!(restarted.record().success_generation, 1);
}

#[test]
fn provider_restart_needs_its_own_success_and_keeps_prior_running_time() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 4)));
    let mut restarted = observed(110, 1);
    restarted.provider.bus_owner = ":1.32".into();
    restarted.provider.invocation_id = "cd".repeat(16);
    restarted.provider.pid = 500;
    restarted.receipt.packet_count = 0;
    assert!(!state.observe(&restarted));
    assert!(state.status(reading(111)).usable);
    assert_eq!(state.record().success_generation, 1);
    restarted.receipt.packet_count = 1;
    assert!(state.observe(&restarted));
    assert_eq!(state.record().success_generation, 2);
}

#[test]
fn loss_of_wifi_and_overdue_sync_do_not_make_running_time_unknown() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    state.tick(reading(100 + 48 * 3600));
    assert_eq!(
        state.status(reading(100 + 48 * 3600)).sync_age,
        SyncAge::Known {
            seconds: 48 * 3600,
            overdue: false
        }
    );
    state.tick(reading(101 + 48 * 3600));
    let status = state.status(reading(101 + 48 * 3600));
    assert!(status.usable);
    assert_eq!(
        status.sync_age,
        SyncAge::Known {
            seconds: 48 * 3600 + 1,
            overdue: true
        }
    );
}

#[test]
fn accepted_forward_and_backward_steps_use_boot_elapsed_time() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    let mut forward = observed(200, 2);
    forward.receipt.realtime_us += 3600 * SECOND;
    forward.notification_realtime_us = Some(forward.receipt.realtime_us + 100);
    forward.now.realtime_us += 3600 * SECOND;
    assert!(state.observe(&forward));
    let mut backward = observed(300, 3);
    backward.receipt.realtime_us -= 24 * 3600 * SECOND;
    backward.notification_realtime_us = Some(backward.receipt.realtime_us + 100);
    backward.now.realtime_us -= 24 * 3600 * SECOND;
    assert!(state.observe(&backward));
    let status = state.status(backward.now);
    assert!(status.usable);
    assert_eq!(status.success_generation, 3);
    assert_eq!(
        status.sync_age,
        SyncAge::Known {
            seconds: 1,
            overdue: false
        }
    );
}

#[test]
fn unexplained_clock_change_stays_invalid_after_restart_until_a_new_receipt() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    let mut changed = reading(110);
    changed.realtime_us += 100 * SECOND;
    state.tick(changed);
    assert!(!state.status(changed).usable);
    let mut restarted = TimeTracker::new(
        "boot-a".into(),
        state.record().clone(),
        TimeReason::WaitingForNetworkSync,
        changed,
        None,
    );
    assert!(!restarted.status(changed).usable);
    let mut new = observed(120, 2);
    new.receipt.realtime_us += 100 * SECOND;
    new.notification_realtime_us = Some(new.receipt.realtime_us + 100);
    new.now.realtime_us += 100 * SECOND;
    assert!(restarted.observe(&new));
    assert!(restarted.status(new.now).usable);
}

#[test]
fn suspend_age_uses_boottime_and_does_not_treat_sleep_as_a_clock_step() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    let mut after_suspend = reading(110);
    after_suspend.realtime_us += 3600 * SECOND;
    after_suspend.boottime_us += 3600 * SECOND;
    state.tick(after_suspend);
    assert!(state.status(after_suspend).usable);
    assert_eq!(
        state.status(after_suspend).sync_age,
        SyncAge::Known {
            seconds: 3610,
            overdue: false
        }
    );
}

fn initialized_rtc() -> (TimeRecord, RtcBootReading) {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    state.record_rtc_write("rpi-rtc".into(), UTC + 101 * SECOND);
    (
        state.record().clone(),
        RtcBootReading {
            boot_id: "boot-b".into(),
            driver: Some("rpi-rtc".into()),
            utc_us: Some(UTC + 200 * SECOND),
            boottime_us: 0,
            retention_established: true,
        },
    )
}

#[test]
fn qualified_retained_rtc_is_usable_but_does_not_invent_cross_boot_sync_age() {
    let (record, boot) = initialized_rtc();
    let now = ClockReading {
        realtime_us: UTC + 210 * SECOND,
        monotonic_us: 10 * SECOND,
        boottime_us: 10 * SECOND,
    };
    let state = TimeTracker::new(
        "boot-b".into(),
        record,
        TimeReason::WaitingForNetworkSync,
        now,
        Some(&boot),
    );
    let status = state.status(now);
    assert!(status.usable);
    assert_eq!(status.source, Some(TimeSource::RetainedRtc));
    assert_eq!(status.sync_age, SyncAge::Unknown);
    assert_eq!(status.success_generation, 1);
}

#[test]
fn absent_uninitialized_unqualified_reset_and_unreadable_rtc_stay_unknown() {
    let (record, boot) = initialized_rtc();
    let now = ClockReading {
        realtime_us: UTC + 210 * SECOND,
        monotonic_us: 10 * SECOND,
        boottime_us: 10 * SECOND,
    };
    let mut absent = boot.clone();
    absent.driver = None;
    let mut unqualified = boot.clone();
    unqualified.retention_established = false;
    let mut reset = boot.clone();
    reset.utc_us = Some(100 * SECOND);
    let mut wrong_boot = boot.clone();
    wrong_boot.boot_id = "boot-a".into();
    let mut unreadable = boot.clone();
    unreadable.utc_us = None;
    for reading in [absent, unqualified, reset, wrong_boot, unreadable] {
        let state = TimeTracker::new(
            "boot-b".into(),
            record.clone(),
            TimeReason::WaitingForNetworkSync,
            now,
            Some(&reading),
        );
        assert!(!state.status(now).usable);
    }
    let state = TimeTracker::new(
        "boot-b".into(),
        TimeRecord::default(),
        TimeReason::MissingState,
        now,
        Some(&boot),
    );
    assert_eq!(state.status(now).reason, Some(TimeReason::RtcUninitialized));
}

#[test]
fn new_boot_does_not_reuse_previous_network_success_as_current_time() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    let mut rebooted = TimeTracker::new(
        "boot-b".into(),
        state.record().clone(),
        TimeReason::WaitingForNetworkSync,
        reading(10),
        None,
    );
    assert!(!rebooted.status(reading(10)).usable);
    assert_eq!(rebooted.status(reading(10)).sync_age, SyncAge::Unknown);
    assert!(rebooted.observe(&observed(20, 1)));
    assert_eq!(rebooted.record().success_generation, 2);
}

#[test]
fn failed_rtc_persistence_does_not_revoke_network_time() {
    let mut state = tracker();
    assert!(state.observe(&observed(100, 1)));
    state.invalidate_rtc(); // write/readback failure creates no new association
    assert!(state.status(reading(101)).usable);
    assert!(state.record().rtc.is_none());
    state.record_rtc_write("rpi-rtc".into(), UTC + 101 * SECOND);
    assert_eq!(state.record().rtc.as_ref().unwrap().sync_generation, 1);
}

#[test]
fn future_receipt_and_generation_overflow_are_rejected() {
    let mut future = observed(100, 1);
    future.receipt.boottime_us = 200 * SECOND;
    assert!(!tracker().observe(&future));
    let record = TimeRecord {
        success_generation: u64::MAX,
        ..TimeRecord::default()
    };
    let mut state = TimeTracker::new(
        "boot-a".into(),
        record,
        TimeReason::WaitingForNetworkSync,
        reading(100),
        None,
    );
    assert!(!state.observe(&observed(100, 1)));
    assert!(!state.status(reading(101)).usable);
}

#[test]
fn same_boot_rtc_service_restart_reuses_the_confirmed_boot_decision() {
    let (record, boot) = initialized_rtc();
    let now = ClockReading {
        realtime_us: UTC + 210 * SECOND,
        monotonic_us: 10 * SECOND,
        boottime_us: 10 * SECOND,
    };
    let state = TimeTracker::new(
        "boot-b".into(),
        record,
        TimeReason::WaitingForNetworkSync,
        now,
        Some(&boot),
    );
    let restarted = TimeTracker::new(
        "boot-b".into(),
        state.record().clone(),
        TimeReason::WaitingForNetworkSync,
        now,
        None,
    );
    assert!(restarted.status(now).usable);
    assert_eq!(restarted.status(now).source, Some(TimeSource::RetainedRtc));
    let next_boot = TimeTracker::new(
        "boot-c".into(),
        state.record().clone(),
        TimeReason::WaitingForNetworkSync,
        now,
        None,
    );
    assert!(!next_boot.status(now).usable);
}
