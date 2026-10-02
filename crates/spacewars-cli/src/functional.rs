//! Supervised entry point for the same workflows used by desktop tests.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, ValueEnum};
use spacewars_control::ControlClient;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum WorkflowArg {
    Settings,
    ClockPause,
}

#[derive(Debug, Args)]
pub(crate) struct Options {
    #[arg(value_enum)]
    workflow: WorkflowArg,
    /// New directory for JSON state, command history, screenshots and logs.
    #[arg(long)]
    artifacts: PathBuf,
    /// Existing app settings file to verify byte-for-byte (never rewritten).
    #[arg(long)]
    settings_file: PathBuf,
    /// Hard workflow deadline; cleanup has a separate 10-second budget.
    #[arg(long, default_value = "60s", value_parser = timeout)]
    timeout: Duration,
}

#[derive(Debug, Args)]
pub(crate) struct WorkerOptions {
    #[command(flatten)]
    options: Options,
    #[arg(long)]
    cleanup: bool,
}

fn timeout(value: &str) -> Result<Duration, String> {
    let duration = crate::parse_timeout(value)?;
    if duration < Duration::from_millis(100) || duration > Duration::from_secs(300) {
        return Err("Workflow timeout must be between 100ms and 300s".into());
    }
    Ok(duration)
}

#[cfg(not(unix))]
pub(crate) fn run(_: &ControlClient, _: Options) -> Result<(), String> {
    Err("Functional workflows require a Unix control socket".into())
}
#[cfg(not(unix))]
pub(crate) fn worker(_: &ControlClient, _: WorkerOptions) -> Result<(), String> {
    Err("Functional workflows require a Unix control socket".into())
}

#[cfg(unix)]
pub(crate) use unix::{run, worker};

#[cfg(unix)]
mod unix {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::DirBuilderExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::Instant;

    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use spacewars_control::workflows::{self, Observation, Session, Workflow};

    const CLEANUP_BUDGET: Duration = Duration::from_secs(10);

    impl WorkflowArg {
        fn name(self) -> &'static str {
            match self {
                Self::Settings => "settings",
                Self::ClockPause => "clock-pause",
            }
        }
        fn workflow(self) -> Workflow {
            match self {
                Self::Settings => Workflow::Settings,
                Self::ClockPause => Workflow::ClockPause,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    struct Baseline {
        session: Session,
        settings_sha256: String,
        autostart_session: String,
    }

    #[derive(Serialize, Deserialize)]
    struct Outcome {
        success: bool,
        error: Option<String>,
        final_state: Option<spacewars_control::UiState>,
    }

    pub(crate) fn run(client: &ControlClient, options: Options) -> Result<(), String> {
        let started = Instant::now();
        // create_dir, not create_dir_all: never mix evidence from separate runs.
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&options.artifacts)
            .map_err(message)?;
        let directory = fs::canonicalize(&options.artifacts).map_err(message)?;
        let execute = supervise_worker(client, &options, &directory, false, options.timeout);
        let outcome: Option<Outcome> = read_json(&directory.join("workflow-result.json")).ok();
        let baseline: Option<Baseline> = read_json(&directory.join("baseline.json")).ok();
        let error = execute.err().or_else(|| match &outcome {
            Some(outcome) if outcome.success => None,
            Some(outcome) => Some(outcome.error.clone().unwrap_or("Workflow failed".into())),
            None => Some("Worker did not produce a workflow result".into()),
        });
        // Cleanup runs in its own supervised child even after a hung/failed worker.
        // Absence of a baseline means preflight failed before any UI action.
        let cleanup = if baseline.is_some() {
            let status = supervise_worker(client, &options, &directory, true, CLEANUP_BUDGET);
            match status {
                Ok(()) => read_json::<Outcome>(&directory.join("cleanup-result.json"))
                    .unwrap_or_else(|error| Outcome {
                        success: false,
                        error: Some(error),
                        final_state: None,
                    }),
                Err(error) => Outcome {
                    success: false,
                    error: Some(error),
                    final_state: None,
                },
            }
        } else {
            Outcome {
                success: false,
                error: Some("Preflight did not establish a recovery anchor".into()),
                final_state: None,
            }
        };
        let settings_after = settings_digest(&options.settings_file);
        let settings_unchanged = baseline
            .as_ref()
            .map(|b| settings_after.as_ref() == Ok(&b.settings_sha256));
        let after = fs::read_to_string(directory.join("status-after.txt")).ok();
        let autostart_unchanged = baseline.as_ref().map(|b| {
            after
                .as_deref()
                .and_then(|s| status_field(s, "autostart_session"))
                == Some(b.autostart_session.as_str())
        });
        let logs = collect_logs(&directory);
        let info: Option<spacewars_control::UiState> =
            read_json(&directory.join("device-info.json")).ok();
        let identity = |id: &str| {
            info.as_ref()
                .and_then(|state| state.controls.iter().find(|c| c.id == id))
                .and_then(|c| c.value.as_deref())
        };
        let success = error.is_none()
            && cleanup.success
            && settings_unchanged == Some(true)
            && autostart_unchanged == Some(true);
        let report = json!({
            "schema_version": 1,
            "runner_version": env!("CARGO_PKG_VERSION"),
            "app_version": identity("info.version"),
            "device_hostname": identity("info.hostname"),
            "workflow": options.workflow.name(),
            "success": success,
            "error": error,
            "duration_ms": started.elapsed().as_millis(),
            "workflow_timeout_ms": options.timeout.as_millis(),
            "cleanup_timeout_ms": CLEANUP_BUDGET.as_millis(),
            "socket": client.socket_path(),
            "settings_file": options.settings_file,
            "settings_unchanged": settings_unchanged,
            "settings_error": settings_after.err(),
            "autostart_session_unchanged": autostart_unchanged,
            "initial_state": baseline.as_ref().map(|b| &b.session.initial),
            "cleanup": cleanup,
            "kiosk_logs": logs,
            "service_recovery": "not-attempted",
            "artifacts": directory,
        });
        write_json(&directory.join("summary.json"), &report)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(message)?
        );
        if success {
            Ok(())
        } else {
            Err(format!(
                "Workflow failed; see {}",
                directory.join("summary.json").display()
            ))
        }
    }

