use super::*;
use crate::network_controller::{Snapshot, WriteResult};
use crate::network_observation::Observation;
use crate::network_policy::{Activity, Clock};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Mutex;

struct World {
    ms: u64,
    radio: Observation,
    activity: Activity,
    writes: Vec<bool>,
    barrier: Option<(SyncSender<()>, Receiver<()>)>,
}
#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);
impl Fake {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(World {
            ms: 0,
            radio: Observation {
                schema: 1,
                bus_owner: ":1.7".into(),
                pid: 71,
                networking_enabled: true,
                wireless_enabled: true,
                wireless_hardware_enabled: true,
                startup: false,
                checkpoint_count: 0,
                state: 70,
                profile_revision: engine_ota::Sha256::digest(b"fixture"),
                profile_count: 1,
                ethernet_available: false,
                wireless_available: true,
            },
            activity: Activity {
                ssh_sessions: Some(false),
                checkpoints: Some(false),
                install_lock: Some(false),
                operations: Some(false),
            },
            writes: Vec::new(),
            barrier: None,
        })))
    }
}
impl Backend for Fake {
    type ShutdownGuard = ();
    fn clock(&mut self) -> Result<Clock> {
        Ok(Clock {
            boot_id: "11111111-2222-3333-4444-555555555555".into(),
            ms: self.0.lock().unwrap().ms,
        })
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        let clock = self.clock()?;
        let world = self.0.lock().unwrap();
        Ok(Snapshot {
            change: None,
            expected: None,
            clock,
            radio: world.radio.clone(),
            activity: world.activity,
        })
    }
    fn shutdown_guard(&mut self) -> Result<Option<()>> {
        Ok(Some(()))
    }
    fn set_wireless(&mut self, _expected: &Observation, enabled: bool) -> Result<WriteResult> {
        let barrier = self.0.lock().unwrap().barrier.take();
        if let Some((entered, resume)) = barrier {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let mut world = self.0.lock().unwrap();
        world.writes.push(enabled);
        world.radio.wireless_enabled = enabled;
        Ok(WriteResult::Returned)
    }
}

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
fn request(path: &Path, operation: &Request) -> Result<serde_json::Value> {
    request_from(path, operation, uid())
}
fn status(path: &Path) -> serde_json::Value {
    request(path, &Request::Status { schema: 1 }).unwrap()
}

#[test]
fn socket_wireless_fallback_changes_only_the_owned_time_reservation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let fake = Fake::new();
    fake.0.lock().unwrap().radio.ethernet_available = true;
    let _server =
        crate::network_runtime::start_with(&temp.path().join("state"), &path, uid(), fake.clone())
            .unwrap();
    let token = "a".repeat(64);
    let admitted = request(
        &path,
        &Request::Admit {
            schema: 1,
            token: token.clone(),
            kind: HoldKind::TimeCheck,
        },
    )
    .unwrap();
    assert_eq!(admitted["availability"], "ethernet-ready");
    fake.0.lock().unwrap().ms = 15_000;
    let fallback = request(
        &path,
        &Request::RequireWireless {
            schema: 1,
            token: token.clone(),
        },
    )
    .unwrap();
    assert!(fallback["error"].is_null());
    assert_eq!(fallback["availability"], "wireless-ready");
    assert_eq!(
        fallback["status"]["policy"]["holds"][0]["expires_in_ms"],
        105_000
    );
    assert!(!fallback.to_string().contains(&token));
    fake.0.lock().unwrap().ms = 120_000;
    let late = request(&path, &Request::RequireWireless { schema: 1, token }).unwrap();
    assert!(!late["error"].is_null());
}

