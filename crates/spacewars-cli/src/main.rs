#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use engine_common::DEFAULT_CONTROL_SOCKET;
#[cfg(test)]
use spacewars_control::UI_STATE_SCHEMA_VERSION;
use spacewars_control::{
    ControlClient, ControlClientError, ControlFailure, UiAction, UiControl, UiPressRequest,
    UiScreen, UiState, UiStatePredicate, parse_runtime_status,
};

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
    /// Print the current screen, accepted actions, and visible controls.
    State {
        /// Emit the versioned state object as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Send one menu action and print the resulting UI state.
    Press {
        /// Action to route through the keyboard/gamepad menu handler.
        #[arg(value_enum)]
        action: UiActionArg,

        /// Reject the action unless this screen is currently visible.
        #[arg(long, value_enum)]
        expect_screen: Option<UiScreenArg>,

        /// Reject the action unless this exact UI revision is current.
        #[arg(long)]
        expect_revision: Option<u64>,

        /// Emit success or structured control failures as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Poll until all supplied UI-state conditions match.
    ///
    /// At least one of --screen, --scenario, or --revision-after is required.
    Wait {
        /// Wait for this screen to be visible.
        #[arg(long, value_enum)]
        screen: Option<UiScreenArg>,

        /// Wait for this active scenario, or the selected launcher scenario.
        #[arg(long)]
        scenario: Option<String>,

        /// Wait for the UI revision to become greater than this value.
        #[arg(long)]
        revision_after: Option<u64>,

        /// Maximum time to wait.
        #[arg(long, default_value = "10s", value_parser = parse_timeout)]
        timeout: Duration,

        /// Emit success or structured timeout failures as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum UiActionArg {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Start,
    Controls,
}

impl From<UiActionArg> for UiAction {
    fn from(action: UiActionArg) -> Self {
        match action {
            UiActionArg::Up => Self::Up,
            UiActionArg::Down => Self::Down,
            UiActionArg::Left => Self::Left,
            UiActionArg::Right => Self::Right,
            UiActionArg::Confirm => Self::Confirm,
            UiActionArg::Back => Self::Back,
            UiActionArg::Start => Self::Start,
            UiActionArg::Controls => Self::Controls,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum UiScreenArg {
    #[value(name = "launcher.main")]
    LauncherMain,
    #[value(name = "launcher.settings")]
    LauncherSettings,
    #[value(name = "launcher.controls")]
    LauncherControls,
    #[value(name = "launcher.touch-test")]
    LauncherTouchTest,
    #[value(name = "gameplay")]
    Gameplay,
    #[value(name = "pause.main")]
    PauseMain,
    #[value(name = "pause.controls")]
    PauseControls,
    #[value(name = "game-over")]
    GameOver,
}

impl From<UiScreenArg> for UiScreen {
    fn from(screen: UiScreenArg) -> Self {
        match screen {
            UiScreenArg::LauncherMain => Self::LauncherMain,
            UiScreenArg::LauncherSettings => Self::LauncherSettings,
            UiScreenArg::LauncherControls => Self::LauncherControls,
            UiScreenArg::LauncherTouchTest => Self::LauncherTouchTest,
            UiScreenArg::Gameplay => Self::Gameplay,
            UiScreenArg::PauseMain => Self::PauseMain,
            UiScreenArg::PauseControls => Self::PauseControls,
            UiScreenArg::GameOver => Self::GameOver,
        }
    }
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

enum CliError {
    Human(String),
    Json(Box<ControlFailure>),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Human(error)) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
        Err(CliError::Json(failure)) => {
            match failure.to_pretty_json() {
                Ok(json) => eprintln!("{json}"),
                Err(error) => eprintln!("Error: {error}"),
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CliError> {
    let args = Args::parse();
    let socket = args
        .socket
        .or_else(|| std::env::var_os("SPACEWARS_CONTROL_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_SOCKET));
    let client = ControlClient::new(socket);

    match args.command {
        Command::Status => request_status(&client).map_err(human_error),
        Command::Screenshot { output } => request_screenshot(&client, output).map_err(human_error),
        Command::Ui {
            command: UiCommand::State { json },
        } => request_ui_state(&client, json).map_err(human_error),
        Command::Ui {
            command:
                UiCommand::Press {
                    action,
                    expect_screen,
                    expect_revision,
                    json,
                },
        } => request_ui_press(
            &client,
            action.into(),
            expect_screen.map(Into::into),
            expect_revision,
            json,
        )
        .map_err(|error| control_error(error, json)),
        Command::Ui {
            command:
                UiCommand::Wait {
                    screen,
                    scenario,
                    revision_after,
                    timeout,
                    json,
                },
        } => request_ui_wait(
            &client,
            UiStatePredicate {
                screen: screen.map(Into::into),
                scenario,
                revision_after,
            },
            timeout,
            json,
        )
        .map_err(|error| control_error(error, json)),
        Command::Host {
            command: HostCommand::Benchmark { timeout },
        } => request_benchmark(&client, timeout).map_err(human_error),
    }
}

fn human_error(error: impl std::fmt::Display) -> CliError {
    CliError::Human(error.to_string())
}

fn control_error(error: ControlClientError, json: bool) -> CliError {
    match (json, error) {
        (true, ControlClientError::Failure(failure)) => CliError::Json(failure),
        (_, error) => human_error(error),
    }
}

fn request_screenshot(
    client: &ControlClient,
    output: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = output
        .to_str()
        .ok_or("screenshot output path must be valid UTF-8")?;
    if output.contains('\n') {
        return Err("screenshot output path must not contain newlines".into());
    }

    let message = client.request(&format!("screenshot\n{output}\n"))?;
    println!("{message}");
    Ok(())
}

fn request_status(client: &ControlClient) -> Result<(), ControlClientError> {
    let message = fetch_status(client)?;
    println!("{message}");
    Ok(())
}

fn request_ui_state(client: &ControlClient, json: bool) -> Result<(), ControlClientError> {
    print_ui_state(&client.ui_state()?, json)
}

fn request_ui_press(
    client: &ControlClient,
    action: UiAction,
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
    json: bool,
) -> Result<(), ControlClientError> {
    let mut request = UiPressRequest::new(action);
    request.expected_screen = expected_screen;
    request.expected_revision = expected_revision;
    print_ui_state(&client.ui_press(&request)?, json)
}

fn request_ui_wait(
    client: &ControlClient,
    predicate: UiStatePredicate,
    timeout: Duration,
    json: bool,
) -> Result<(), ControlClientError> {
    if predicate.is_empty() {
        return Err(ControlClientError::Failure(Box::new(ControlFailure::new(
            spacewars_control::ControlFailureCode::InvalidRequest,
            "ui wait requires --screen, --scenario, or --revision-after",
            None,
        ))));
    }
    print_ui_state(&client.wait_for_ui_state(&predicate, timeout)?, json)
}

fn print_ui_state(state: &UiState, json: bool) -> Result<(), ControlClientError> {
    if json {
        println!("{}", state.to_pretty_json()?);
    } else {
        println!("{}", format_ui_state(state));
    }
    Ok(())
}

fn request_benchmark(
    client: &ControlClient,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("benchmark timeout is too large")?;
    let initial_status = fetch_status_before(client, deadline)?;
    let initial = parse_runtime_status(&initial_status)?;

    client.request_before("host benchmark\n", deadline)?;

    let mut last_status = initial_status;
    loop {
        let status_text = fetch_status_before(client, deadline).map_err(|error| {
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

fn fetch_status(client: &ControlClient) -> Result<String, ControlClientError> {
    client.request("status\n")
}

fn fetch_status_before(
    client: &ControlClient,
    deadline: Instant,
) -> Result<String, ControlClientError> {
    client.request_before("status\n", deadline)
}

fn format_ui_state(state: &UiState) -> String {
    let actions = if state.actions.is_empty() {
        "none".into()
    } else {
        state
            .actions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut output = format!(
        "schema_version={}\nrevision={}\nscreen={}\nactive_scenario={}\nselected_scenario={}\nselected_control={}\nscenario_revision={}\npaused={}\nbenchmark_active={}\nerror={}\nactions={}\ncontrols={}",
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
        actions,
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
    fn parses_ui_press_with_preconditions() {
        let args = Args::try_parse_from([
            "spacewars-cli",
            "ui",
            "press",
            "confirm",
            "--expect-screen",
            "launcher.main",
            "--expect-revision",
            "12",
            "--json",
        ])
        .unwrap();

        assert!(matches!(
            args.command,
            Command::Ui {
                command: UiCommand::Press {
                    action: UiActionArg::Confirm,
                    expect_screen: Some(UiScreenArg::LauncherMain),
                    expect_revision: Some(12),
                    json: true,
                }
            }
        ));
    }

    #[test]
    fn press_help_lists_all_actions() {
        let error = Args::try_parse_from(["spacewars-cli", "ui", "press", "unknown"])
            .unwrap_err()
            .to_string();

        for action in UiAction::ALL {
            assert!(
                error.contains(action.as_str()),
                "missing {action} in {error}"
            );
        }
    }

    #[test]
    fn parses_combined_ui_wait_predicate() {
        let args = Args::try_parse_from([
            "spacewars-cli",
            "ui",
            "wait",
            "--screen",
            "gameplay",
            "--scenario",
            "pizza",
            "--revision-after",
            "8",
            "--timeout",
            "750ms",
        ])
        .unwrap();

        assert!(matches!(
            args.command,
            Command::Ui {
                command: UiCommand::Wait {
                    screen: Some(UiScreenArg::Gameplay),
                    scenario: Some(scenario),
                    revision_after: Some(8),
                    timeout,
                    json: false,
                }
            } if scenario == "pizza" && timeout == Duration::from_millis(750)
        ));
    }

    #[test]
    fn formats_human_readable_ui_state() {
        let state = UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 8,
            screen: UiScreen::PauseMain,
            active_scenario: Some("pizza".into()),
            selected_scenario: "pizza".into(),
            selected_control: Some("pause.resume".into()),
            controls: vec![
                UiControl::new("pause.resume", "Resume", true),
                UiControl::new("pause.restart", "Restart Round", true),
            ],
            actions: UiAction::ALL.to_vec(),
            scenario_revision: Some(3),
            paused: true,
            benchmark_active: false,
            error: Some("Paused by controller".into()),
        };

        assert_eq!(
            format_ui_state(&state),
            "schema_version=1\nrevision=8\nscreen=pause.main\nactive_scenario=pizza\nselected_scenario=pizza\nselected_control=pause.resume\nscenario_revision=3\npaused=true\nbenchmark_active=false\nerror=Paused by controller\nactions=up,down,left,right,confirm,back,start,controls\ncontrols=2\ncontrol=pause.resume label=\"Resume\" enabled=true\ncontrol=pause.restart label=\"Restart Round\" enabled=true"
        );
    }

    #[test]
    fn parses_bounded_timeouts() {
        assert_eq!(parse_timeout("3s").unwrap(), Duration::from_secs(3));
        assert_eq!(parse_timeout("750ms").unwrap(), Duration::from_millis(750));
        assert!(parse_timeout("0s").is_err());
        assert!(parse_timeout("3").is_err());
    }

    #[test]
    fn empty_wait_predicate_is_rejected_before_polling() {
        let error = request_ui_wait(
            &ControlClient::new(Path::new("/unused")),
            UiStatePredicate::default(),
            Duration::from_secs(1),
            false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("requires --screen"));
    }
}
