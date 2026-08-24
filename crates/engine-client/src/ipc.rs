#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

use engine_common::DEFAULT_CONTROL_SOCKET;
use slint::Timer;
#[cfg(unix)]
use slint::{ComponentHandle, Rgba8Pixel, SharedPixelBuffer, TimerMode};
#[cfg(unix)]
use spacewars_control::{
    ControlFailure, ControlFailureCode, ProtocolError, RuntimeStatus, UI_PRESS_COMMAND,
    UI_STATE_COMMAND, UI_STATE_SCHEMA_VERSION, UiAction, UiControl, UiPressRequest, UiScreen,
    UiState, parse_runtime_status,
};

use crate::MainWindow;
#[cfg(unix)]
use crate::ui_inventory::{
    ScreenVisibility, UiInventoryContext, classify_screen, inventory_for_screen,
};

#[cfg(unix)]
#[derive(Debug)]
struct ControlRequest {
    command: ControlCommand,
    response: ResponseWriter,
}

#[cfg(unix)]
#[derive(Debug)]
enum ControlCommand {
    Screenshot { output: PathBuf },
    Status,
    UiState,
    UiPress(UiPressRequest),
    HostBenchmark,
}

#[cfg(unix)]
#[derive(Debug)]
enum CommandParseError {
    Legacy(String),
    Structured(Box<ControlFailure>),
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct UiStateObservation {
    screen: UiScreen,
    active_scenario: Option<String>,
    selected_scenario: String,
    selected_control: Option<String>,
    controls: Vec<UiControl>,
    actions: Vec<UiAction>,
    scenario_revision: Option<u64>,
    paused: bool,
    benchmark_active: bool,
    error: Option<String>,
}

#[cfg(unix)]
#[derive(Debug, Default)]
struct UiStateTracker {
    revision: u64,
    last_observation: Option<UiStateObservation>,
}

#[cfg(unix)]
impl UiStateTracker {
    fn observe(&mut self, observation: UiStateObservation) -> UiState {
        if self.last_observation.as_ref() != Some(&observation) {
            self.revision = self.revision.saturating_add(1);
            self.last_observation = Some(observation.clone());
        }

        UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: self.revision,
            screen: observation.screen,
            active_scenario: observation.active_scenario,
            selected_scenario: observation.selected_scenario,
            selected_control: observation.selected_control,
            controls: observation.controls,
            actions: observation.actions,
            scenario_revision: observation.scenario_revision,
            paused: observation.paused,
            benchmark_active: observation.benchmark_active,
            error: observation.error,
        }
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct ResponseWriter {
    stream: std::os::unix::net::UnixStream,
}

#[cfg(unix)]
impl ResponseWriter {
    fn ok(mut self, message: impl AsRef<str>) {
        let _ = writeln!(self.stream, "ok {}", message.as_ref());
    }

    fn error(mut self, message: impl AsRef<str>) {
        let _ = writeln!(self.stream, "error {}", message.as_ref());
    }

    fn control_failure(self, failure: ControlFailure) {
        match failure.to_json() {
            Ok(json) => self.error(json),
            Err(error) => self.error(error.to_string()),
        }
    }
}

pub fn control_socket_path() -> PathBuf {
    std::env::var_os("SPACEWARS_CONTROL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_SOCKET))
}

#[cfg(unix)]
pub fn start_control_server(window: &MainWindow, socket_path: PathBuf) -> Option<Timer> {
    let (tx, rx) = mpsc::channel();
    if let Err(err) = spawn_listener(socket_path.clone(), tx) {
        tracing::warn!(
            path = %socket_path.display(),
            error = %err,
            "failed to start control socket."
        );
        return None;
    }

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let mut ui_state_tracker = UiStateTracker::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        while let Ok(request) = rx.try_recv() {
            handle_request(&window, request, &mut ui_state_tracker);
        }
    });