#[test]
fn network_watch_revokes_old_authority_before_a_blocking_policy_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("state"), fake.clone()).unwrap();
    let generation = controller.status().unwrap().policy.generation;
    controller
        .set_policy(
            generation,
            Policy {
                partial_offline: true,
                stay_connected: false,
            },
        )
        .unwrap();
    for ms in (0..=110_000).step_by(1000) {
        fake.0.lock().unwrap().ms = ms;
        controller.tick().unwrap();
    }
    assert!(!fake.0.lock().unwrap().radio.wireless_enabled);
    let watch = Watch::default();
    let _server = NetworkServer::bind_watched(&path, uid(), controller, watch.clone()).unwrap();
    let before = status(&path);
    let clock = Clock {
        boot_id: "11111111-2222-3333-4444-555555555555".into(),
        ms: 110_000,
    };
    assert!(
        watch
            .read(|| Some(clock.clone()))
            .unwrap()
            .status
            .policy
            .policy
            .partial_offline
    );
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::sync_channel(1);
    fake.0.lock().unwrap().barrier = Some((entered_tx, resume_rx));
    let work = thread::spawn(move || {
        request(
            &path,
            &Request::SetPolicy {
                schema: 1,
                generation: before["status"]["policy"]["generation"].as_u64().unwrap(),
                policy: Policy::default(),
            },
        )
        .unwrap()
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        watch.read(|| Some(clock.clone())).is_none(),
        "provider guard must not see old mode during write"
    );
    resume_tx.send(()).unwrap();
    assert!(work.join().unwrap()["error"].is_null());
    assert!(
        !watch
            .read(|| Some(clock.clone()))
            .unwrap()
            .status
            .policy
            .policy
            .partial_offline
    );
}

#[test]
fn runtime_start_owns_policy_before_replacing_a_stale_socket_and_preserves_saved_policy() {
    use crate::network_runtime::start_with;
    use std::os::unix::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let state = temp.path().join("state");
    let server = start_with(&state, &path, uid(), Fake::new()).unwrap();
    let original = fs::metadata(&path).unwrap().ino();
    assert!(start_with(&state, &path, uid(), Fake::new()).is_err());
    assert_eq!(fs::metadata(&path).unwrap().ino(), original);
    let before = status(&path);
    let reply = request(
        &path,
        &Request::SetPolicy {
            schema: 1,
            generation: before["status"]["policy"]["generation"].as_u64().unwrap(),
            policy: Policy {
                partial_offline: false,
                stay_connected: true,
            },
        },
    )
    .unwrap();
    assert!(reply["error"].is_null());
    drop(server);
    let _server = start_with(&state, &path, uid(), Fake::new()).unwrap();
    assert_eq!(
        status(&path)["status"]["policy"]["policy"]["stay_connected"],
        true
    );

    let regular = temp.path().join("regular");
    fs::write(&regular, b"keep this file").unwrap();
    assert!(
        start_with(
            &temp.path().join("other-state"),
            &regular,
            uid(),
            Fake::new()
        )
        .is_err()
    );
    assert_eq!(fs::read(&regular).unwrap(), b"keep this file");
}

#[test]
fn native_off_write_without_a_live_install_guard_is_never_submitted() {
    use crate::network_runtime::Native;
    let expected = Fake::new().0.lock().unwrap().radio.clone();
    // No host bus or install path is touched: the guard check precedes both.
    let mut backend = Native::default();
    assert_eq!(
        backend.set_wireless(&expected, false).unwrap(),
        WriteResult::NotSubmitted
    );
}

#[test]
#[ignore = "run explicitly as root inside a user namespace; uses only private state and a fake radio"]
fn shared_setup_client_recovers_capacity_from_expired_ssh_admissions() {
    assert_eq!(uid(), 0);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let fake = Fake::new();
    fake.0.lock().unwrap().activity.ssh_sessions = Some(true);
    let _server =
        crate::network_runtime::start_with(&temp.path().join("state"), &path, uid(), fake.clone())
            .unwrap();
    for n in 0..32 {
        let reply = request(
            &path,
            &Request::Admit {
                schema: 1,
                token: format!("{n:064x}"),
                kind: HoldKind::Ssh,
            },
        )
        .unwrap();
        assert!(reply["error"].is_null());
        assert_eq!(reply["availability"], "session-protected");
    }
    let hold = spacewars_control::network::SetupHold::at(&path).unwrap();
    futures_lite::future::block_on(async {
        // Live SSH admission slots cannot be evicted by the setup screen.
        assert!(hold.admit().await.is_err());
        fake.0.lock().unwrap().ms = 120_000;
        assert_eq!(
            status(&path)["status"]["policy"]["holds"]
                .as_array()
                .unwrap()
                .len(),
            32
        );
        hold.admit().await.unwrap();
        let admitted = status(&path);
        let holds = admitted["status"]["policy"]["holds"].as_array().unwrap();
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0]["kind"], "setup");
        hold.renew().await.unwrap();
        hold.release().await.unwrap();
        assert!(
            status(&path)["status"]["policy"]["holds"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    });
    assert!(fake.0.lock().unwrap().writes.is_empty());
}

