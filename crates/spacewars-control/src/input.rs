//! Bounded virtual-controller input; no Linux device codes in the protocol.
use crate::{ControlClient, ControlClientError, ProtocolError, UiScreen, UiState};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const INPUT_PRESS_COMMAND: &str = "input press";
pub const INPUT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputButton {
    Up,
    Down,
    Left,
    Right,
    South,
    East,
    North,
    West,
    LeftShoulder,
    RightShoulder,
    Start,
    Select,
}

impl InputButton {
    pub const ALL: [Self; 12] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::South,
        Self::East,
        Self::North,
        Self::West,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::Start,
        Self::Select,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::South => "south",
            Self::East => "east",
            Self::North => "north",
            Self::West => "west",
            Self::LeftShoulder => "left-shoulder",
            Self::RightShoulder => "right-shoulder",
            Self::Start => "start",
            Self::Select => "select",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputProfile {
    #[default]
    Standard,
    Picade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPressRequest {
    pub schema_version: u32,
    pub player: u8,
    pub button: InputButton,
    pub profile: InputProfile,
    pub hold_ms: u64,
    pub expected_screen: UiScreen,
    pub expected_revision: u64,
    pub expected_scenario_revision: Option<u64>,
}

impl InputPressRequest {
    pub fn new(state: &UiState, button: InputButton) -> Self {
        Self {
            schema_version: INPUT_SCHEMA_VERSION,
            player: 1,
            button,
            profile: InputProfile::Standard,
            hold_ms: 120,
            expected_screen: state.screen,
            expected_revision: state.revision,
            expected_scenario_revision: state.scenario_revision,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != INPUT_SCHEMA_VERSION {
            return Err(ProtocolError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: INPUT_SCHEMA_VERSION,
            });
        }
        if !(1..=2).contains(&self.player) || !(50..=2000).contains(&self.hold_ms) {
            return Err(ProtocolError::InvalidInput(
                "player must be 1 or 2; hold_ms must be 50..=2000".into(),
            ));
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_str(json)?;
        request.validate()?;
        Ok(request)
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_string(self)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputReleaseReason {
    Elapsed,
    ContextChanged,
    InputCleared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputPressResult {
    pub schema_version: u32,
    pub player: u8,
    pub button: InputButton,
    pub release_reason: InputReleaseReason,
    pub state: UiState,
}

impl InputPressResult {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        Ok(serde_json::to_string(self)?)
    }
}

impl ControlClient {
    /// Returns only after the app releases the button. The app owns the lease,
    /// so disconnecting or killing this client cannot leave it held indefinitely.
    pub fn input_press_before(
        &self,
        request: &InputPressRequest,
        deadline: Instant,
    ) -> Result<InputPressResult, ControlClientError> {
        let result: InputPressResult = serde_json::from_str(&self.request_before(
            &format!("{INPUT_PRESS_COMMAND}\n{}\n", request.to_json()?),
            deadline,
        )?)
        .map_err(ProtocolError::from)?;
        if result.schema_version != INPUT_SCHEMA_VERSION {
            return Err(ProtocolError::UnsupportedSchemaVersion {
                found: result.schema_version,
                supported: INPUT_SCHEMA_VERSION,
            }
            .into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn json() -> serde_json::Value {
        serde_json::json!({"schema_version": 1, "player": 1, "button": "west",
            "profile": "picade", "hold_ms": 120, "expected_screen": "gameplay",
            "expected_revision": 4, "expected_scenario_revision": 2})
    }

    #[test]
    fn named_buttons_round_trip_without_physical_device_codes() {
        for button in InputButton::ALL {
            let mut value = json();
            value["button"] = button.as_str().into();
            let request = InputPressRequest::from_json(&value.to_string()).unwrap();
            assert_eq!(request.button, button);
            assert_eq!(
                InputPressRequest::from_json(&request.to_json().unwrap()).unwrap(),
                request
            );
        }
    }

    #[test]
    fn rejects_bad_versions_ranges_names_and_extra_fields() {
        for (field, value) in [
            ("schema_version", serde_json::json!(2)),
            ("player", serde_json::json!(0)),
            ("player", serde_json::json!(3)),
            ("hold_ms", serde_json::json!(49)),
            ("hold_ms", serde_json::json!(2001)),
            ("button", serde_json::json!(308)),
            ("button", serde_json::json!("power")),
            ("device", serde_json::json!("/dev/input/event4")),
        ] {
            let mut request = json();
            request[field] = value;
            assert!(
                InputPressRequest::from_json(&request.to_string()).is_err(),
                "{field}"
            );
        }
    }
}