    Some(timer)
}

#[cfg(not(unix))]
pub fn start_control_server(_window: &MainWindow, _socket_path: PathBuf) -> Option<Timer> {
    tracing::info!("control socket is unavailable on this platform.");
    None
}

#[cfg(unix)]
fn spawn_listener(
    socket_path: PathBuf,
    tx: mpsc::Sender<ControlRequest>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    tracing::info!(path = %socket_path.display(), "control socket listening.");

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_stream(stream, &tx),
                Err(err) => tracing::warn!(error = %err, "control socket accept failed."),
            }
        }
    });

    Ok(())
}

#[cfg(unix)]
fn handle_stream(mut stream: std::os::unix::net::UnixStream, tx: &mpsc::Sender<ControlRequest>) {
    let mut body = String::new();
    if let Err(err) = stream.read_to_string(&mut body) {
        let _ = writeln!(stream, "error failed to read command: {err}");
        return;
    }

    let response = ResponseWriter { stream };
    let command = match parse_command(&body) {
        Ok(command) => command,
        Err(CommandParseError::Legacy(message)) => {
            response.error(message);
            return;
        }
        Err(CommandParseError::Structured(failure)) => {
            response.control_failure(*failure);
            return;
        }
    };

    if let Err(err) = tx.send(ControlRequest { command, response }) {
        err.0
            .response
            .error("control socket request dropped because UI loop is unavailable");
        tracing::warn!("control socket request dropped because UI loop is unavailable.");
    }
}

#[cfg(unix)]
fn parse_command(body: &str) -> Result<ControlCommand, CommandParseError> {
    let mut lines = body.lines();
    match lines.next() {
        Some("screenshot") => {
            let Some(output) = lines.next() else {
                return Err(CommandParseError::Legacy(
                    "missing screenshot output path".into(),
                ));
            };
            if output.is_empty() {
                return Err(CommandParseError::Legacy(
                    "screenshot output path must not be empty".into(),
                ));
            }
            if lines.next().is_some() {
                return Err(CommandParseError::Legacy("too many command lines".into()));
            }
            Ok(ControlCommand::Screenshot {
                output: PathBuf::from(output),
            })
        }
        Some("status") => {
            if lines.next().is_some() {
                return Err(CommandParseError::Legacy("too many command lines".into()));
            }
            Ok(ControlCommand::Status)
        }
        Some(UI_STATE_COMMAND) => {
            if lines.next().is_some() {
                return Err(CommandParseError::Legacy("too many command lines".into()));
            }
            Ok(ControlCommand::UiState)
        }
        Some(UI_PRESS_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_press_request("ui press requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_press_request(
                    "ui press accepts exactly one JSON request line",
                ));
            }
            UiPressRequest::from_json(payload)
                .map(ControlCommand::UiPress)
                .map_err(|error| invalid_press_request(error.to_string()))
        }
        Some("host benchmark") => {
            if lines.next().is_some() {
                return Err(CommandParseError::Legacy("too many command lines".into()));
            }
            Ok(ControlCommand::HostBenchmark)
        }
        Some(command) => Err(CommandParseError::Legacy(format!(
            "unknown command {command:?}"
        ))),
        None => Err(CommandParseError::Legacy("empty command".into())),
    }
}

#[cfg(unix)]
fn invalid_press_request(message: impl Into<String>) -> CommandParseError {
    CommandParseError::Structured(Box::new(ControlFailure::new(
        ControlFailureCode::InvalidRequest,
        message,
        None,
    )))
}

