//! Shared protocol types for controlling a running Space-Wars client.

use std::fmt;

use serde::{Deserialize, Serialize};

mod client;

pub use client::{ControlClient, ControlClientError, UiStatePredicate};

pub const UI_STATE_COMMAND: &str = "ui state";
pub const UI_PRESS_COMMAND: &str = "ui press";
pub const UI_ACTIVATE_COMMAND: &str = "ui activate";
pub const UI_STATE_SCHEMA_VERSION: u32 = 1;
pub const NO_ACTIVE_SCENARIO_DIAGNOSTICS: &str = "No active scenario diagnostics.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiScreen {
    #[serde(rename = "launcher.main")]
    LauncherMain,
    #[serde(rename = "launcher.settings")]
    LauncherSettings,
    #[serde(rename = "launcher.controls")]
    LauncherControls,
    #[serde(rename = "launcher.touch-test")]
    LauncherTouchTest,
    #[serde(rename = "gameplay")]
    Gameplay,
    #[serde(rename = "pause.main")]
    PauseMain,
    #[serde(rename = "pause.controls")]
    PauseControls,
    #[serde(rename = "game-over")]
    GameOver,
}

impl UiScreen {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LauncherMain => "launcher.main",
            Self::LauncherSettings => "launcher.settings",
            Self::LauncherControls => "launcher.controls",
            Self::LauncherTouchTest => "launcher.touch-test",
            Self::Gameplay => "gameplay",
            Self::PauseMain => "pause.main",
            Self::PauseControls => "pause.controls",
            Self::GameOver => "game-over",
        }
    }

    pub const fn is_launcher(self) -> bool {
        matches!(
            self,
            Self::LauncherMain
                | Self::LauncherSettings
                | Self::LauncherControls
                | Self::LauncherTouchTest
        )
    }
}

impl fmt::Display for UiScreen {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Start,
    Controls,
}

impl UiAction {
    pub const ALL: [Self; 8] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Confirm,
        Self::Back,
        Self::Start,
        Self::Controls,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Confirm => "confirm",
            Self::Back => "back",
            Self::Start => "start",
            Self::Controls => "controls",
        }
    }

    pub const fn code(self) -> i32 {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
            Self::Confirm => 4,
            Self::Back => 5,
            Self::Start => 6,
            Self::Controls => 7,
        }
    }

    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Up),
            1 => Some(Self::Down),
            2 => Some(Self::Left),
            3 => Some(Self::Right),
            4 => Some(Self::Confirm),
            5 => Some(Self::Back),
            6 => Some(Self::Start),
            7 => Some(Self::Controls),
            _ => None,
        }
    }
}

