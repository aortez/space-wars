use super::*;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
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
            .expect("health fixtures require dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Clone)]
struct Unit(Arc<Mutex<(u32, u8)>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> &str {
        "active"
    }
    #[zbus(property, name = "InvocationID")]
    fn invocation_id(&self) -> Vec<u8> {
        vec![self.0.lock().unwrap().1; 16]
    }
}
#[derive(Clone)]
struct Service(Arc<Mutex<(u32, u8)>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> u32 {
        self.0.lock().unwrap().0
    }
}

fn expected() -> KioskExpectation {
    let path = std::env::current_exe().unwrap();
    // Cargo follows the developer's umask; packaged executables are 0755.
    // Only remove group/world write bits on this test artifact. The file lock
    // makes this one-time preparation safe when nextest uses separate processes.
    let file = File::open(&path).unwrap();
    file.lock().unwrap();
    let mode = file.metadata().unwrap().mode();
    if mode & 0o022 != 0 {
        file.set_permissions(fs::Permissions::from_mode(mode & !0o022))
            .unwrap();
    }
    file.unlock().unwrap();
    let bytes = fs::read(path).unwrap();
    let mut image = engine_ota::ImageIdentity::parse(include_bytes!(
        "../../engine-ota/tests/fixtures/image-identity-v1.json"
    ))
    .unwrap();
    image.binaries.client.bytes = bytes.len() as u64;
    image.binaries.client.sha256 = engine_ota::Sha256::digest(&bytes);
    KioskExpectation::from_image(&image).unwrap()
}

fn server(
    path: &Path,
    reply: impl FnOnce(HealthRequest) -> String + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        if request.is_empty() {
            return;
        } // credential rejection before sending
        let request = HealthRequest::from_json(request.lines().nth(1).unwrap()).unwrap();
        let _ = stream.write_all(reply(request).as_bytes());
    })
}

fn response(request: HealthRequest) -> HealthState {
    HealthState {
        schema_version: 1,
        nonce: request.nonce,
        process_id: std::process::id(),
        build_revision: "aaaaaaaaaaaa".into(),
        ui_responses: 10,
        presented_frames: Some(100),
    }
}

#[test]
fn control_challenge_checks_os_credentials_nonce_size_and_framing() {
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::geteuid() };
        for case in [
            "good",
            "wrong-pid",
            "wrong-uid",
            "stale",
            "oversized",
            "missing-newline",
            "extra-json",
        ] {
            let path = dir.path().join(case);
            let thread = server(&path, move |request| {
                let mut state = response(request);
                if case == "stale" {
                    state.nonce += 1;
                }
                let json = serde_json::to_string(&state).unwrap();
                match case {
                    "oversized" => "x".repeat(MAX_RESPONSE + 1),
                    "missing-newline" => format!("ok {json}"),
                    "extra-json" => format!("ok {json}\n{{}}\n"),
                    _ => format!("ok {json}\n"),
                }
            });
            let pid = std::process::id() + u32::from(case == "wrong-pid");
            let peer_uid = uid + u32::from(case == "wrong-uid");
            // A wrong socket owner is rejected before connecting; don't leave
            // that fixture's accept waiting for a client that should never come.
            if case == "wrong-uid" {
                let _ = UnixStream::connect(&path);
            }
            let result = challenge(&path, pid, peer_uid, 42).await;
            assert_eq!(result.is_ok(), case == "good", "{case}: {result:?}");
            thread.join().unwrap();
        }
    });
}

#[test]
fn live_process_and_private_systemd_evidence_produce_only_coherent_samples() {
    futures_lite::future::block_on(async {
        let bus = PrivateBus::new();
        let state = Arc::new(Mutex::new((std::process::id(), 42)));
        let _systemd = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name("org.freedesktop.systemd1")
            .unwrap()
            .serve_at(UNIT, Unit(state.clone()))
            .unwrap()
            .serve_at(UNIT, Service(state.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let connection = bus.connect().await;
        let dir = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let expected = expected();
        let uid = unsafe { libc::geteuid() };
        for case in [
            "good",
            "restart",
            "wrong-revision",
            "wrong-response-pid",
            "no-display",
            "state-failed",
        ] {
            *state.lock().unwrap() = (std::process::id(), 42);
            let path = dir.path().join(case);
            let changed = state.clone();
            let thread = server(&path, move |request| {
                let mut reply = response(request);
                match case {
                    "restart" => changed.lock().unwrap().1 = 43,
                    "wrong-revision" => reply.build_revision = "wrong".into(),
                    "wrong-response-pid" => reply.process_id += 1,
                    "no-display" => reply.presented_frames = None,
                    _ => {}
                }
                format!("ok {}\n", serde_json::to_string(&reply).unwrap())
            });
            let result = observe_connection(
                &connection,
                &ProbePaths {
                    socket: &path,
                    executable: &executable,
                    uid,
                    executable_owner: uid,
                },
                &expected,
                || {
                    if case == "state-failed" {
                        return invalid("fixture read-only data");
                    }
                    write_state_probe(dir.path())
                },
            )
            .await;
            assert_eq!(result.is_ok(), case == "good", "{case}: {result:?}");
            if let Ok(health) = result {
                assert_eq!(health.process_id, std::process::id());
                assert_eq!(health.invocation_id, "2a".repeat(16));
                assert_eq!(health.control_response, 10);
                assert_eq!(health.presented_frames, 100);
                assert!(health.start_ticks > 0);
            }
            thread.join().unwrap();
        }
    });
}

#[test]
fn executable_identity_checks_bytes_owner_and_inode() {
    let mut expected = expected();
    let path = std::env::current_exe().unwrap();
    let pid = std::process::id();
    let uid = unsafe { libc::geteuid() };
    executable(pid, &path, uid, &expected).unwrap();
    assert!(executable(pid, &path, uid + 1, &expected).is_err());
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("other-executable");
    fs::copy(&path, &copy).unwrap();
    assert!(executable(pid, &copy, uid, &expected).is_err());
    expected.executable_sha256[0] ^= 1;
    assert!(executable(pid, &path, uid, &expected).is_err());
    expected.executable_bytes += 1;
    assert!(executable(pid, &path, uid, &expected).is_err());
}

#[test]
fn process_start_parser_handles_comm_parentheses_and_rejects_wrong_identity() {
    let fields = std::iter::once("S")
        .chain(std::iter::repeat_n("0", 18))
        .chain(["1234", "0"])
        .collect::<Vec<_>>()
        .join(" ");
    let text = format!("57 (name with ) and (\n) {fields}");
    assert_eq!(start_ticks(&text, 57).unwrap(), 1234);
    assert!(start_ticks(&text, 58).is_err());
    assert!(start_ticks("57 (truncated) S 0", 57).is_err());
    assert!(start_ticks(&text.replace("1234", "0"), 57).is_err());
    assert!(process(std::process::id(), unsafe { libc::geteuid() } + 1).is_err());
}

#[test]
fn required_state_probe_preserves_user_files_and_removes_temporary_data() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.toml"), b"keep this").unwrap();
    write_state_probe(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join("settings.toml")).unwrap(),
        b"keep this"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(write_state_probe(&dir.path().join("absent")).is_err());
}