#[cfg(unix)]
fn handle_request(
    window: &MainWindow,
    request: ControlRequest,
    ui_state_tracker: &mut UiStateTracker,
) {
    match request.command {
        ControlCommand::Screenshot { output } => match write_window_screenshot(window, &output) {
            Ok(()) => request
                .response
                .ok(format!("screenshot saved to {}", output.display())),
            Err(err) => request.response.error(err.to_string()),
        },
        ControlCommand::Status => request.response.ok(window.get_runtime_diagnostics()),
        ControlCommand::UiState => {
            match ui_state(window, ui_state_tracker).and_then(|state| state.to_json()) {
                Ok(json) => request.response.ok(json),
                Err(error) => request.response.error(error.to_string()),
            }
        }
        ControlCommand::UiPress(press) => {
            handle_ui_press(window, press, request.response, ui_state_tracker);
        }
        ControlCommand::HostBenchmark => {
            if !window.get_scenario_benchmark_available() {
                request
                    .response
                    .error("the selected scenario does not support benchmark mode");
            } else if window.get_launcher_visible() {
                window.invoke_launcher_start_benchmark();
                if window.get_launcher_visible() {
                    let detail = window.get_launcher_error_text();
                    if detail.is_empty() {
                        request
                            .response
                            .error("benchmark did not leave the launcher");
                    } else {
                        request.response.error(detail);
                    }
                } else {
                    request.response.ok("benchmark requested");
                }
            } else {
                window.invoke_ingame_start_benchmark();
                request.response.ok("benchmark requested");
            }
        }
    }
}

#[cfg(unix)]
fn handle_ui_press(
    window: &MainWindow,
    request: UiPressRequest,
    response: ResponseWriter,
    ui_state_tracker: &mut UiStateTracker,
) {
    let state = match ui_state(window, ui_state_tracker) {
        Ok(state) => state,
        Err(error) => {
            response.error(error.to_string());
            return;
        }
    };

    if let Err(failure) = validate_ui_press(&request, &state) {
        response.control_failure(*failure);
        return;
    }

    window.invoke_ui_action(request.action.code());
    match ui_state(window, ui_state_tracker).and_then(|state| state.to_json()) {
        Ok(json) => response.ok(json),
        Err(error) => response.error(error.to_string()),
    }
}

#[cfg(unix)]
fn validate_ui_press(request: &UiPressRequest, state: &UiState) -> Result<(), Box<ControlFailure>> {
    if let Some(expected) = request.expected_screen
        && state.screen != expected
    {
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::WrongScreen,
            format!(
                "expected screen {expected}, but current screen is {}; inspect `spacewars-cli ui state` and retry",
                state.screen
            ),
            Some(state.clone()),
        )));
    }

    if let Some(expected) = request.expected_revision
        && state.revision != expected
    {
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::StaleRevision,
            format!(
                "expected UI revision {expected}, but current revision is {}; inspect `spacewars-cli ui state` and retry",
                state.revision
            ),
            Some(state.clone()),
        )));
    }

    if !state.actions.contains(&request.action) {
        let available = if state.actions.is_empty() {
            "none".into()
        } else {
            state
                .actions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::ActionUnavailable,
            format!(
                "action {} is unavailable on {}; available actions: {available}",
                request.action, state.screen
            ),
            Some(state.clone()),
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn ui_state(window: &MainWindow, tracker: &mut UiStateTracker) -> Result<UiState, ProtocolError> {
    let screen = classify_screen(ScreenVisibility {
        launcher: window.get_launcher_visible(),
        launcher_controls: window.get_launcher_controls_visible(),
        launcher_settings: window.get_launcher_settings_visible(),
        touch_test: window.get_touch_test_visible(),
        ingame_menu: window.get_ingame_menu_visible(),
        ingame_controls: window.get_ingame_controls_visible(),
        game_over: window.get_game_over_visible(),
    });
    let runtime = runtime_status_for_screen(screen, window.get_runtime_diagnostics().as_str())?;
    let selected_scenario = window.get_launcher_scenario().to_string();
    let inventory = inventory_for_screen(
        screen,
        &UiInventoryContext {
            selected_scenario: selected_scenario.clone(),
            launcher_focus_index: window.get_launcher_focus_index(),
            launcher_settings_focus_index: window.get_launcher_settings_focus_index(),
            launcher_controls_focus_index: window.get_launcher_controls_focus_index(),
            ingame_menu_focus_index: window.get_ingame_menu_focus_index(),
            game_over_focus_index: window.get_game_over_focus_index(),
            benchmark_available: window.get_scenario_benchmark_available(),
            launch_available: selected_scenario != "nes" || window.get_launcher_nes_rom_supported(),
            launcher_error: non_empty(window.get_launcher_error_text().as_str()),
            scenario_error: non_empty(window.get_scenario_error_text().as_str()),
            renderer: window.get_launcher_renderer().to_string(),
            raster_scale: window.get_launcher_raster_scale_text().to_string(),
            spacewars_preset: window.get_launcher_spacewars_preset().to_string(),
            spacewars_planets: window.get_launcher_use_planets().to_string(),
            spacewars_asteroids: window.get_launcher_asteroids_enabled().to_string(),
            spacewars_player_health: window.get_launcher_player_health_text().to_string(),
            spacewars_player_2: window.get_launcher_p2_controller().to_string(),
            pizza_desired_balls: window.get_launcher_pizza_desired_balls_text().to_string(),
            pizza_spawn_rate: window.get_launcher_pizza_spawn_rate_text().to_string(),
            nes_cartridge_name: window.get_launcher_nes_rom_name().to_string(),
        },
    );

    Ok(tracker.observe(UiStateObservation {
        screen,
        active_scenario: runtime.active_scenario,
        selected_scenario,
        selected_control: inventory.selected_control,
        controls: inventory.controls,
        actions: inventory.actions,
        scenario_revision: runtime.scenario_revision,
        paused: runtime.paused,
        benchmark_active: runtime.benchmark_active,
        error: inventory.error,
    }))
}

#[cfg(unix)]
fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.into())
}