impl fmt::Display for UiAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiControl {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl UiControl {
    pub fn new(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled,
            value: None,
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    pub schema_version: u32,
    pub revision: u64,
    pub screen: UiScreen,
    pub active_scenario: Option<String>,
    pub selected_scenario: String,
    #[serde(default)]
    pub selected_control: Option<String>,
    #[serde(default)]
    pub controls: Vec<UiControl>,
    #[serde(default)]
    pub actions: Vec<UiAction>,
    pub scenario_revision: Option<u64>,
    pub paused: bool,
    pub benchmark_active: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPressRequest {
    pub schema_version: u32,
    pub action: UiAction,
    #[serde(default)]
    pub expected_screen: Option<UiScreen>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

impl UiPressRequest {
    pub fn new(action: UiAction) -> Self {
        Self {
            schema_version: UI_STATE_SCHEMA_VERSION,
            action,
            expected_screen: None,
            expected_revision: None,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_str(json)?;
        validate_schema_version(request.schema_version)?;
        Ok(request)
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        validate_schema_version(self.schema_version)?;
        Ok(serde_json::to_string(self)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiActivateRequest {
    pub schema_version: u32,
    pub control_id: String,
    #[serde(default)]
    pub expected_screen: Option<UiScreen>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

impl UiActivateRequest {
    pub fn new(control_id: impl Into<String>) -> Self {
        Self {
            schema_version: UI_STATE_SCHEMA_VERSION,
            control_id: control_id.into(),
            expected_screen: None,
            expected_revision: None,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_str(json)?;
        validate_schema_version(request.schema_version)?;
        Ok(request)
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        validate_schema_version(self.schema_version)?;
        Ok(serde_json::to_string(self)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlFailureCode {
    InvalidRequest,
    WrongScreen,
    StaleRevision,
    ActionUnavailable,
    ControlUnavailable,
    ControlDisabled,
    Timeout,
}

impl fmt::Display for ControlFailureCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::InvalidRequest => "invalid-request",
            Self::WrongScreen => "wrong-screen",
            Self::StaleRevision => "stale-revision",
            Self::ActionUnavailable => "action-unavailable",
            Self::ControlUnavailable => "control-unavailable",
            Self::ControlDisabled => "control-disabled",
            Self::Timeout => "timeout",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFailure {
    pub schema_version: u32,
    pub code: ControlFailureCode,
    pub message: String,
    #[serde(default)]
    pub current_state: Option<UiState>,
}

impl ControlFailure {
    pub fn new(
        code: ControlFailureCode,
        message: impl Into<String>,
        current_state: Option<UiState>,
    ) -> Self {
        Self {
            schema_version: UI_STATE_SCHEMA_VERSION,
            code,
            message: message.into(),
            current_state,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let failure: Self = serde_json::from_str(json)?;
        validate_schema_version(failure.schema_version)?;
        Ok(failure)
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        validate_schema_version(self.schema_version)?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, ProtocolError> {
        validate_schema_version(self.schema_version)?;
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl UiState {
    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let state: Self = serde_json::from_str(json)?;
        state.validate()?;
        Ok(state)
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        validate_schema_version(self.schema_version)
    }
}

fn validate_schema_version(schema_version: u32) -> Result<(), ProtocolError> {
    if schema_version != UI_STATE_SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchemaVersion {
            found: schema_version,
            supported: UI_STATE_SCHEMA_VERSION,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub active_scenario: Option<String>,
    pub scenario_revision: Option<u64>,
    pub paused: bool,
    pub benchmark_active: bool,
}

impl RuntimeStatus {
    pub fn inactive() -> Self {
        Self {
            active_scenario: None,
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
        }
    }
}

pub fn parse_runtime_status(status: &str) -> Result<RuntimeStatus, ProtocolError> {
    if status.trim() == NO_ACTIVE_SCENARIO_DIAGNOSTICS {
        return Ok(RuntimeStatus::inactive());
    }

    let scenario = required_status_value(status, "scenario")?;
    if scenario.is_empty() {
        return Err(ProtocolError::InvalidRuntimeField {
            field: "scenario",
            value: scenario.into(),
        });
    }

    Ok(RuntimeStatus {
        active_scenario: Some(scenario.into()),
        scenario_revision: Some(parse_status_value(status, "scenario_revision")?),
        paused: parse_status_value(status, "paused")?,
        benchmark_active: parse_status_value(status, "benchmark_active")?,
    })
}

fn parse_status_value<T>(status: &str, field: &'static str) -> Result<T, ProtocolError>
where
    T: std::str::FromStr,
{
    let value = required_status_value(status, field)?;
    value
        .parse()
        .map_err(|_| ProtocolError::InvalidRuntimeField {
            field,
            value: value.into(),
        })
}

fn required_status_value<'a>(
    status: &'a str,
    field: &'static str,
) -> Result<&'a str, ProtocolError> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix('='))
        .ok_or(ProtocolError::MissingRuntimeField(field))
}

#[derive(Debug)]
pub enum ProtocolError {
    Json(serde_json::Error),
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    MissingRuntimeField(&'static str),
    InvalidRuntimeField { field: &'static str, value: String },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid control protocol JSON: {error}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "unsupported control schema version {found}; this client supports version {supported}"
            ),
            Self::MissingRuntimeField(field) => {
                write!(formatter, "runtime diagnostics are missing {field}")
            }
            Self::InvalidRuntimeField { field, value } => {
                write!(
                    formatter,
                    "runtime diagnostics have invalid {field} {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::UnsupportedSchemaVersion { .. }
            | Self::MissingRuntimeField(_)
            | Self::InvalidRuntimeField { .. } => None,
        }
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> UiState {
        UiState {
            schema_version: UI_STATE_SCHEMA_VERSION,
            revision: 42,
            screen: UiScreen::LauncherMain,
            active_scenario: None,
            selected_scenario: "spacewars".into(),
            selected_control: Some("launcher.scenario".into()),
            controls: vec![
                UiControl::new("launcher.scenario.previous", "‹", true).with_value("spacewars"),
                UiControl::new("launcher.scenario.next", "›", true).with_value("spacewars"),
                UiControl::new("launcher.start", "Start Game", true),
            ],
            actions: vec![
                UiAction::Up,
                UiAction::Down,
                UiAction::Left,
                UiAction::Right,
                UiAction::Confirm,
                UiAction::Start,
                UiAction::Controls,
            ],
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
            error: None,
        }
    }

    #[test]
    fn screen_names_are_stable_and_round_trip() {
        let cases = [
            (UiScreen::LauncherMain, "launcher.main"),
            (UiScreen::LauncherSettings, "launcher.settings"),
            (UiScreen::LauncherControls, "launcher.controls"),
            (UiScreen::LauncherTouchTest, "launcher.touch-test"),
            (UiScreen::Gameplay, "gameplay"),
            (UiScreen::PauseMain, "pause.main"),
            (UiScreen::PauseControls, "pause.controls"),
            (UiScreen::GameOver, "game-over"),
        ];

        for (screen, name) in cases {
            assert_eq!(screen.as_str(), name);
            assert_eq!(screen.to_string(), name);
            assert_eq!(
                serde_json::to_string(&screen).unwrap(),
                format!("\"{name}\"")
            );
            assert_eq!(
                serde_json::from_str::<UiScreen>(&format!("\"{name}\"")).unwrap(),
                screen
            );
        }
    }

    #[test]
    fn ui_state_json_round_trips_and_rejects_other_versions() {
        let state = sample_state();
        let json = state.to_json().unwrap();
        assert_eq!(UiState::from_json(&json).unwrap(), state);

        let mut unsupported = sample_state();
        unsupported.schema_version += 1;
        assert!(matches!(
            unsupported.to_json(),
            Err(ProtocolError::UnsupportedSchemaVersion { .. })
        ));
        assert!(matches!(
            UiState::from_json(&serde_json::to_string(&unsupported).unwrap()),
            Err(ProtocolError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn ui_state_additions_accept_the_original_v1_payload() {
        let original = r#"{
            "schema_version": 1,
            "revision": 2,
            "screen": "gameplay",
            "active_scenario": "pizza",
            "selected_scenario": "pizza",
            "scenario_revision": 7,
            "paused": false,
            "benchmark_active": false
        }"#;

        let state = UiState::from_json(original).unwrap();

        assert_eq!(state.selected_control, None);
        assert!(state.controls.is_empty());
        assert!(state.actions.is_empty());
        assert_eq!(state.error, None);
    }

    #[test]
    fn action_names_codes_and_json_are_stable() {
        for (code, action) in UiAction::ALL.into_iter().enumerate() {
            assert_eq!(UiAction::from_code(code as i32), Some(action));
            assert_eq!(action.code(), code as i32);
            assert_eq!(action.to_string(), action.as_str());
            assert_eq!(
                serde_json::to_string(&action).unwrap(),
                format!("\"{}\"", action.as_str())
            );
        }
        assert_eq!(UiAction::from_code(99), None);
    }

    #[test]
    fn failure_code_names_are_stable() {
        for (code, name) in [
            (ControlFailureCode::InvalidRequest, "invalid-request"),
            (ControlFailureCode::WrongScreen, "wrong-screen"),
            (ControlFailureCode::StaleRevision, "stale-revision"),
            (ControlFailureCode::ActionUnavailable, "action-unavailable"),
            (
                ControlFailureCode::ControlUnavailable,
                "control-unavailable",
            ),
            (ControlFailureCode::ControlDisabled, "control-disabled"),
            (ControlFailureCode::Timeout, "timeout"),
        ] {
            assert_eq!(code.to_string(), name);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{name}\""));
        }
    }

    #[test]
    fn mutation_requests_and_structured_failures_round_trip() {
        let mut request = UiPressRequest::new(UiAction::Confirm);
        request.expected_screen = Some(UiScreen::LauncherMain);
        request.expected_revision = Some(4);
        assert_eq!(
            UiPressRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );

        let mut request = UiActivateRequest::new("launcher.settings");
        request.expected_screen = Some(UiScreen::LauncherMain);
        request.expected_revision = Some(5);
        assert_eq!(
            UiActivateRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );

        let failure = ControlFailure::new(
            ControlFailureCode::StaleRevision,
            "expected revision 3 but current revision is 4",
            Some(sample_state()),
        );
        assert_eq!(
            ControlFailure::from_json(&failure.to_json().unwrap()).unwrap(),
            failure
        );
    }

    #[test]
    fn control_values_are_optional_in_json() {
        let json = sample_state().to_json().unwrap();
        let controls = serde_json::from_str::<serde_json::Value>(&json).unwrap()["controls"]
            .as_array()
            .unwrap()
            .clone();

        assert_eq!(controls[0]["value"], "spacewars");
        assert!(controls[2].get("value").is_none());
    }

    #[test]
    fn parses_active_runtime_diagnostics_and_ignores_extra_fields() {
        assert_eq!(
            parse_runtime_status(
                "scenario=pizza\nscenario_revision=9\npaused=true\nbenchmark_active=false\nfps=59.8\n"
            )
            .unwrap(),
            RuntimeStatus {
                active_scenario: Some("pizza".into()),
                scenario_revision: Some(9),
                paused: true,
                benchmark_active: false,
            }
        );
    }

    #[test]
    fn parses_inactive_runtime_diagnostics() {
        assert_eq!(
            parse_runtime_status(NO_ACTIVE_SCENARIO_DIAGNOSTICS).unwrap(),
            RuntimeStatus::inactive()
        );
    }

    #[test]
    fn rejects_incomplete_or_invalid_runtime_diagnostics() {
        assert!(matches!(
            parse_runtime_status("scenario=spacewars\npaused=false\nbenchmark_active=false\n"),
            Err(ProtocolError::MissingRuntimeField("scenario_revision"))
        ));
        assert!(matches!(
            parse_runtime_status(
                "scenario=spacewars\nscenario_revision=abc\npaused=false\nbenchmark_active=false\n"
            ),
            Err(ProtocolError::InvalidRuntimeField {
                field: "scenario_revision",
                ..
            })
        ));
    }
}
