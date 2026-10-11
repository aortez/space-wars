#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Behavior {
    Normal,
    Hang,
    ReplaceSocket,
    ChangeSettings,
}

struct App {
    directory: tempfile::TempDir,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    commands: Arc<Mutex<Vec<String>>>,
}

impl App {
    fn new(behavior: Behavior) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("settings.toml"),
            "untouched preferences\n",
        )
        .unwrap();
        let listener = UnixListener::bind(directory.path().join("control.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let worker = {
            let stop = stop.clone();
            let commands = commands.clone();
            let socket_path = directory.path().join("control.sock");
            let settings_path = directory.path().join("settings.toml");
            thread::spawn(move || {
                let mut hung = Vec::<UnixStream>::new();
                let mut replacements = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            let mut request = String::new();
                            stream.read_to_string(&mut request).unwrap();
                            commands.lock().unwrap().push(request.clone());
                            if matches!(behavior, Behavior::Hang)
                                && request.starts_with("ui activate\n")
                            {
                                // Never EOF: only the external watchdog can bound this run.
                                hung.push(stream);
                                continue;
                            }
                            let replacing = matches!(behavior, Behavior::ReplaceSocket)
                                && request.starts_with("ui activate\n");
                            if replacing {
                                fs::remove_file(&socket_path).unwrap();
                                replacements.push(UnixListener::bind(&socket_path).unwrap());
                            }
                            if matches!(behavior, Behavior::ChangeSettings)
                                && request.starts_with("ui activate\n")
                            {
                                fs::write(&settings_path, "external preference change\n").unwrap();
                            }
                            let reply = if request == "status\n" {
                                "ok settings_save_pending=false\nautostart_session=manual"
                                    .to_owned()
                            } else if request == "ui state\n" || replacing {
                                format!(
                                    "ok {}",
                                    json!({
                                        "schema_version": 1, "revision": 1,
                                        "screen": "launcher.main", "selected_scenario": "clock",
                                        "active_scenario": null, "scenario_revision": null,
                                        "paused": false, "benchmark_active": false,
                                        "controls": [{"id": "launcher.sound", "label": "App Settings", "enabled": true}],
                                        "actions": []
                                    })
                                )
                            } else {
                                "error unexpected test request".into()
                            };
                            let _ = stream.write_all(reply.as_bytes());
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
            })
        };
        Self {
            directory,
            stop,
            worker: Some(worker),
            commands,
        }
    }

    fn run(&self, workflow: &str, timeout: &str) -> (std::process::Output, Value) {
        let path = self.directory.path();
        let output = Command::new(env!("CARGO_BIN_EXE_spacewars-cli"))
            .arg("--socket")
            .arg(path.join("control.sock"))
            .args(["functional", workflow, "--timeout", timeout])
            .arg("--settings-file")
            .arg(path.join("settings.toml"))
            .arg("--artifacts")
            .arg(path.join("report"))
            .output()
            .unwrap();
        let report =
            serde_json::from_slice(&fs::read(path.join("report/summary.json")).unwrap()).unwrap();
        (output, report)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn watchdog_terminates_hung_worker_and_cleanup_does_not_hide_failure() {
    let app = App::new(Behavior::Hang);
    let started = Instant::now();
    let (output, report) = app.run("settings", "1s");
    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(8));
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("whole-case deadline")
    );
    assert_eq!(report["success"], false);
    assert_eq!(report["cleanup"]["success"], true);
    assert_eq!(report["settings_unchanged"], true);
    assert_eq!(report["autostart_session_unchanged"], true);
    assert_eq!(
        fs::read_to_string(app.directory.path().join("settings.toml")).unwrap(),
        "untouched preferences\n"
    );
    assert!(
        app.commands
            .lock()
            .unwrap()
            .iter()
            .any(|command| command.starts_with("ui activate\n"))
    );
    assert!(
        app.directory
            .path()
            .join("report/command-history.jsonl")
            .exists()
    );
    let printed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(printed, report);
}

#[test]
fn unsupported_clock_precondition_is_reported_without_ui_actions() {
    let app = App::new(Behavior::Normal);
    let (output, report) = app.run("clock-pause", "5s");
    assert!(!output.status.success());
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("already running or paused Clock")
    );
    assert_eq!(report["cleanup"]["success"], true);
    assert!(
        app.commands
            .lock()
            .unwrap()
            .iter()
            .all(|command| command == "status\n"
                || command == "ui state\n"
                || command.starts_with("screenshot\n"))
    );
}

#[test]
fn existing_artifact_directory_is_never_reused() {
    let app = App::new(Behavior::Normal);
    let (first, _) = app.run("clock-pause", "5s");
    let count = app.commands.lock().unwrap().len();
    let summary = fs::read(app.directory.path().join("report/summary.json")).unwrap();
    let (second, _) = app.run("clock-pause", "5s");
    assert!(!first.status.success() && !second.status.success());
    assert_eq!(app.commands.lock().unwrap().len(), count);
    assert_eq!(
        fs::read(app.directory.path().join("report/summary.json")).unwrap(),
        summary
    );
}

#[test]
fn changed_socket_refuses_cleanup_on_the_replacement_application() {
    let app = App::new(Behavior::ReplaceSocket);
    let (output, report) = app.run("settings", "5s");
    assert!(!output.status.success());
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("Control socket changed")
    );
    assert_eq!(report["cleanup"]["success"], false);
    assert!(
        report["cleanup"]["error"]
            .as_str()
            .unwrap()
            .contains("Control socket changed")
    );
    assert_eq!(report["settings_unchanged"], true);
}

#[test]
fn settings_change_is_reported_and_never_overwritten_by_cleanup() {
    let app = App::new(Behavior::ChangeSettings);
    let (output, report) = app.run("settings", "5s");
    assert!(!output.status.success());
    assert_eq!(report["settings_unchanged"], false);
    assert_eq!(report["cleanup"]["success"], true);
    assert_eq!(
        fs::read_to_string(app.directory.path().join("settings.toml")).unwrap(),
        "external preference change\n"
    );
}

#[test]
fn missing_settings_path_produces_a_preflight_report_without_mutations() {
    let app = App::new(Behavior::Normal);
    fs::remove_file(app.directory.path().join("settings.toml")).unwrap();
    let (output, report) = app.run("settings", "5s");
    assert!(!output.status.success());
    assert_eq!(report["settings_unchanged"], Value::Null);
    assert!(report["initial_state"].is_null());
    assert!(
        app.commands
            .lock()
            .unwrap()
            .iter()
            .all(|command| command == "status\n" || command == "ui state\n")
    );
}
