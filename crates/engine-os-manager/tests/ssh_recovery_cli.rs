//! Opt-in owner qualification on a Pi, using isolated state and accepted TCP
//! sockets. Never run against the device's real policy or network namespace.
#![cfg(target_os = "linux")]

use engine_os_manager::network_controller::{Backend, Controller, Snapshot, WriteResult};
use engine_os_manager::network_observation::Observation;
use engine_os_manager::network_policy::Clock;
use engine_os_manager::network_service::{NetworkServer, SOCKET_PATH, STATE_DIRECTORY};
use engine_os_manager::{Error, Result, linux};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct UnavailableRadio;

impl Backend for UnavailableRadio {
    type ShutdownGuard = ();

    fn clock(&mut self) -> Result<Clock> {
        Ok(Clock {
            boot_id: linux::boot_id()?,
            ms: linux::clock_reading()?.boottime_us / 1000,
        })
    }

    fn snapshot(&mut self) -> Result<Snapshot> {
        Err(Error::Invalid(
            "isolated qualification has no radio provider".into(),
        ))
    }

    fn shutdown_guard(&mut self) -> Result<Option<()>> {
        panic!("qualification must not acquire a radio shutdown guard")
    }

    fn set_wireless(&mut self, _: &Observation, _: bool) -> Result<WriteResult> {
        panic!("qualification must not write radio settings")
    }
}

fn same_directory(left: &Path, right: &Path) {
    let left = fs::symlink_metadata(left).unwrap();
    let right = fs::symlink_metadata(right).unwrap();
    assert!(left.is_dir() && right.is_dir());
    assert_eq!((left.dev(), left.ino()), (right.dev(), right.ino()));
    assert_eq!(left.uid(), 0);
    assert_eq!(left.mode() & 0o777, 0o700);
}

fn seed(root: &Path, current_boot: bool) -> serde_json::Value {
    let source = fs::read_to_string(root.join("retained-state.json")).unwrap();
    assert!(source.len() < 16 * 1024);
    let envelope: serde_json::Value = serde_json::from_str(&source).unwrap();
    let original = &envelope["record"];
    assert_eq!(original["deployments"], serde_json::json!([]));
    assert_eq!(original["policy"]["partial_offline"], false);
    assert_eq!(original["radio"]["owned_off"], true);
    assert_eq!(original["radio"]["pending"]["phase"], "returned");
    assert_eq!(original["radio"]["pending"]["enabled"], true);
    assert_eq!(original["radio"]["suspended"], "external-radio-change");
    let previous_boot = original["radio"]["witness"]["boot_id"].as_str().unwrap();
    let boot_id = linux::boot_id().unwrap();
    assert_ne!(previous_boot, boot_id);

    // Retain the captured record's field order for its typed-store checksum.
    // The negative control changes only its boot identity and checksum.
    let record = source
        .strip_prefix("{\"record\":")
        .unwrap()
        .rsplit_once(",\"checksum\":")
        .unwrap()
        .0;
    assert_eq!(
        envelope["checksum"],
        format!("{:x}", Sha256::digest(record))
    );
    let record = if current_boot {
        record.replace(previous_boot, &boot_id)
    } else {
        record.to_owned()
    };
    let checksum = format!("{:x}", Sha256::digest(record.as_bytes()));
    let bytes = format!("{{\"record\":{record},\"checksum\":\"{checksum}\"}}");
    let state = Path::new(STATE_DIRECTORY);
    fs::create_dir(state).unwrap();
    fs::set_permissions(state, fs::Permissions::from_mode(0o700)).unwrap();
    for (name, contents) in [("state.json", bytes.as_bytes()), ("lock", b"")] {
        let file = state.join(name);
        fs::write(&file, contents).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    serde_json::from_str(&record).unwrap()
}

#[test]
#[ignore = "owner-only: requires private network/mount namespaces and bound qualification state"]
fn actual_ssh_helper_recovers_an_old_boot_but_rejects_current_boot_faults() {
    assert_eq!(unsafe { libc::geteuid() }, 0);
    let root = std::env::var("SPACEWARS_SSH_QUALIFICATION_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(root.parent(), Some(Path::new("/data")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("spacewars-ssh-qualification-")
    );
    assert_eq!(root.canonicalize().unwrap(), root);
    for namespace in ["net", "mnt"] {
        assert_ne!(
            fs::read_link(format!("/proc/self/ns/{namespace}")).unwrap(),
            fs::read_link(format!("/proc/1/ns/{namespace}")).unwrap(),
        );
    }
    same_directory(Path::new("/data/spacewars-system"), &root.join("state"));
    same_directory(Path::new("/run/spacewars-system"), &root.join("run"));
    let binary = root.join("engine-os-manager");
    assert!(binary.is_file());
    let listener = TcpListener::bind("127.0.0.1:22").unwrap();

    for coordinator in [false, true] {
        for current_boot in [false, true] {
            let name = format!("coordinator-{coordinator}-current-boot-{current_boot}");
            println!("Starting {name}");
            let expected = seed(root, current_boot);
            let server = coordinator.then(|| {
                let controller =
                    Controller::open(Path::new(STATE_DIRECTORY), UnavailableRadio).unwrap();
                NetworkServer::bind(Path::new(SOCKET_PATH), 1000, controller).unwrap()
            });
            let mut peer = TcpStream::connect("127.0.0.1:22").unwrap();
            peer.write_all(b"SSH-2.0-fixture\r\n").unwrap();
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let stdin: OwnedFd = socket.try_clone().unwrap().into();
            let mut child = Command::new(&binary)
                .arg("ssh-network-admit")
                .stdin(Stdio::from(stdin))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(if current_boot { 55 } else { 10 });
            while child.try_wait().unwrap().is_none() {
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("SSH helper exceeded its bound: {name}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.success(),
                !current_boot,
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if current_boot {
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("SSH network protection is unavailable")
                );
            }
            let mut bytes = [0; 17];
            socket.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"SSH-2.0-fixture\r\n");
            drop(server);
            let after: serde_json::Value = serde_json::from_slice(
                &fs::read(Path::new(STATE_DIRECTORY).join("state.json")).unwrap(),
            )
            .unwrap();
            for field in ["radio", "policy", "deployments"] {
                assert_eq!(after["record"][field], expected[field], "{name}: {field}");
            }
            fs::rename(
                STATE_DIRECTORY,
                Path::new(STATE_DIRECTORY).with_file_name(&name),
            )
            .unwrap();
            if Path::new(SOCKET_PATH).exists() {
                fs::rename(SOCKET_PATH, Path::new(SOCKET_PATH).with_file_name(&name)).unwrap();
            }
            println!("Passed {name}; saved fault and TCP bytes preserved");
        }
    }
}
