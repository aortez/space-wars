use super::*;
use engine_ota::ImageIdentity;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

fn plan() -> Plan {
    let image = ImageIdentity::parse(include_bytes!(
        "../../engine-ota/tests/fixtures/image-identity-v1.json"
    ))
    .unwrap();
    let hash = Sha256::digest(b"synthetic");
    let good = json!({"slot":"b","build_id":"old","generation":12,
        "payload":hash,"expanded":hash,"data":image.data});
    let candidate = json!({"slot":"a","build_id":image.build_id,"generation":14,
        "payload":hash,"expanded":hash,"data":image.data});
    let boot = "22222222-2222-2222-2222-222222222222";
    serde_json::from_value(json!({
        "schema":1,"case":"trial-reset","action":"normal-reboot","selected_ms":21000,
        "expectation":{"schema":1,"binding":{"device_id":hash,"layout_id":hash,
            "boot_identity":image.boot_identity,"hardware_profile":hash},
            "kernel_build_id":"a".repeat(40),"image":image,"transaction":hash,
            "origin_boot_id":"11111111-1111-1111-1111-111111111111",
            "trial_boot_id":boot,"previous_good":good,"candidate":candidate},
        "health":{"boot_id":boot,"build_id":image.build_id,
            "invocation_id":"a".repeat(32),"process_id":123,"start_ticks":100,
            "uptime_ms":20000,"presented_frames":2,"control_response":3}
    }))
    .unwrap()
}

#[test]
fn reset_plan_binds_action_boot_image_and_both_time_limits() {
    let selected = plan();
    selected.check_time(21_000).unwrap();
    selected.check_time(30_999).unwrap();
    for now in [20_999, 31_000, 45_000] {
        assert!(selected.check_time(now).is_err());
    }
    let mut late = plan();
    late.selected_ms = 44_000;
    late.health.uptime_ms = 43_000;
    late.check_time(44_999).unwrap();
    assert!(late.check_time(45_000).is_err());
    for field in ["schema", "health-time", "boot", "image"] {
        let mut bad = plan();
        match field {
            "schema" => bad.schema = 2,
            "health-time" => bad.health.uptime_ms = 21_001,
            "boot" => bad.health.boot_id = "another-boot".into(),
            _ => bad.health.build_id = "another-image".into(),
        }
        assert!(bad.check_time(21_000).is_err());
    }
    for (field, value) in [("case", "kiosk-hang"), ("action", "tryboot")] {
        let mut json = serde_json::to_value(plan()).unwrap();
        json[field] = value.into();
        assert!(serde_json::from_value::<Plan>(json).is_err());
    }
    let native = serde_json::to_value(&selected).unwrap();
    assert_eq!(
        selected.fingerprint().unwrap(),
        Sha256::digest(&serde_json::to_vec(&native).unwrap())
    );
}

struct PrivateBus(Child, String);
impl PrivateBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("trial reset fixtures require a private dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct State {
    case: &'static str,
    gets: usize,
    checks: usize,
    requests: usize,
    consumed: bool,
    queued: bool,
    set_parameter: bool,
}

