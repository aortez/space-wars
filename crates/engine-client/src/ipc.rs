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
    CLOCK_MESSAGE_COMMAND, CLOCK_STATE_COMMAND, CLOCK_TRIGGER_COMMAND, ClockMessageRequest,
    ClockState, ClockTriggerRequest, ControlFailure, ControlFailureCode, HOST_PAUSE_COMMAND,
    HostPauseRequest, ProtocolError, RuntimeStatus, UI_ACTIVATE_COMMAND, UI_PRESS_COMMAND,
    UI_STATE_COMMAND, UI_STATE_SCHEMA_VERSION, UiAction, UiActivateRequest, UiControl,
    UiPressRequest, UiScreen, UiState, parse_runtime_status,
};

#[cfg(unix)]
use crate::ui_inventory::{
    ScreenVisibility, UiInventoryContext, classify_screen, inventory_for_screen,
};
use crate::{MainWindow, host};

#[cfg(unix)]
mod input;
#[cfg(unix)]
mod screenshot;
#[cfg(unix)]
use input::InputAutomation;
#[cfg(unix)]
use spacewars_control::{INPUT_PRESS_COMMAND, InputPressRequest};

#[cfg(unix)]
#[derive(Debug)]
struct ControlRequest {
    command: ControlCommand,
    response: ResponseWriter,
}

#[cfg(unix)]
#[derive(Debug)]
enum ControlCommand {
    Health(spacewars_control::HealthRequest),
    InputPress(InputPressRequest),
    Screenshot { output: PathBuf },
    Status,
    UiState,
    UiPress(UiPressRequest),
    UiActivate(UiActivateRequest),
    HostPause(HostPauseRequest),
    ClockState,
    ClockTrigger(ClockTriggerRequest),
    ClockMessage(ClockMessageRequest),
    HostBenchmark,
    RasterUpload(crate::raster::publication::UploadFormat),
    RasterTextures(crate::raster::texture_upload::Mode),
    GpuScene(crate::gpu_scene::Mode),
    GpuStream(crate::gpu_scene::StreamMode),
    GpuStrokes(crate::gpu_scene::StrokeMode),
    GpuJoins(crate::gpu_scene::JoinMode),
    GpuStrokeWriter(crate::gpu_scene::StrokeWriter),
    GpuAdmission(crate::gpu_scene::AdmissionMode),
    GpuPlan(crate::gpu_scene::PlanMode),
    GpuSource(crate::gpu_scene::SourceMode),
    GpuText(crate::gpu_text::Mode),
    CameraZoom(crate::camera_probe::ZoomMode),
    RenderTrace(&'static str),
    ClockBenchmark(crate::client_scenarios::ClockBenchmarkCase),
}

#[cfg(unix)]
#[derive(Debug)]
enum CommandParseError {
    Legacy(String),
    Structured(Box<ControlFailure>),
}

#[cfg(unix)]
enum ClockMutation {
    Trigger(ClockTriggerRequest),
    Message(ClockMessageRequest),
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
fn health_state(nonce: u64) -> spacewars_control::HealthState {
    thread_local! { static RESPONSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
    let ui_responses = RESPONSES.with(|count| {
        let next = count.get().saturating_add(1);
        count.set(next);
        next
    });
    #[cfg(all(target_os = "linux", feature = "pi-kiosk"))]
    let presented_frames = i_slint_backend_linuxkms::profiling::presented_frames();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk")))]
    let presented_frames = None;
    spacewars_control::HealthState {
        schema_version: spacewars_control::HEALTH_SCHEMA_VERSION,
        nonce,
        process_id: std::process::id(),
        build_revision: env!("SPACEWARS_BUILD_REVISION").into(),
        ui_responses,
        presented_frames,
    }
}

#[cfg(unix)]
pub fn start_control_server(
    window: &MainWindow,
    socket_path: PathBuf,
    scenario_controls: host::SharedScenarioControls,
    input: crate::input::SharedInput,
    gamepads: crate::input::SharedGamepadInput,
) -> Option<Timer> {
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
    let mut input_automation = InputAutomation::new(input, gamepads);
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        input_automation.tick(&window, &mut ui_state_tracker);
        while let Ok(request) = rx.try_recv() {
            handle_request(
                &window,
                request,
                &mut ui_state_tracker,
                &scenario_controls,
                &mut input_automation,
            );
        }
    });

