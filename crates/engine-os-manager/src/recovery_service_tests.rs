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
            .expect("recovery fixtures require dbus-daemon");
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
    parameter: String,
    reject: bool,
    retain: bool,
    calls: Vec<String>,
}
#[derive(Clone)]
struct Login(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login {
    fn set_reboot_parameter(&self, value: &str) -> zbus::fdo::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("parameter:{value}"));
        if state.reject {
            return Err(zbus::fdo::Error::AccessDenied("fixture".into()));
        }
        if !state.retain {
            state.parameter = value.into();
        }
        Ok(())
    }
    #[zbus(property)]
    fn reboot_parameter(&self) -> String {
        self.0.lock().unwrap().parameter.clone()
    }
}
#[derive(Clone)]
struct Manager(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn start_unit(&self, unit: &str, mode: &str) -> zbus::zvariant::OwnedObjectPath {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(format!("start:{unit}:{mode}"));
        zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/1").unwrap()
    }
}

#[test]
fn normal_reboot_clears_and_checks_the_argument_and_reauthorizes_before_fixed_job() {
    futures_lite::future::block_on(async {
        let bus = PrivateBus::new();
        for case in ["good", "denied", "retained", "changed-boot"] {
            let state = Arc::new(Mutex::new(State {
                parameter: "0 tryboot".into(),
                reject: case == "denied",
                retain: case == "retained",
                calls: Vec::new(),
            }));
            let server = zbus::connection::Builder::address(bus.1.as_str())
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
                .method_timeout(Duration::from_secs(2))
                .build()
                .await
                .unwrap();
            let result = normal_reboot_on(&client, || {
                state.lock().unwrap().calls.push("authorize".into());
                if case == "changed-boot" {
                    Err(ota_error("boot changed"))
                } else {
                    Ok(())
                }
            })
            .await;
            assert_eq!(result.is_ok(), case == "good", "{case}: {result:?}");
            let calls = state.lock().unwrap().calls.clone();
            if case == "good" {
                assert_eq!(
                    calls,
                    [
                        "parameter:",
                        "authorize",
                        "start:reboot.target:replace-irreversibly"
                    ]
                );
            } else {
                assert!(
                    !calls.iter().any(|c| c.starts_with("start:")),
                    "{case}: {calls:?}"
                );
            }
            server.release_name("org.freedesktop.login1").await.unwrap();
            server
                .release_name("org.freedesktop.systemd1")
                .await
                .unwrap();
        }
    });
}

#[test]
fn canonical_reboot_preserves_existing_requests_and_rechecks_authority_before_normal_job() {
    futures_lite::future::block_on(async {
        for case in [
            "good",
            "trial",
            "foreign",
            "changed-boot",
            "changed-parameter",
        ] {
            let bus = PrivateBus::new();
            let initial = match case {
                "trial" => "0 tryboot",
                "foreign" => "foreign",
                _ => "",
            };
            let state = Arc::new(Mutex::new(State {
                parameter: initial.into(),
                reject: false,
                retain: false,
                calls: vec![],
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
                .method_timeout(Duration::from_secs(2))
                .build()
                .await
                .unwrap();
            let result = canonical_reboot_on(&client, || {
                let mut state = state.lock().unwrap();
                state.calls.push("authorize".into());
                if case == "changed-parameter" {
                    state.parameter = "foreign".into();
                }
                if case == "changed-boot" {
                    Err(ota_error("boot changed"))
                } else {
                    Ok(())
                }
            })
            .await;
            assert_eq!(result.is_ok(), case == "good");
            let state = state.lock().unwrap();
            if matches!(case, "trial" | "foreign") {
                assert!(state.calls.is_empty());
                assert_eq!(state.parameter, initial);
            } else if case == "good" {
                assert_eq!(
                    state.calls,
                    ["authorize", "start:reboot.target:replace-irreversibly"]
                );
            } else {
                assert_eq!(state.calls, ["authorize"]);
                if case == "changed-parameter" {
                    assert_eq!(state.parameter, "foreign");
                }
            }
        }
    });
}
