#![cfg(unix)]

use std::any::Any;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};
use spacewars_control::{
    ControlClient, ControlClientError, ControlFailure, ControlFailureCode, HostPauseRequest,
    UiAction, UiActivateRequest, UiPressRequest, UiScreen, UiState, UiStatePredicate,
};
use tempfile::{Builder, TempDir};

const ARTIFACT_REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const READINESS_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
const TRANSITION_TIMEOUT: Duration = Duration::from_secs(10);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn launcher_navigation_uses_the_public_control_api() {
    run_functional_test("launcher-navigation", |harness| {
        let mut state = harness.wait_until_ready();
        assert_launcher_main(&state);

        let mut reached = BTreeSet::from([selected_control(&state)]);
        for action in [
            UiAction::Down,
            UiAction::Right,
            UiAction::Down,
            UiAction::Left,
            UiAction::Up,
            UiAction::Up,
        ] {
            state = harness.press_guarded(action, &state);
            reached.insert(selected_control(&state));
        }
        assert_eq!(
            reached,
            BTreeSet::from([
                "launcher.controls".into(),
                "launcher.quit".into(),
                "launcher.scenario".into(),
                "launcher.settings".into(),
                "launcher.start".into(),
            ])
        );
        assert_eq!(selected_control(&state), "launcher.scenario");

        state = harness.activate_guarded("launcher.settings", &state);
        assert_eq!(state.screen, UiScreen::LauncherSettings);
        assert_eq!(
            control_ids(&state),
            [
                "launcher.settings.renderer.previous",
                "launcher.settings.renderer.next",
                "launcher.settings.raster-scale.previous",
                "launcher.settings.raster-scale.next",
                "launcher.settings.spacewars.preset.previous",
                "launcher.settings.spacewars.preset.next",
                "launcher.settings.spacewars.planets.previous",
                "launcher.settings.spacewars.planets.next",
                "launcher.settings.spacewars.asteroids.previous",
                "launcher.settings.spacewars.asteroids.next",
                "launcher.settings.spacewars.player-health.previous",
                "launcher.settings.spacewars.player-health.next",
                "launcher.settings.spacewars.player-2.previous",
                "launcher.settings.spacewars.player-2.next",
                "launcher.settings.back",
                "launcher.settings.start",
            ]
        );
        assert_eq!(state.actions, UiAction::ALL);

        let renderer_before = control_value(&state, "launcher.settings.renderer.next");
        let adjusted = harness.activate_guarded("launcher.settings.renderer.next", &state);
        assert!(adjusted.revision > state.revision);
        assert_ne!(
            control_value(&adjusted, "launcher.settings.renderer.next"),
            renderer_before
        );
        state = harness.activate_guarded("launcher.settings.back", &adjusted);
        assert_eq!(state.screen, UiScreen::LauncherMain);

        state = harness.activate_guarded("launcher.controls", &state);
        assert_eq!(state.screen, UiScreen::LauncherControls);
        assert_eq!(
            control_ids(&state),
            [
                "launcher.controls.back",
                "launcher.controls.touch-test",
                "launcher.controls.start",
            ]
        );

        state = harness.activate_guarded("launcher.controls.touch-test", &state);
        assert_eq!(state.screen, UiScreen::LauncherTouchTest);
        assert_eq!(control_ids(&state), ["launcher.touch-test.done"]);
        assert_eq!(state.actions, [UiAction::Back, UiAction::Controls]);

        let unavailable = harness.expect_press_failure(
            UiAction::Up,
            Some(UiScreen::LauncherTouchTest),
            Some(state.revision),
        );
        assert_eq!(unavailable.code, ControlFailureCode::ActionUnavailable);
        assert_eq!(unavailable.current_state.as_ref(), Some(&state));

        state = harness.activate_guarded("launcher.touch-test.done", &state);
        assert_eq!(state.screen, UiScreen::LauncherControls);
        state = harness.activate_guarded("launcher.controls.back", &state);
        assert_eq!(state.screen, UiScreen::LauncherMain);

        let wrong_screen = harness.expect_activate_failure(
            "launcher.settings",
            Some(UiScreen::PauseMain),
            Some(state.revision),
        );
        assert_eq!(wrong_screen.code, ControlFailureCode::WrongScreen);
        assert_eq!(wrong_screen.current_state.as_ref(), Some(&state));

        let stale = harness.expect_activate_failure(
            "launcher.settings",
            Some(UiScreen::LauncherMain),
            Some(state.revision.saturating_add(1)),
        );
        assert_eq!(stale.code, ControlFailureCode::StaleRevision);
        assert_eq!(stale.current_state.as_ref(), Some(&state));

        let unavailable = harness.expect_activate_failure(
            "launcher.missing",
            Some(UiScreen::LauncherMain),
            Some(state.revision),
        );
        assert_eq!(unavailable.code, ControlFailureCode::ControlUnavailable);
        assert_eq!(unavailable.current_state.as_ref(), Some(&state));

        let unavailable = harness.expect_press_failure(
            UiAction::Back,
            Some(UiScreen::LauncherMain),
            Some(state.revision),
        );
        assert_eq!(unavailable.code, ControlFailureCode::ActionUnavailable);
        assert_eq!(unavailable.current_state.as_ref(), Some(&state));

        let original_scenario = state.selected_scenario.clone();
        state = harness.activate_until_scenario("nes", state);
        assert!(!control_enabled(&state, "launcher.start"));
        let disabled = harness.expect_activate_failure(
            "launcher.start",
            Some(UiScreen::LauncherMain),
            Some(state.revision),
        );
        assert_eq!(disabled.code, ControlFailureCode::ControlDisabled);
        assert_eq!(disabled.current_state.as_ref(), Some(&state));
        assert_eq!(harness.state(), state);

        state = harness.activate_until_scenario(&original_scenario, state);
        assert!(control_enabled(&state, "launcher.start"));
        assert_eq!(harness.state(), state);
        harness.capture_screenshot("launcher.png");
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn launcher_can_run_the_spacewars_menu_lifecycle() {
    run_functional_test("launcher-spacewars-menu-lifecycle", |harness| {
        let mut state = harness.wait_until_ready();
        assert_launcher_main(&state);

        let unavailable = harness.expect_pause_failure(&state);
        assert_eq!(unavailable.code, ControlFailureCode::WrongScreen);
        assert_eq!(unavailable.current_state.as_ref(), Some(&state));
        assert_eq!(harness.state(), state);

        let launcher_revision = state.revision;
        harness.activate_guarded("launcher.start", &state);

        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                scenario: Some("spacewars".into()),
                revision_after: Some(launcher_revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert_eq!(state.screen, UiScreen::Gameplay);
        assert_eq!(state.active_scenario.as_deref(), Some("spacewars"));
        assert_eq!(state.selected_scenario, "spacewars");
        let gameplay_scenario_revision = state
            .scenario_revision
            .expect("gameplay must report a scenario revision");
        assert!(!state.paused);
        assert!(!state.benchmark_active);
        assert!(state.controls.is_empty());
        assert!(state.actions.is_empty());
        harness.capture_screenshot("gameplay.png");

        let pause_requested = harness.pause_guarded(&state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                scenario: Some("spacewars".into()),
                revision_after: Some(pause_requested.revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert!(state.paused);
        assert!(!state.benchmark_active);
        assert_eq!(
            control_ids(&state),
            [
                "pause.resume",
                "pause.restart",
                "pause.benchmark",
                "pause.controls",
                "pause.return-to-launcher",
            ]
        );
        harness.capture_screenshot("pause.png");

        let benchmark_requested = harness.activate_guarded("pause.benchmark", &state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                scenario: Some("spacewars".into()),
                revision_after: Some(benchmark_requested.revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert!(!state.paused);
        assert!(state.benchmark_active);
        let benchmark_scenario_revision = state
            .scenario_revision
            .expect("benchmark must report a scenario revision");
        assert_ne!(benchmark_scenario_revision, gameplay_scenario_revision);
        harness.capture_screenshot("benchmark.png");

        let pause_requested = harness.pause_guarded(&state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                scenario: Some("spacewars".into()),
                revision_after: Some(pause_requested.revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert!(state.paused);
        assert!(state.benchmark_active);

        let restart_requested = harness.activate_guarded("pause.restart", &state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                scenario: Some("spacewars".into()),
                revision_after: Some(restart_requested.revision),
            },
            TRANSITION_TIMEOUT,
        );
        let restarted_scenario_revision = state
            .scenario_revision
            .expect("restarted gameplay must report a scenario revision");
        assert_ne!(restarted_scenario_revision, benchmark_scenario_revision);
        assert!(!state.paused);
        assert!(!state.benchmark_active);
        assert!(state.controls.is_empty());
        assert!(state.actions.is_empty());
        harness.capture_screenshot("restarted-gameplay.png");

        let pause_requested = harness.pause_guarded(&state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                scenario: Some("spacewars".into()),
                revision_after: Some(pause_requested.revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert!(state.paused);
        assert!(!state.benchmark_active);

        let return_from_revision = state.revision;
        harness.activate_guarded("pause.return-to-launcher", &state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::LauncherMain),
                scenario: None,
                revision_after: Some(return_from_revision),
            },
            TRANSITION_TIMEOUT,
        );
        assert_launcher_main(&state);
        assert_eq!(state.scenario_revision, None);
        assert!(!state.paused);
        assert!(!state.benchmark_active);
        assert_eq!(state.error, None);
        harness.capture_screenshot("returned-launcher.png");

        let returned_launcher_revision = state.revision;
        harness.activate_guarded("launcher.start", &state);
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                scenario: Some("spacewars".into()),
                revision_after: Some(returned_launcher_revision),
            },
            TRANSITION_TIMEOUT,
        );
        let relaunched_scenario_revision = state
            .scenario_revision
            .expect("relaunched gameplay must report a scenario revision");
        assert_ne!(relaunched_scenario_revision, restarted_scenario_revision);
        assert!(!state.paused);
        assert!(!state.benchmark_active);
        assert!(state.controls.is_empty());
        assert!(state.actions.is_empty());
        harness.capture_screenshot("relaunched-gameplay.png");
    });
}

fn run_functional_test(name: &'static str, workflow: impl FnOnce(&mut FunctionalHarness)) {
    let mut harness = FunctionalHarness::spawn(name)
        .unwrap_or_else(|error| panic!("could not start {name}: {error}"));
    let result = catch_unwind(AssertUnwindSafe(|| workflow(&mut harness)));
    if let Err(payload) = result {
        harness.failure = Some(panic_message(payload.as_ref()));
        drop(harness);
        resume_unwind(payload);
    }
}

struct FunctionalHarness {
    test_name: &'static str,
    started_at: Instant,
    run_directory: Option<TempDir>,
    _socket_path: OwnedSocketPath,
    client: ControlClient,
    child: OwnedChild,
    history: Vec<Value>,
    last_state: Option<UiState>,
    failure: Option<String>,
}

impl FunctionalHarness {
    fn spawn(test_name: &'static str) -> Result<Self, String> {
        let artifact_root = workspace_root().join("target/functional-test-artifacts");
        fs::create_dir_all(&artifact_root).map_err(|error| {
            format!(
                "could not create artifact root {}: {error}",
                artifact_root.display()
            )
        })?;
        let run_directory = Builder::new()
            .prefix(&format!("{test_name}-"))
            .tempdir_in(&artifact_root)
            .map_err(|error| format!("could not create test directory: {error}"))?;
        let config_directory = run_directory.path().join("config");
        fs::create_dir(&config_directory)
            .map_err(|error| format!("could not create isolated config directory: {error}"))?;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
            .as_nanos();
        let socket_path = std::env::temp_dir().join(format!(
            "spacewars-functional-{}-{nonce}.sock",
            std::process::id()
        ));
        let log_path = run_directory.path().join("engine-client.log");
        let log = File::create(&log_path)
            .map_err(|error| format!("could not create {}: {error}", log_path.display()))?;
        let stderr_log = log
            .try_clone()
            .map_err(|error| format!("could not clone engine log handle: {error}"))?;

        let arguments = [
            "--config-dir".to_string(),
            config_directory.display().to_string(),
            "--seed".into(),
            "4242".into(),
            "--renderer".into(),
            "raster".into(),
            "--raster-scale".into(),
            "1.0".into(),
        ];
        let mut command = Command::new(env!("CARGO_BIN_EXE_engine-client"));
        command
            .args(&arguments)
            .current_dir(workspace_root())
            .env("RUST_LOG", "info")
            .env("SLINT_BACKEND", "winit-software")
            .env("SPACEWARS_CONTROL_SOCKET", &socket_path)
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr_log));
        let child = OwnedChild(
            command
                .spawn()
                .map_err(|error| format!("could not spawn engine-client: {error}"))?,
        );

        Ok(Self {
            test_name,
            started_at: Instant::now(),
            run_directory: Some(run_directory),
            _socket_path: OwnedSocketPath(socket_path.clone()),
            client: ControlClient::new(socket_path),
            child,
            history: vec![json!({
                "elapsed_ms": 0,
                "command": "spawn engine-client",
                "arguments": arguments,
            })],
            last_state: None,
            failure: None,
        })
    }

    fn wait_until_ready(&mut self) -> UiState {
        let deadline = Instant::now()
            .checked_add(READINESS_TIMEOUT)
            .expect("readiness deadline must fit in Instant");
        let mut last_error = None;

        loop {
            match self.child.0.try_wait() {
                Ok(Some(status)) => {
                    panic!("engine-client exited before readiness with {status}");
                }
                Ok(None) => {}
                Err(error) => panic!("could not inspect engine-client: {error}"),
            }

            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "timed out after {READINESS_TIMEOUT:?} waiting for engine-client readiness; last error: {}",
                    last_error.as_deref().unwrap_or("none")
                );
            }
            let request_deadline = deadline.min(
                now.checked_add(REQUEST_TIMEOUT)
                    .expect("request deadline must fit in Instant"),
            );
            match self.client.ui_state_before(request_deadline) {
                Ok(state) => {
                    self.record_state("ui state (readiness)", &state);
                    return state;
                }
                Err(error) if is_retryable_readiness_error(&error) => {
                    last_error = Some(error.to_string());
                }
                Err(error) => {
                    self.record_error("ui state (readiness)", &error);
                    panic!("engine-client readiness failed: {error}");
                }
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
        }
    }

    fn state(&mut self) -> UiState {
        let result = self.client.ui_state_before(request_deadline());
        self.require_state_result("ui state", result)
    }

    fn press_guarded(&mut self, action: UiAction, state: &UiState) -> UiState {
        self.press(action, Some(state.screen), Some(state.revision))
    }

    fn press(
        &mut self,
        action: UiAction,
        expected_screen: Option<UiScreen>,
        expected_revision: Option<u64>,
    ) -> UiState {
        let request = press_request(action, expected_screen, expected_revision);
        let command = format!(
            "ui press {action} --expect-screen={} --expect-revision={}",
            optional_screen(expected_screen),
            optional_revision(expected_revision)
        );
        let result = self.client.ui_press_before(&request, request_deadline());
        self.require_state_result(&command, result)
    }

    fn activate_guarded(&mut self, control_id: &str, state: &UiState) -> UiState {
        self.activate(control_id, Some(state.screen), Some(state.revision))
    }

    fn activate(
        &mut self,
        control_id: &str,
        expected_screen: Option<UiScreen>,
        expected_revision: Option<u64>,
    ) -> UiState {
        let request = activate_request(control_id, expected_screen, expected_revision);
        let command = format!(
            "ui activate {control_id:?} --expect-screen={} --expect-revision={}",
            optional_screen(expected_screen),
            optional_revision(expected_revision)
        );
        let result = self.client.ui_activate_before(&request, request_deadline());
        self.require_state_result(&command, result)
    }

    fn activate_until_scenario(&mut self, target: &str, mut state: UiState) -> UiState {
        if state.selected_scenario == target {
            return state;
        }
        let mut visited = BTreeSet::from([state.selected_scenario.clone()]);
        loop {
            state = self.activate_guarded("launcher.scenario.next", &state);
            if state.selected_scenario == target {
                return state;
            }
            assert!(
                visited.insert(state.selected_scenario.clone()),
                "scenario {target:?} is not reachable through launcher.scenario.next; visited {visited:?}"
            );
        }
    }

    fn pause_guarded(&mut self, state: &UiState) -> UiState {
        let request = pause_request(Some(state.screen), Some(state.revision));
        let command = format!(
            "host pause --expect-screen={} --expect-revision={}",
            state.screen, state.revision
        );
        let result = self.client.host_pause_before(&request, request_deadline());
        self.require_state_result(&command, result)
    }

    fn expect_pause_failure(&mut self, state: &UiState) -> ControlFailure {
        let request = pause_request(Some(state.screen), Some(state.revision));
        let command = format!(
            "host pause --expect-screen={} --expect-revision={}",
            state.screen, state.revision
        );
        match self.client.host_pause_before(&request, request_deadline()) {
            Err(ControlClientError::Failure(failure)) => {
                if let Some(state) = &failure.current_state {
                    self.last_state = Some(state.clone());
                }
                self.history.push(json!({
                    "elapsed_ms": self.elapsed_ms(),
                    "command": command,
                    "outcome": "rejected",
                    "failure": failure,
                }));
                *failure
            }
            Ok(state) => {
                self.record_state(&command, &state);
                panic!("{command} succeeded, but a structured rejection was expected");
            }
            Err(error) => {
                self.record_error(&command, &error);
                panic!("{command} returned an unexpected error: {error}");
            }
        }
    }

    fn expect_press_failure(
        &mut self,
        action: UiAction,
        expected_screen: Option<UiScreen>,
        expected_revision: Option<u64>,
    ) -> ControlFailure {
        let request = press_request(action, expected_screen, expected_revision);
        let command = format!(
            "ui press {action} --expect-screen={} --expect-revision={}",
            optional_screen(expected_screen),
            optional_revision(expected_revision)
        );
        match self.client.ui_press_before(&request, request_deadline()) {
            Err(ControlClientError::Failure(failure)) => {
                if let Some(state) = &failure.current_state {
                    self.last_state = Some(state.clone());
                }
                self.history.push(json!({
                    "elapsed_ms": self.elapsed_ms(),
                    "command": command,
                    "outcome": "rejected",
                    "failure": failure,
                }));
                *failure
            }
            Ok(state) => {
                self.record_state(&command, &state);
                panic!("{command} succeeded, but a structured rejection was expected");
            }
            Err(error) => {
                self.record_error(&command, &error);
                panic!("{command} returned an unexpected error: {error}");
            }
        }
    }

    fn expect_activate_failure(
        &mut self,
        control_id: &str,
        expected_screen: Option<UiScreen>,
        expected_revision: Option<u64>,
    ) -> ControlFailure {
        let request = activate_request(control_id, expected_screen, expected_revision);
        let command = format!(
            "ui activate {control_id:?} --expect-screen={} --expect-revision={}",
            optional_screen(expected_screen),
            optional_revision(expected_revision)
        );
        match self.client.ui_activate_before(&request, request_deadline()) {
            Err(ControlClientError::Failure(failure)) => {
                if let Some(state) = &failure.current_state {
                    self.last_state = Some(state.clone());
                }
                self.history.push(json!({
                    "elapsed_ms": self.elapsed_ms(),
                    "command": command,
                    "outcome": "rejected",
                    "failure": failure,
                }));
                *failure
            }
            Ok(state) => {
                self.record_state(&command, &state);
                panic!("{command} succeeded, but a structured rejection was expected");
            }
            Err(error) => {
                self.record_error(&command, &error);
                panic!("{command} returned an unexpected error: {error}");
            }
        }
    }

    fn wait_for(&mut self, predicate: UiStatePredicate, timeout: Duration) -> UiState {
        let command = format!(
            "ui wait --screen={} --scenario={} --revision-after={} --timeout={timeout:?}",
            optional_screen(predicate.screen),
            predicate.scenario.as_deref().unwrap_or("none"),
            optional_revision(predicate.revision_after),
        );
        let result = self.client.wait_for_ui_state(&predicate, timeout);
        self.require_state_result(&command, result)
    }

    fn capture_screenshot(&mut self, name: &str) -> PathBuf {
        let path = self.run_path().join(name);
        let response = self.screenshot_request(&path, TRANSITION_TIMEOUT);
        match response {
            Ok(message) => {
                self.history.push(json!({
                    "elapsed_ms": self.elapsed_ms(),
                    "command": format!("screenshot {}", path.display()),
                    "outcome": "ok",
                    "response": message,
                }));
            }
            Err(error) => {
                self.record_error_message(
                    &format!("screenshot {}", path.display()),
                    &error.to_string(),
                    error.failure(),
                );
                panic!("screenshot capture failed: {error}");
            }
        }
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("could not read screenshot {}: {error}", path.display())
        });
        assert!(
            bytes.starts_with(PNG_SIGNATURE),
            "{} is not a PNG",
            path.display()
        );
        path
    }

    fn require_state_result(
        &mut self,
        command: &str,
        result: Result<UiState, ControlClientError>,
    ) -> UiState {
        match result {
            Ok(state) => {
                self.record_state(command, &state);
                state
            }
            Err(error) => {
                self.record_error(command, &error);
                panic!("{command} failed: {error}");
            }
        }
    }

    fn record_state(&mut self, command: &str, state: &UiState) {
        self.last_state = Some(state.clone());
        self.history.push(json!({
            "elapsed_ms": self.elapsed_ms(),
            "command": command,
            "outcome": "ok",
            "state": state,
        }));
    }

    fn record_error(&mut self, command: &str, error: &ControlClientError) {
        if let Some(state) = error
            .failure()
            .and_then(|failure| failure.current_state.as_ref())
        {
            self.last_state = Some(state.clone());
        }
        self.record_error_message(command, &error.to_string(), error.failure());
    }

    fn record_error_message(
        &mut self,
        command: &str,
        message: &str,
        failure: Option<&ControlFailure>,
    ) {
        self.history.push(json!({
            "elapsed_ms": self.elapsed_ms(),
            "command": command,
            "outcome": "error",
            "error": message,
            "failure": failure,
        }));
    }

    fn screenshot_request(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> Result<String, ControlClientError> {
        let output = path
            .to_str()
            .expect("functional-test paths must be valid UTF-8");
        let deadline = Instant::now()
            .checked_add(timeout)
            .expect("screenshot deadline must fit in Instant");
        self.client
            .request_before(&format!("screenshot\n{output}\n"), deadline)
    }

    fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }

    fn run_path(&self) -> &Path {
        self.run_directory
            .as_ref()
            .expect("run directory must exist until harness cleanup")
            .path()
    }

    fn capture_live_failure_artifacts(&mut self) {
        let deadline = Instant::now()
            .checked_add(ARTIFACT_REQUEST_TIMEOUT)
            .expect("artifact deadline must fit in Instant");
        if let Ok(state) = self.client.ui_state_before(deadline) {
            self.record_state("ui state (failure capture)", &state);
        }

        let screenshot = self.run_path().join("failure.png");
        match self.screenshot_request(&screenshot, ARTIFACT_REQUEST_TIMEOUT) {
            Ok(message) => self.history.push(json!({
                "elapsed_ms": self.elapsed_ms(),
                "command": format!("screenshot {} (failure capture)", screenshot.display()),
                "outcome": "ok",
                "response": message,
            })),
            Err(error) => self.record_error_message(
                "screenshot (failure capture)",
                &error.to_string(),
                error.failure(),
            ),
        }
    }

    fn write_failure_artifacts(&self) {
        if let Some(state) = &self.last_state {
            write_json(&self.run_path().join("last-state.json"), state);
        }
        write_json(&self.run_path().join("command-history.json"), &self.history);
        write_json(
            &self.run_path().join("summary.json"),
            &json!({
                "schema_version": 1,
                "name": self.test_name,
                "duration_ms": self.elapsed_ms(),
                "result": {
                    "success": false,
                    "error": self.failure.as_deref().unwrap_or("test panicked"),
                },
                "engine_log": "engine-client.log",
                "last_state": self.last_state.as_ref().map(|_| "last-state.json"),
                "command_history": "command-history.json",
                "failure_screenshot": self
                    .run_path()
                    .join("failure.png")
                    .exists()
                    .then_some("failure.png"),
            }),
        );
    }

    fn terminate_child(&mut self) {
        match self.child.0.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self.child.0.kill();
                let _ = self.child.0.wait();
            }
            Err(_) => {
                let _ = self.child.0.kill();
                let _ = self.child.0.wait();
            }
        }
    }
}