    Some(timer)
}

#[cfg(not(unix))]
pub fn start_control_server(
    _window: &MainWindow,
    _socket_path: PathBuf,
    _scenario_controls: host::SharedScenarioControls,
    _input: crate::input::SharedInput,
    _gamepads: crate::input::SharedGamepadInput,
) -> Option<Timer> {
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
    let mode = if socket_path == Path::new(spacewars_control::KIOSK_CONTROL_SOCKET) {
        // The runtime directory is owned by the kiosk service. Its group also
        // admits the independent root service with its restricted capabilities.
        0o660
    } else {
        0o600
    };
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(mode))?;
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
        Some(spacewars_control::HEALTH_COMMAND) => {
            let payload = lines
                .next()
                .ok_or_else(|| invalid_mutation_request("health requires one JSON request line"))?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "health accepts exactly one JSON request line",
                ));
            }
            spacewars_control::HealthRequest::from_json(payload)
                .map(ControlCommand::Health)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some(INPUT_PRESS_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_mutation_request("input press requires one JSON request line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "input press accepts exactly one JSON request line",
                ));
            }
            InputPressRequest::from_json(payload)
                .map(ControlCommand::InputPress)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
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
                invalid_mutation_request("ui press requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "ui press accepts exactly one JSON request line",
                ));
            }
            UiPressRequest::from_json(payload)
                .map(ControlCommand::UiPress)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some(UI_ACTIVATE_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_mutation_request("ui activate requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "ui activate accepts exactly one JSON request line",
                ));
            }
            UiActivateRequest::from_json(payload)
                .map(ControlCommand::UiActivate)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some(HOST_PAUSE_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_mutation_request("host pause requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "host pause accepts exactly one JSON request line",
                ));
            }
            HostPauseRequest::from_json(payload)
                .map(ControlCommand::HostPause)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some(CLOCK_STATE_COMMAND) => {
            if lines.next().is_some() {
                return Err(invalid_mutation_request("clock state takes no arguments"));
            }
            Ok(ControlCommand::ClockState)
        }
        Some(CLOCK_TRIGGER_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_mutation_request("clock trigger requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "clock trigger accepts exactly one JSON request line",
                ));
            }
            ClockTriggerRequest::from_json(payload)
                .map(ControlCommand::ClockTrigger)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some(CLOCK_MESSAGE_COMMAND) => {
            let payload = lines.next().ok_or_else(|| {
                invalid_mutation_request("clock message requires a JSON request on the second line")
            })?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "clock message accepts exactly one JSON request line",
                ));
            }
            ClockMessageRequest::from_json(payload)
                .map(ControlCommand::ClockMessage)
                .map_err(|error| invalid_mutation_request(error.to_string()))
        }
        Some("host benchmark") => {
            if lines.next().is_some() {
                return Err(CommandParseError::Legacy("too many command lines".into()));
            }
            Ok(ControlCommand::HostBenchmark)
        }
        Some("host raster-upload") => {
            let value = lines
                .next()
                .ok_or_else(|| invalid_mutation_request("missing rgb or rgba"))?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "raster-upload accepts exactly one format line",
                ));
            }
            crate::raster::publication::UploadFormat::parse(value)
                .map(ControlCommand::RasterUpload)
                .map_err(invalid_mutation_request)
        }
        Some("host raster-textures") => {
            let value = lines
                .next()
                .ok_or_else(|| invalid_mutation_request("missing texture mode"))?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "raster-textures accepts exactly one mode line",
                ));
            }
            crate::raster::texture_upload::Mode::parse(value)
                .map(ControlCommand::RasterTextures)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-scene") => {
            let mode = match lines.next() {
                Some("on" | "triangles") => crate::gpu_scene::Mode::Triangles,
                Some("paths") => crate::gpu_scene::Mode::Paths,
                Some("circles") => crate::gpu_scene::Mode::Circles,
                Some("retained") => crate::gpu_scene::Mode::Retained,
                Some("retained-cold") => crate::gpu_scene::Mode::RetainedCold,
                Some("retained-adaptive") => crate::gpu_scene::Mode::RetainedAdaptive,
                Some("retained-zoom") => crate::gpu_scene::Mode::RetainedZoom,
                Some("retained-hybrid") => crate::gpu_scene::Mode::RetainedHybrid,
                Some("retained-outlines") => crate::gpu_scene::Mode::RetainedOutlines,
                Some("retained-paced") => crate::gpu_scene::Mode::RetainedPaced,
                Some("retained-memoized") => crate::gpu_scene::Mode::RetainedMemoized,
                Some("retained-scan") => crate::gpu_scene::Mode::RetainedScan,
                Some("retained-ranges") => crate::gpu_scene::Mode::RetainedRanges,
                Some("retained-culled") => crate::gpu_scene::Mode::RetainedCulled,
                Some("retained-resident") => crate::gpu_scene::Mode::RetainedResident,
                Some("retained-resident-passes") => crate::gpu_scene::Mode::RetainedResidentPasses,
                Some("retained-resident-buffers") => {
                    crate::gpu_scene::Mode::RetainedResidentBuffers
                }
                Some("retained-resident-uniforms") => {
                    crate::gpu_scene::Mode::RetainedResidentUniforms
                }
                Some("off") => crate::gpu_scene::Mode::Off,
                _ => {
                    return Err(invalid_mutation_request(
                        "gpu-scene requires triangles, circles, retained, retained-cold, retained-adaptive, retained-zoom, retained-hybrid, retained-outlines, retained-paced, retained-memoized, retained-scan, retained-ranges, retained-culled, retained-resident, retained-resident-passes, retained-resident-buffers, retained-resident-uniforms, paths or off (on selects triangles)",
                    ));
                }
            };
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-scene accepts exactly one mode line",
                ));
            }
            Ok(ControlCommand::GpuScene(mode))
        }
        Some("host gpu-stream") => {
            let value = lines
                .next()
                .ok_or_else(|| invalid_mutation_request("missing stream policy"))?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-stream accepts exactly one policy line",
                ));
            }
            crate::gpu_scene::StreamMode::parse(value)
                .map(ControlCommand::GpuStream)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-strokes") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-strokes accepts exactly one mode line",
                ));
            }
            crate::gpu_scene::StrokeMode::parse(value)
                .map(ControlCommand::GpuStrokes)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-joins") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-joins accepts exactly one mode line",
                ));
            }
            crate::gpu_scene::JoinMode::parse(value)
                .map(ControlCommand::GpuJoins)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-stroke-writer") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-stroke-writer accepts exactly one mode line",
                ));
            }
            crate::gpu_scene::StrokeWriter::parse(value)
                .map(ControlCommand::GpuStrokeWriter)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-admission") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-admission accepts exactly one policy line",
                ));
            }
            crate::gpu_scene::AdmissionMode::parse(value)
                .map(ControlCommand::GpuAdmission)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-plan") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-plan accepts exactly one policy line",
                ));
            }
            crate::gpu_scene::PlanMode::parse(value)
                .map(ControlCommand::GpuPlan)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-source") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-source accepts exactly one policy line",
                ));
            }
            crate::gpu_scene::SourceMode::parse(value)
                .map(ControlCommand::GpuSource)
                .map_err(invalid_mutation_request)
        }
        Some("host gpu-text") => {
            let value = lines.next().unwrap_or_default();
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "gpu-text accepts exactly one mode line",
                ));
            }
            crate::gpu_text::Mode::parse(value)
                .map(ControlCommand::GpuText)
                .map_err(invalid_mutation_request)
        }
        Some("host camera-zoom" | "host gpu-zoom") => {
            let value = lines
                .next()
                .ok_or_else(|| invalid_mutation_request("missing zoom mode"))?;
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "camera-zoom accepts exactly one mode line",
                ));
            }
            crate::camera_probe::ZoomMode::parse(value)
                .map(ControlCommand::CameraZoom)
                .map_err(invalid_mutation_request)
        }
        Some("host render-trace") => {
            let command = match lines.next() {
                Some("start") => "start",
                Some("start-text") => "start-text",
                Some("start-work") => "start-work",
                Some("start-geometry") => "start-geometry",
                Some("read") => "read",
                Some("cancel") => "cancel",
                _ => {
                    return Err(invalid_mutation_request(
                        "render-trace requires start, start-text, start-work, start-geometry, read or cancel",
                    ));
                }
            };
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "render-trace accepts exactly one command line",
                ));
            }
            Ok(ControlCommand::RenderTrace(command))
        }
        Some("host clock-benchmark") => {
            let case = match lines.next() {
                Some("idle") => crate::client_scenarios::ClockBenchmarkCase::Idle,
                Some("rain") => crate::client_scenarios::ClockBenchmarkCase::Rain,
                Some("aquarium") => crate::client_scenarios::ClockBenchmarkCase::Aquarium,
                _ => {
                    return Err(invalid_mutation_request(
                        "clock-benchmark requires idle, rain or aquarium",
                    ));
                }
            };
            if lines.next().is_some() {
                return Err(invalid_mutation_request(
                    "clock-benchmark accepts exactly one fixture line",
                ));
            }
            Ok(ControlCommand::ClockBenchmark(case))
        }
        Some(command) => Err(CommandParseError::Legacy(format!(
            "unknown command {command:?}"
        ))),
        None => Err(CommandParseError::Legacy("empty command".into())),
    }
}

