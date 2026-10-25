use super::*;
use zbus::zvariant::{Str, Value};

#[test]
fn pid1_collection_is_not_a_failure_receipt_or_permission_to_restart() {
    let (u, s) = fixture();
    let before = decode(u, s).unwrap();
    before.settling(&before).unwrap();
    let mut collecting = before.clone();
    collecting.pid = 0;
    collecting.active = "deactivating".into();
    collecting.sub = "stop-sigterm".into();
    collecting.result = "signal".into();
    collecting.settling(&before).unwrap();
    assert!(collecting.failed(&before).is_err());
    for case in 0..5 {
        let mut changed = collecting.clone();
        match case {
            0 => changed.invocation = "02".repeat(16),
            1 => changed.exec_start_us += 1,
            2 => changed.pid = 321,
            3 => changed.result = "timeout".into(),
            4 => changed.restarts = 1,
            _ => unreachable!(),
        }
        assert!(changed.settling(&before).is_err());
    }
}

fn string(s: &str) -> OwnedValue {
    OwnedValue::from(Str::from(s))
}
fn fixture() -> (Properties, Properties) {
    let mut u = Properties::new();
    for (key, value) in [
        ("Id", UNIT),
        ("LoadState", "loaded"),
        ("CollectMode", "inactive"),
        ("ActiveState", "activating"),
        ("SubState", "start"),
        ("FailureAction", "none"),
        ("SuccessAction", "none"),
        ("StartLimitAction", "none"),
        ("JobTimeoutAction", "none"),
    ] {
        u.insert(key.into(), string(value));
    }
    for key in ["OnFailure", "OnSuccess"] {
        u.insert(
            key.into(),
            OwnedValue::try_from(Value::from(Vec::<String>::new())).unwrap(),
        );
    }
    u.insert(
        "InvocationID".into(),
        OwnedValue::try_from(Value::from(vec![1_u8; 16])).unwrap(),
    );
    let mut s = Properties::new();
    for (key, value) in [
        ("Type", "oneshot"),
        ("ExitType", "main"),
        ("Restart", "no"),
        ("KillMode", "control-group"),
        ("Result", "success"),
    ] {
        s.insert(key.into(), string(value));
    }
    s.insert("SendSIGKILL".into(), true.into());
    s.insert("TimeoutStopUSec".into(), 5_000_000_u64.into());
    for key in ["ExecStop", "ExecStopPost"] {
        s.insert(
            key.into(),
            OwnedValue::try_from(Value::from(Commands::new())).unwrap(),
        );
    }
    for key in [
        "SuccessExitStatus",
        "RestartForceExitStatus",
        "RestartPreventExitStatus",
    ] {
        s.insert(
            key.into(),
            OwnedValue::try_from(Value::from((Vec::<i32>::new(), Vec::<i32>::new()))).unwrap(),
        );
    }
    s.insert("MainPID".into(), 123_u32.into());
    s.insert("ExecMainPID".into(), 123_u32.into());
    s.insert("ExecMainStartTimestampMonotonic".into(), 200_000_u64.into());
    s.insert("ExecMainCode".into(), 0_i32.into());
    s.insert("ExecMainStatus".into(), 0_i32.into());
    s.insert("NRestarts".into(), 0_u32.into());
    (u, s)
}

#[test]
fn properties_require_exact_policy_types_and_no_failure_hooks() {
    let (u, s) = fixture();
    decode(u, s).unwrap().running().unwrap();
    for case in [
        "restart",
        "kill-mode",
        "failure-action",
        "on-failure",
        "stop-command",
        "missing",
        "wrong-type",
        "timeout",
        "empty-invocation",
        "forced-restart",
        "success-signal",
    ] {
        let (mut u, mut s) = fixture();
        match case {
            "restart" => {
                s.insert("Restart".into(), string("always"));
            }
            "kill-mode" => {
                s.insert("KillMode".into(), string("process"));
            }
            "failure-action" => {
                u.insert("FailureAction".into(), string("reboot"));
            }
            "on-failure" => {
                u.insert(
                    "OnFailure".into(),
                    OwnedValue::try_from(Value::from(vec!["reboot.target".to_string()])).unwrap(),
                );
            }
            "stop-command" => {
                s.insert(
                    "ExecStopPost".into(),
                    OwnedValue::try_from(Value::from(vec![(
                        "/bin/false".to_string(),
                        Vec::<String>::new(),
                        false,
                        0_u64,
                        0_u64,
                        0_u64,
                        0_u64,
                        0_u32,
                        0_i32,
                        0_i32,
                    )]))
                    .unwrap(),
                );
            }
            "missing" => {
                s.remove("ExecMainPID");
            }
            "wrong-type" => {
                s.insert("MainPID".into(), string("123"));
            }
            "timeout" => {
                s.insert("TimeoutStopUSec".into(), 60_000_000_u64.into());
            }
            "empty-invocation" => {
                u.insert(
                    "InvocationID".into(),
                    OwnedValue::try_from(Value::from(vec![0_u8; 16])).unwrap(),
                );
            }
            "forced-restart" | "success-signal" => {
                s.insert(
                    if case == "forced-restart" {
                        "RestartForceExitStatus"
                    } else {
                        "SuccessExitStatus"
                    }
                    .into(),
                    OwnedValue::try_from(Value::from((Vec::<i32>::new(), vec![9_i32]))).unwrap(),
                );
            }
            _ => unreachable!(),
        }
        assert!(decode(u, s).is_err(), "{case}");
    }
}

#[test]
fn failed_state_must_be_the_same_invocation_killed_by_sigkill() {
    let (u, s) = fixture();
    let before = decode(u, s).unwrap();
    let mut after = before.clone();
    after.active = "failed".into();
    after.sub = "failed".into();
    after.pid = 0;
    after.exec_code = libc::CLD_KILLED;
    after.exec_status = libc::SIGKILL;
    after.result = "signal".into();
    after.failed(&before).unwrap();
    assert!(after.running().is_err());
    for case in 0..9 {
        let mut changed = after.clone();
        match case {
            0 => changed.pid = 321,
            1 => changed.exec_pid = 321,
            2 => changed.exec_start_us += 1,
            3 => changed.invocation = "02".repeat(16),
            4 => changed.exec_status = libc::SIGTERM,
            5 => changed.exec_code = libc::CLD_EXITED,
            6 => changed.result = "timeout".into(),
            7 => changed.restarts = 1,
            8 => changed.active = "inactive".into(),
            _ => unreachable!(),
        }
        assert!(changed.failed(&before).is_err(), "case {case}");
    }
}