impl Drop for FunctionalHarness {
    fn drop(&mut self) {
        let preserve = self.failure.is_some() || thread::panicking();
        if preserve {
            self.capture_live_failure_artifacts();
        }
        self.terminate_child();

        if preserve {
            self.write_failure_artifacts();
            if let Some(run_directory) = self.run_directory.take() {
                let path = run_directory.keep();
                eprintln!("Preserved functional-test artifacts at {}", path.display());
            }
        }
    }
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
}

struct OwnedSocketPath(PathBuf);

impl Drop for OwnedSocketPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn press_request(
    action: UiAction,
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
) -> UiPressRequest {
    let mut request = UiPressRequest::new(action);
    request.expected_screen = expected_screen;
    request.expected_revision = expected_revision;
    request
}

fn activate_request(
    control_id: &str,
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
) -> UiActivateRequest {
    let mut request = UiActivateRequest::new(control_id);
    request.expected_screen = expected_screen;
    request.expected_revision = expected_revision;
    request
}

fn pause_request(
    expected_screen: Option<UiScreen>,
    expected_revision: Option<u64>,
) -> HostPauseRequest {
    let mut request = HostPauseRequest::new();
    request.expected_screen = expected_screen;
    request.expected_revision = expected_revision;
    request
}

fn assert_launcher_main(state: &UiState) {
    assert_eq!(state.screen, UiScreen::LauncherMain);
    assert_eq!(state.active_scenario, None);
    assert_eq!(state.selected_scenario, "spacewars");
    assert_eq!(selected_control(state), "launcher.scenario");
    assert_eq!(
        control_ids(state),
        [
            "launcher.scenario.previous",
            "launcher.scenario.next",
            "launcher.start",
            "launcher.settings",
            "launcher.controls",
            "launcher.quit",
        ]
    );
    assert_eq!(
        state.actions,
        [
            UiAction::Up,
            UiAction::Down,
            UiAction::Left,
            UiAction::Right,
            UiAction::Confirm,
            UiAction::Start,
            UiAction::Controls,
        ]
    );
}

