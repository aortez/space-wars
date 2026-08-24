//! Shared protocol types for controlling a running Space-Wars client.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const UI_STATE_COMMAND: &str = "ui state";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    pub schema_version: u32,
    pub revision: u64,
    pub screen: UiScreen,
    pub active_scenario: Option<String>,
    pub selected_scenario: String,
    pub scenario_revision: Option<u64>,
    pub paused: bool,
    pub benchmark_active: bool,
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
        if self.schema_version != UI_STATE_SCHEMA_VERSION {
            return Err(ProtocolError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: UI_STATE_SCHEMA_VERSION,
            });
        }
        Ok(())
    }
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
            Self::Json(error) => write!(formatter, "invalid UI state JSON: {error}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "unsupported UI state schema version {found}; this client supports version {supported}"
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
            scenario_revision: None,
            paused: false,
            benchmark_active: false,
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