    fn supervise_worker(
        client: &ControlClient,
        options: &Options,
        directory: &Path,
        cleanup: bool,
        budget: Duration,
    ) -> Result<(), String> {
        let phase = if cleanup { "cleanup" } else { "workflow" };
        let log = File::create(directory.join(format!("{phase}-worker.log"))).map_err(message)?;
        let mut command = Command::new(std::env::current_exe().map_err(message)?);
        command
            .arg("--socket")
            .arg(client.socket_path())
            .arg("functional-worker")
            .arg(options.workflow.name())
            .arg("--artifacts")
            .arg(directory)
            .arg("--settings-file")
            .arg(&options.settings_file)
            .arg("--timeout")
            .arg(format!("{}ms", budget.as_millis()))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().map_err(message)?))
            .stderr(Stdio::from(log));
        if cleanup {
            command.arg("--cleanup");
        }
        supervise(command, budget).map_err(|error| format!("{phase}: {error}"))
    }

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    fn supervise(mut command: Command, budget: Duration) -> Result<(), String> {
        let deadline = Instant::now() + budget;
        let mut child = OwnedChild(command.spawn().map_err(message)?);
        loop {
            if let Some(status) = child.0.try_wait().map_err(message)? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("worker exited with {status}"))
                };
            }
            if Instant::now() >= deadline {
                return Err("whole-case deadline elapsed; worker terminated".into());
            }
            thread::sleep(
                Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    pub(crate) fn worker(client: &ControlClient, worker: WorkerOptions) -> Result<(), String> {
        let options = worker.options;
        let deadline = Instant::now() + options.timeout;
        let directory = &options.artifacts;
        let mut history = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("command-history.jsonl"))
            .map_err(message)?;
        let started = Instant::now();
        let phase = if worker.cleanup {
            "cleanup"
        } else {
            "workflow"
        };
        let mut capture_failure = worker.cleanup
            && read_json::<Outcome>(&directory.join("workflow-result.json"))
                .map_or(true, |outcome| !outcome.success);
        let observe = |observation: Observation, limit: Instant| {
            let result = (|| {
                writeln!(history, "{}", json!({"phase": phase, "elapsed_ms": started.elapsed().as_millis(), "observation": observation})).map_err(message)?;
                if let Some(state) = &observation.state {
                    write_json(&directory.join("last-state.json"), state)?;
                    if capture_failure {
                        capture_failure = false;
                        write_json(&directory.join("failure-state.json"), state)?;
                        if let Err(error) =
                            screenshot(client, &directory.join("failure.png"), limit)
                        {
                            eprintln!("Failure screenshot warning: {error}");
                        }
                    }
                }
                if let Some(name) = &observation.checkpoint {
                    write_json(&directory.join(format!("{name}.json")), &observation.state)?;
                    screenshot(client, &directory.join(format!("{name}.png")), limit)?;
                }
                Ok::<_, String>(())
            })();
            // Artifact/storage failure must not prevent menu cleanup.
            if worker.cleanup {
                if let Err(error) = result {
                    eprintln!("Cleanup artifact warning: {error}");
                }
                Ok(())
            } else {
                result
            }
        };
        let result = if worker.cleanup {
            let baseline: Baseline = read_json(&directory.join("baseline.json"))?;
            workflows::restore(client, &baseline.session, deadline, observe).and_then(|state| {
                let status = client
                    .request_before("status\n", deadline)
                    .map_err(message)?;
                fs::write(directory.join("status-after.txt"), status).map_err(message)?;
                Ok(state)
            })
        } else {
            prepare(client, &options, deadline).and_then(|baseline| {
                write_json(&directory.join("baseline.json"), &baseline)?;
                workflows::run(
                    client,
                    &baseline.session,
                    options.workflow.workflow(),
                    deadline,
                    observe,
                )
            })
        };
        let outcome = match result {
            Ok(state) => Outcome {
                success: true,
                error: None,
                final_state: Some(state),
            },
            Err(error) => Outcome {
                success: false,
                error: Some(error),
                final_state: None,
            },
        };
        write_json(&directory.join(format!("{phase}-result.json")), &outcome)?;
        // The supervisor reads this result even on a workflow failure.
        Ok(())
    }

    fn prepare(
        client: &ControlClient,
        options: &Options,
        deadline: Instant,
    ) -> Result<Baseline, String> {
        let until = deadline.min(Instant::now() + Duration::from_secs(3));
        let status = loop {
            let status = client.request_before("status\n", until).map_err(message)?;
            fs::write(options.artifacts.join("status-before.txt"), &status).map_err(message)?;
            if status_field(&status, "settings_save_pending") == Some("false")
                && matches!(
                    status_field(&status, "autostart_session"),
                    Some("manual" | "automatic")
                )
            {
                break status;
            }
            if Instant::now() >= until {
                return Err("Timed out waiting for settled settings and automatic/manual session diagnostics".into());
            }
            thread::sleep(
                Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())),
            );
        };
        let autostart_session = status_field(&status, "autostart_session")
            .ok_or("App does not expose automatic/manual session diagnostics")?
            .to_owned();
        let session = Session::capture(
            client,
            deadline.min(Instant::now() + Duration::from_secs(3)),
        )?;
        let settings_sha256 = settings_digest(&options.settings_file)?;
        Ok(Baseline {
            session,
            settings_sha256,
            autostart_session,
        })
    }

    fn settings_digest(path: &Path) -> Result<String, String> {
        let metadata = fs::metadata(path).map_err(message)?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err("Settings must be an existing regular file under 1 MiB".into());
        }
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(message)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(message)?;
        if bytes.len() > 1024 * 1024 {
            return Err("Settings exceed 1 MiB".into());
        }
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    fn screenshot(client: &ControlClient, path: &Path, deadline: Instant) -> Result<(), String> {
        let path_text = path.to_str().ok_or("Screenshot path is not UTF-8")?;
        if path_text.contains(['\n', '\r']) {
            return Err("Screenshot path contains a newline".into());
        }
        client
            .request_before(&format!("screenshot\n{path_text}\n"), deadline)
            .map_err(message)?;
        let file = File::open(path).map_err(message)?;
        let mut reader = png::Decoder::new(file).read_info().map_err(message)?;
        if reader.output_buffer_size() > 64 * 1024 * 1024 {
            return Err("Screenshot exceeds the 64 MiB decoded limit".into());
        }
        let mut bytes = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut bytes).map_err(message)?;
        if frame.width == 0
            || frame.height == 0
            || frame.color_type != png::ColorType::Rgba
            || frame.bit_depth != png::BitDepth::Eight
        {
            return Err("Screenshot is not a nonempty 8-bit RGBA image".into());
        }
        let bytes = &bytes[..frame.buffer_size()];
        if bytes.chunks_exact(4).any(|p| p[3] != 255)
            || !bytes.chunks_exact(4).any(|p| p[..3] != bytes[..3])
        {
            return Err("Screenshot is transparent or blank".into());
        }
        Ok(())
    }

    fn collect_logs(directory: &Path) -> Value {
        let helper = Path::new("/usr/sbin/spacewars-logs");
        if !helper.is_file() {
            return json!({"status": "unavailable", "reason": "Kiosk log helper is not installed"});
        }
        let result = (|| {
            let log = File::create(directory.join("kiosk.log")).map_err(message)?;
            let mut command = Command::new("/usr/bin/sudo");
            command
                .args(["-n", "/usr/sbin/spacewars-logs", "--lines", "120"])
                .stdin(Stdio::null())
                .stderr(Stdio::from(log.try_clone().map_err(message)?))
                .stdout(Stdio::from(log));
            supervise(command, Duration::from_secs(3))
        })();
        match result {
            Ok(()) => json!({"status": "captured", "path": "kiosk.log"}),
            Err(error) => json!({"status": "failed", "error": error}),
        }
    }

    fn status_field<'a>(status: &'a str, field: &str) -> Option<&'a str> {
        status.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == field).then_some(value)
        })
    }

    fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
        serde_json::from_slice(&fs::read(path).map_err(message)?).map_err(message)
    }

    fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
        let temporary = path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(value).map_err(message)?,
        )
        .map_err(message)?;
        fs::rename(temporary, path).map_err(message)
    }

    fn message(error: impl std::fmt::Display) -> String {
        error.to_string()
    }
}
