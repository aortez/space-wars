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
    ProtocolError, RuntimeStatus, UI_STATE_COMMAND, UI_STATE_SCHEMA_VERSION, UiScreen, UiState,
    parse_runtime_status,
};

use crate::MainWindow;

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
    HostBenchmark,
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ScreenVisibility {
    launcher: bool,
    launcher_controls: bool,
    launcher_settings: bool,
    touch_test: bool,
    ingame_menu: bool,
    ingame_controls: bool,
    game_over: bool,
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct UiStateObservation {
    screen: UiScreen,
    active_scenario: Option<String>,
    selected_scenario: String,
    scenario_revision: Option<u64>,
    paused: bool,
    benchmark_active: bool,
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
            scenario_revision: observation.scenario_revision,
            paused: observation.paused,
            benchmark_active: observation.benchmark_active,
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
        Err(message) => {
            response.error(message);
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
fn parse_command(body: &str) -> Result<ControlCommand, String> {
    let mut lines = body.lines();
    match lines.next() {
        Some("screenshot") => {
            let Some(output) = lines.next() else {
                return Err("missing screenshot output path".into());
            };
            if output.is_empty() {
                return Err("screenshot output path must not be empty".into());
            }
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::Screenshot {
                output: PathBuf::from(output),
            })
        }
        Some("status") => {
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::Status)
        }
        Some(UI_STATE_COMMAND) => {
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::UiState)
        }
        Some("host benchmark") => {
            if lines.next().is_some() {
                return Err("too many command lines".into());
            }
            Ok(ControlCommand::HostBenchmark)
        }
        Some(command) => Err(format!("unknown command {command:?}")),
        None => Err("empty command".into()),
    }
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

    Ok(tracker.observe(UiStateObservation {
        screen,
        active_scenario: runtime.active_scenario,
        selected_scenario: window.get_launcher_scenario().into(),
        scenario_revision: runtime.scenario_revision,
        paused: runtime.paused,
        benchmark_active: runtime.benchmark_active,
    }))
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
fn classify_screen(visibility: ScreenVisibility) -> UiScreen {
    if visibility.touch_test {
        UiScreen::LauncherTouchTest
    } else if visibility.launcher {
        if visibility.launcher_controls {
            UiScreen::LauncherControls
        } else if visibility.launcher_settings {
            UiScreen::LauncherSettings
        } else {
            UiScreen::LauncherMain
        }
    } else if visibility.game_over {
        UiScreen::GameOver
    } else if visibility.ingame_controls {
        UiScreen::PauseControls
    } else if visibility.ingame_menu {
        UiScreen::PauseMain
    } else {
        UiScreen::Gameplay
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
            ControlCommand::Status | ControlCommand::UiState | ControlCommand::HostBenchmark => {
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
    fn parse_host_benchmark_command() {
        assert!(matches!(
            parse_command("host benchmark\n"),
            Ok(ControlCommand::HostBenchmark)
        ));
        assert!(parse_command("host benchmark\nextra\n").is_err());
    }

    #[test]
    fn classifies_all_ui_screens() {
        let cases = [
            (
                ScreenVisibility {
                    launcher: true,
                    ..Default::default()
                },
                UiScreen::LauncherMain,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    launcher_settings: true,
                    ..Default::default()
                },
                UiScreen::LauncherSettings,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    launcher_controls: true,
                    ..Default::default()
                },
                UiScreen::LauncherControls,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    touch_test: true,
                    ..Default::default()
                },
                UiScreen::LauncherTouchTest,
            ),
            (ScreenVisibility::default(), UiScreen::Gameplay),
            (
                ScreenVisibility {
                    ingame_menu: true,
                    ..Default::default()
                },
                UiScreen::PauseMain,
            ),
            (
                ScreenVisibility {
                    ingame_menu: true,
                    ingame_controls: true,
                    ..Default::default()
                },
                UiScreen::PauseControls,
            ),
            (
                ScreenVisibility {
                    game_over: true,
                    ..Default::default()
                },
                UiScreen::GameOver,
            ),
        ];

        for (visibility, expected) in cases {
            assert_eq!(classify_screen(visibility), expected);
        }
    }

    #[test]
    fn classifies_the_topmost_visible_screen() {
        assert_eq!(
            classify_screen(ScreenVisibility {
                launcher: true,
                launcher_controls: true,
                launcher_settings: true,
                touch_test: true,
                ingame_menu: true,
                ingame_controls: true,
                game_over: true,
            }),
            UiScreen::LauncherTouchTest
        );
        assert_eq!(
            classify_screen(ScreenVisibility {
                launcher: true,
                launcher_controls: true,
                launcher_settings: true,
                game_over: true,
                ..Default::default()
            }),
            UiScreen::LauncherControls
        );
        assert_eq!(
            classify_screen(ScreenVisibility {
                ingame_menu: true,
                ingame_controls: true,
                game_over: true,
                ..Default::default()
            }),
            UiScreen::GameOver
        );
    }

    #[test]
    fn ui_revision_changes_only_with_observed_state() {
        let observation = UiStateObservation {
            screen: UiScreen::Gameplay,
            active_scenario: Some("pizza".into()),
            selected_scenario: "pizza".into(),
            scenario_revision: Some(7),
            paused: false,
            benchmark_active: false,
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
