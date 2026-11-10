#![cfg(target_os = "linux")]
use engine_os_manager::service::{DeviceStatus, StatusServer, authorized, read_status};
use engine_os_manager::store::TimeStore;
use engine_os_manager::time::*;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn snapshot() -> DeviceStatus {
    DeviceStatus {
        schema: 1,
        boot_id: "fixture-boot".into(),
        service_healthy: true,
        provider_error: None,
        persistence_error: None,
        rtc_error: None,
        recovery: Default::default(),
        time_check: None,
        update: None,
        install_policy: None,
        install_admission: None,
        time: TimeStatus {
            usable: false,
            source: None,
            reason: Some(TimeReason::MissingState),
            utc_us: None,
            last_confirmed_sync_utc_us: None,
            sync_age: SyncAge::Unknown,
            success_generation: 0,
        },
    }
}

#[test]
fn explicit_sync_command_is_authenticated_and_status_reads_never_enqueue_work() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("device.sock");
    let calls = Arc::new(AtomicUsize::new(0));
    let requested = calls.clone();
    let _server = StatusServer::bind_with_sync(
        &path,
        unsafe { libc::geteuid() },
        Arc::new(|| Ok(snapshot())),
        Some(Arc::new(move || {
            requested.fetch_add(1, Ordering::SeqCst);
            Ok(engine_os_manager::time_runtime::Accepted {
                schema: 1,
                received_ms: 1234,
                coalesced: false,
            })
        })),
    )
    .unwrap();
    for _ in 0..3 {
        read_status(&path).unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let request = |value: &[u8]| {
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(value).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        serde_json::from_str::<serde_json::Value>(&reply).unwrap()
    };
    assert_eq!(
        request(b"{\"schema\":1,\"command\":\"sync-now\"}\n")["received_ms"],
        1234
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        request(b"{\"schema\":1,\"command\":\"sync-now\",\"deadline\":999}\n")["error"].is_string()
    );
    assert!(request(b"{\"schema\":2,\"command\":\"sync-now\"}\n")["error"].is_string());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    if unsafe { libc::geteuid() } != 0 {
        assert!(engine_os_manager::service::sync_now(&path).is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "client authenticates root before sending mutation"
        );
    }
}