fn selected_control(state: &UiState) -> String {
    state
        .selected_control
        .clone()
        .unwrap_or_else(|| panic!("{} has no selected control", state.screen))
}

fn control_ids(state: &UiState) -> Vec<&str> {
    state
        .controls
        .iter()
        .map(|control| control.id.as_str())
        .collect()
}

fn control_value<'a>(state: &'a UiState, id: &str) -> Option<&'a str> {
    state
        .controls
        .iter()
        .find(|control| control.id == id)
        .and_then(|control| control.value.as_deref())
}

fn control_enabled(state: &UiState, id: &str) -> bool {
    state
        .controls
        .iter()
        .find(|control| control.id == id)
        .unwrap_or_else(|| panic!("{} does not expose control {id:?}", state.screen))
        .enabled
}

fn optional_screen(screen: Option<UiScreen>) -> &'static str {
    screen.map(UiScreen::as_str).unwrap_or("none")
}

fn optional_revision(revision: Option<u64>) -> String {
    revision
        .map(|revision| revision.to_string())
        .unwrap_or_else(|| "none".into())
}

fn request_deadline() -> Instant {
    Instant::now()
        .checked_add(TRANSITION_TIMEOUT)
        .expect("functional-test request deadline must fit in Instant")
}

fn is_retryable_readiness_error(error: &ControlClientError) -> bool {
    match error {
        ControlClientError::DeadlineElapsed => true,
        ControlClientError::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WouldBlock
        ),
        _ => false,
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).into()
    } else {
        "non-string panic payload".into()
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("engine-client must live under <workspace>/crates")
        .to_path_buf()
}

fn write_json(path: &Path, value: &impl Serialize) {
    if let Ok(json) = serde_json::to_vec_pretty(value) {
        let _ = fs::write(path, json);
    }
}
