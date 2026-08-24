use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{
    ControlFailure, ControlFailureCode, ProtocolError, UI_ACTIVATE_COMMAND, UI_PRESS_COMMAND,
    UI_STATE_COMMAND, UiActivateRequest, UiPressRequest, UiScreen, UiState,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Client for the engine's local control socket.
#[derive(Debug, Clone)]
pub struct ControlClient {
    socket_path: PathBuf,
}

impl ControlClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn request(&self, request: &str) -> Result<String, ControlClientError> {
        self.request_with_timeout(request, None)
    }

    pub fn request_before(
        &self,
        request: &str,
        deadline: Instant,
    ) -> Result<String, ControlClientError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ControlClientError::DeadlineElapsed);
        }
        self.request_with_timeout(request, Some(remaining))
    }

    pub fn ui_state(&self) -> Result<UiState, ControlClientError> {
        self.parse_ui_state(self.request(&format!("{UI_STATE_COMMAND}\n"))?)
    }

    pub fn ui_state_before(&self, deadline: Instant) -> Result<UiState, ControlClientError> {
        self.parse_ui_state(self.request_before(&format!("{UI_STATE_COMMAND}\n"), deadline)?)
    }

    pub fn ui_press(&self, request: &UiPressRequest) -> Result<UiState, ControlClientError> {
        let payload = request.to_json()?;
        self.parse_ui_state(self.request(&format!("{UI_PRESS_COMMAND}\n{payload}\n"))?)
    }

    pub fn ui_press_before(
        &self,
        request: &UiPressRequest,
        deadline: Instant,
    ) -> Result<UiState, ControlClientError> {
        let payload = request.to_json()?;
        self.parse_ui_state(
            self.request_before(&format!("{UI_PRESS_COMMAND}\n{payload}\n"), deadline)?,
        )
    }

    pub fn ui_activate(&self, request: &UiActivateRequest) -> Result<UiState, ControlClientError> {
        let payload = request.to_json()?;
        self.parse_ui_state(self.request(&format!("{UI_ACTIVATE_COMMAND}\n{payload}\n"))?)
    }

    pub fn ui_activate_before(
        &self,
        request: &UiActivateRequest,
        deadline: Instant,
    ) -> Result<UiState, ControlClientError> {
        let payload = request.to_json()?;
        self.parse_ui_state(
            self.request_before(&format!("{UI_ACTIVATE_COMMAND}\n{payload}\n"), deadline)?,
        )
    }

    pub fn wait_for_ui_state(
        &self,
        predicate: &UiStatePredicate,
        timeout: Duration,
    ) -> Result<UiState, ControlClientError> {
        let Some(deadline) = Instant::now().checked_add(timeout) else {
            return Err(ControlClientError::Failure(Box::new(ControlFailure::new(
                ControlFailureCode::Timeout,
                "UI wait timeout is too large",
                None,
            ))));
        };
        let mut last_state = None;

        loop {
            match self.ui_state_before(deadline) {
                Ok(state) if predicate.matches(&state) => return Ok(state),
                Ok(state) => last_state = Some(state),
                Err(ControlClientError::DeadlineElapsed) => {
                    return Err(wait_timeout(predicate, timeout, last_state));
                }
                Err(ControlClientError::Io(ref error)) if is_deadline_io_error(error) => {
                    return Err(wait_timeout(predicate, timeout, last_state));
                }
                Err(error) => return Err(error),
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(wait_timeout(predicate, timeout, last_state));
            }
            std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
        }
    }

    fn parse_ui_state(&self, json: String) -> Result<UiState, ControlClientError> {
        Ok(UiState::from_json(&json)?)
    }

    #[cfg(unix)]
    fn request_with_timeout(
        &self,
        request: &str,
        timeout: Option<Duration>,
    ) -> Result<String, ControlClientError> {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;
        stream.write_all(request.as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        parse_response(&response)
    }

    #[cfg(not(unix))]
    fn request_with_timeout(
        &self,
        _request: &str,
        _timeout: Option<Duration>,
    ) -> Result<String, ControlClientError> {
        Err(ControlClientError::UnsupportedPlatform)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiStatePredicate {
    pub screen: Option<UiScreen>,
    pub scenario: Option<String>,
    pub revision_after: Option<u64>,
}

impl UiStatePredicate {
    pub fn is_empty(&self) -> bool {
        self.screen.is_none() && self.scenario.is_none() && self.revision_after.is_none()
    }

    pub fn matches(&self, state: &UiState) -> bool {
        self.screen.is_none_or(|screen| state.screen == screen)
            && self.scenario.as_deref().is_none_or(|scenario| {
                state
                    .active_scenario
                    .as_deref()
                    .unwrap_or(&state.selected_scenario)
                    == scenario
            })
            && self
                .revision_after
                .is_none_or(|revision| state.revision > revision)
    }

    fn description(&self) -> String {
        let mut conditions = Vec::new();
        if let Some(screen) = self.screen {
            conditions.push(format!("screen={screen}"));
        }
        if let Some(scenario) = &self.scenario {
            conditions.push(format!("scenario={scenario}"));
        }
        if let Some(revision) = self.revision_after {
            conditions.push(format!("revision>{revision}"));
        }
        if conditions.is_empty() {
            "any UI state".into()
        } else {
            conditions.join(", ")
        }
    }
}

#[derive(Debug)]
pub enum ControlClientError {
    Io(io::Error),
    Protocol(ProtocolError),
    Failure(Box<ControlFailure>),
    ServerMessage(String),
    UnexpectedResponse(String),
    DeadlineElapsed,
    UnsupportedPlatform,
}

impl ControlClientError {
    pub fn failure(&self) -> Option<&ControlFailure> {
        match self {
            Self::Failure(failure) => Some(failure),
            _ => None,
        }
    }

    pub fn into_failure(self) -> Option<ControlFailure> {
        match self {
            Self::Failure(failure) => Some(*failure),
            _ => None,
        }
    }
}

impl fmt::Display for ControlClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "control socket I/O failed: {error}"),
            Self::Protocol(error) => error.fmt(formatter),
            Self::Failure(failure) => {
                write!(formatter, "{}: {}", failure.code, failure.message)?;
                if let Some(state) = &failure.current_state {
                    write!(
                        formatter,
                        "; current screen={} revision={}",
                        state.screen, state.revision
                    )?;
                }
                Ok(())
            }
            Self::ServerMessage(message) => formatter.write_str(message),
            Self::UnexpectedResponse(response) => {
                write!(
                    formatter,
                    "unexpected response from engine-client: {response:?}"
                )
            }
            Self::DeadlineElapsed => formatter.write_str("control request deadline elapsed"),
            Self::UnsupportedPlatform => {
                formatter.write_str("Space-Wars control requires Unix domain sockets")
            }
        }
    }
}

