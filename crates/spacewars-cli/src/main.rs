use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use engine_common::DEFAULT_CONTROL_SOCKET;
#[cfg(test)]
use spacewars_control::UI_STATE_SCHEMA_VERSION;
use spacewars_control::{UI_STATE_COMMAND, UiControl, UiState, parse_runtime_status};

#[derive(Debug, Parser)]
#[command(name = "spacewars-cli", about = "Space-Wars runtime control helper")]
struct Args {
    /// Engine-client Unix control socket.
    #[arg(long)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print diagnostics from the running UI.
    Status,

    /// Ask the running UI to write a PNG screenshot.
    Screenshot {
        /// Output path on the machine running engine-client.
        output: PathBuf,
    },

    /// Inspect or control the visible UI.
    Ui {
        #[command(subcommand)]
        command: UiCommand,
    },

    /// Control the lifecycle of the selected or active scenario.
    Host {
        #[command(subcommand)]
        command: HostCommand,
    },
}

#[derive(Debug, Subcommand)]
enum UiCommand {
    /// Print the current screen and scenario state.
    State {
        /// Emit the versioned state object as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum HostCommand {
    /// Start a new benchmark instance and wait until the host confirms it.
    Benchmark {
        /// Maximum time to wait for a new benchmark instance.
        #[arg(long, default_value = "3s", value_parser = parse_timeout)]
        timeout: Duration,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let socket = args
        .socket
        .or_else(|| std::env::var_os("SPACEWARS_CONTROL_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_SOCKET));

    match args.command {
        Command::Status => request_status(&socket)?,
        Command::Screenshot { output } => request_screenshot(&socket, output)?,
        Command::Ui {
            command: UiCommand::State { json },
        } => request_ui_state(&socket, json)?,
        Command::Host {
            command: HostCommand::Benchmark { timeout },
        } => request_benchmark(&socket, timeout)?,
    }

    Ok(())
}

#[cfg(unix)]
fn request_screenshot(socket: &Path, output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let output = output
        .to_str()
        .ok_or("screenshot output path must be valid UTF-8")?;
    if output.contains('\n') {
        return Err("screenshot output path must not contain newlines".into());
    }

    let message = send_request(socket, &format!("screenshot\n{output}\n"))?;
    println!("{message}");
    Ok(())
}

#[cfg(unix)]
fn request_status(socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let message = fetch_status(socket)?;
    println!("{message}");
    Ok(())
}

#[cfg(unix)]
fn request_ui_state(socket: &Path, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let state = UiState::from_json(&send_request(socket, &format!("{UI_STATE_COMMAND}\n"))?)?;
    if json {
        println!("{}", state.to_pretty_json()?);
    } else {
        println!("{}", format_ui_state(&state));
    }
    Ok(())
}

#[cfg(unix)]
fn request_benchmark(socket: &Path, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("benchmark timeout is too large")?;
    let initial_status = fetch_status_before(socket, deadline)?;
    let initial = parse_runtime_status(&initial_status)?;

    send_request_before(socket, "host benchmark\n", deadline)?;

    let mut last_status = initial_status;
    loop {
        let status_text = fetch_status_before(socket, deadline).map_err(|error| {
            format!(
                "failed while waiting up to {} for a new benchmark instance after scenario revision {}: {}; last status:\n{}",
                format_duration(timeout),
                format_scenario_revision(initial.scenario_revision),
                error,
                last_status,
            )
        })?;
        let status = parse_runtime_status(&status_text)?;
        if status.benchmark_active
            && status.scenario_revision.is_some()
            && status.scenario_revision != initial.scenario_revision
        {
            println!("benchmark started");
            println!("{status_text}");
            return Ok(());
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "timed out after {} waiting for a new benchmark instance after scenario revision {}; last status:\n{}",
                format_duration(timeout),
                format_scenario_revision(initial.scenario_revision),
                status_text,
            )
            .into());
        }
        last_status = status_text;
        std::thread::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(now)));
    }
}

#[cfg(unix)]
fn fetch_status(socket: &Path) -> Result<String, Box<dyn std::error::Error>> {
    send_request(socket, "status\n")
}

#[cfg(unix)]
fn fetch_status_before(
    socket: &Path,
    deadline: Instant,
) -> Result<String, Box<dyn std::error::Error>> {
    send_request_before(socket, "status\n", deadline)
}

#[cfg(unix)]
fn send_request_before(
    socket: &Path,
    request: &str,
    deadline: Instant,
) -> Result<String, Box<dyn std::error::Error>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("control request deadline elapsed".into());
    }
    send_request_with_timeout(socket, request, Some(remaining))
}

#[cfg(unix)]
fn send_request(socket: &Path, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    send_request_with_timeout(socket, request, None)
}

#[cfg(unix)]
fn send_request_with_timeout(
    socket: &Path,
    request: &str,
    timeout: Option<Duration>,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(timeout)?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    match parse_response(&response) {
        Ok(message) => Ok(message.to_string()),
        Err(message) => Err(message.into()),
    }
}

fn parse_response(response: &str) -> Result<&str, String> {
    let response = response.trim_end();
    if let Some(message) = response.strip_prefix("ok ") {
        Ok(message)
    } else if let Some(message) = response.strip_prefix("error ") {
        Err(message.to_string())
    } else {
        Err(format!(
            "unexpected response from engine-client: {response:?}"
        ))
    }
}