#[test]
fn update_commands_are_explicit_bounded_and_authenticate_the_service_before_sending() {
    use engine_os_manager::service::{UpdateAction, update_request};
    use std::sync::Mutex;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("updates.sock");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let received = calls.clone();
    let _server = StatusServer::bind_with_actions(
        &path,
        unsafe { libc::geteuid() },
        Arc::new(|| Ok(snapshot())),
        None,
        Some(Arc::new(move |action| {
            received.lock().unwrap().push(match action {
                UpdateAction::Check => "check",
                UpdateAction::Select(engine_ota::Channel::Nightly) => "nightly",
                UpdateAction::Select(engine_ota::Channel::Release) => "release",
            });
            Ok(engine_os_manager::update_runtime::Accepted {
                schema: 1,
                received_ms: 123,
                coalesced: false,
            })
        })),
        None,
    )
    .unwrap();
    for _ in 0..3 {
        read_status(&path).unwrap();
    }
    assert!(calls.lock().unwrap().is_empty());
    let request = |body: &str| {
        let mut socket = UnixStream::connect(&path).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(socket, "{body}").unwrap();
        let mut text = String::new();
        BufReader::new(socket).read_line(&mut text).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    };
    assert_eq!(
        request(r#"{"schema":1,"command":"check-updates"}"#)["received_ms"],
        123
    );
    assert_eq!(
        request(r#"{"schema":1,"command":"update-channel","channel":"nightly"}"#)["received_ms"],
        123
    );
    for bad in [
        r#"{"schema":2,"command":"check-updates"}"#,
        r#"{"schema":1,"command":"check-updates","url":"https://untrusted.invalid/"}"#,
        r#"{"schema":1,"command":"update-channel","channel":"custom"}"#,
    ] {
        assert!(request(bad)["error"].is_string());
    }
    assert_eq!(*calls.lock().unwrap(), ["check", "nightly"]);
    if unsafe { libc::geteuid() } != 0 {
        assert!(update_request(&path, UpdateAction::Check).is_err());
        assert!(update_request(&path, UpdateAction::Select(engine_ota::Channel::Release)).is_err());
        assert_eq!(*calls.lock().unwrap(), ["check", "nightly"]);
    }
}

#[test]
fn install_policy_changes_are_validated_before_the_handler_and_shown_in_status() {
    use engine_os_manager::install_policy::{Change, Mode, Status, Window};
    use engine_os_manager::service::install_policy;
    use serde_json::json;
    use std::sync::Mutex;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("policy.sock");
    let uid = unsafe { libc::geteuid() };
    let defaults = Status {
        generation: 0,
        mode: Mode::Ask,
        window: Window::default(),
    };
    let saved = Arc::new(Mutex::new(defaults.clone()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (shown, changed, received) = (saved.clone(), saved.clone(), calls.clone());
    let _server = StatusServer::bind_with_actions(
        &path,
        uid,
        Arc::new(move || {
            let mut status = snapshot();
            status.install_policy = Some(shown.lock().unwrap().clone());
            Ok(status)
        }),
        None,
        None,
        Some(Arc::new(move |change: Change| {
            received.lock().unwrap().push(change);
            let mut status = changed.lock().unwrap();
            status.generation += 1;
            status.mode = change.mode.unwrap_or(status.mode);
            status.window = change.window.unwrap_or(status.window);
            Ok(status.clone())
        })),
    )
    .unwrap();
    let request = |socket: &std::path::Path, body: &str| {
        let mut socket = UnixStream::connect(socket).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(socket, "{body}").unwrap();
        let mut text = String::new();
        BufReader::new(socket).read_line(&mut text).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    };
    assert_eq!(read_status(&path).unwrap().install_policy, Some(defaults));
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        request(
            &path,
            r#"{"schema":1,"command":"install-policy","mode":"automatic"}"#
        ),
        json!({"schema": 1, "install_policy": {
            "generation": 1, "mode": "automatic",
            "window": {"start_minute": 180, "end_minute": 300},
        }})
    );
    assert_eq!(
        request(
            &path,
            r#"{"schema":1,"command":"install-policy","window":{"start_minute":1350,"end_minute":60}}"#
        )["install_policy"],
        json!({
            "generation": 2, "mode": "automatic",
            "window": {"start_minute": 1350, "end_minute": 60},
        })
    );
    assert_eq!(
        read_status(&path).unwrap().install_policy,
        Some(Status {
            generation: 2,
            mode: Mode::Automatic,
            window: Window {
                start_minute: 1350,
                end_minute: 60,
            },
        })
    );
    for bad in [
        r#"{"schema":1,"command":"install-policy"}"#,
        r#"{"schema":1,"command":"install-policy","mode":null,"window":null}"#,
        r#"{"schema":1,"command":"install-policy","mode":"always"}"#,
        r#"{"schema":1,"command":"install-policy","window":{"start_minute":180,"end_minute":180}}"#,
        r#"{"schema":1,"command":"install-policy","window":{"start_minute":180,"end_minute":1440}}"#,
        r#"{"schema":1,"command":"install-policy","window":{"start_minute":180}}"#,
        r#"{"schema":1,"command":"install-policy","mode":"automatic","generation":2}"#,
        r#"{"schema":2,"command":"install-policy","mode":"automatic"}"#,
    ] {
        assert!(request(&path, bad)["error"].is_string(), "{bad}");
    }
    assert_eq!(calls.lock().unwrap().len(), 2);
    if uid != 0 {
        // The client refuses a service that is not root's before sending anything.
        let ask = Change {
            mode: Some(Mode::Ask),
            window: None,
        };
        assert!(install_policy(&path, ask).is_err());
        assert_eq!(calls.lock().unwrap().len(), 2);
    }

    // A service without the saved preference refuses the change, and a status
    // from before the setting existed still reads.
    let bare = temp.path().join("bare.sock");
    let _bare = StatusServer::bind(&bare, uid, Arc::new(|| Ok(snapshot()))).unwrap();
    assert_eq!(
        request(
            &bare,
            r#"{"schema":1,"command":"install-policy","mode":"automatic"}"#
        ),
        json!({"schema": 1, "error": "unavailable"})
    );
    let mut older = serde_json::to_value(snapshot()).unwrap();
    for field in ["install_policy", "install_admission"] {
        older.as_object_mut().unwrap().remove(field).unwrap();
    }
    let older = serde_json::from_value::<DeviceStatus>(older).unwrap();
    assert!(older.install_policy.is_none() && older.install_admission.is_none());
}

#[test]
fn only_healthy_usable_time_feeds_other_device_decisions() {
    let mut status = snapshot();
    assert!(status.usable_time().is_none());
    status.time.usable = true;
    status.time.utc_us = Some(1_794_310_200_000_000);
    assert_eq!(status.usable_time().unwrap().0.as_second(), 1_794_310_200);
    status.service_healthy = false;
    assert!(status.usable_time().is_none());
}

#[test]
fn private_state_is_atomic_exclusive_and_detects_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("time");
    let store = TimeStore::open(&directory).unwrap();
    assert!(TimeStore::open(&directory).is_err());
    assert_eq!(store.load().1, Some(TimeReason::MissingState));
    let record = TimeRecord::default();
    store.save(&record).unwrap();
    assert_eq!(store.load(), (record.clone(), None));
    assert_eq!(
        fs::metadata(directory.join("state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("state.json")).unwrap()).unwrap();
    value["record"]["success_generation"] = 99.into();
    fs::write(
        directory.join("state.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert_eq!(store.load().1, Some(TimeReason::CorruptState));
    store.save(&record).unwrap();
    drop(store);
    assert_eq!(TimeStore::open(&directory).unwrap().load(), (record, None));
}

#[test]
fn state_rejects_public_permissions_symlinks_and_oversized_files() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("time");
    let store = TimeStore::open(&directory).unwrap();
    let target = temp.path().join("foreign");
    fs::write(&target, b"{}").unwrap();
    symlink(&target, directory.join("state.json")).unwrap();
    assert_eq!(store.load().1, Some(TimeReason::CorruptState));
    store.save(&TimeRecord::default()).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"{}");
    fs::set_permissions(
        directory.join("state.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(store.load().1, Some(TimeReason::CorruptState));
    fs::set_permissions(
        directory.join("state.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::write(directory.join("state.json"), vec![b' '; 16385]).unwrap();
    assert_eq!(store.load().1, Some(TimeReason::CorruptState));
    drop(store);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(TimeStore::open(&directory).is_err());
    symlink(&directory, temp.path().join("alias")).unwrap();
    assert!(TimeStore::open(&temp.path().join("alias")).is_err());
}

fn response(path: &std::path::Path, request: &[u8]) -> serde_json::Value {
    let mut stream = UnixStream::connect(path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    stream.write_all(request).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn status_and_bounded_subscription_use_typed_versioned_requests() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("status.sock");
    let uid = unsafe { libc::geteuid() };
    let _server = StatusServer::bind(&socket, uid, Arc::new(|| Ok(snapshot()))).unwrap();
    let status = read_status(&socket).unwrap();
    assert_eq!(status.boot_id, "fixture-boot");
    assert!(!status.time.usable);
    assert_eq!(
        fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o660
    );
    let subscription = response(
        &socket,
        b"{\"schema\":1,\"command\":\"subscribe\",\"seconds\":1}\n",
    );
    assert_eq!(subscription["schema"], 1);
    assert!(subscription.get("error").is_none());
    for bad in [
        b"{\"schema\":2,\"command\":\"status\"}\n".as_slice(),
        b"{\"schema\":1,\"command\":\"status\",\"path\":\"/etc/shadow\"}\n",
        b"{\"schema\":1,\"command\":\"execute\"}\n",
        b"{\"schema\":1,\"command\":\"subscribe\",\"seconds\":61}\n",
        b"{\"schema\":1,\"command\":\"subscribe\",\"seconds\":0}\n",
        b"not json\n",
    ] {
        assert!(response(&socket, bad).get("error").is_some());
    }
    let mut oversized = vec![b' '; 1025];
    oversized.push(b'\n');
    assert!(response(&socket, &oversized).get("error").is_some());
}

#[test]
fn peer_authorization_uses_credentials_not_request_fields() {
    assert!(authorized(0, 1000));
    assert!(authorized(1000, 1000));
    assert!(!authorized(1001, 1000));
    let uid = unsafe { libc::geteuid() };
    if uid != 0 {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("status.sock");
        let _server = StatusServer::bind(&socket, uid + 1, Arc::new(|| Ok(snapshot()))).unwrap();
        assert!(read_status(&socket).is_err());
    }
}

#[test]
fn connection_capacity_is_bounded_without_an_unbounded_queue_of_workers() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("status.sock");
    let uid = unsafe { libc::geteuid() };
    let _server = StatusServer::bind(&socket, uid, Arc::new(|| Ok(snapshot()))).unwrap();
    let mut clients = Vec::new();
    for _ in 0..8 {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(b"{\"schema\":1,\"command\":\"subscribe\",\"seconds\":60}\n")
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        clients.push(reader);
    }
    let start = Instant::now();
    assert!(read_status(&socket).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    drop(clients);
}

#[test]
fn incomplete_requests_have_an_absolute_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("status.sock");
    let uid = unsafe { libc::geteuid() };
    let _server = StatusServer::bind(&socket, uid, Arc::new(|| Ok(snapshot()))).unwrap();
    let started = Instant::now();
    assert!(response(&socket, b"{").get("error").is_some());
    assert!(started.elapsed() >= Duration::from_secs(4));
    assert!(started.elapsed() < Duration::from_secs(7));
}
