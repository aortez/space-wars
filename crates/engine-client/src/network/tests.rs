use super::worker::{Backend, Trial};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) fn network() -> Network {
    Network {
        id: "test-wifi".into(),
        device: "/wifi".into(),
        interface: "wlan0".into(),
        name: "Test network".into(),
        ssid: b"Test network".to_vec(),
        ap: "/ap".into(),
        security: Security::WpaPsk,
        strength: 75,
        ..Network::default()
    }
}

#[derive(Default, Clone, Copy)]
enum Failure {
    #[default]
    None,
    Checkpoint,
    Activate,
    WrongPassword,
    Hang,
    LostReply,
    LostConnection,
    Denied,
    Keep,
    Rollback,
    ManageDenied,
}

struct Fake {
    inventory: Mutex<Inventory>,
    calls: Mutex<Vec<&'static str>>,
    failure: Failure,
    ready_calls: AtomicUsize,
    events: async_channel::Sender<&'static str>,
    scan_release: Option<async_channel::Receiver<()>>,
    fail_inventory: AtomicBool,
}

impl Fake {
    fn call(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
        self.events.try_send(name).unwrap();
    }
}

impl Backend for Fake {
    async fn manage(&self, id: &str, change: ProfileChange) -> Result<(), String> {
        self.call("manage");
        assert_eq!(id, "profile-a");
        if matches!(self.failure, Failure::ManageDenied) {
            return Err("Profile change denied.".into());
        }
        let mut inventory = self.inventory.lock().unwrap();
        let profile = inventory.profiles.iter_mut().find(|p| p.id == id).unwrap();
        match change {
            ProfileChange::Autoconnect(enabled) => profile.autoconnect = enabled,
            _ => panic!("unexpected profile operation"),
        }
        Ok(())
    }