#[cfg(unix)]
fn invalid_mutation_request(message: impl Into<String>) -> CommandParseError {
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
    scenario_controls: &host::SharedScenarioControls,
    input_automation: &mut InputAutomation,
) {
    if (window.get_install_fenced() || window.get_updates_visible())
        && !matches!(
            &request.command,
            ControlCommand::Health(_)
                | ControlCommand::UiState
                | ControlCommand::Status
                | ControlCommand::Screenshot { .. }
                | ControlCommand::UiPress(_)
                | ControlCommand::UiActivate(_)
        )
    {
        request
            .response
            .error("Install handoff blocks app mutations.");
        return;
    }
    match request.command {
        ControlCommand::InputPress(press) => {
            input_automation.press(window, press, request.response, ui_state_tracker)
        }
        ControlCommand::ClockState => handle_clock_request(
            window,
            None,
            request.response,
            ui_state_tracker,
            scenario_controls,
        ),
        ControlCommand::ClockTrigger(trigger) => handle_clock_request(
            window,
            Some(ClockMutation::Trigger(trigger)),
            request.response,
            ui_state_tracker,
            scenario_controls,
        ),
        ControlCommand::ClockMessage(message) => handle_clock_request(
            window,
            Some(ClockMutation::Message(message)),
            request.response,
            ui_state_tracker,
            scenario_controls,
        ),
        ControlCommand::Screenshot { output } => {
            screenshot::request(window, output, request.response);
        }
        ControlCommand::Status => {
            let mut diagnostics = window.get_runtime_diagnostics().to_string();
            if !window.get_launcher_visible()
                && !window.get_launcher_busy()
                && let Some(status) = scenario_controls.borrow().pizza_status()
            {
                diagnostics.push_str("\npizza=");
                diagnostics.push_str(
                    &serde_json::to_string(status).expect("bounded Pizza status serializes"),
                );
            }
            diagnostics.push_str(&format!(
                "\nraster_upload_format={}",
                crate::raster::publication::requested_format().name(),
            ));
            diagnostics.push_str(&crate::raster::texture_upload::diagnostics());
            diagnostics.push_str(&crate::gpu_scene::diagnostics());
            diagnostics.push_str(&crate::camera_probe::diagnostics());
            diagnostics.push_str(&crate::gpu_text::diagnostics());
            diagnostics.push_str(&format!(
                "\nperformance_overlay_enabled={}\nperformance_overlay_text={}\nperformance_overlay_details={}",
                window.get_performance_overlay_enabled(),
                window.get_performance_overlay_text(),
                window.get_performance_overlay_details().replace('\n', " | "),
            ));
            #[cfg(all(target_os = "linux", feature = "pi-kiosk"))]
            diagnostics.push_str(&i_slint_backend_linuxkms::profiling::diagnostics());
            diagnostics.push('\n');
            diagnostics.push_str(window.get_autostart_diagnostics().as_str());
            let launch = window.get_launcher_diagnostics();
            if !launch.is_empty() {
                diagnostics.push('\n');
                diagnostics.push_str(launch.as_str());
            }
            diagnostics.push_str(&format!(
                "\nmaster_volume_percent={}\nmaster_muted={}\nsettings_save_pending={}",
                window.get_sound_volume_percent(),
                window.get_sound_muted(),
                window.get_settings_save_pending()
            ));
            request.response.ok(diagnostics);
        }
        ControlCommand::Health(health) => {
            // Exercise presentation while the launcher or a paused game is
            // otherwise static. This is not user input and does not affect idle
            // or postponement accounting. The next sample observes completion.
            window.window().request_redraw();
            match serde_json::to_string(&health_state(health.nonce)) {
                Ok(json) => request.response.ok(json),
                Err(error) => request.response.error(error.to_string()),
            }
        }
        ControlCommand::UiState => {
            match ui_state(window, ui_state_tracker).and_then(|state| state.to_json()) {
                Ok(json) => request.response.ok(json),
                Err(error) => request.response.error(error.to_string()),
            }
        }
        ControlCommand::UiPress(press) => {
            handle_ui_press(window, press, request.response, ui_state_tracker);
        }
        ControlCommand::UiActivate(activation) => {
            handle_ui_activate(window, activation, request.response, ui_state_tracker);
        }
        ControlCommand::HostPause(pause) => {
            handle_host_pause(
                window,
                pause,
                request.response,
                ui_state_tracker,
                scenario_controls,
            );
        }
        ControlCommand::HostBenchmark => {
            if window.get_launcher_busy() {
                request.response.error("a launch is already in progress");
            } else if !window.get_scenario_benchmark_available() {
                request
                    .response
                    .error("the selected scenario does not support benchmark mode");
            } else if window.get_launcher_visible() {
                window.invoke_launcher_start_benchmark();
                if window.get_launcher_busy() {
                    request.response.ok("benchmark requested");
                } else if window.get_launcher_visible() {
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
        ControlCommand::RasterUpload(format) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("raster-upload requires an active raster scene (not native video)");
            } else {
                match crate::raster::publication::request_format(format) {
                    Ok(()) => {
                        window.window().request_redraw();
                        tracing::info!(
                            format = format.name(),
                            "experimental raster upload format requested."
                        );
                        request.response.ok(format!(
                            "raster_upload_format={} (from next raster frame; not saved)",
                            format.name()
                        ));
                    }
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::RasterTextures(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("raster-textures requires an active raster scene (not native video)");
            } else {
                match crate::raster::texture_upload::request(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "raster_texture_mode={} (from next raster frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuScene(mode) => {
            let diagnostics = window.get_runtime_diagnostics();
            let active_scene = gpu_scene_source_supported(mode, diagnostics.as_str());
            if mode != crate::gpu_scene::Mode::Off
                && (window.get_launcher_visible()
                    || window.get_launcher_busy()
                    || !window.get_raster_visible()
                    || window.get_native_video_visible()
                    || !active_scene)
            {
                request
                    .response
                    .error("gpu-scene requires an active raster scene at 1x; paths reference is Clock-only");
            } else {
                match crate::gpu_scene::request(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_requested={} mode={} (from next frame; not saved)",
                        mode != crate::gpu_scene::Mode::Off,
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuStream(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("gpu-stream requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_stream(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_stream_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuStrokes(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("gpu-strokes requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_strokes(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_stroke_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuJoins(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
                || !crate::gpu_scene::requested()
            {
                request
                    .response
                    .error("gpu-joins requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_joins(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_join_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuStrokeWriter(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
                || !crate::gpu_scene::requested()
            {
                request
                    .response
                    .error("gpu-stroke-writer requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_writer(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_stroke_writer_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuAdmission(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
                || !crate::gpu_scene::requested()
            {
                request
                    .response
                    .error("gpu-admission requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_admission(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_admission_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuSource(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("gpu-source requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_source(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_source_policy={} (from next frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuPlan(mode) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !window.get_raster_visible()
                || window.get_native_video_visible()
            {
                request
                    .response
                    .error("gpu-plan requires an active GPU scene");
            } else {
                match crate::gpu_scene::request_plan(window, mode) {
                    Ok(()) => request.response.ok(format!(
                        "gpu_scene_plan_policy={} (from next GPU frame; not saved)",
                        mode.name()
                    )),
                    Err(error) => request.response.error(error),
                }
            }
        }
        ControlCommand::GpuText(mode) => match crate::gpu_text::request(window, mode) {
            Ok(()) => request.response.ok(format!(
                "gpu_text_requested={} (fresh cache from next safe text frame; not saved)",
                mode.name()
            )),
            Err(error) => request.response.error(error),
        },
        ControlCommand::CameraZoom(mode) => {
            if mode != crate::camera_probe::ZoomMode::Off
                && (window.get_launcher_visible()
                    || window.get_launcher_busy()
                    || window.get_native_video_visible()
                    || !window
                        .get_runtime_diagnostics()
                        .lines()
                        .any(|line| line == "scenario=spacewars"))
            {
                request
                    .response
                    .error("camera-zoom requires an active Spacewars scene");
            } else {
                crate::camera_probe::request(window, mode);
                request.response.ok(format!(
                    "camera_zoom_mode={} (presentation only; not saved)",
                    mode.name()
                ));
            }
        }
        ControlCommand::RenderTrace(command) => {
            #[cfg(all(target_os = "linux", feature = "pi-kiosk"))]
            match i_slint_backend_linuxkms::profiling::render_trace(command) {
                Ok(report) => request.response.ok(report),
                Err(error) => request.response.error(error),
            }
            #[cfg(not(all(target_os = "linux", feature = "pi-kiosk")))]
            {
                let _ = command;
                request
                    .response
                    .error("render-trace requires the LinuxKMS kiosk build");
            }
        }
        ControlCommand::ClockBenchmark(case) => {
            if window.get_launcher_visible()
                || window.get_launcher_busy()
                || !scenario_controls.borrow_mut().request_clock_benchmark(case)
            {
                request
                    .response
                    .error("clock-benchmark requires an active Clock with no pending host request");
            } else {
                request
                    .response
                    .ok("Clock benchmark requested; fixture settings are not saved");
            }
        }
    }
}

#[cfg(unix)]
fn gpu_scene_source_supported(mode: crate::gpu_scene::Mode, diagnostics: &str) -> bool {
    diagnostics.lines().any(|line| line == "renderer=raster")
        && diagnostics.lines().any(|line| line == "raster_scale=1.00")
        && (mode != crate::gpu_scene::Mode::Paths
            || diagnostics.lines().any(|line| line == "scenario=clock"))
}

#[cfg(unix)]
fn handle_clock_request(
    window: &MainWindow,
    mutation: Option<ClockMutation>,
    response: ResponseWriter,
    tracker: &mut UiStateTracker,
    controls: &host::SharedScenarioControls,
) {
    let ui = match ui_state(window, tracker) {
        Ok(state) => state,
        Err(error) => {
            response.error(error.to_string());
            return;
        }
    };
    let mut controls = controls.borrow_mut();
    let clock = clock_snapshot(window, &controls);
    if ui.active_scenario.as_deref() != Some("clock") || ui.screen.is_launcher() {
        response.control_failure(ControlFailure::new(
            ControlFailureCode::ControlUnavailable,
            "Clock controls require an active Clock scenario",
            Some(ui),
        ));
        return;
    }
    let Some(clock) = clock.filter(|clock| Some(clock.scenario_revision) == ui.scenario_revision)
    else {
        response.control_failure(ControlFailure::new(
            ControlFailureCode::ControlUnavailable,
            "Clock state is not ready",
            Some(ui),
        ));
        return;
    };
    if let Some(mutation) = mutation {
        let validation = match &mutation {
            ClockMutation::Trigger(trigger) => validate_clock_trigger(trigger, &ui, &clock),
            ClockMutation::Message(message) => validate_clock_message(message, &ui, &clock),
        };
        if let Err(failure) = validation {
            response.control_failure(*failure);
            return;
        }
        let accepted = match mutation {
            ClockMutation::Trigger(trigger) => controls.request_clock_event(trigger.event),
            ClockMutation::Message(message) => {
                controls.request_clock_settings(engine_common::ClockSettings {
                    marquee_message: message.message,
                    ..clock.settings
                })
            }
        };
        if !accepted {
            response.control_failure(ControlFailure::new(
                ControlFailureCode::ActionUnavailable,
                "Another host control is pending",
                Some(ui),
            ));
            return;
        }
    }
    match clock_snapshot(window, &controls).unwrap().to_json() {
        Ok(json) => response.ok(json),
        Err(error) => response.error(error.to_string()),
    }
}

#[cfg(unix)]
fn clock_snapshot(window: &MainWindow, controls: &host::ScenarioControls) -> Option<ClockState> {
    controls.clock_state().map(|mut state| {
        // Message acknowledgements include durable persistence, even though
        // storage now runs independently of the simulation/UI thread.
        state.settings_pending |= window.get_settings_save_pending();
        state.settings_error = non_empty(window.get_clock_settings_error().as_str());
        state
    })
}

#[cfg(unix)]
fn validate_clock_message(
    request: &ClockMessageRequest,
    ui: &UiState,
    clock: &ClockState,
) -> Result<(), Box<ControlFailure>> {
    let failure = if request.expected_scenario_revision != clock.scenario_revision
        || request.expected_message != clock.settings.marquee_message
    {
        Some((
            ControlFailureCode::StaleRevision,
            "Clock instance or message changed",
        ))
    } else if !ui.paused || !clock.paused {
        Some((
            ControlFailureCode::WrongScreen,
            "Pause Clock before changing its marquee message",
        ))
    } else {
        None
    };
    if let Some((code, message)) = failure {
        let mut failure = ControlFailure::new(code, message, Some(ui.clone()));
        failure.current_clock_state = Some(clock.clone());
        return Err(Box::new(failure));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_clock_trigger(
    request: &ClockTriggerRequest,
    ui: &UiState,
    clock: &ClockState,
) -> Result<(), Box<ControlFailure>> {
    let failure = if request.expected_scenario_revision != clock.scenario_revision
        || request.expected_event_id != clock.event_id
    {
        Some((
            ControlFailureCode::StaleRevision,
            "Clock instance or event changed",
        ))
    } else if ui.screen != UiScreen::Gameplay || clock.paused {
        Some((
            ControlFailureCode::WrongScreen,
            "Resume Clock gameplay before triggering an event",
        ))
    } else if clock
        .events
        .iter()
        .any(|event| event.kind == request.event && event.blocked_by_duck)
    {
        Some((
            ControlFailureCode::ActionUnavailable,
            "This event needs the duck's arena; wait for departure, or take control and dismiss the duck",
        ))
    } else if clock
        .events
        .iter()
        .any(|event| event.kind == request.event && event.blocked_by_crow)
    {
        Some((
            ControlFailureCode::ActionUnavailable,
            "A crow is already visiting; wait for departure",
        ))
    } else if let Some(unavailable) = clock
        .events
        .iter()
        .find(|event| event.kind == request.event)
        .and_then(|event| event.blocked_by_arena)
    {
        Some((ControlFailureCode::ActionUnavailable, unavailable.notice()))
    } else if !clock.can_trigger {
        Some((
            ControlFailureCode::ActionUnavailable,
            "Clock is busy; wait for idle before triggering another event",
        ))
    } else {
        None
    };
    if let Some((code, message)) = failure {
        let mut failure = ControlFailure::new(code, message, Some(ui.clone()));
        failure.current_clock_state = Some(clock.clone());
        return Err(Box::new(failure));
    }
    Ok(())
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
fn handle_ui_activate(
    window: &MainWindow,
    request: UiActivateRequest,
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

    if let Err(failure) = validate_ui_activate(&request, &state) {
        response.control_failure(*failure);
        return;
    }

    if !crate::ui_activation::activate(window, &request.control_id) {
        response.control_failure(ControlFailure::new(
            ControlFailureCode::ControlUnavailable,
            format!(
                "control {:?} is visible and enabled on {}, but has no semantic activation mapping",
                request.control_id, state.screen
            ),
            Some(state),
        ));
        return;
    }

    match ui_state(window, ui_state_tracker).and_then(|state| state.to_json()) {
        Ok(json) => response.ok(json),
        Err(error) => response.error(error.to_string()),
    }
}

#[cfg(unix)]
fn handle_host_pause(
    window: &MainWindow,
    request: HostPauseRequest,
    response: ResponseWriter,
    ui_state_tracker: &mut UiStateTracker,
    scenario_controls: &host::SharedScenarioControls,
) {
    let state = match ui_state(window, ui_state_tracker) {
        Ok(state) => state,
        Err(error) => {
            response.error(error.to_string());
            return;
        }
    };

    if let Err(failure) = validate_host_pause(&request, &state) {
        response.control_failure(*failure);
        return;
    }

    scenario_controls.borrow_mut().request_pause();
    match state.to_json() {
        Ok(json) => response.ok(json),
        Err(error) => response.error(error.to_string()),
    }
}

#[cfg(unix)]
fn validate_ui_press(request: &UiPressRequest, state: &UiState) -> Result<(), Box<ControlFailure>> {
    validate_ui_preconditions(request.expected_screen, request.expected_revision, state)?;

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
fn validate_ui_activate(
    request: &UiActivateRequest,
    state: &UiState,
) -> Result<(), Box<ControlFailure>> {
    validate_ui_preconditions(request.expected_screen, request.expected_revision, state)?;

    let Some(control) = state
        .controls
        .iter()
        .find(|control| control.id == request.control_id)
    else {
        let available = if state.controls.is_empty() {
            "none".into()
        } else {
            state
                .controls
                .iter()
                .map(|control| control.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::ControlUnavailable,
            format!(
                "control {:?} is unavailable on {}; visible controls: {available}",
                request.control_id, state.screen
            ),
            Some(state.clone()),
        )));
    };

    if !control.enabled {
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::ControlDisabled,
            format!(
                "control {:?} is disabled on {}; inspect `spacewars-cli ui state` and retry",
                request.control_id, state.screen
            ),
            Some(state.clone()),
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn validate_host_pause(
    request: &HostPauseRequest,
    state: &UiState,
) -> Result<(), Box<ControlFailure>> {
    validate_ui_preconditions(request.expected_screen, request.expected_revision, state)?;

    if state.screen != UiScreen::Gameplay {
        return Err(Box::new(ControlFailure::new(
            ControlFailureCode::WrongScreen,
            format!(
                "host pause requires gameplay, but current screen is {}; inspect `spacewars-cli ui state` and retry",
                state.screen
            ),
            Some(state.clone()),
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn validate_ui_preconditions(
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
    state: &UiState,
) -> Result<(), Box<ControlFailure>> {
    if let Some(expected) = expected_screen
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

    if let Some(expected) = expected_revision
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

    Ok(())
}

#[cfg(unix)]
fn ui_state(window: &MainWindow, tracker: &mut UiStateTracker) -> Result<UiState, ProtocolError> {
    let screen = classify_screen(ScreenVisibility {
        updates: window.get_updates_visible() || window.get_install_fenced(),
        launcher_busy: window.get_launcher_busy(),
        sound: window.get_sound_visible(),
        device_info: window.get_device_info_visible(),
        controllers: window.get_controllers_visible(),
        network: window.get_network_visible(),
        autostart: window.get_autostart_settings_visible(),
        launcher: window.get_launcher_visible(),
        training: window.get_training_visible(),
        genomes: window.get_genomes_visible(),
        launcher_controls: window.get_launcher_controls_visible(),
        launcher_settings: window.get_launcher_settings_visible(),
        touch_test: window.get_touch_test_visible(),
        ingame_menu: window.get_ingame_menu_visible(),
        ingame_controls: window.get_ingame_controls_visible(),
        ingame_clock: window.get_ingame_clock_visible(),
        clock_fonts: window.get_clock_fonts_visible(),
        game_over: window.get_game_over_visible(),
    });
    let runtime = runtime_status_for_screen(screen, window.get_runtime_diagnostics().as_str())?;
    let selected_scenario = window.get_launcher_scenario().to_string();
    let inventory = inventory_for_screen(
        screen,
        &UiInventoryContext {
            updates_controls: crate::updates::inventory(window),
            updates_focus: window.get_updates_focus(),
            updates_message: window.get_updates_message().to_string(),
            genome_controls: if window.get_genomes_visible() {
                crate::genome_controls::inventory(window)
            } else {
                Vec::new()
            },
            genome_selected: crate::genome_controls::selected_id(window).into(),
            genome_error: non_empty(window.get_genomes_error().as_str()),
            training_controls: if window.get_training_visible() {
                crate::training_controls::inventory(window)
            } else {
                Vec::new()
            },
            training_selected: crate::training_controls::selected_id(window).into(),
            training_error: non_empty(window.get_training_error().as_str()),
            training_launcher_label: window.get_training_launcher_label().to_string(),
            scoreboard_controls: crate::scoreboard::inventory(window),
            font_controls: if window.get_clock_fonts_visible() {
                crate::clock_fonts::inventory(window)
            } else {
                Vec::new()
            },
            font_focus: window.get_clock_font_focus(),
            automatic: window.get_autostart_running(),
            autostart_controls: crate::autostart::controls(window),
            autostart_focus: window.get_autostart_focus_index(),
            controller_controls: if window.get_controllers_visible() {
                crate::controller_controls::inventory(window)
            } else {
                Vec::new()
            },
            controller_focus: window.get_controllers_focus_index(),
            network_controls: if window.get_network_visible() {
                crate::network_controls::inventory(window)
            } else {
                Vec::new()
            },
            network_focus: window.get_network_focus_index(),
            device_info_controls: if matches!(screen, UiScreen::LauncherInfo | UiScreen::PauseInfo)
            {
                crate::device_info::inventory(window)
            } else {
                Vec::new()
            },
            launcher_busy_stage: window.get_launcher_busy_stage().to_string(),
            launcher_busy_elapsed: window.get_launcher_busy_elapsed().to_string(),
            sound_focus_index: window.get_sound_focus_index(),
            sound_volume_percent: window.get_sound_volume_percent(),
            sound_muted: window.get_sound_muted(),
            performance_overlay_enabled: window.get_performance_overlay_enabled(),
            settings_save_pending: window.get_settings_save_pending(),
            settings_save_error: non_empty(window.get_settings_save_error().as_str()),
            selected_scenario: selected_scenario.clone(),
            world_seed: window.get_launcher_seed_text().to_string(),
            launcher_focus_index: window.get_launcher_focus_index(),
            launcher_show_all: window.get_launcher_show_all(),
            launcher_settings_focus_index: window.get_launcher_settings_focus_index(),
            launcher_controls_focus_index: window.get_launcher_controls_focus_index(),
            ingame_menu_focus_index: window.get_ingame_menu_focus_index(),
            ingame_clock_focus_index: window.get_ingame_clock_focus_index(),
            clock_preview: engine_common::ClockEventKind::ALL
                .get(window.get_clock_preview_index() as usize)
                .map_or("", |event| event.label())
                .into(),
            clock_controls_pending: window.get_clock_controls_pending(),
            clock_settings_error: non_empty(window.get_clock_settings_error().as_str()),
            game_over_focus_index: window.get_game_over_focus_index(),
            benchmark_available: window.get_scenario_benchmark_available(),
            launch_available: selected_scenario != "nes" || window.get_launcher_nes_rom_supported(),
            launcher_error: non_empty(window.get_launcher_error_text().as_str()),
            scenario_error: non_empty(window.get_scenario_error_text().as_str()),
            renderer: window.get_launcher_renderer().to_string(),
            raster_only: window.get_raster_only(),
            raster_scale: window.get_launcher_raster_scale_text().to_string(),
            match_length: window.get_launcher_match_length().to_string(),
            combat_break_interval: window.get_launcher_combat_break_interval().to_string(),
            combat_break_duration: window.get_launcher_combat_break_duration().to_string(),
            combat_mission: window.get_launcher_combat_mission().to_string(),
            combat_asteroid_interval: window.get_launcher_combat_asteroid_interval().to_string(),
            loose_dirt: window.get_launcher_loose_dirt(),
            dirt_mode: window.get_launcher_dirt_mode().to_string(),
            combat_asteroid_strength: window.get_launcher_combat_asteroid_strength().to_string(),
            spacewars_preset: window.get_launcher_spacewars_preset().to_string(),
            spacewars_planets: window.get_launcher_use_planets().to_string(),
            spacewars_asteroids: window.get_launcher_asteroids_enabled().to_string(),
            spacewars_player_health: window.get_launcher_player_health_text().to_string(),
            spacewars_player_1: window.get_launcher_p1_controller().to_string(),
            spacewars_player_2: window.get_launcher_p2_controller().to_string(),
            pizza_desired_balls: window.get_launcher_pizza_desired_balls_text().to_string(),
            pizza_live_ball_limit: window.get_launcher_pizza_live_ball_limit_text().to_string(),
            pizza_spawn_rate: window.get_launcher_pizza_spawn_rate_text().to_string(),
            pizza_automatic_events: window.get_launcher_pizza_automatic_events(),
            pizza_event_available: window.get_pizza_event_available(),
            pizza_event_can_trigger: window.get_pizza_event_can_trigger(),
            pizza_event_status: window.get_pizza_event_status().to_string(),
            expedition_players: window.get_launcher_expedition_players().to_string(),
            clock_time_format: window.get_launcher_clock_time_format().to_string(),
            clock_show_date: window.get_launcher_clock_show_date(),
            clock_event_profile: window.get_launcher_clock_event_profile().to_string(),
            clock_falling_enabled: window.get_launcher_clock_falling_enabled(),
            clock_color_cycle_enabled: window.get_launcher_clock_color_cycle_enabled(),
            clock_meltdown_enabled: window.get_launcher_clock_meltdown_enabled(),
            clock_duck_enabled: window.get_launcher_clock_duck_enabled(),
            clock_crow: if window.get_launcher_clock_crow_enabled() {
                window.get_launcher_clock_crow_water_tolerance().to_string()
            } else {
                "Off".into()
            },
            clock_explosion_enabled: window.get_launcher_clock_explosion_enabled(),
            clock_aquarium_enabled: window.get_launcher_clock_aquarium_enabled(),
            clock_marquee_enabled: window.get_launcher_clock_marquee_enabled(),
            clock_digit_slide_enabled: window.get_launcher_clock_digit_slide_enabled(),
            clock_cell_swap_enabled: window.get_launcher_clock_cell_swap_enabled(),
            clock_grass_enabled: window.get_launcher_clock_grass_enabled(),
            clock_slot_machine_enabled: window.get_launcher_clock_slot_machine_enabled(),
            clock_rain: if window.get_launcher_clock_rain_enabled() {
                window.get_launcher_clock_rain_amount().to_string()
            } else {
                "Off".into()
            },
            clock_marquee_preset: window.get_launcher_clock_marquee_preset().to_string(),
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
    fn player_duck_event_restrictions_are_public_and_trigger_failures_are_explicit() {
        use crate::client_scenarios::{ScenarioStartMode, registration};
        use crate::render::Viewport;
        use engine_common::{ClockEventKind, Settings};
        use scenario_clock::{ClockAction, ClockReading};
        let mut scenario = registration("clock")
            .unwrap()
            .create(
                42,
                &Settings::default(),
                Viewport::new(800.0, 480.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        scenario.step(
            &[
                ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
                ClockAction::toggle_player_duck(1),
            ],
            Duration::ZERO,
        );
        let mut clock = scenario.clock_state().unwrap();
        clock.scenario_revision = 7;
        let mut ui = UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 1,
            screen: UiScreen::Gameplay,
            active_scenario: Some("clock".into()),
            selected_scenario: "clock".into(),
            selected_control: None,
            controls: vec![],
            actions: vec![],
            scenario_revision: Some(7),
            paused: false,
            benchmark_active: false,
            error: None,
        };
        assert_eq!(
            ClockState::from_json(&clock.to_json().unwrap()).unwrap(),
            clock
        );
        for event in ClockEventKind::ALL {
            let request = ClockTriggerRequest::new(&clock, event);
            let result = validate_clock_trigger(&request, &ui, &clock);
            if clock
                .events
                .iter()
                .find(|entry| entry.kind == event)
                .unwrap()
                .blocked_by_duck
            {
                let failure = result.unwrap_err();
                assert_eq!(failure.code, ControlFailureCode::ActionUnavailable);
                assert!(failure.message.contains("dismiss the duck"));
                assert_eq!(failure.current_clock_state, Some(clock.clone()));
            } else {
                result.unwrap();
            }
        }
        scenario.step(
            &[ClockAction::preview_event(ClockEventKind::Crow)],
            Duration::ZERO,
        );
        for _ in 0..121 {
            scenario.step(&[], Duration::from_millis(16));
        }
        clock = scenario.clock_state().unwrap();
        clock.scenario_revision = 7;
        assert!(clock.can_trigger);
        let request = ClockTriggerRequest::new(&clock, ClockEventKind::Crow);
        let failure = validate_clock_trigger(&request, &ui, &clock).unwrap_err();
        assert_eq!(failure.code, ControlFailureCode::ActionUnavailable);
        assert!(failure.message.contains("crow is already visiting"));
        assert_eq!(failure.current_clock_state, Some(clock.clone()));
        validate_clock_trigger(
            &ClockTriggerRequest::new(&clock, ClockEventKind::Rain),
            &ui,
            &clock,
        )
        .unwrap();
        // Whatever the run's arena would refuse is unavailable, for its reason.
        let mut refused = clock.clone();
        let rain = refused
            .events
            .iter_mut()
            .find(|event| event.kind == ClockEventKind::Rain)
            .unwrap();
        rain.blocked_by_arena = Some(engine_common::ClockArenaUnavailable::LayoutIncompatible);
        let request = ClockTriggerRequest::new(&refused, ClockEventKind::Rain);
        let failure = validate_clock_trigger(&request, &ui, &refused).unwrap_err();
        assert_eq!(failure.code, ControlFailureCode::ActionUnavailable);
        assert_eq!(
            failure.message,
            engine_common::ClockArenaUnavailable::LayoutIncompatible.notice()
        );
        assert_eq!(failure.current_clock_state, Some(refused));
        let request = ClockTriggerRequest::new(&clock, ClockEventKind::Marquee);
        ui.screen = UiScreen::PauseMain;
        assert_eq!(
            validate_clock_trigger(&request, &ui, &clock)
                .unwrap_err()
                .code,
            ControlFailureCode::WrongScreen
        );
    }

    #[test]
    fn busy_screen_exposes_read_only_progress_and_rejects_launch_actions() {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, PlatformError, WindowAdapter};
        use std::rc::Rc;

        struct TestPlatform;
        impl Platform for TestPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
            }
        }
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        window.set_launcher_visible(true);
        window.set_launcher_settings_visible(true);
        window.set_launcher_busy(true);
        window.set_launcher_scenario("pizza".into());
        window.set_launcher_busy_stage("saving_settings".into());
        window.set_launcher_busy_elapsed("12.4 s".into());
        window.set_launcher_diagnostics("launch_state=busy\nlaunch_save_ms=0".into());
        // A previous instance must not appear active behind the busy launcher.
        window.set_runtime_diagnostics(
            "scenario=clock\nscenario_revision=4\npaused=true\nbenchmark_active=false".into(),
        );
        let controls = host::new_scenario_controls();
        let (input, gamepads) = crate::input::new_shared_input();
        let mut input_automation = InputAutomation::new(input, gamepads);
        let mut tracker = UiStateTracker::default();
        let mut request = |command| {
            let (mut reader, stream) = std::os::unix::net::UnixStream::pair().unwrap();
            reader
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            handle_request(
                &window,
                ControlRequest {
                    command,
                    response: ResponseWriter { stream },
                },
                &mut tracker,
                &controls,
                &mut input_automation,
            );
            let mut reply = String::new();
            reader.read_to_string(&mut reply).unwrap();
            reply
        };
        let reply = request(ControlCommand::UiState);
        let state = UiState::from_json(reply.trim().strip_prefix("ok ").unwrap()).unwrap();
        assert_eq!(state.screen, UiScreen::LauncherBusy);
        assert_eq!(state.selected_scenario, "pizza");
        assert!(state.active_scenario.is_none());
        assert!(state.scenario_revision.is_none());
        assert!(state.actions.is_empty());
        assert_eq!(state.controls[0].value.as_deref(), Some("saving_settings"));
        assert_eq!(state.controls[1].value.as_deref(), Some("12.4 s"));
        assert!(request(ControlCommand::Status).contains("launch_state=busy"));
        for (command, expected) in [
            (
                ControlCommand::UiPress(UiPressRequest::new(UiAction::Start)),
                ControlFailureCode::ActionUnavailable,
            ),
            (
                ControlCommand::UiActivate(UiActivateRequest::new("launcher.start")),
                ControlFailureCode::ControlUnavailable,
            ),
            (
                ControlCommand::UiActivate(UiActivateRequest::new("launcher.busy.stage")),
                ControlFailureCode::ControlDisabled,
            ),
        ] {
            let reply = request(command);
            let failure =
                ControlFailure::from_json(reply.trim().strip_prefix("error ").unwrap()).unwrap();
            assert_eq!(failure.code, expected);
            assert_eq!(failure.current_state, Some(state.clone()));
        }
        assert_eq!(
            request(ControlCommand::HostBenchmark),
            "error a launch is already in progress\n"
        );
        // Status retains the previous launch timings after the overlay closes.
        window.set_launcher_busy(false);
        window.set_launcher_diagnostics("launch_state=complete\nlaunch_save_ms=13512".into());
        assert!(request(ControlCommand::Status).contains("launch_save_ms=13512"));
        // Benchmark callers must receive an acknowledgement, not a launch
        // failure merely because the menu is still visible during preparation.
        window.set_scenario_benchmark_available(true);
        let weak = window.as_weak();
        window.on_launcher_start_benchmark(move || weak.upgrade().unwrap().set_launcher_busy(true));
        assert_eq!(
            request(ControlCommand::HostBenchmark),
            "ok benchmark requested\n"
        );
    }

    #[test]
    fn clock_benchmark_parser_accepts_only_one_known_fixture() {
        use crate::client_scenarios::ClockBenchmarkCase;
        for (name, expected) in [
            ("idle", ClockBenchmarkCase::Idle),
            ("rain", ClockBenchmarkCase::Rain),
            ("aquarium", ClockBenchmarkCase::Aquarium),
        ] {
            assert!(matches!(
                parse_command(&format!("host clock-benchmark\n{name}\n")),
                Ok(ControlCommand::ClockBenchmark(case)) if case == expected
            ));
        }
        for body in [
            "host clock-benchmark\n",
            "host clock-benchmark\nfalling\n",
            "host clock-benchmark\nRAIN\n",
            "host clock-benchmark\nrain\nstatus\n",
        ] {
            assert!(parse_command(body).is_err(), "{body:?}");
        }
    }

    #[test]
    fn parse_plan_control_rejects_missing_invalid_or_extra_input() {
        for mode in [
            crate::gpu_scene::PlanMode::Reference,
            crate::gpu_scene::PlanMode::Reserved,
            crate::gpu_scene::PlanMode::Scratch,
            crate::gpu_scene::PlanMode::Prepared,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-plan\n{}\n", mode.name())),
                Ok(ControlCommand::GpuPlan(actual)) if actual == mode)
            );
        }
        for command in [
            "host gpu-plan\n",
            "host gpu-plan\non\n",
            "host gpu-plan\nreserved\nstatus\n",
        ] {
            assert!(parse_command(command).is_err());
        }
    }

    #[test]
    fn parse_source_control_is_explicit_and_bounded() {
        for mode in [
            crate::gpu_scene::SourceMode::Reference,
            crate::gpu_scene::SourceMode::Pooled,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-source\n{}\n", mode.name())), Ok(ControlCommand::GpuSource(actual)) if actual == mode)
            );
        }
        for body in [
            "host gpu-source\n",
            "host gpu-source\non\n",
            "host gpu-source\npooled\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn parse_stroke_control_is_explicit_and_bounded() {
        for mode in [
            crate::gpu_scene::StrokeMode::Off,
            crate::gpu_scene::StrokeMode::Conservative,
            crate::gpu_scene::StrokeMode::Checked,
        ] {
            assert!(matches!(
                parse_command(&format!("host gpu-strokes\n{}\n", mode.name())),
                Ok(ControlCommand::GpuStrokes(actual)) if actual == mode
            ));
        }
        for body in [
            "host gpu-strokes\n",
            "host gpu-strokes\non\n",
            "host gpu-strokes\nconservative\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn parse_join_control_is_explicit_and_bounded() {
        for mode in [
            crate::gpu_scene::JoinMode::Reference,
            crate::gpu_scene::JoinMode::Exact,
            crate::gpu_scene::JoinMode::Checked,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-joins\n{}\n", mode.name())),
                Ok(ControlCommand::GpuJoins(actual)) if actual == mode)
            );
        }
        for body in [
            "host gpu-joins\n",
            "host gpu-joins\non\n",
            "host gpu-joins\nexact\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn parse_stroke_writer_control_is_explicit_and_bounded() {
        for mode in [
            crate::gpu_scene::StrokeWriter::Stock,
            crate::gpu_scene::StrokeWriter::Bulk,
            crate::gpu_scene::StrokeWriter::BulkStatic,
            crate::gpu_scene::StrokeWriter::BulkPolygon,
            crate::gpu_scene::StrokeWriter::BulkEdges,
            crate::gpu_scene::StrokeWriter::BulkEdgesChecked,
            crate::gpu_scene::StrokeWriter::BulkPrepared,
            crate::gpu_scene::StrokeWriter::BulkPreparedChecked,
            crate::gpu_scene::StrokeWriter::BulkSinglePass,
            crate::gpu_scene::StrokeWriter::BulkSinglePassChecked,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-stroke-writer\n{}\n", mode.name())),
                Ok(ControlCommand::GpuStrokeWriter(actual)) if actual == mode)
            );
        }
        for body in [
            "host gpu-stroke-writer\n",
            "host gpu-stroke-writer\non\n",
            "host gpu-stroke-writer\nbulk\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn parse_admission_control_is_explicit_and_bounded() {
        for mode in [
            crate::gpu_scene::AdmissionMode::Repeated,
            crate::gpu_scene::AdmissionMode::Settled,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-admission\n{}\n", mode.name())),
                Ok(ControlCommand::GpuAdmission(actual)) if actual == mode)
            );
        }
        for body in [
            "host gpu-admission\n",
            "host gpu-admission\non\n",
            "host gpu-admission\nsettled\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn parse_screenshot_command() {
        let command = parse_command("screenshot\n/tmp/shot.png\n").unwrap();
        match command {
            ControlCommand::Screenshot { output } => {
                assert_eq!(output, PathBuf::from("/tmp/shot.png"));
            }
            ControlCommand::Status
            | ControlCommand::Health(_)
            | ControlCommand::InputPress(_)
            | ControlCommand::UiState
            | ControlCommand::UiPress(_)
            | ControlCommand::UiActivate(_)
            | ControlCommand::HostPause(_)
            | ControlCommand::ClockState
            | ControlCommand::ClockTrigger(_)
            | ControlCommand::ClockMessage(_)
            | ControlCommand::HostBenchmark
            | ControlCommand::ClockBenchmark(_)
            | ControlCommand::RasterTextures(_)
            | ControlCommand::GpuScene(_)
            | ControlCommand::GpuStream(_)
            | ControlCommand::GpuStrokes(_)
            | ControlCommand::GpuJoins(_)
            | ControlCommand::GpuStrokeWriter(_)
            | ControlCommand::GpuAdmission(_)
            | ControlCommand::GpuPlan(_)
            | ControlCommand::GpuSource(_)
            | ControlCommand::GpuText(_)
            | ControlCommand::CameraZoom(_)
            | ControlCommand::RenderTrace(_)
            | ControlCommand::RasterUpload(_) => {
                panic!("expected screenshot command")
            }
        }
    }

    #[test]
    fn input_press_parser_requires_named_bounded_guarded_request() {
        let payload = r#"{"schema_version":1,"player":1,"button":"west","profile":"picade","hold_ms":120,"expected_screen":"gameplay","expected_revision":4,"expected_scenario_revision":2}"#;
        assert!(matches!(
            parse_command(&format!("input press\n{payload}\n")).unwrap(),
            ControlCommand::InputPress(_)
        ));
        for body in [
            "input press\n".into(),
            format!("input press\n{payload}\nextra\n"),
            format!("input press\n{}\n", payload.replace("120", "2001")),
            format!("input press\n{}\n", payload.replace("west", "power")),
        ] {
            assert!(matches!(
                parse_command(&body),
                Err(CommandParseError::Structured(_))
            ));
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
    fn health_requires_a_single_versioned_challenge() {
        assert!(matches!(
            parse_command("health\n{\"schema_version\":1,\"nonce\":42}\n"),
            Ok(ControlCommand::Health(spacewars_control::HealthRequest {
                nonce: 42,
                ..
            }))
        ));
        for body in [
            "health\n",
            "health\n{\"schema_version\":2,\"nonce\":42}\n",
            "health\n{\"schema_version\":1,\"nonce\":42}\nextra\n",
            "health\n{\"schema_version\":1,\"nonce\":42,\"healthy\":true}\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn health_responses_advance_and_keep_the_callers_challenge() {
        let first = health_state(42);
        let second = health_state(43);
        assert_eq!(first.nonce, 42);
        assert_eq!(second.nonce, 43);
        assert_eq!(first.process_id, std::process::id());
        assert_eq!(first.build_revision, env!("SPACEWARS_BUILD_REVISION"));
        assert_eq!(second.ui_responses, first.ui_responses + 1);
        // Unit tests have not activated a DRM output. UI responses alone are
        // deliberately insufficient to confirm presentation health.
        assert_eq!(first.presented_frames, None);
    }

    #[test]
    fn clock_commands_validate_schema_and_required_guards() {
        assert!(matches!(
            parse_command("clock state\n"),
            Ok(ControlCommand::ClockState)
        ));
        let request = ClockTriggerRequest {
            schema_version: spacewars_control::CLOCK_STATE_SCHEMA_VERSION,
            event: engine_common::ClockEventKind::Falling,
            expected_scenario_revision: 9,
            expected_event_id: 2,
        };
        assert!(
            matches!(parse_command(&format!("clock trigger\n{}\n", request.to_json().unwrap())), Ok(ControlCommand::ClockTrigger(parsed)) if parsed == request)
        );
        for body in [
            "clock state\nextra\n",
            "clock trigger\n",
            "clock trigger\n{}\n",
            "clock trigger\n{}\nextra\n",
            "clock trigger\n{\"schema_version\":2,\"expected_scenario_revision\":9,\"expected_event_id\":2}\n",
            "clock message\n",
            "clock message\n{}\n",
            "clock message\n{}\nextra\n",
        ] {
            assert!(
                matches!(parse_command(body), Err(CommandParseError::Structured(failure)) if failure.code == ControlFailureCode::InvalidRequest)
            );
        }
        let request = ClockMessageRequest {
            schema_version: spacewars_control::CLOCK_STATE_SCHEMA_VERSION,
            message: "HELLO!".parse().unwrap(),
            expected_scenario_revision: 9,
            expected_message: engine_common::ClockMarqueeMessage::default(),
        };
        assert!(
            matches!(parse_command(&format!("clock message\n{}\n", request.to_json().unwrap())), Ok(ControlCommand::ClockMessage(parsed)) if parsed == request)
        );
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
    fn parse_ui_activate_command_and_reject_invalid_payloads_structurally() {
        let request = UiActivateRequest {
            schema_version: UI_STATE_SCHEMA_VERSION,
            control_id: "launcher.settings".into(),
            expected_screen: Some(UiScreen::LauncherMain),
            expected_revision: Some(12),
        };
        let body = format!("ui activate\n{}\n", request.to_json().unwrap());
        assert!(matches!(
            parse_command(&body),
            Ok(ControlCommand::UiActivate(parsed)) if parsed == request
        ));

        for body in [
            "ui activate\n",
            "ui activate\nnot-json\n",
            "ui activate\n{}\nextra\n",
        ] {
            assert!(matches!(
                parse_command(body),
                Err(CommandParseError::Structured(failure))
                    if failure.code == ControlFailureCode::InvalidRequest
            ));
        }
    }

    #[test]
    fn parse_host_pause_command_and_reject_invalid_payloads_structurally() {
        let request = HostPauseRequest {
            schema_version: UI_STATE_SCHEMA_VERSION,
            expected_screen: Some(UiScreen::Gameplay),
            expected_revision: Some(12),
        };
        let body = format!("host pause\n{}\n", request.to_json().unwrap());
        assert!(matches!(
            parse_command(&body),
            Ok(ControlCommand::HostPause(parsed)) if parsed == request
        ));

        for body in [
            "host pause\n",
            "host pause\nnot-json\n",
            "host pause\n{}\nextra\n",
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
    fn parse_raster_upload_is_strict_and_never_ignores_extra_lines() {
        use crate::raster::publication::UploadFormat;
        for (name, expected) in [("rgb", UploadFormat::Rgb), ("rgba", UploadFormat::Rgba)] {
            assert!(matches!(
                parse_command(&format!("host raster-upload\n{name}\n")),
                Ok(ControlCommand::RasterUpload(format)) if format == expected
            ));
        }
        for body in [
            "host raster-upload\n",
            "host raster-upload\nbgra\n",
            "host raster-upload\nrgba\nextra\n",
            "host raster-upload\nrgba rgb\n",
        ] {
            assert!(matches!(
                parse_command(body),
                Err(CommandParseError::Structured(_))
            ));
        }
    }

    #[test]
    fn texture_lifetime_parser_rejects_unknown_modes_and_extra_lines() {
        for name in ["slint", "fresh", "single", "triple"] {
            assert!(
                matches!(parse_command(&format!("host raster-textures\n{name}\n")),
                Ok(ControlCommand::RasterTextures(mode)) if mode.name() == name)
            );
        }
        for body in [
            "host raster-textures\n",
            "host raster-textures\ndouble\n",
            "host raster-textures\nsingle\nstatus\n",
        ] {
            assert!(matches!(
                parse_command(body),
                Err(CommandParseError::Structured(_))
            ));
        }
    }

    #[test]
    fn direct_scene_parser_is_explicit_and_strict() {
        for (name, mode) in [
            ("triangles", crate::gpu_scene::Mode::Triangles),
            ("paths", crate::gpu_scene::Mode::Paths),
            ("circles", crate::gpu_scene::Mode::Circles),
            ("retained", crate::gpu_scene::Mode::Retained),
            ("retained-cold", crate::gpu_scene::Mode::RetainedCold),
            (
                "retained-adaptive",
                crate::gpu_scene::Mode::RetainedAdaptive,
            ),
            ("retained-zoom", crate::gpu_scene::Mode::RetainedZoom),
            ("retained-hybrid", crate::gpu_scene::Mode::RetainedHybrid),
            (
                "retained-outlines",
                crate::gpu_scene::Mode::RetainedOutlines,
            ),
            ("retained-paced", crate::gpu_scene::Mode::RetainedPaced),
            ("retained-scan", crate::gpu_scene::Mode::RetainedScan),
            ("retained-ranges", crate::gpu_scene::Mode::RetainedRanges),
            ("retained-culled", crate::gpu_scene::Mode::RetainedCulled),
            (
                "retained-resident",
                crate::gpu_scene::Mode::RetainedResident,
            ),
            (
                "retained-resident-passes",
                crate::gpu_scene::Mode::RetainedResidentPasses,
            ),
            (
                "retained-resident-buffers",
                crate::gpu_scene::Mode::RetainedResidentBuffers,
            ),
            (
                "retained-resident-uniforms",
                crate::gpu_scene::Mode::RetainedResidentUniforms,
            ),
            (
                "retained-memoized",
                crate::gpu_scene::Mode::RetainedMemoized,
            ),
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-scene\n{name}\n")), Ok(ControlCommand::GpuScene(actual)) if actual == mode)
            );
        }
        assert!(matches!(
            parse_command("host gpu-scene\non\n"),
            Ok(ControlCommand::GpuScene(crate::gpu_scene::Mode::Triangles))
        ));
        assert!(matches!(
            parse_command("host gpu-scene\noff\n"),
            Ok(ControlCommand::GpuScene(crate::gpu_scene::Mode::Off))
        ));
        for body in [
            "host gpu-scene\n",
            "host gpu-scene\ntrue\n",
            "host gpu-scene\non\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn render_trace_parser_rejects_implicit_or_extra_commands() {
        for command in [
            "start",
            "start-text",
            "start-work",
            "start-geometry",
            "read",
            "cancel",
        ] {
            assert!(
                matches!(parse_command(&format!("host render-trace\n{command}\n")),
                Ok(ControlCommand::RenderTrace(parsed)) if parsed == command)
            );
        }
        for body in [
            "host render-trace\n",
            "host render-trace\non\n",
            "host render-trace\nstart\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn gpu_text_parser_requires_one_explicit_policy() {
        for name in ["vector", "cpu", "cpu-run-faces", "cpu-shape-faces"] {
            assert!(matches!(parse_command(&format!("host gpu-text\n{name}\n")),
                Ok(ControlCommand::GpuText(mode)) if mode.name() == name));
        }
        for body in [
            "host gpu-text\n",
            "host gpu-text\ngpu\n",
            "host gpu-text\ncpu\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn camera_zoom_parser_accepts_only_one_explicit_mode_and_legacy_alias() {
        for command in ["host camera-zoom", "host gpu-zoom"] {
            for mode in [
                crate::camera_probe::ZoomMode::Off,
                crate::camera_probe::ZoomMode::Sweep,
                crate::camera_probe::ZoomMode::SweepStop,
            ] {
                assert!(
                    matches!(parse_command(&format!("{command}\n{}\n", mode.name())),
                    Ok(ControlCommand::CameraZoom(actual)) if actual == mode)
                );
            }
            for value in ["", "on", "sweep\nstatus"] {
                assert!(parse_command(&format!("{command}\n{value}\n")).is_err());
            }
        }
    }

    #[test]
    fn gpu_stream_parser_accepts_only_one_explicit_policy() {
        for mode in [
            crate::gpu_scene::StreamMode::Shared,
            crate::gpu_scene::StreamMode::Exact,
            crate::gpu_scene::StreamMode::Bucketed,
        ] {
            assert!(
                matches!(parse_command(&format!("host gpu-stream\n{}\n", mode.name())), Ok(ControlCommand::GpuStream(actual)) if actual == mode)
            );
        }
        for body in [
            "host gpu-stream\n",
            "host gpu-stream\non\n",
            "host gpu-stream\nbucketed\nstatus\n",
        ] {
            assert!(parse_command(body).is_err());
        }
    }

    #[test]
    fn gpu_scene_allows_split_raster_scenes_but_not_vector_or_scaled_sources() {
        use crate::gpu_scene::Mode;
        for scenario in ["spacewars", "clock", "pizza"] {
            let source = format!("scenario={scenario}\nrenderer=raster\nraster_scale=1.00");
            assert!(gpu_scene_source_supported(Mode::Triangles, &source));
            assert!(gpu_scene_source_supported(Mode::Circles, &source));
            assert_eq!(
                gpu_scene_source_supported(Mode::Paths, &source),
                scenario == "clock"
            );
            assert!(!gpu_scene_source_supported(
                Mode::Triangles,
                &source.replace("renderer=raster", "renderer=vector")
            ));
            assert!(!gpu_scene_source_supported(
                Mode::Triangles,
                &source.replace("1.00", "2.00")
            ));
        }
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
    fn launcher_revision_tracks_visible_training_state() {
        use engine_agent::training_job::{JobSnapshot, JobState};
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, PlatformError, WindowAdapter};
        use std::rc::Rc;

        struct TestPlatform;
        impl Platform for TestPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
            }
        }
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        window.set_launcher_visible(true);
        let mut tracker = UiStateTracker::default();
        let mut snapshot = JobSnapshot::default();
        crate::training_controls::publish(&window, &snapshot);
        let loading = ui_state(&window, &mut tracker).unwrap();
        snapshot.state = JobState::Ready;
        crate::training_controls::publish(&window, &snapshot);
        assert_eq!(ui_state(&window, &mut tracker).unwrap(), loading);

        snapshot.state = JobState::Running;
        snapshot.run_id = Some("run-1-1".into());
        crate::training_controls::publish(&window, &snapshot);
        let active = ui_state(&window, &mut tracker).unwrap();
        assert_eq!(active.revision, loading.revision + 1);
        assert_eq!(
            active
                .controls
                .iter()
                .find(|c| c.id == "launcher.training")
                .unwrap()
                .label,
            "Training · Active"
        );
        for state in [JobState::Saving, JobState::Validating, JobState::Running] {
            snapshot.state = state;
            crate::training_controls::publish(&window, &snapshot);
            assert_eq!(ui_state(&window, &mut tracker).unwrap(), active);
        }
        snapshot.state = JobState::Paused;
        snapshot.can_resume = true;
        crate::training_controls::publish(&window, &snapshot);
        let paused = ui_state(&window, &mut tracker).unwrap();
        assert_eq!(paused.revision, active.revision + 1);
        assert_eq!(
            paused
                .controls
                .iter()
                .find(|c| c.id == "launcher.training")
                .unwrap()
                .label,
            "Training · Saved run"
        );
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
    fn ui_activate_validation_checks_preconditions_visibility_and_enabled_state() {
        let mut state = UiState {
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
        let mut request = UiActivateRequest::new("launcher.start");
        request.expected_screen = Some(UiScreen::LauncherSettings);
        request.expected_revision = Some(6);
        assert_eq!(
            validate_ui_activate(&request, &state).unwrap_err().code,
            ControlFailureCode::WrongScreen
        );

        request.expected_screen = Some(UiScreen::LauncherMain);
        assert_eq!(
            validate_ui_activate(&request, &state).unwrap_err().code,
            ControlFailureCode::StaleRevision
        );

        request.expected_revision = Some(7);
        request.control_id = "launcher.missing".into();
        let unavailable = validate_ui_activate(&request, &state).unwrap_err();
        assert_eq!(unavailable.code, ControlFailureCode::ControlUnavailable);
        assert_eq!(unavailable.current_state, Some(state.clone()));

        request.control_id = "launcher.start".into();
        state.controls[0].enabled = false;
        assert_eq!(
            validate_ui_activate(&request, &state).unwrap_err().code,
            ControlFailureCode::ControlDisabled
        );

        state.controls[0].enabled = true;
        assert_eq!(validate_ui_activate(&request, &state), Ok(()));
    }

    #[test]
    fn host_pause_validation_requires_current_gameplay() {
        let mut state = UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 7,
            screen: UiScreen::LauncherMain,
            active_scenario: None,
            selected_scenario: "spacewars".into(),
            selected_control: Some("launcher.start".into()),
            controls: vec![UiControl::new("launcher.start", "Start Game", true)],
            actions: UiAction::ALL.to_vec(),
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
            error: None,
        };
        let mut request = HostPauseRequest::new();
        request.expected_screen = Some(UiScreen::Gameplay);
        request.expected_revision = Some(7);
        assert_eq!(
            validate_host_pause(&request, &state).unwrap_err().code,
            ControlFailureCode::WrongScreen
        );

        state.screen = UiScreen::Gameplay;
        state.active_scenario = Some("spacewars".into());
        state.selected_control = None;
        state.controls.clear();
        state.actions.clear();
        assert_eq!(validate_host_pause(&request, &state), Ok(()));

        request.expected_revision = Some(6);
        assert_eq!(
            validate_host_pause(&request, &state).unwrap_err().code,
            ControlFailureCode::StaleRevision
        );
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
