use super::*;
use engine_ota::ImageIdentity;
use std::cell::RefCell;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};

fn plan() -> Plan {
    let image = ImageIdentity::parse(include_bytes!(
        "../../engine-ota/tests/fixtures/image-identity-v1.json"
    ))
    .unwrap();
    let hash = Sha256::digest(b"synthetic");
    let good = json!({"slot":"b","build_id":"old","generation":12,"payload":hash,"expanded":hash,"data":image.data});
    let candidate = json!({"slot":"a","build_id":image.build_id,"generation":14,
        "payload":Sha256::digest(b"candidate"),"expanded":hash,"data":image.data});
    let boot = "22222222-2222-2222-2222-222222222222";
    serde_json::from_value(json!({"schema":1,"case":"kiosk-crash","action":"bounded-sigkill-series",
        "selected_ms":21000,"policy":Policy::fixed(),
        "expectation":{"schema":1,"binding":{"device_id":hash,"layout_id":hash,
            "boot_identity":image.boot_identity,"hardware_profile":hash},"kernel_build_id":"a".repeat(40),
            "image":image,"transaction":hash,"origin_boot_id":"11111111-1111-1111-1111-111111111111",
            "trial_boot_id":boot,"previous_good":good,"candidate":candidate},
        "health":{"boot_id":boot,"build_id":image.build_id,"invocation_id":"a".repeat(32),
            "process_id":123,"start_ticks":100,"uptime_ms":20000,"presented_frames":2,"control_response":3}
    })).unwrap()
}

#[test]
fn plan_binds_fixed_policy_action_image_and_early_deadline() {
    let selected = plan();
    selected.initial_time(21_000).unwrap();
    selected.initial_time(30_999).unwrap();
    for now in [20_999, 31_000, 45_000] {
        assert!(selected.initial_time(now).is_err());
    }
    for key in [
        "max_crashes",
        "interval_ms",
        "max_gap_ms",
        "last_at_least_ms",
        "latest_ms",
    ] {
        let mut value = serde_json::to_value(&selected).unwrap();
        value["policy"][key] = json!(value["policy"][key].as_u64().unwrap() + 1);
        let changed: Plan = serde_json::from_value(value).unwrap();
        assert!(changed.initial_time(21_000).is_err());
        assert_ne!(
            changed.fingerprint().unwrap(),
            selected.fingerprint().unwrap()
        );
    }
    for key in ["case", "action"] {
        let mut value = serde_json::to_value(&selected).unwrap();
        value[key] = json!("unbounded");
        assert!(serde_json::from_value::<Plan>(value).is_err());
    }
    for key in ["boot_id", "build_id"] {
        let mut value = serde_json::to_value(&selected).unwrap();
        value["health"][key] = json!("wrong");
        assert!(
            serde_json::from_value::<Plan>(value)
                .unwrap()
                .initial_time(21_000)
                .is_err()
        );
    }
    let mut late = selected.clone();
    late.selected_ms = 44_000;
    late.health.uptime_ms = 43_000;
    late.initial_time(44_999).unwrap();
    assert!(late.initial_time(45_000).is_err());
    let mut value = serde_json::to_value(&selected).unwrap();
    value["extra"] = json!(true);
    assert!(serde_json::from_value::<Plan>(value).is_err());
    let parsed: Plan =
        serde_json::from_str(&serde_json::to_string_pretty(&selected).unwrap()).unwrap();
    assert_eq!(
        parsed.fingerprint().unwrap(),
        selected.fingerprint().unwrap()
    );
}

struct Fake {
    now: u64,
    delay: u64,
    calls: u32,
    failure: &'static str,
}
impl Series for Fake {
    fn wait_until(&mut self, at: u64) -> Result<()> {
        self.now = self.now.max(at);
        Ok(())
    }
    fn kill_once(&mut self, window: Window, previous: &[Receipt]) -> Result<Receipt> {
        self.calls += 1;
        if !previous.is_empty() {
            self.now += self.delay;
        }
        window.check(self.now)?;
        if self.calls == 2 && self.failure == "uncertain" {
            return Err(Error::Invalid("lost signal result".into()));
        }
        let mut health = plan().health;
        health.process_id += window.step;
        health.start_ticks += u64::from(window.step);
        health.invocation_id = format!("{:032x}", window.step);
        health.uptime_ms = self.now;
        if self.calls == 2 && self.failure == "same-process" {
            health = previous[0].health_before.clone();
        }
        let result = Receipt {
            step: window.step,
            health_before: health,
            signal_requested_ms: self.now,
            exit_observed_ms: self.now
                + if self.failure == "slow-exit" {
                    5_000
                } else {
                    10
                },
        };
        self.now = result.exit_observed_ms;
        Ok(result)
    }
}