#[test]
#[ignore = "run explicitly as root inside a user namespace; uses only private state and a fake radio"]
fn shared_setup_client_interoperates_with_the_root_coordinator() {
    assert_eq!(uid(), 0);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let state = temp.path().join("state");
    let fake = Fake::new();
    let server = crate::network_runtime::start_with(&state, &path, uid(), fake.clone()).unwrap();
    let hold = spacewars_control::network::SetupHold::at(&path).unwrap();
    futures_lite::future::block_on(async {
        hold.admit().await.unwrap();
        assert_eq!(
            status(&path)["status"]["policy"]["holds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fake.0.lock().unwrap().radio.profile_revision =
            engine_ota::Sha256::digest(b"setup profile");
        hold.renew().await.unwrap();
        assert_eq!(status(&path)["status"]["suspended"], "profiles-changed");
        hold.release().await.unwrap();
        assert!(
            status(&path)["status"]["policy"]["holds"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(status(&path)["status"]["suspended"], "profiles-changed");
        hold.admit().await.unwrap();
    });
    drop(server);
    let _server = crate::network_runtime::start_with(&state, &path, uid(), fake).unwrap();
    futures_lite::future::block_on(async {
        assert!(hold.renew().await.is_err()); // A restart loses temporary ownership.
        hold.admit().await.unwrap();
        hold.release().await.unwrap();
    });
}

#[test]
fn runtime_retries_initial_owner_contention_without_initializing_over_the_owner() {
    use crate::network_runtime::{spawn_with, start_with};
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let path = temp.path().join("network.sock");
    let held = Controller::open(&state, Fake::new()).unwrap();
    let (attempt_tx, attempt_rx) = mpsc::sync_channel(1);
    let service_path = path.clone();
    let _runtime = spawn_with(move || {
        let result = start_with(&state, &service_path, uid(), Fake::new());
        let _ = attempt_tx.try_send(result.is_ok());
        result
    });
    assert!(!attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap());
    assert!(!path.exists());
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if attempt_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
        {
            break;
        }
    }
    assert!(status(&path)["error"].is_null());
}

#[test]
fn runtime_restarts_a_stopped_coordinator_after_releasing_its_store() {
    use crate::network_runtime::{spawn_with, start_with};
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let path = temp.path().join("network.sock");
    let service_path = path.clone();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let _runtime = spawn_with(move || {
        let server = start_with(&state, &service_path, uid(), Fake::new())?;
        started_tx.try_send(server.stop.clone()).unwrap();
        Ok(server)
    });
    let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let before = status(&path);
    let reply = request(
        &path,
        &Request::SetPolicy {
            schema: 1,
            generation: before["status"]["policy"]["generation"].as_u64().unwrap(),
            policy: Policy {
                partial_offline: false,
                stay_connected: true,
            },
        },
    )
    .unwrap();
    assert!(reply["error"].is_null());
    first.store(true, Ordering::Relaxed);
    let _second = started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let after = status(&path);
    assert_eq!(after["status"]["policy"]["policy"]["stay_connected"], true);
    assert!(
        after["status"]["policy"]["generation"].as_u64().unwrap()
            > reply["status"]["policy"]["generation"].as_u64().unwrap()
    );
}
fn raw(path: &Path, bytes: &[u8]) -> serde_json::Value {
    let mut stream = UnixStream::connect(path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn child(path: &Path, operation: &Request) -> serde_json::Value {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "network_service::tests::subprocess_request",
            "--nocapture",
        ])
        .env("SPACEWARS_NETWORK_TEST_SOCKET", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(operation).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let reply = output
        .lines()
        .find_map(|line| line.split_once("NETWORK_REPLY=").map(|(_, reply)| reply))
        .unwrap();
    serde_json::from_str(reply).unwrap()
}

// Invoked by child() to exercise real peer credentials without Python or other
// optional packages on the target image. Ordinary suite execution is a no-op.
#[test]
fn subprocess_request() {
    let Some(path) = std::env::var_os("SPACEWARS_NETWORK_TEST_SOCKET") else {
        return;
    };
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(FRAME_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= FRAME_LIMIT);
    let operation: Request = serde_json::from_slice(&bytes).unwrap();
    let reply = request(Path::new(&path), &operation).unwrap();
    println!("NETWORK_REPLY={reply}");
}

#[test]
fn real_peers_own_temporary_holds_but_deployments_survive_process_gaps() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let controller = Controller::open(&temp.path().join("state"), Fake::new()).unwrap();
    let _server = NetworkServer::bind(&path, uid(), controller).unwrap();
    assert!(request_from(&path, &Request::Status { schema: 1 }, uid() + 1).is_err());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o660
    );
    let token = HoldToken::generate().unwrap().expose().to_owned();
    let request_admit = Request::Admit {
        schema: 1,
        token: token.clone(),
        kind: HoldKind::Setup,
    };
    let admitted = request(&path, &request_admit).unwrap();
    assert_eq!(admitted["availability"], "wireless-ready");
    assert!(!admitted.to_string().contains(&token));
    assert_eq!(
        request(&path, &request_admit).unwrap()["status"]["policy"]["holds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let release = Request::Release {
        schema: 1,
        token: token.clone(),
    };
    assert_eq!(child(&path, &release)["error"], "rejected");
    assert!(
        request(
            &path,
            &Request::Renew {
                schema: 1,
                token: token.clone()
            }
        )
        .unwrap()["error"]
            .is_null()
    );
    assert!(request(&path, &release).unwrap()["error"].is_null());
    assert!(request(&path, &release).unwrap()["error"].is_null());
    let deploy = Request::Admit {
        schema: 1,
        token,
        kind: HoldKind::Deployment,
    };
    assert!(child(&path, &deploy)["error"].is_null());
    assert_eq!(
        status(&path)["status"]["policy"]["holds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(request(&path, &release).unwrap()["error"].is_null());
}

#[test]
fn narrow_protocol_rejects_spoofed_identity_arbitrary_operations_and_stale_policy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let fake = Fake::new();
    let controller = Controller::open(&temp.path().join("state"), fake.clone()).unwrap();
    let _server = NetworkServer::bind(&path, uid(), controller).unwrap();
    if uid() != 0 {
        let before = status(&path);
        assert_eq!(
            request(
                &path,
                &Request::Admit {
                    schema: 1,
                    token: HoldToken::generate().unwrap().expose().into(),
                    kind: HoldKind::Ssh,
                }
            )
            .unwrap()["error"],
            "rejected"
        );
        assert_eq!(
            status(&path)["status"]["policy"],
            before["status"]["policy"]
        );
    } else {
        let reply = request(
            &path,
            &Request::Admit {
                schema: 1,
                token: HoldToken::generate().unwrap().expose().into(),
                kind: HoldKind::Ssh,
            },
        )
        .unwrap();
        assert!(reply["error"].is_null());
        assert_eq!(reply["availability"], "session-protected");
        assert_eq!(reply["status"]["policy"]["holds"][0]["kind"], "ssh");
    }
    for bytes in [
        b"{\"schema\":2,\"command\":\"status\"}\n".as_slice(),
        b"{\"schema\":1,\"command\":\"status\",\"uid\":0}\n",
        b"{\"schema\":1,\"command\":\"status\",\"pid\":1}\n",
        b"{\"schema\":1,\"command\":\"execute\",\"path\":\"/bin/sh\"}\n",
        b"{\"schema\":1,\"command\":\"restart\",\"unit\":\"sshd\"}\n",
        b"not-json\n",
    ] {
        assert_eq!(raw(&path, bytes)["error"], "invalid-request");
    }
    let mut large = vec![b' '; FRAME_LIMIT + 1];
    large.push(b'\n');
    assert_eq!(raw(&path, &large)["error"], "invalid-request");
    let generation = status(&path)["status"]["policy"]["generation"]
        .as_u64()
        .unwrap();
    let change = Request::SetPolicy {
        schema: 1,
        generation,
        policy: Policy {
            partial_offline: false,
            stay_connected: true,
        },
    };
    assert!(request(&path, &change).unwrap()["error"].is_null());
    assert_eq!(
        request(
            &path,
            &Request::SetPolicy {
                schema: 1,
                generation,
                policy: Policy::default()
            }
        )
        .unwrap()["error"],
        "rejected"
    );
    assert!(
        status(&path)["status"]["policy"]["policy"]["stay_connected"]
            .as_bool()
            .unwrap()
    );
    assert!(fake.0.lock().unwrap().writes.is_empty());
    fs::write(temp.path().join("state/state.json"), b"{}").unwrap();
    assert_eq!(status(&path)["error"], "unavailable");
    assert_eq!(request(&path, &change).unwrap()["error"], "rejected");
    assert!(fake.0.lock().unwrap().writes.is_empty());
}

#[test]
fn client_deadline_includes_a_stalled_reply() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let worker = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(200));
    });
    let start = Instant::now();
    assert!(
        request_before(
            &path,
            &Request::Status { schema: 1 },
            uid(),
            start + Duration::from_millis(30)
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_millis(150));
    worker.join().unwrap();
}

#[test]
fn a_socket_hold_during_shutdown_waits_for_restoration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("state"), fake.clone()).unwrap();
    let generation = controller.status().unwrap().policy.generation;
    controller
        .set_policy(
            generation,
            Policy {
                partial_offline: true,
                stay_connected: false,
            },
        )
        .unwrap();
    for ms in (0..=110_000).step_by(1000) {
        fake.0.lock().unwrap().ms = ms;
        controller.status().unwrap();
    }
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::sync_channel(1);
    fake.0.lock().unwrap().barrier = Some((entered_tx, resume_rx));
    let _server = NetworkServer::bind(&path, uid(), controller).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        ack_tx
            .send(request(
                &path,
                &Request::Admit {
                    schema: 1,
                    token: HoldToken::generate().unwrap().expose().into(),
                    kind: HoldKind::Setup,
                },
            ))
            .unwrap();
    });
    assert!(matches!(
        ack_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    resume_tx.send(()).unwrap();
    let reply = ack_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(reply["availability"], "wireless-ready");
    assert_eq!(reply["status"]["actual"]["wireless_enabled"], true);
    assert_eq!(fake.0.lock().unwrap().writes, [false, true]);
    worker.join().unwrap();
}

#[test]
fn expired_queue_work_and_changed_process_identity_cannot_mutate_policy() {
    let temp = tempfile::tempdir().unwrap();
    let controller = Controller::open(&temp.path().join("state"), Fake::new()).unwrap();
    let (tx, rx) = mpsc::sync_channel(4);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let worker = thread::spawn(move || coordinate(controller, rx, &stopping, Watch::default()));
    let identity = Caller {
        uid: uid(),
        pid: std::process::id(),
        process_start_ticks: kiosk_health::process(std::process::id(), uid()).unwrap(),
    };
    for expired in [true, false] {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        tx.send(Work {
            request: Request::Admit {
                schema: 1,
                token: HoldToken::generate().unwrap().expose().into(),
                kind: HoldKind::Setup,
            },
            caller: Caller {
                process_start_ticks: identity.process_start_ticks + u64::from(!expired),
                ..identity
            },
            deadline: if expired {
                Instant::now() - Duration::from_secs(1)
            } else {
                Instant::now() + Duration::from_secs(2)
            },
            reply: reply_tx,
        })
        .unwrap();
        let reply = reply_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            serde_json::to_value(reply).unwrap()["error"],
            if expired { "expired" } else { "owner-gone" }
        );
    }
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    tx.send(Work {
        request: Request::Status { schema: 1 },
        caller: identity,
        deadline: Instant::now() + Duration::from_secs(2),
        reply: reply_tx,
    })
    .unwrap();
    assert!(
        reply_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .status
            .unwrap()
            .policy
            .holds
            .is_empty()
    );
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
}

