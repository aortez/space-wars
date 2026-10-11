#[cfg(test)]
use std::path::Path;
mod clock;
mod input;
mod logs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use engine_common::DEFAULT_CONTROL_SOCKET;
#[cfg(test)]
use spacewars_control::UI_STATE_SCHEMA_VERSION;
use spacewars_control::{
    ControlClient, ControlClientError, ControlFailure, HostPauseRequest, UiAction,
    UiActivateRequest, UiControl, UiPressRequest, UiScreen, UiState, UiStatePredicate,
    parse_runtime_status,
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
    /// Simulate bounded, named controller input through the running app.
    Input {
        #[command(subcommand)]
        command: input::InputCommand,
    },
    /// Read the Pi kiosk's journal, even when the application is not running.
    Logs(logs::LogOptions),

    /// Inspect, trigger, or synchronize with Clock animations.
    Clock {
        #[command(subcommand)]
        command: clock::ClockCommand,
    },
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

    /// Activate one visible control by its stable ID.
    Activate {
        /// Stable control ID reported by `ui state`.
        control_id: String,

        /// Reject the activation unless this screen is currently visible.
        #[arg(long, value_enum)]
        expect_screen: Option<UiScreenArg>,

        /// Reject the activation unless this exact UI revision is current.
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
    #[value(name = "launcher.busy")]
    LauncherBusy,
    #[value(name = "launcher.main")]
    LauncherMain,
    #[value(name = "launcher.sound")]
    LauncherSound,
    #[value(name = "launcher.info")]
    LauncherInfo,
    #[value(name = "launcher.autostart")]
    LauncherAutostart,
    #[value(name = "launcher.controllers")]
    LauncherControllers,
    #[value(name = "launcher.network")]
    LauncherNetwork,
    #[value(name = "launcher.settings")]
    LauncherSettings,
    #[value(name = "launcher.clock-fonts")]
    LauncherClockFonts,
    #[value(name = "launcher.controls")]
    LauncherControls,
    #[value(name = "launcher.touch-test")]
    LauncherTouchTest,
    #[value(name = "gameplay")]
    Gameplay,
    #[value(name = "pause.main")]
    PauseMain,
    #[value(name = "pause.sound")]
    PauseSound,
    #[value(name = "pause.info")]
    PauseInfo,
    #[value(name = "pause.autostart")]
    PauseAutostart,
    #[value(name = "pause.controllers")]
    PauseControllers,
    #[value(name = "pause.network")]
    PauseNetwork,
    #[value(name = "pause.controls")]
    PauseControls,
    #[value(name = "pause.clock")]
    PauseClock,
    #[value(name = "pause.clock-fonts")]
    PauseClockFonts,
    #[value(name = "game-over")]
    GameOver,
}

impl From<UiScreenArg> for UiScreen {
    fn from(screen: UiScreenArg) -> Self {
        match screen {
            UiScreenArg::LauncherBusy => Self::LauncherBusy,
            UiScreenArg::LauncherMain => Self::LauncherMain,
            UiScreenArg::LauncherSound => Self::LauncherSound,
            UiScreenArg::LauncherInfo => Self::LauncherInfo,
            UiScreenArg::LauncherAutostart => Self::LauncherAutostart,
            UiScreenArg::LauncherControllers => Self::LauncherControllers,
            UiScreenArg::LauncherNetwork => Self::LauncherNetwork,
            UiScreenArg::LauncherSettings => Self::LauncherSettings,
            UiScreenArg::LauncherClockFonts => Self::LauncherClockFonts,
            UiScreenArg::LauncherControls => Self::LauncherControls,
            UiScreenArg::LauncherTouchTest => Self::LauncherTouchTest,
            UiScreenArg::Gameplay => Self::Gameplay,
            UiScreenArg::PauseMain => Self::PauseMain,
            UiScreenArg::PauseSound => Self::PauseSound,
            UiScreenArg::PauseInfo => Self::PauseInfo,
            UiScreenArg::PauseAutostart => Self::PauseAutostart,
            UiScreenArg::PauseControllers => Self::PauseControllers,
            UiScreenArg::PauseNetwork => Self::PauseNetwork,
            UiScreenArg::PauseControls => Self::PauseControls,
            UiScreenArg::PauseClock => Self::PauseClock,
            UiScreenArg::PauseClockFonts => Self::PauseClockFonts,
            UiScreenArg::GameOver => Self::GameOver,
        }
    }
}

#[derive(Debug, Subcommand)]
enum HostCommand {
    /// Pause active gameplay and wait for the pause menu.
    Pause {
        /// Reject the request unless this exact UI revision is current.
        #[arg(long)]
        expect_revision: Option<u64>,

        /// Maximum time for the pause menu to become visible.
        #[arg(long, default_value = "3s", value_parser = parse_timeout)]
        timeout: Duration,

        /// Emit success or structured control failures as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Start a new benchmark instance and wait until the host confirms it.
    Benchmark {
        /// Maximum time to wait for a new benchmark instance.
        #[arg(long, default_value = "3s", value_parser = parse_timeout)]
        timeout: Duration,
    },

    /// Experimental raster RGB/RGBA publication for live GPU A/B tests; not saved.
    /// Requires a GPU-enabled kiosk currently presenting a raster scene with FemtoVG.
    RasterUpload {
        #[arg(value_parser = ["rgb", "rgba"])]
        format: String,
    },

    /// Session-only GPU texture-lifetime experiment; select RGBA upload first.
    RasterTextures {
        #[arg(value_parser = ["slint", "fresh", "single", "triple"])]
        mode: String,
    },

    /// Session-only GPU scenes at 1x, including split views/minimaps. Paths is Clock-only.
    GpuScene {
        #[arg(value_parser = ["on", "off", "triangles", "circles", "retained", "retained-cold", "retained-adaptive", "retained-zoom", "retained-hybrid", "retained-outlines", "retained-paced", "retained-memoized", "retained-scan", "retained-ranges", "retained-culled", "retained-resident", "retained-resident-passes", "retained-resident-buffers", "retained-resident-uniforms", "paths"])]
        mode: String,
    },

    /// Session-only triangle upload policy; enable a non-resident GPU scene first.
    /// Does not change geometry mode or frame pacing. Resets when leaving the scene.
    GpuStream {
        #[arg(value_parser = ["shared", "exact", "bucketed"])]
        policy: String,
    },

    /// Session-only guarded stroke rejection; non-off modes require retained-culled.
    /// Checked verifies skipped geometry, not performance. Nothing is saved.
    GpuStrokes {
        #[arg(value_parser = ["off", "conservative", "checked"])]
        mode: String,
    },

    /// Session-only exact round-join shortcut; exact/checked require retained-culled.
    /// Checked verifies the original math, not performance. Nothing is saved.
    GpuJoins {
        #[arg(value_parser = ["reference", "exact", "checked"])]
        mode: String,
    },

    /// Session-only stroke output experiment; non-stock requires retained-culled.
    /// Emits the same geometry and resets when leaving the scene. Nothing is saved.
    GpuStrokeWriter {
        #[arg(value_parser = ["stock", "bulk", "bulk-static", "bulk-polygon", "bulk-edges", "bulk-edges-checked", "bulk-prepared", "bulk-prepared-checked", "bulk-single-pass", "bulk-single-pass-checked", "bulk-single-pass-layouts", "bulk-single-pass-arcs", "bulk-single-pass-classify", "bulk-single-pass-prefix", "bulk-single-pass-prefix-checked"])]
        mode: String,
    },

    /// Session-only exact-scale cache promotion experiment; settled requires retained-culled.
    /// Settled requires three consecutive presentations; nothing is saved.
    GpuAdmission {
        #[arg(value_parser = ["repeated", "settled"])]
        policy: String,
    },

    /// Session-only CPU plan preparation experiment; requires an active GPU scene.
    /// Prepared adds per-camera/object transforms to bounded scratch; nothing is saved.
    GpuPlan {
        #[arg(value_parser = ["reference", "reserved", "scratch", "prepared"])]
        policy: String,
    },
    /// Session-only source point-buffer allocation experiment; no scene contents are cached.
    GpuSource {
        #[arg(value_parser = ["reference", "pooled"])]
        policy: String,
    },

    /// Select vector or CPU-generated GPU glyph masks; reset the session cache.
    GpuText {
        #[arg(value_parser = ["vector", "cpu", "cpu-run-faces", "cpu-shape-faces"])]
        mode: String,
    },
    /// Session-only Spacewars camera sweep (44..640 world units, 480-tick cycle).
    /// Shared by software, raster-to-GPU and direct geometry; not physics/settings.
    /// sweep-stop holds both heights after tick 1920 to measure settled-view reuse.
    #[command(visible_alias = "gpu-zoom")]
    CameraZoom {
        #[arg(value_parser = ["off", "sweep", "sweep-stop"])]
        mode: String,
    },

    /// Capture 240 complete LinuxKMS GPU loops with opt-in driver CPU clocks.
    /// start-text also times nested text and path work; start is the overhead control.
    /// start-work adds coarse host and scene callbacks to start-text.
    /// start-geometry also enables detailed retained-geometry encoding clocks.
    /// Read returns CSV with UI markers; cancel retains completed samples. Not saved.
    RenderTrace {
        #[arg(value_parser = ["start", "start-text", "start-work", "start-geometry", "read", "cancel"])]
        command: String,
    },

    /// Run the existing deterministic Clock fixture visibly, without changing saved settings.
    /// Requires an active Clock; restarts the fixture at 08:08. Normal restart exits it.
    ClockBenchmark {
        #[arg(value_parser = ["idle", "rain"])]
        fixture: String,
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
        Command::Input { command } => input::run(&client, command),
        Command::Logs(options) => logs::run(&options).map_err(human_error),
        Command::Clock { command } => clock::run(&client, command),
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
                UiCommand::Activate {
                    control_id,
                    expect_screen,
                    expect_revision,
                    json,
                },
        } => request_ui_activate(
            &client,
            control_id,
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
            command:
                HostCommand::Pause {
                    expect_revision,
                    timeout,
                    json,
                },
        } => request_host_pause(&client, expect_revision, timeout, json)
            .map_err(|error| control_error(error, json)),
        Command::Host {
            command: HostCommand::Benchmark { timeout },
        } => request_benchmark(&client, timeout).map_err(human_error),
        Command::Host {
            command: HostCommand::RasterUpload { format },
        } => {
            let reply = client
                .request_before(
                    &format!("host raster-upload\n{format}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::RasterTextures { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host raster-textures\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuScene { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-scene\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuStream { policy },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-stream\n{policy}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuStrokes { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-strokes\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuJoins { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-joins\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuText { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-text\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuStrokeWriter { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-stroke-writer\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuAdmission { policy },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-admission\n{policy}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuPlan { policy },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-plan\n{policy}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::GpuSource { policy },
        } => {
            let reply = client
                .request_before(
                    &format!("host gpu-source\n{policy}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::CameraZoom { mode },
        } => {
            let reply = client
                .request_before(
                    &format!("host camera-zoom\n{mode}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::RenderTrace { command },
        } => {
            let reply = client
                .request_before(
                    &format!("host render-trace\n{command}\n"),
                    Instant::now() + Duration::from_secs(3),
                )
                .map_err(human_error)?;
            println!("{reply}");
            Ok(())
        }
        Command::Host {
            command: HostCommand::ClockBenchmark { fixture, timeout },
        } => request_benchmark_command(
            &client,
            timeout,
            &format!("host clock-benchmark\n{fixture}\n"),
        )
        .map_err(human_error),
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

fn request_ui_activate(
    client: &ControlClient,
    control_id: String,
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
    json: bool,
) -> Result<(), ControlClientError> {
    let mut request = UiActivateRequest::new(control_id);
    request.expected_screen = expected_screen;
    request.expected_revision = expected_revision;
    print_ui_state(&client.ui_activate(&request)?, json)
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

fn request_host_pause(
    client: &ControlClient,
    expected_revision: Option<u64>,
    timeout: Duration,
    json: bool,
) -> Result<(), ControlClientError> {
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        ControlClientError::Failure(Box::new(ControlFailure::new(
            spacewars_control::ControlFailureCode::Timeout,
            "host pause timeout is too large",
            None,
        )))
    })?;
    let observed = client.ui_state_before(deadline)?;
    let mut request = HostPauseRequest::new();
    request.expected_screen = Some(UiScreen::Gameplay);
    request.expected_revision = Some(expected_revision.unwrap_or(observed.revision));
    let accepted = client.host_pause_before(&request, deadline)?;
    let predicate = UiStatePredicate {
        screen: Some(UiScreen::PauseMain),
        scenario: accepted.active_scenario.clone(),
        revision_after: Some(accepted.revision),
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    print_ui_state(&client.wait_for_ui_state(&predicate, remaining)?, json)
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
    request_benchmark_command(client, timeout, "host benchmark\n")
}

fn request_benchmark_command(
    client: &ControlClient,
    timeout: Duration,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("benchmark timeout is too large")?;
    let initial_status = fetch_status_before(client, deadline)?;
    let initial = parse_runtime_status(&initial_status)?;

    client.request_before(command, deadline)?;

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
    fn parses_host_pause_subcommand() {
        let args = Args::try_parse_from([
            "spacewars-cli",
            "host",
            "pause",
            "--expect-revision",
            "12",
            "--timeout",
            "750ms",
            "--json",
        ])
        .unwrap();

        assert!(matches!(
            args.command,
            Command::Host {
                command: HostCommand::Pause {
                    expect_revision: Some(12),
                    timeout,
                    json: true,
                }
            } if timeout == Duration::from_millis(750)
        ));
    }

    #[test]
    fn parses_only_explicit_clock_benchmark_fixtures() {
        for expected in ["idle", "rain"] {
            let args = Args::try_parse_from([
                "spacewars-cli",
                "host",
                "clock-benchmark",
                expected,
                "--timeout",
                "10s",
            ])
            .unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::ClockBenchmark { fixture, timeout } }
                    if fixture == expected && timeout == Duration::from_secs(10)
            ));
        }
        for invalid in ["falling", "RAIN", "idle\nstatus", ""] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "clock-benchmark", invalid])
                    .is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "clock-benchmark"]).is_err());
    }

    #[test]
    fn parses_only_explicit_raster_upload_formats() {
        for expected in ["rgb", "rgba"] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "raster-upload", expected]).unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::RasterUpload { format } } if format == expected
            ));
        }
        for invalid in ["RGB", "bgra", "rgba\nstatus", ""] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "raster-upload", invalid]).is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "raster-upload"]).is_err());
    }

    #[test]
    fn parses_only_explicit_texture_lifetime_modes() {
        for expected in ["slint", "fresh", "single", "triple"] {
            let args = Args::try_parse_from(["spacewars-cli", "host", "raster-textures", expected])
                .unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::RasterTextures { mode } } if mode == expected)
            );
        }
        for invalid in ["", "rgba", "single\nstatus", "TRIPLE", "double"] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "raster-textures", invalid])
                    .is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "raster-textures"]).is_err());
    }

    #[test]
    fn render_trace_accepts_only_explicit_bounded_commands() {
        for command in [
            "start",
            "start-text",
            "start-work",
            "start-geometry",
            "read",
            "cancel",
        ] {
            let args = Args::try_parse_from(["cli", "host", "render-trace", command]).unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::RenderTrace { command: parsed } } if parsed == command)
            );
        }
        for command in ["", "on", "forever", "start\nstatus"] {
            assert!(Args::try_parse_from(["cli", "host", "render-trace", command]).is_err());
        }
        assert!(Args::try_parse_from(["cli", "host", "render-trace"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_text_policies() {
        for expected in ["vector", "cpu", "cpu-run-faces", "cpu-shape-faces"] {
            let args = Args::try_parse_from(["cli", "host", "gpu-text", expected]).unwrap();
            assert!(matches!(args.command,
                Command::Host {command: HostCommand::GpuText { mode }} if mode == expected));
        }
        for invalid in ["on", "", "cpu\nstatus"] {
            assert!(Args::try_parse_from(["cli", "host", "gpu-text", invalid]).is_err());
        }
        assert!(Args::try_parse_from(["cli", "host", "gpu-text"]).is_err());
    }

    #[test]
    fn parses_explicit_camera_zoom_modes_and_legacy_alias() {
        for command in ["camera-zoom", "gpu-zoom"] {
            for expected in ["off", "sweep", "sweep-stop"] {
                let args =
                    Args::try_parse_from(["spacewars-cli", "host", command, expected]).unwrap();
                assert!(matches!(args.command,
                    Command::Host {command: HostCommand::CameraZoom { mode }} if mode == expected));
            }
            for invalid in ["on", "auto", "", "sweep\nstatus"] {
                assert!(Args::try_parse_from(["spacewars-cli", "host", command, invalid]).is_err());
            }
            assert!(Args::try_parse_from(["spacewars-cli", "host", command]).is_err());
        }
    }

    #[test]
    fn parses_only_explicit_gpu_plan_policies() {
        for policy in ["reference", "reserved", "scratch", "prepared"] {
            let args = Args::try_parse_from(["spacewars-cli", "host", "gpu-plan", policy]).unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::GpuPlan { policy: actual } } if actual == policy)
            );
        }
        for invalid in ["", "on", "auto", "reserved\nstatus"] {
            assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-plan", invalid]).is_err());
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-plan"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_source_policies() {
        for policy in ["reference", "pooled"] {
            let args = Args::try_parse_from(["cli", "host", "gpu-source", policy]).unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::GpuSource { policy: actual }} if actual == policy)
            );
        }
        for value in ["", "on", "pooled\nstatus"] {
            assert!(Args::try_parse_from(["cli", "host", "gpu-source", value]).is_err());
        }
        assert!(Args::try_parse_from(["cli", "host", "gpu-source"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_stream_policies() {
        for expected in ["shared", "exact", "bucketed"] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-stream", expected]).unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::GpuStream { policy } } if policy == expected)
            );
        }
        for invalid in ["on", "off", "auto", "split", "bucketed\nstatus"] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "gpu-stream", invalid]).is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-stream"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_stroke_modes() {
        for expected in ["off", "conservative", "checked"] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-strokes", expected]).unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::GpuStrokes { mode } } if mode == expected));
        }
        for invalid in ["on", "auto", "fast", "conservative\nstatus"] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "gpu-strokes", invalid]).is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-strokes"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_join_modes() {
        for expected in ["reference", "exact", "checked"] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-joins", expected]).unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::GpuJoins { mode } } if mode == expected));
        }
        for invalid in ["on", "off", "auto", "fast", "exact\nstatus"] {
            assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-joins", invalid]).is_err());
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-joins"]).is_err());
    }

    #[test]
    fn parses_only_explicit_admission_modes() {
        for expected in ["repeated", "settled"] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-admission", expected]).unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::GpuAdmission { policy } } if policy == expected));
        }
        for invalid in ["on", "off", "auto", "settled\nstatus"] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "gpu-admission", invalid]).is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-admission"]).is_err());
    }

    #[test]
    fn parses_only_explicit_stroke_writer_modes() {
        for expected in [
            "stock",
            "bulk",
            "bulk-static",
            "bulk-polygon",
            "bulk-edges",
            "bulk-edges-checked",
            "bulk-prepared",
            "bulk-prepared-checked",
            "bulk-single-pass",
            "bulk-single-pass-checked",
            "bulk-single-pass-layouts",
            "bulk-single-pass-arcs",
            "bulk-single-pass-classify",
            "bulk-single-pass-prefix",
            "bulk-single-pass-prefix-checked",
        ] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-stroke-writer", expected])
                    .unwrap();
            assert!(matches!(args.command,
                Command::Host { command: HostCommand::GpuStrokeWriter { mode } } if mode == expected));
        }
        for invalid in [
            "on",
            "off",
            "auto",
            "fast",
            "bulk\nstatus",
            "bulk-single-pass-prefix\nstatus",
        ] {
            assert!(
                Args::try_parse_from(["spacewars-cli", "host", "gpu-stroke-writer", invalid])
                    .is_err()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-stroke-writer"]).is_err());
    }

    #[test]
    fn parses_only_explicit_gpu_scene_modes() {
        for expected in [
            "on",
            "off",
            "triangles",
            "circles",
            "retained",
            "retained-cold",
            "retained-adaptive",
            "retained-zoom",
            "retained-hybrid",
            "retained-outlines",
            "retained-paced",
            "retained-memoized",
            "retained-scan",
            "retained-ranges",
            "retained-culled",
            "retained-resident",
            "retained-resident-passes",
            "retained-resident-buffers",
            "retained-resident-uniforms",
            "paths",
        ] {
            let args =
                Args::try_parse_from(["spacewars-cli", "host", "gpu-scene", expected]).unwrap();
            assert!(
                matches!(args.command, Command::Host { command: HostCommand::GpuScene { mode } } if mode == expected)
            );
        }
        for invalid in ["", "yes", "true", "ON", "on\nstatus"] {
            assert!(Args::try_parse_from(["spacewars-cli", "host", "gpu-scene", invalid]).is_err());
        }
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
    fn parses_ui_activate_with_preconditions() {
        let args = Args::try_parse_from([
            "spacewars-cli",
            "ui",
            "activate",
            "launcher.settings",
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
                command: UiCommand::Activate {
                    control_id,
                    expect_screen: Some(UiScreenArg::LauncherMain),
                    expect_revision: Some(12),
                    json: true,
                }
            } if control_id == "launcher.settings"
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
    fn clock_commands_parse_guards_and_require_a_known_wait_phase() {
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "state", "--json"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "events", "--json"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "falling"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "meltdown"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "duck"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "crow"]).is_ok());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "explosion"]).is_ok());
        for phase in ["warning", "exploding", "reforming"] {
            assert!(
                Args::try_parse_from([
                    "spacewars-cli",
                    "clock",
                    "wait",
                    "--event",
                    "explosion",
                    "--phase",
                    phase
                ])
                .is_ok()
            );
        }
        for phase in ["entering", "perched", "hopping", "flying", "leaving"] {
            assert!(
                Args::try_parse_from([
                    "spacewars-cli",
                    "clock",
                    "wait",
                    "--event",
                    "crow",
                    "--phase",
                    phase
                ])
                .is_ok()
            );
        }
        for phase in ["opening", "running", "exiting", "resetting"] {
            assert!(
                Args::try_parse_from([
                    "spacewars-cli",
                    "clock",
                    "wait",
                    "--event",
                    "duck",
                    "--phase",
                    phase
                ])
                .is_ok()
            );
        }
        for phase in ["melting", "draining", "reforming"] {
            assert!(
                Args::try_parse_from([
                    "spacewars-cli",
                    "clock",
                    "wait",
                    "--event",
                    "meltdown",
                    "--phase",
                    phase
                ])
                .is_ok()
            );
        }
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger", "unknown"]).is_err());
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "trigger"]).is_err());
        assert!(
            Args::try_parse_from(["spacewars-cli", "clock", "wait", "--lifecycle", "idle"]).is_ok()
        );
        assert!(
            Args::try_parse_from([
                "spacewars-cli",
                "clock",
                "wait",
                "--event",
                "color-cycle",
                "--phase",
                "cycling"
            ])
            .is_ok()
        );
        assert!(
            Args::try_parse_from([
                "spacewars-cli",
                "clock",
                "trigger",
                "color-cycle",
                "--expect-scenario-revision",
                "3",
                "--expect-event-id",
                "2",
                "--timeout",
                "2s",
                "--json"
            ])
            .is_ok()
        );
        assert!(
            Args::try_parse_from([
                "spacewars-cli",
                "clock",
                "wait",
                "--phase",
                "reforming",
                "--event-id",
                "3",
                "--min-phase-tick",
                "30"
            ])
            .is_ok()
        );
        assert!(Args::try_parse_from(["spacewars-cli", "clock", "wait"]).is_err());
        assert!(
            Args::try_parse_from(["spacewars-cli", "clock", "wait", "--phase", "unknown"]).is_err()
        );
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