    async fn inventory(&self) -> Result<Inventory, String> {
        if self.fail_inventory.swap(false, Ordering::Relaxed) {
            return Err("Inventory temporarily unavailable.".into());
        }
        Ok(self.inventory.lock().unwrap().clone())
    }
    async fn scan(&self, _: &[String]) -> Result<(), String> {
        self.call("scan");
        if let Some(release) = &self.scan_release {
            release.recv().await.unwrap();
        }
        Ok(())
    }
    async fn checkpoint(&self, device: &str) -> Result<String, String> {
        assert_eq!(device, "/wifi");
        self.call("checkpoint");
        if matches!(self.failure, Failure::Checkpoint) {
            Err("Checkpoint unavailable.".into())
        } else {
            Ok("/checkpoint".into())
        }
    }
    async fn activate(
        &self,
        network: &Network,
        password: Option<&str>,
        trial: &mut Trial,
    ) -> Result<(), String> {
        assert_eq!(trial.checkpoint, "/checkpoint");
        if network.saved.is_some() {
            assert_eq!(network.saved.as_deref(), Some("/saved/exact"));
            assert_eq!(password, None);
        } else {
            assert_eq!(password, Some("secret123"));
        }
        self.call("activate");
        if matches!(self.failure, Failure::LostReply) {
            return futures_lite::future::pending().await;
        }
        trial.active = "/active".into();
        trial.created_profile = network.saved.is_none().then(|| "/candidate".into());
        match self.failure {
            Failure::Activate => Err("Activation failed.".into()),
            Failure::Hang => futures_lite::future::pending().await,
            _ => Ok(()),
        }
    }
    async fn ready(&self, _: &Trial) -> Result<bool, String> {
        if matches!(self.failure, Failure::WrongPassword) {
            return Err("Connection failed. Check the password and try again.".into());
        }
        let initial = self.ready_calls.fetch_add(1, Ordering::Relaxed) == 0;
        Ok(initial || !matches!(self.failure, Failure::LostConnection))
    }
    async fn keep(&self, _: &Trial) -> Result<(), String> {
        self.call("keep");
        if matches!(self.failure, Failure::Keep) {
            Err("Could not save.".into())
        } else {
            Ok(())
        }
    }
    async fn rollback(&self, trial: &Trial) -> Result<(), String> {
        assert_eq!(trial.checkpoint, "/checkpoint");
        self.call("rollback");
        if matches!(self.failure, Failure::Rollback) {
            Err("Daemon unavailable.".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
enum Finish {
    Keep,
    Cancel,
    Close,
    CancelConfirmed,
    CloseConfirmed,
    Timeout,
}

fn exercise(failure: Failure, finish: Finish, valid_inventory: bool) -> (Vec<&'static str>, View) {
    exercise_target(failure, finish, valid_inventory, false)
}

fn exercise_target(
    failure: Failure,
    finish: Finish,
    valid_inventory: bool,
    saved: bool,
) -> (Vec<&'static str>, View) {
    async_io::block_on(async {
        let (tx, rx) = async_channel::bounded(2);
        let shared = Arc::new(Mutex::new(View::default()));
        let (events, received) = async_channel::unbounded();
        let fake = Fake {
            inventory: Mutex::new(Inventory {
                can_connect: !matches!(failure, Failure::Denied),
                networks: if valid_inventory {
                    vec![network()]
                } else {
                    vec![]
                },
                profiles: if saved && valid_inventory {
                    vec![SavedNetwork {
                        id: "profile-a".into(),
                        autoconnect: false,
                        network: Some(Network {
                            saved: Some("/saved/exact".into()),
                            ..network()
                        }),
                        ..SavedNetwork::default()
                    }]
                } else {
                    vec![]
                },
                ..Inventory::default()
            }),
            calls: Mutex::new(vec![]),
            failure,
            ready_calls: AtomicUsize::new(0),
            events,
            scan_release: None,
            fail_inventory: AtomicBool::new(false),
        };
        tx.send(if saved {
            Command::ConnectProfile {
                id: "profile-a".into(),
            }
        } else {
            Command::Connect {
                id: "test-wifi".into(),
                password: Some("secret123".into()),
            }
        })
        .await
        .unwrap();
        let duration = if matches!(finish, Finish::Timeout) {
            Duration::from_millis(5)
        } else {
            Duration::from_secs(10)
        };
        let run = worker::run_with_timing(&fake, rx, shared.clone(), duration, duration);
        let driver = async {
            if matches!(finish, Finish::Close | Finish::Cancel) {
                while received.recv().await.unwrap() != "activate" {}
                if matches!(finish, Finish::Close) {
                    tx.close();
                    return;
                }
                tx.send(Command::Cancel).await.unwrap();
            } else if matches!(
                finish,
                Finish::Keep | Finish::CancelConfirmed | Finish::CloseConfirmed
            ) && valid_inventory
                && !matches!(
                    failure,
                    Failure::Checkpoint
                        | Failure::Activate
                        | Failure::WrongPassword
                        | Failure::Denied
                )
            {
                loop {
                    if shared.lock().unwrap().phase == Phase::Confirm {
                        break;
                    }
                    async_io::Timer::after(Duration::from_millis(1)).await;
                }
                match finish {
                    Finish::CloseConfirmed => {
                        tx.close();
                        return;
                    }
                    Finish::CancelConfirmed => tx.send(Command::Cancel).await.unwrap(),
                    _ => tx.send(Command::Keep).await.unwrap(),
                }
            }
            loop {
                let view = shared.lock().unwrap().clone();
                if view.phase == Phase::Idle && !view.status.is_empty() {
                    break;
                }
                async_io::Timer::after(Duration::from_millis(1)).await;
            }
            tx.close();
        };
        worker::bounded(Duration::from_secs(5), async {
            futures_lite::future::zip(run, driver).await;
            Ok(())
        })
        .await
        .expect("test driver must finish");
        let calls = fake.calls.lock().unwrap().clone();
        let view = shared.lock().unwrap().clone();
        assert!(!view.status.contains("secret123"));
        (calls, view)
    })
}

#[test]
fn keep_is_explicit_and_happens_only_after_activation() {
    let (calls, view) = exercise(Failure::None, Finish::Keep, true);
    assert_eq!(calls, ["checkpoint", "activate", "keep"]);
    assert!(view.status.starts_with("Connection kept"));
}

#[test]
fn cancellation_close_timeouts_and_save_failure_all_restore() {
    for (failure, finish) in [
        (Failure::Hang, Finish::Cancel),
        (Failure::Hang, Finish::Close),
        (Failure::LostReply, Finish::Cancel),
        (Failure::LostReply, Finish::Close),
        (Failure::Hang, Finish::Timeout),
        (Failure::None, Finish::Timeout),
        (Failure::None, Finish::CancelConfirmed),
        (Failure::None, Finish::CloseConfirmed),
        (Failure::Activate, Finish::Keep),
        (Failure::Keep, Finish::Keep),
        (Failure::LostConnection, Finish::Keep),
    ] {
        let (calls, _) = exercise(failure, finish, true);
        assert!(calls.starts_with(&["checkpoint", "activate"]));
        assert_eq!(calls.last(), Some(&"rollback"));
    }
}

#[test]
fn wrong_password_after_activation_rolls_back_and_never_saves() {
    let (calls, view) = exercise(Failure::WrongPassword, Finish::Keep, true);
    assert_eq!(calls, ["checkpoint", "activate", "rollback"]);
    assert!(view.status.contains("Check the password"));
    assert!(view.status.contains("configuration restored"));
}

#[test]
fn opening_scans_once_keeps_cached_results_and_rescan_is_rate_limited() {
    async_io::block_on(async {
        let (events, received) = async_channel::unbounded();
        let (release, scan_release) = async_channel::bounded(1);
        let fake = Fake {
            inventory: Mutex::new(Inventory {
                can_connect: true,
                can_scan: true,
                devices: vec!["/wifi".into()],
                networks: vec![network()],
                ..Inventory::default()
            }),
            calls: Mutex::new(vec![]),
            failure: Failure::None,
            ready_calls: AtomicUsize::new(0),
            events,
            scan_release: Some(scan_release),
            fail_inventory: AtomicBool::new(false),
        };
        let (mut session, latest, receiver) = Session::simulated(View::default());
        let run = worker::run(&fake, receiver.clone(), latest.clone());
        let driver = async {
            assert_eq!(received.recv().await.unwrap(), "scan");
            assert_eq!(session.snapshot().phase, Phase::Discovering);
            assert_eq!(session.snapshot().inventory.networks, [network()]);
            session.send(Command::Cancel);
            session.send(Command::Scan);
            assert!(receiver.is_empty(), "a scan is not a cancellable trial");
            release.send(()).await.unwrap();
            while session.snapshot().phase != Phase::Idle {
                async_io::Timer::after(Duration::from_millis(1)).await;
            }
            session.send(Command::Scan);
            while !session.snapshot().status.contains("already requested") {
                async_io::Timer::after(Duration::from_millis(1)).await;
            }
            assert_eq!(*fake.calls.lock().unwrap(), ["scan"]);
            fake.fail_inventory.store(true, Ordering::Relaxed);
            session.send(Command::Refresh);
            while session.snapshot().inventory_error.is_none() {
                async_io::Timer::after(Duration::from_millis(1)).await;
            }
            let view = session.snapshot();
            assert_eq!(view.inventory.networks, [network()]);
            assert!(!view.inventory.can_connect);
            assert!(!view.inventory.can_scan);
            drop(session);
        };
        worker::bounded(Duration::from_secs(5), async {
            futures_lite::future::zip(run, driver).await;
            Ok(())
        })
        .await
        .expect("discovery driver must finish");
    });
}

#[test]
fn missing_permission_never_starts_a_transaction() {
    let (calls, view) = exercise(Failure::Denied, Finish::Keep, true);
    assert!(calls.is_empty());
    assert!(view.status.contains("not permitted"));
}

#[test]
fn no_checkpoint_means_no_connection_mutation() {
    let (calls, _) = exercise(Failure::Checkpoint, Finish::Keep, true);
    assert_eq!(calls, ["checkpoint"]);
    let (calls, view) = exercise(Failure::None, Finish::Keep, false);
    assert!(calls.is_empty());
    assert!(view.status.contains("no longer visible"));
}

#[test]
fn failed_recovery_is_not_reported_as_success() {
    let (calls, view) = exercise(Failure::Rollback, Finish::Timeout, true);
    assert_eq!(calls.last(), Some(&"rollback"));
    assert!(view.status.contains("Recovery could not be confirmed"));
}

#[test]
fn password_validation_covers_personal_security_boundaries() {
    assert!(!Security::WpaPsk.valid_password("short"));
    assert!(Security::WpaPsk.valid_password("eight888"));
    assert!(Security::WpaPsk.valid_password(&"a".repeat(64)));
    assert!(!Security::WpaPsk.valid_password(&"z".repeat(64)));
    assert!(!Security::WpaPsk.valid_password("eight888\n"));
    assert!(Security::Sae.valid_password("x"));
    assert!(!Security::Sae.valid_password(""));
    assert!(Security::Open.valid_password(""));
    assert!(!Security::Unsupported.valid_password("secret123"));
}

#[test]
fn session_busy_gate_rejects_duplicates_and_secrets_never_enter_view() {
    let (mut session, latest, receiver) = Session::simulated(View::default());
    session.send(Command::Connect {
        id: "id".into(),
        password: Some("secret123".into()),
    });
    session.send(Command::Keep);
    session.send(Command::Scan);
    assert_eq!(receiver.len(), 1);
    assert_eq!(latest.lock().unwrap().phase, Phase::Connecting);
    assert!(!format!("{:?}", session.snapshot()).contains("secret123"));
    drop(session);
    assert!(receiver.is_closed());
}

#[test]
fn saved_changes_are_blocked_during_every_busy_phase() {
    for phase in [
        Phase::Discovering,
        Phase::Connecting,
        Phase::Confirm,
        Phase::Saving,
        Phase::Restoring,
        Phase::Managing,
    ] {
        let (mut session, _, receiver) = Session::simulated(View {
            phase,
            ..View::default()
        });
        for change in [
            ProfileChange::Autoconnect(false),
            ProfileChange::Prefer,
            ProfileChange::Forget { allow_active: true },
        ] {
            session.send(Command::Manage {
                id: "profile-a".into(),
                change,
            });
        }
        session.send(Command::ConnectProfile {
            id: "profile-a".into(),
        });
        assert!(
            receiver.is_empty(),
            "queued a saved-profile action during {phase:?}"
        );
    }
}

#[test]
fn manual_profile_with_autoconnect_off_uses_keep_and_restore_trials() {
    let (calls, view) = exercise_target(Failure::None, Finish::Keep, true, true);
    assert_eq!(calls, ["checkpoint", "activate", "keep"]);
    assert!(view.status.starts_with("Connection kept"));
    assert!(!view.inventory.profiles[0].autoconnect);
    let (calls, _) = exercise_target(Failure::None, Finish::CancelConfirmed, true, true);
    assert_eq!(calls, ["checkpoint", "activate", "rollback"]);
    let (calls, view) = exercise_target(Failure::None, Finish::Keep, false, true);
    assert!(calls.is_empty());
    assert!(view.status.contains("no longer exists"));
}

#[test]
fn profile_worker_revalidates_stale_views_and_refreshes_successes_and_failures() {
    for case in [
        "success",
        "denied",
        "removed",
        "active",
        "permission",
        "manual",
        "read-error",
    ] {
        async_io::block_on(async {
            let (events, _received) = async_channel::unbounded();
            let fake = Fake {
                inventory: Mutex::new(Inventory {
                    can_manage: true,
                    profiles: vec![SavedNetwork {
                        id: "profile-a".into(),
                        autoconnect: true,
                        ..SavedNetwork::default()
                    }],
                    ..Inventory::default()
                }),
                calls: Mutex::new(vec![]),
                failure: if case == "denied" {
                    Failure::ManageDenied
                } else {
                    Failure::None
                },
                ready_calls: AtomicUsize::new(0),
                events,
                scan_release: None,
                fail_inventory: AtomicBool::new(false),
            };
            let (mut session, latest, receiver) = Session::simulated(View {
                phase: Phase::Discovering,
                ..View::default()
            });
            let run = worker::run(&fake, receiver, latest);
            let driver = async {
                while session.snapshot().phase != Phase::Idle {
                    async_io::Timer::after(Duration::from_millis(1)).await;
                }
                // Change daemon state after the UI's snapshot, before its click.
                let change = {
                    let mut inventory = fake.inventory.lock().unwrap();
                    match case {
                        "removed" => inventory.profiles.clear(),
                        "active" => inventory.profiles[0].connected = true,
                        "permission" => inventory.can_manage = false,
                        "manual" => inventory.profiles[0].autoconnect = false,
                        "read-error" => fake.fail_inventory.store(true, Ordering::Relaxed),
                        _ => {}
                    }
                    match case {
                        "active" => ProfileChange::Forget {
                            allow_active: false,
                        },
                        "manual" => ProfileChange::Prefer,
                        _ => ProfileChange::Autoconnect(false),
                    }
                };
                session.send(Command::Manage {
                    id: "profile-a".into(),
                    change,
                });
                assert_eq!(session.snapshot().phase, Phase::Managing);
                // A duplicate button press cannot initiate a second change.
                session.send(Command::Manage {
                    id: "profile-a".into(),
                    change,
                });
                while session.snapshot().phase != Phase::Idle {
                    async_io::Timer::after(Duration::from_millis(1)).await;
                }
                let view = session.snapshot();
                assert_eq!(view.inventory, *fake.inventory.lock().unwrap());
                let expected = match case {
                    "success" => "setting saved",
                    "denied" => "denied",
                    "removed" => "no longer exists",
                    "active" => "now active",
                    "permission" => "check permissions",
                    "manual" => "Enable Connect automatically",
                    "read-error" => "temporarily unavailable",
                    _ => unreachable!(),
                };
                assert!(view.status.contains(expected), "{case}: {}", view.status);
                let expected_calls: &[&str] = if matches!(case, "success" | "denied") {
                    &["manage"]
                } else {
                    &[]
                };
                assert_eq!(*fake.calls.lock().unwrap(), expected_calls, "{case}");
                if case == "success" {
                    assert!(!view.inventory.profiles[0].autoconnect);
                }
                drop(session);
            };
            worker::bounded(Duration::from_secs(5), async {
                futures_lite::future::zip(run, driver).await;
                Ok(())
            })
            .await
            .expect("profile worker must finish");
        });
    }
}