#[derive(Clone)]
struct Login(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login {
    fn set_reboot_parameter(&self, _value: &str) {
        self.0.lock().unwrap().set_parameter = true;
    }

    #[zbus(property)]
    fn reboot_parameter(&self) -> String {
        let mut s = self.0.lock().unwrap();
        s.gets += 1;
        if s.case == "occupied" || (s.case == "parameter-changed" && s.gets >= 2) {
            "foreign".into()
        } else {
            String::new()
        }
    }
}

#[derive(Clone)]
struct Manager(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn start_unit(
        &self,
        unit: &str,
        mode: &str,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        let case = {
            let mut s = self.0.lock().unwrap();
            assert!(s.consumed);
            assert_eq!(s.checks, 3);
            assert_eq!(unit, "reboot.target");
            assert_eq!(mode, "replace-irreversibly");
            s.requests += 1;
            s.case
        };
        if case == "lost-reply" {
            async_io::Timer::after(Duration::from_millis(600)).await;
        }
        if case == "denied" {
            return Err(zbus::fdo::Error::AccessDenied("fixture".into()));
        }
        Ok(zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/7").unwrap())
    }
}

#[test]
fn native_bus_requests_once_after_durable_intent_and_keeps_uncertain_consumption() {
    futures_lite::future::block_on(async {
        for case in [
            "good",
            "occupied",
            "before-consume-expired",
            "consume-error",
            "after-consume-expired",
            "final-expired",
            "parameter-changed",
            "denied",
            "lost-reply",
            "receipt-error",
        ] {
            let private = PrivateBus::new();
            let state = Arc::new(Mutex::new(State {
                case,
                gets: 0,
                checks: 0,
                requests: 0,
                consumed: false,
                queued: false,
                set_parameter: false,
            }));
            let _server = zbus::connection::Builder::address(private.1.as_str())
                .unwrap()
                .name("org.freedesktop.login1")
                .unwrap()
                .name("org.freedesktop.systemd1")
                .unwrap()
                .serve_at("/org/freedesktop/login1", Login(state.clone()))
                .unwrap()
                .serve_at("/org/freedesktop/systemd1", Manager(state.clone()))
                .unwrap()
                .build()
                .await
                .unwrap();
            let client = zbus::connection::Builder::address(private.1.as_str())
                .unwrap()
                .method_timeout(Duration::from_millis(300))
                .build()
                .await
                .unwrap();
            let bus = NativeBus {
                properties: zbus::Proxy::new(
                    &client,
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.DBus.Properties",
                )
                .await
                .unwrap(),
                manager: zbus::Proxy::new(
                    &client,
                    "org.freedesktop.systemd1",
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                )
                .await
                .unwrap(),
            };
            let temporary = tempfile::tempdir().unwrap();
            let directory = File::open(temporary.path()).unwrap();
            let outcome = request_once(
                &bus,
                || {
                    hang::create_record(&directory, "transaction.json", &json!({"consumed":true}))?;
                    state.lock().unwrap().consumed = true;
                    require(case != "consume-error", "uncertain durability")
                },
                || {
                    let mut s = state.lock().unwrap();
                    s.checks += 1;
                    require(
                        !(case == "before-consume-expired" && s.checks == 1
                            || case == "after-consume-expired" && s.checks == 2),
                        "expired",
                    )
                },
                || {
                    let mut s = state.lock().unwrap();
                    s.checks += 1;
                    require(
                        case != "final-expired",
                        "deadline expired after parameter read",
                    )
                },
                |job| {
                    assert_eq!(job, "/org/freedesktop/systemd1/job/7");
                    require(case != "receipt-error", "receipt storage failed")?;
                    state.lock().unwrap().queued = true;
                    Ok(())
                },
            )
            .await;
            assert_eq!(outcome.is_ok(), case == "good", "{case}: {outcome:?}");
            let consumed = {
                let s = state.lock().unwrap();
                assert!(!s.set_parameter, "{case}");
                assert_eq!(s.queued, case == "good", "{case}");
                let requested = matches!(case, "good" | "denied" | "lost-reply" | "receipt-error");
                assert_eq!(s.requests, usize::from(requested), "{case}");
                s.consumed
            };
            if consumed {
                let before = state.lock().unwrap().requests;
                // The same native record also excludes an intent written by
                // the other fault helper; it cannot be replaced or replayed.
                assert!(
                    request_once(
                        &bus,
                        || hang::create_record(
                            &directory,
                            "transaction.json",
                            &json!({"replay":true})
                        ),
                        || Ok(()),
                        || Ok(()),
                        |_| Ok(())
                    )
                    .await
                    .is_err()
                );
                assert_eq!(state.lock().unwrap().requests, before);
                assert_eq!(
                    std::fs::read_to_string(temporary.path().join("transaction.json")).unwrap(),
                    "{\"consumed\":true}"
                );
            }
        }
    });
}