fn format_ui_state(state: &UiState) -> String {
    let mut output = format!(
        "schema_version={}\nrevision={}\nscreen={}\nactive_scenario={}\nselected_scenario={}\nselected_control={}\nscenario_revision={}\npaused={}\nbenchmark_active={}\nerror={}\ncontrols={}",
        state.schema_version,
        state.revision,
        state.screen,
        state.active_scenario.as_deref().unwrap_or("none"),
        state.selected_scenario,
        state.selected_control.as_deref().unwrap_or("none"),
        format_scenario_revision(state.scenario_revision),
        state.paused,
        state.benchmark_active,
        state.error.as_deref().unwrap_or("none"),
        state.controls.len(),
    );
    for control in &state.controls {
        output.push_str(&format_control(control));
    }
    output
}

fn format_control(control: &UiControl) -> String {
    let value = control
        .value
        .as_deref()
        .map(|value| format!(" value={value:?}"))
        .unwrap_or_default();
    format!(
        "\ncontrol={} label={:?} enabled={}{}",
        control.id, control.label, control.enabled, value
    )
}

fn format_scenario_revision(revision: Option<u64>) -> String {
    revision
        .map(|revision| revision.to_string())
        .unwrap_or_else(|| "none".into())
}

fn parse_timeout(value: &str) -> Result<Duration, String> {
    let duration = if let Some(milliseconds) = value.strip_suffix("ms") {
        Duration::from_millis(parse_timeout_number(milliseconds, value)?)
    } else if let Some(seconds) = value.strip_suffix('s') {
        Duration::from_secs(parse_timeout_number(seconds, value)?)
    } else {
        return Err("timeout must use an s or ms suffix, for example 3s or 500ms".into());
    };

    if duration.is_zero() {
        return Err("timeout must be greater than zero".into());
    }
    Ok(duration)
}

fn parse_timeout_number(number: &str, original: &str) -> Result<u64, String> {
    number
        .parse()
        .map_err(|_| format!("invalid timeout {original:?}"))
}

fn format_duration(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(not(unix))]
fn request_screenshot(_socket: &Path, _output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli screenshot requires Unix domain sockets".into())
}

#[cfg(not(unix))]
fn request_status(_socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli status requires Unix domain sockets".into())
}

#[cfg(not(unix))]
fn request_ui_state(_socket: &Path, _json: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli ui state requires Unix domain sockets".into())
}

#[cfg(not(unix))]
fn request_benchmark(_socket: &Path, _timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    Err("spacewars-cli host benchmark requires Unix domain sockets".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_subcommand() {
        let args = Args::try_parse_from(["spacewars-cli", "status"]).unwrap();

        assert!(matches!(args.command, Command::Status));
    }

    #[test]
    fn parses_host_benchmark_subcommand() {
        let args = Args::try_parse_from(["spacewars-cli", "host", "benchmark"]).unwrap();

        let Command::Host {
            command: HostCommand::Benchmark { timeout },
        } = args.command
        else {
            panic!("expected host benchmark command");
        };
        assert_eq!(timeout, Duration::from_secs(3));
    }

    #[test]
    fn parses_ui_state_json_subcommand() {
        let args = Args::try_parse_from(["spacewars-cli", "ui", "state", "--json"]).unwrap();

        assert!(matches!(
            args.command,
            Command::Ui {
                command: UiCommand::State { json: true }
            }
        ));
    }

    #[test]
    fn accepts_multiline_success_response() {
        assert_eq!(
            parse_response("ok scenario=pizza\nfps=59.8\nframes_total=123\n").unwrap(),
            "scenario=pizza\nfps=59.8\nframes_total=123"
        );
    }

    #[test]
    fn reports_engine_and_protocol_errors() {
        assert_eq!(
            parse_response("error screenshot failed\n").unwrap_err(),
            "screenshot failed"
        );
        assert_eq!(
            parse_response("").unwrap_err(),
            "unexpected response from engine-client: \"\""
        );
    }

    #[test]
    fn formats_human_readable_ui_state() {
        let state = UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 8,
            screen: spacewars_control::UiScreen::PauseMain,
            active_scenario: Some("pizza".into()),
            selected_scenario: "pizza".into(),
            selected_control: Some("pause.resume".into()),
            controls: vec![
                UiControl::new("pause.resume", "Resume", true),
                UiControl::new("pause.restart", "Restart Round", true),
            ],
            scenario_revision: Some(3),
            paused: true,
            benchmark_active: false,
            error: Some("Paused by controller".into()),
        };

        assert_eq!(
            format_ui_state(&state),
            "schema_version=1\nrevision=8\nscreen=pause.main\nactive_scenario=pizza\nselected_scenario=pizza\nselected_control=pause.resume\nscenario_revision=3\npaused=true\nbenchmark_active=false\nerror=Paused by controller\ncontrols=2\ncontrol=pause.resume label=\"Resume\" enabled=true\ncontrol=pause.restart label=\"Restart Round\" enabled=true"
        );
    }

    #[test]
    fn parses_bounded_timeouts() {
        assert_eq!(parse_timeout("3s").unwrap(), Duration::from_secs(3));
        assert_eq!(parse_timeout("750ms").unwrap(), Duration::from_millis(750));
        assert!(parse_timeout("0s").is_err());
        assert!(parse_timeout("3").is_err());
    }
}