impl std::error::Error for ControlClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Protocol(error) => Some(error),
            Self::Failure(_)
            | Self::ServerMessage(_)
            | Self::UnexpectedResponse(_)
            | Self::DeadlineElapsed
            | Self::UnsupportedPlatform => None,
        }
    }
}

impl From<io::Error> for ControlClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ProtocolError> for ControlClientError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

fn parse_response(response: &str) -> Result<String, ControlClientError> {
    let response = response.trim_end();
    if let Some(message) = response.strip_prefix("ok ") {
        Ok(message.into())
    } else if let Some(message) = response.strip_prefix("error ") {
        match ControlFailure::from_json(message) {
            Ok(failure) => Err(ControlClientError::Failure(Box::new(failure))),
            Err(_) => Err(ControlClientError::ServerMessage(message.into())),
        }
    } else {
        Err(ControlClientError::UnexpectedResponse(response.into()))
    }
}

fn is_deadline_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

fn wait_timeout(
    predicate: &UiStatePredicate,
    timeout: Duration,
    current_state: Option<UiState>,
) -> ControlClientError {
    ControlClientError::Failure(Box::new(ControlFailure::new(
        ControlFailureCode::Timeout,
        format!(
            "timed out after {} waiting for {}",
            format_duration(timeout),
            predicate.description()
        ),
        current_state,
    )))
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
    use crate::{UI_STATE_SCHEMA_VERSION, UiAction, UiControl};
    #[cfg(unix)]
    use std::io::{Read, Write};
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(unix)]
    use std::thread;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    fn state(screen: UiScreen, revision: u64) -> UiState {
        UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision,
            screen,
            active_scenario: None,
            selected_scenario: "spacewars".into(),
            selected_control: Some("launcher.start".into()),
            controls: vec![UiControl::new("launcher.start", "Start Game", true)],
            actions: vec![UiAction::Confirm, UiAction::Start],
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
            error: None,
        }
    }

    #[test]
    fn parses_success_legacy_error_and_structured_error() {
        assert_eq!(
            parse_response("ok scenario=pizza\nfps=59.8\n").unwrap(),
            "scenario=pizza\nfps=59.8"
        );
        assert!(matches!(
            parse_response("error screenshot failed\n"),
            Err(ControlClientError::ServerMessage(message)) if message == "screenshot failed"
        ));

        let failure = ControlFailure::new(
            ControlFailureCode::WrongScreen,
            "expected launcher.main",
            Some(state(UiScreen::PauseMain, 8)),
        );
        let response = format!("error {}\n", failure.to_json().unwrap());
        assert!(matches!(
            parse_response(&response),
            Err(ControlClientError::Failure(parsed)) if *parsed == failure
        ));
        assert!(matches!(
            parse_response(""),
            Err(ControlClientError::UnexpectedResponse(response)) if response.is_empty()
        ));
    }

    #[test]
    fn state_predicates_combine_screen_scenario_and_revision() {
        let predicate = UiStatePredicate {
            screen: Some(UiScreen::LauncherMain),
            scenario: Some("spacewars".into()),
            revision_after: Some(7),
        };

        assert!(predicate.matches(&state(UiScreen::LauncherMain, 8)));
        assert!(!predicate.matches(&state(UiScreen::PauseMain, 8)));
        assert!(!predicate.matches(&state(UiScreen::LauncherMain, 7)));

        let mut active = state(UiScreen::Gameplay, 9);
        active.active_scenario = Some("pizza".into());
        assert!(
            UiStatePredicate {
                scenario: Some("pizza".into()),
                ..Default::default()
            }
            .matches(&active)
        );
    }

    #[cfg(unix)]
    #[test]
    fn wait_polls_until_a_later_state_matches() {
        let socket = TestSocket::new("wait-success");
        let listener = UnixListener::bind(&socket.0).unwrap();
        let first = state(UiScreen::LauncherMain, 1);
        let second = state(UiScreen::LauncherSettings, 2);
        let responses = [first, second.clone()];
        let server = thread::spawn(move || {
            for state in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                stream.read_to_string(&mut request).unwrap();
                assert_eq!(request, "ui state\n");
                writeln!(stream, "ok {}", state.to_json().unwrap()).unwrap();
            }
        });

        let result = ControlClient::new(&socket.0)
            .wait_for_ui_state(
                &UiStatePredicate {
                    screen: Some(UiScreen::LauncherSettings),
                    revision_after: Some(1),
                    ..Default::default()
                },
                Duration::from_secs(1),
            )
            .unwrap();

        assert_eq!(result, second);
        server.join().unwrap();
    }

    #[test]
    fn wait_timeout_is_structured_and_retains_the_last_state() {
        let last_state = state(UiScreen::LauncherMain, 3);
        let error = wait_timeout(
            &UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                revision_after: Some(3),
                ..Default::default()
            },
            Duration::from_millis(250),
            Some(last_state.clone()),
        );
        let failure = error.failure().unwrap();

        assert_eq!(failure.code, ControlFailureCode::Timeout);
        assert!(failure.message.contains("screen=gameplay"));
        assert!(failure.message.contains("revision>3"));
        assert_eq!(failure.current_state, Some(last_state));
    }

    #[cfg(unix)]
    struct TestSocket(PathBuf);

    #[cfg(unix)]
    impl TestSocket {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!(
                "spacewars-control-{}-{nonce}-{label}.sock",
                std::process::id()
            )))
        }
    }

    #[cfg(unix)]
    impl Drop for TestSocket {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