#[test]
fn bounded_series_interrupts_every_possible_health_window() {
    // Sweep early admission and storage/observation latency. Each accepted run
    // must break continuity before 60 s, and leave less than 60 s before the
    // immutable trial deadline after the final verified exit.
    for first in [1, 10_000, 21_000, 35_000, 44_999] {
        for delay in [0, 1_000, 5_000, 14_000] {
            let mut fake = Fake {
                now: first,
                delay,
                calls: 0,
                failure: "none",
            };
            let receipts = run_series(&mut fake).unwrap();
            assert!(receipts.len() <= 6);
            assert!(receipts[0].signal_requested_ms < engine_ota::recovery::HEALTH_WINDOW_MS);
            for pair in receipts.windows(2) {
                assert!(
                    pair[1].signal_requested_ms - pair[0].signal_requested_ms
                        < engine_ota::recovery::HEALTH_WINDOW_MS
                );
            }
            let last = receipts.last().unwrap();
            assert!((125_000..170_000).contains(&last.signal_requested_ms));
            assert!(
                engine_ota::recovery::TRIAL_DEADLINE_MS - last.signal_requested_ms
                    < engine_ota::recovery::HEALTH_WINDOW_MS
            );
        }
    }
}

#[test]
fn expired_spacing_changed_process_and_uncertain_exit_end_the_series() {
    for (failure, delay, expected_calls) in [
        ("none", 15_000, 2),
        ("same-process", 0, 2),
        ("uncertain", 0, 2),
        ("slow-exit", 0, 1),
    ] {
        let mut fake = Fake {
            now: 21_000,
            delay,
            calls: 0,
            failure,
        };
        assert!(run_series(&mut fake).is_err());
        assert_eq!(fake.calls, expected_calls);
    }
    let mut late = Fake {
        now: 45_000,
        delay: 0,
        calls: 0,
        failure: "none",
    };
    assert!(run_series(&mut late).is_err());
    assert_eq!(late.calls, 1);
    for window in [
        Window {
            step: 7,
            earliest_ms: 0,
            latest_ms: 170_000,
        },
        Window {
            step: 6,
            earliest_ms: 0,
            latest_ms: 170_000,
        },
    ] {
        assert!(window.check(170_000).is_err());
    }
    assert!(
        Window {
            step: 1,
            earliest_ms: 30_000,
            latest_ms: 45_000
        }
        .check(29_999)
        .is_err()
    );
}

#[test]
fn durable_step_and_fresh_recheck_precede_exactly_one_signal() {
    for fail in ["consume", "recheck", "signal", "exit", "none"] {
        let calls = RefCell::new(Vec::new());
        let result = one_crash(
            || {
                calls.borrow_mut().push("consume");
                require(fail != "consume", "consume")
            },
            || {
                calls.borrow_mut().push("recheck");
                require(fail != "recheck", "recheck")?;
                Ok(21_000)
            },
            || {
                calls.borrow_mut().push("signal");
                require(fail != "signal", "signal")
            },
            || {
                calls.borrow_mut().push("exit");
                require(fail != "exit", "exit")?;
                Ok(21_010)
            },
        );
        let expected = match fail {
            "consume" => 1,
            "recheck" => 2,
            "signal" => 3,
            _ => 4,
        };
        assert_eq!(
            *calls.borrow(),
            ["consume", "recheck", "signal", "exit"][..expected]
        );
        assert_eq!(result.is_ok(), fail == "none");
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn real_pidfd_observes_crash_without_signalling_the_replacement() {
    let mut first = ChildGuard(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pinned = Pinned::open(first.0.id()).unwrap();
    assert!(pinned.wait_exited().is_err());
    assert!(first.0.try_wait().unwrap().is_none());
    pinned.signal(libc::SIGKILL).unwrap();
    pinned.wait_exited().unwrap();
    assert_eq!(first.0.wait().unwrap().signal(), Some(libc::SIGKILL));
    let mut second = ChildGuard(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(pinned.signal(libc::SIGKILL).is_err());
    assert!(second.0.try_wait().unwrap().is_none());
}

#[test]
fn shared_consumption_and_step_records_survive_lost_results() {
    let dir = tempfile::tempdir().unwrap();
    let parent = File::open(dir.path()).unwrap();
    let name = format!("{}.json", plan().expectation.transaction());
    hang::create_record(
        &parent,
        &name,
        &json!({"case":"trial-reset","consumed":true}),
    )
    .unwrap();
    assert!(
        hang::create_record(
            &parent,
            &name,
            &json!({"case":"kiosk-crash","consumed":true})
        )
        .is_err()
    );
    let step = "transaction.crash-1.json";
    hang::create_record(
        &parent,
        step,
        &json!({"signal_delivery":"not-yet-established"}),
    )
    .unwrap();
    assert!(hang::create_record(&parent, step, &json!({})).is_err());
    std::fs::write(dir.path().join("partial.json"), b"").unwrap();
    assert!(hang::create_record(&parent, "partial.json", &json!({})).is_err());
}
