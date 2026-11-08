use super::*;
use std::cell::RefCell;
use std::fs::File;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn plan() -> Plan {
    let image: engine_ota::ImageIdentity = engine_ota::ImageIdentity::parse(include_bytes!(
        "../../engine-ota/tests/fixtures/image-identity-v1.json"
    ))
    .unwrap();
    let hash = Sha256::digest(b"fixture");
    let boot = "22222222-2222-2222-2222-222222222222";
    serde_json::from_value(json!({"schema":1,"case":"recovery-service-failure","action":"kill-recovery-observer",
        "selected_ms":21000,"instance":{"start_ticks":10,"boot_id":boot,"unit":{
            "active":"activating","sub":"start","invocation":"01".repeat(16),"pid":123,"exec_pid":123,
            "exec_start_us":100000,"exec_code":0,"exec_status":0,"result":"success","restarts":0}},
        "trial":{"recovery":hash,"watchdog_armed_ms":1400,"watchdog_timeout_seconds":210},
        "expectation":{"schema":1,"binding":{"device_id":hash,"layout_id":hash,"boot_identity":image.boot_identity,"hardware_profile":hash},
            "kernel_build_id":"a".repeat(40),"image":image,"transaction":hash,
            "origin_boot_id":"11111111-1111-1111-1111-111111111111","trial_boot_id":boot,
            "previous_good":{"slot":"b","build_id":"old","generation":12,"payload":hash,"expanded":hash,"data":image.data},
            "candidate":{"slot":"a","build_id":image.build_id,"generation":15,"payload":Sha256::digest(b"candidate"),"expanded":hash,"data":image.data}}
    })).unwrap()
}

#[test]
fn plan_requires_exact_identity_action_and_early_limits() {
    let p = plan();
    let selected = p.fingerprint().unwrap();
    p.validate(selected).unwrap();
    p.time(21000).unwrap();
    p.time(30999).unwrap();
    for at in [20999, 31000, 45000, 210000] {
        assert!(p.time(at).is_err());
    }
    assert!(p.validate(Sha256::digest(b"other")).is_err());
    for case in [
        "action",
        "case",
        "boot",
        "pid",
        "restart",
        "watchdog",
        "selection",
        "extra",
    ] {
        let mut v = serde_json::to_value(&p).unwrap();
        match case {
            "action" | "case" => v[case] = json!("arbitrary"),
            "boot" => v["instance"]["boot_id"] = json!("old"),
            "pid" => v["instance"]["unit"]["pid"] = json!(1),
            "restart" => v["instance"]["unit"]["restarts"] = json!(1),
            "watchdog" => v["trial"]["watchdog_timeout_seconds"] = json!(211),
            "selection" => v["selected_ms"] = json!(45000),
            "extra" => v["arbitrary"] = json!(true),
            _ => unreachable!(),
        }
        match serde_json::from_value::<Plan>(v) {
            Err(_) => (),
            Ok(p) => assert!(p.validate(p.fingerprint().unwrap()).is_err(), "{case}"),
        }
    }
}

#[test]
fn durable_consumption_and_final_recheck_precede_one_signal() {
    for failure in ["consume", "recheck", "signal", "exit", "late-exit", "good"] {
        let calls = RefCell::new(Vec::new());
        let result = one_kill(
            || {
                calls.borrow_mut().push("consume");
                require(failure != "consume", "consume")
            },
            || {
                calls.borrow_mut().push("recheck");
                require(failure != "recheck", "expired")?;
                Ok(21000)
            },
            || {
                calls.borrow_mut().push("signal");
                require(failure != "signal", "uncertain signal")
            },
            || {
                calls.borrow_mut().push("exit");
                require(failure != "exit", "uncertain exit")?;
                Ok(if failure == "late-exit" { 26000 } else { 21010 })
            },
        );
        let count = match failure {
            "consume" => 1,
            "recheck" => 2,
            "signal" => 3,
            _ => 4,
        };
        assert_eq!(
            *calls.borrow(),
            ["consume", "recheck", "signal", "exit"][..count]
        );
        assert_eq!(result.is_ok(), failure == "good");
    }
}

#[test]
fn actual_pinned_exit_keeps_consumption_and_does_not_target_replacement() {
    let mut child = ChildGuard(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pinned = Pinned::open(child.0.id()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let parent = File::open(directory.path()).unwrap();
    let result = one_kill(
        || {
            hang::create_record(
                &parent,
                "transaction.json",
                &json!({"case":"recovery-service-failure","consumed":true}),
            )
        },
        || Ok(21000),
        || pinned.signal(libc::SIGKILL),
        || {
            pinned.wait_exited()?;
            Ok(21001)
        },
    );
    if result.is_err() {
        let _ = child.0.kill();
    }
    let status = child.0.wait().unwrap();
    result.unwrap();
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    assert!(
        hang::create_record(&parent, "transaction.json", &json!({"case":"kiosk-hang"})).is_err()
    );
    let mut replacement = ChildGuard(Command::new("/bin/sleep").arg("30").spawn().unwrap());
    assert!(pinned.signal(libc::SIGKILL).is_err());
    assert!(replacement.0.try_wait().unwrap().is_none());
}