#[test]
fn unauthorized_peers_and_excess_or_incomplete_connections_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let controller = Controller::open(&temp.path().join("state"), Fake::new()).unwrap();
    let _server = NetworkServer::bind(&path, uid(), controller).unwrap();
    let mut clients = Vec::new();
    for _ in 0..CLIENTS {
        let mut client = UnixStream::connect(&path).unwrap();
        client.write_all(b"{").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(7)))
            .unwrap();
        clients.push(client);
    }
    thread::sleep(Duration::from_millis(250));
    let started = Instant::now();
    assert!(request(&path, &Request::Status { schema: 1 }).is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    for client in &mut clients {
        let mut bytes = String::new();
        client.read_to_string(&mut bytes).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&bytes).unwrap()["error"],
            "invalid-request"
        );
    }
    if uid() != 0 {
        let other = temp.path().join("other.sock");
        let controller = Controller::open(&temp.path().join("other"), Fake::new()).unwrap();
        let _other = NetworkServer::bind(&other, uid() + 1, controller).unwrap();
        assert!(request(&other, &Request::Status { schema: 1 }).is_err());
    }
}

#[test]
fn a_dead_coordinator_is_unavailable_rather_than_temporary_queue_pressure() {
    let (mut client, server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let caller = peer(&server, uid()).unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    let worker = thread::spawn(move || handle(server, caller, tx).unwrap());
    client
        .write_all(b"{\"schema\":1,\"command\":\"status\"}\n")
        .unwrap();
    let mut line = String::new();
    BufReader::new(client).read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["error"],
        "unavailable"
    );
    worker.join().unwrap();
}