#[cfg(unix)]
fn runtime_status_for_screen(
    screen: UiScreen,
    diagnostics: &str,
) -> Result<RuntimeStatus, ProtocolError> {
    if screen.is_launcher() {
        Ok(RuntimeStatus::inactive())
    } else {
        parse_runtime_status(diagnostics)
    }
}

#[cfg(unix)]
fn write_window_screenshot(
    window: &MainWindow,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let snapshot = window.window().take_snapshot()?;
    write_rgba_png(output, &snapshot)?;
    Ok(())
}

#[cfg(unix)]
fn write_rgba_png(
    output: &Path,
    pixels: &SharedPixelBuffer<Rgba8Pixel>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::create(output)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, pixels.width(), pixels.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels.as_bytes())?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parse_screenshot_command() {
        let command = parse_command("screenshot\n/tmp/shot.png\n").unwrap();
        match command {
            ControlCommand::Screenshot { output } => {
                assert_eq!(output, PathBuf::from("/tmp/shot.png"));
            }
            ControlCommand::Status
            | ControlCommand::UiState
            | ControlCommand::UiPress(_)
            | ControlCommand::HostBenchmark => {
                panic!("expected screenshot command")
            }
        }
    }

    #[test]
    fn reject_extra_lines() {
        assert!(parse_command("screenshot\n/tmp/shot.png\nextra\n").is_err());
    }

    #[test]
    fn parse_status_command() {
        assert!(matches!(
            parse_command("status\n"),
            Ok(ControlCommand::Status)
        ));
        assert!(parse_command("status\nextra\n").is_err());
    }

    #[test]
    fn parse_ui_state_command() {
        assert!(matches!(
            parse_command("ui state\n"),
            Ok(ControlCommand::UiState)
        ));
        assert!(parse_command("ui state\nextra\n").is_err());
    }

    #[test]
    fn parse_ui_press_command_and_reject_invalid_payloads_structurally() {
        let request = UiPressRequest {
            schema_version: UI_STATE_SCHEMA_VERSION,
            action: UiAction::Confirm,
            expected_screen: Some(UiScreen::LauncherMain),
            expected_revision: Some(12),
        };
        let body = format!("ui press\n{}\n", request.to_json().unwrap());
        assert!(matches!(
            parse_command(&body),
            Ok(ControlCommand::UiPress(parsed)) if parsed == request
        ));

        for body in [
            "ui press\n",
            "ui press\nnot-json\n",
            "ui press\n{}\nextra\n",
        ] {
            assert!(matches!(
                parse_command(body),
                Err(CommandParseError::Structured(failure))
                    if failure.code == ControlFailureCode::InvalidRequest
            ));
        }
    }

    #[test]
    fn parse_host_benchmark_command() {
        assert!(matches!(
            parse_command("host benchmark\n"),
            Ok(ControlCommand::HostBenchmark)
        ));
        assert!(parse_command("host benchmark\nextra\n").is_err());
    }

    #[test]
    fn ui_revision_changes_only_with_observed_state() {
        let observation = UiStateObservation {
            screen: UiScreen::Gameplay,
            active_scenario: Some("pizza".into()),
            selected_scenario: "pizza".into(),
            selected_control: None,
            controls: Vec::new(),
            actions: Vec::new(),
            scenario_revision: Some(7),
            paused: false,
            benchmark_active: false,
            error: None,
        };
        let mut tracker = UiStateTracker::default();

        let first = tracker.observe(observation.clone());
        let unchanged = tracker.observe(observation.clone());
        let mut changed_observation = observation;
        changed_observation.paused = true;
        let changed = tracker.observe(changed_observation);

        assert_eq!(first.revision, 1);
        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(changed.revision, first.revision + 1);
    }

    #[test]
    fn ui_revision_changes_with_inventory_state() {
        let mut observation = UiStateObservation {
            screen: UiScreen::LauncherMain,
            active_scenario: None,
            selected_scenario: "pizza".into(),
            selected_control: Some("launcher.start".into()),
            controls: vec![UiControl::new("launcher.start", "Start Game", true)],
            actions: vec![UiAction::Confirm],
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
            error: None,
        };
        let mut tracker = UiStateTracker::default();
        let first = tracker.observe(observation.clone());

        observation.controls[0].value = Some("ready".into());
        let value_changed = tracker.observe(observation.clone());
        observation.error = Some("Could not start".into());
        let error_changed = tracker.observe(observation);

        assert_eq!(value_changed.revision, first.revision + 1);
        assert_eq!(error_changed.revision, value_changed.revision + 1);
    }

    #[test]
    fn ui_press_validation_checks_screen_revision_and_available_action() {
        let state = UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 7,
            screen: UiScreen::LauncherMain,
            active_scenario: None,
            selected_scenario: "spacewars".into(),
            selected_control: Some("launcher.start".into()),
            controls: vec![UiControl::new("launcher.start", "Start Game", true)],
            actions: vec![UiAction::Confirm, UiAction::Start],
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
            error: None,
        };
        let mut request = UiPressRequest::new(UiAction::Confirm);
        request.expected_screen = Some(UiScreen::LauncherSettings);
        request.expected_revision = Some(6);
        assert_eq!(
            validate_ui_press(&request, &state).unwrap_err().code,
            ControlFailureCode::WrongScreen
        );

        request.expected_screen = Some(UiScreen::LauncherMain);
        assert_eq!(
            validate_ui_press(&request, &state).unwrap_err().code,
            ControlFailureCode::StaleRevision
        );

        request.expected_revision = Some(7);
        request.action = UiAction::Back;
        let unavailable = validate_ui_press(&request, &state).unwrap_err();
        assert_eq!(unavailable.code, ControlFailureCode::ActionUnavailable);
        assert_eq!(unavailable.current_state, Some(state.clone()));

        request.action = UiAction::Start;
        assert_eq!(validate_ui_press(&request, &state), Ok(()));
    }

    #[test]
    fn launcher_state_ignores_stale_runtime_diagnostics() {
        let stale = "scenario=pizza\nscenario_revision=7\npaused=false\nbenchmark_active=true";

        assert_eq!(
            runtime_status_for_screen(UiScreen::LauncherMain, stale).unwrap(),
            RuntimeStatus::inactive()
        );
        assert_eq!(
            runtime_status_for_screen(UiScreen::Gameplay, stale)
                .unwrap()
                .active_scenario
                .as_deref(),
            Some("pizza")
        );
    }
}
