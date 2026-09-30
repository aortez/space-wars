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
}

struct Fake {
    inventory: Inventory,
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
    async fn inventory(&self) -> Result<Inventory, String> {
        if self.fail_inventory.swap(false, Ordering::Relaxed) {
            return Err("Inventory temporarily unavailable.".into());
        }
        Ok(self.inventory.clone())
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
        _: &Network,
        password: Option<&str>,
        trial: &mut Trial,
    ) -> Result<(), String> {
        assert_eq!(trial.checkpoint, "/checkpoint");
        assert_eq!(password, Some("secret123"));
        self.call("activate");
        if matches!(self.failure, Failure::LostReply) {
            return futures_lite::future::pending().await;
        }
        trial.active = "/active".into();
        trial.created_profile = Some("/candidate".into());
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
    async_io::block_on(async {
        let (tx, rx) = async_channel::bounded(2);
        let shared = Arc::new(Mutex::new(View::default()));
        let (events, received) = async_channel::unbounded();
        let fake = Fake {
            inventory: Inventory {
                can_connect: !matches!(failure, Failure::Denied),
                networks: if valid_inventory {
                    vec![network()]
                } else {
                    vec![]
                },
                ..Inventory::default()
            },
            calls: Mutex::new(vec![]),
            failure,
            ready_calls: AtomicUsize::new(0),
            events,
            scan_release: None,
            fail_inventory: AtomicBool::new(false),
        };
        tx.send(Command::Connect {
            id: "test-wifi".into(),
            password: Some("secret123".into()),
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
            inventory: Inventory {
                can_connect: true,
                can_scan: true,
                devices: vec!["/wifi".into()],
                networks: vec![network()],
                ..Inventory::default()
            },
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
