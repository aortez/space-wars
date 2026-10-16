use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

struct PrivateBus(Child, String);
impl PrivateBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("activation fixtures require dbus-daemon");
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
    parameter: String,
    calls: Vec<String>,
    begun: bool,
    failed: bool,
    queued: bool,
    guards: u32,
}

#[derive(Clone)]
struct Login(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login {
    async fn set_reboot_parameter(&self, value: &str) -> zbus::fdo::Result<()> {
        let case = {
            let mut state = self.0.lock().unwrap();
            state.calls.push(format!("set:{value}"));
            assert!(state.begun);
            if value.is_empty() {
                assert!(state.failed || state.case == "failure-storage");
            }
            if state.case == "denied" || (state.case == "cleanup-denied" && value.is_empty()) {
                return Err(zbus::fdo::Error::AccessDenied("fixture".into()));
            }
            if state.case != "retained" {
                state.parameter = value.into();
            }
            state.case
        };
        if !value.is_empty() && case == "lost-set-reply" {
            async_io::Timer::after(Duration::from_millis(600)).await;
        }
        Ok(())
    }
    #[zbus(property)]
    fn reboot_parameter(&self) -> String {
        let mut state = self.0.lock().unwrap();
        state.calls.push("get".into());
        state.parameter.clone()
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
            let mut state = self.0.lock().unwrap();
            assert!(state.begun && !state.failed);
            assert_eq!(state.parameter, TRIAL_PARAMETER);
            assert_eq!(state.guards, 2);
            state.calls.push(format!("start:{unit}:{mode}"));
            state.case
        };
        if case == "lost-job-reply" {
            async_io::Timer::after(Duration::from_millis(600)).await;
        }
        if matches!(case, "job-denied" | "failure-storage" | "cleanup-denied") {
            return Err(zbus::fdo::Error::AccessDenied("fixture".into()));
        }
        Ok(zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/1").unwrap())
    }
}

struct Permit(Arc<Mutex<State>>);
impl RequestPermit for Permit {
    fn begin(&mut self) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("begin".into());
        if s.begun || s.case == "begin-failure" {
            return Err(error("request unavailable"));
        }
        s.begun = true;
        Ok(())
    }
    fn authorize(&mut self) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("authorize".into());
        s.guards += 1;
        if s.case == "foreign-final" && s.guards == 2 {
            s.parameter = "foreign".into();
        }
        if s.case == "foreign-before-set" && s.guards == 1 {
            s.parameter = "foreign".into();
        }
        if (s.case == "first-guard" && s.guards == 1) || (s.case == "last-guard" && s.guards == 2) {
            return Err(error("changed context"));
        }
        Ok(())
    }
    fn queued(&mut self) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("queued".into());
        s.queued = true;
        Ok(())
    }
    fn fail(&mut self) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("fail".into());
        if s.case == "failure-storage" {
            return Err(error("storage failure"));
        }
        s.failed = true;
        Ok(())
    }
}

#[test]
fn activation_orders_durable_authority_and_fixed_reboot_and_preserves_uncertain_failures() {
    futures_lite::future::block_on(async {
        for case in [
            "good",
            "occupied",
            "begin-failure",
            "first-guard",
            "last-guard",
            "denied",
            "retained",
            "foreign-final",
            "foreign-before-set",
            "job-denied",
            "lost-set-reply",
            "lost-job-reply",
            "failure-storage",
            "cleanup-denied",
        ] {
            let bus = PrivateBus::new();
            let state = Arc::new(Mutex::new(State {
                case,
                parameter: if case == "occupied" {
                    "foreign".into()
                } else {
                    String::new()
                },
                calls: Vec::new(),
                begun: false,
                failed: false,
                queued: false,
                guards: 0,
            }));
            let _server = zbus::connection::Builder::address(bus.1.as_str())
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
            let client = zbus::connection::Builder::address(bus.1.as_str())
                .unwrap()
                .method_timeout(Duration::from_millis(200))
                .build()
                .await
                .unwrap();
            let mut permit = Permit(state.clone());
            let result = request_on(&client, &mut permit).await;
            assert_eq!(result.is_ok(), case == "good", "{case}: {result:?}");
            {
                let s = state.lock().unwrap();
                let calls = &s.calls;
                if case == "good" {
                    assert_eq!(
                        calls,
                        &[
                            "get",
                            "begin",
                            "authorize",
                            "get",
                            "set:0 tryboot",
                            "get",
                            "authorize",
                            "get",
                            "start:reboot.target:replace-irreversibly",
                            "queued"
                        ]
                    );
                    assert!(s.queued && !s.failed);
                } else {
                    assert!(!s.queued);
                    if !matches!(
                        case,
                        "job-denied" | "lost-job-reply" | "failure-storage" | "cleanup-denied"
                    ) {
                        assert!(
                            !calls.iter().any(|call| call.starts_with("start:")),
                            "{case}: {calls:?}"
                        );
                    }
                    if matches!(case, "occupied" | "begin-failure") {
                        assert!(!s.begun && !s.failed);
                        assert!(!calls.iter().any(|call| call.starts_with("set:")));
                    } else {
                        assert!(s.begun);
                        assert_eq!(s.failed, case != "failure-storage");
                        assert!(
                            result
                                .as_ref()
                                .unwrap_err()
                                .to_string()
                                .contains("attempt consumed")
                        );
                    }
                    if matches!(case, "occupied" | "foreign-final" | "foreign-before-set") {
                        assert_eq!(s.parameter, "foreign");
                        assert!(!calls.contains(&"set:".into()));
                    } else if case == "cleanup-denied" {
                        assert_eq!(s.parameter, TRIAL_PARAMETER);
                        assert!(
                            result
                                .as_ref()
                                .unwrap_err()
                                .to_string()
                                .contains("parameter cleanup")
                        );
                    } else {
                        assert!(s.parameter.is_empty(), "{case}: {calls:?}");
                    }
                }
            }
            if !matches!(case, "good" | "occupied" | "begin-failure") {
                let before = state
                    .lock()
                    .unwrap()
                    .calls
                    .iter()
                    .filter(|s| s.starts_with("set:") || s.starts_with("start:"))
                    .count();
                assert!(request_on(&client, &mut permit).await.is_err());
                let after = state
                    .lock()
                    .unwrap()
                    .calls
                    .iter()
                    .filter(|s| s.starts_with("set:") || s.starts_with("start:"))
                    .count();
                assert_eq!(before, after, "{case}: replayed a reboot mutation");
            }
        }
    });
}
