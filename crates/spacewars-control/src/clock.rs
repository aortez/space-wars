//! Public Clock controls. Animation ticks deliberately do not change UI revision.
use crate::{ControlClient, ControlClientError, ControlFailure, ControlFailureCode, ProtocolError};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const CLOCK_STATE_COMMAND: &str = "clock state";
pub const CLOCK_TRIGGER_COMMAND: &str = "clock trigger";
pub const CLOCK_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub schema_version: u32,
    pub scenario_revision: u64,
    pub paused: bool,
    pub profile: String,
    pub phase: String,
    pub event_id: u64,
    pub phase_tick: u64,
    pub simulation_tick: u64,
    pub next_event_tick: Option<u64>,
    pub body_count: usize,
    pub collider_count: usize,
    pub reading: Option<[u8; 3]>,
    /// Latest target digits, including during a fall. Blank 12-hour slots are null.
    pub display_digits: [Option<u8>; 4],
    pub can_trigger: bool,
    pub trigger_pending: bool,
}

impl ClockState {
    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let state: Self = serde_json::from_str(json)?;
        validate_version(state.schema_version)?;
        Ok(state)
    }
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        validate_version(self.schema_version)?;
        Ok(serde_json::to_string(self)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockTriggerRequest {
    pub schema_version: u32,
    pub expected_scenario_revision: u64,
    pub expected_event_id: u64,
}

impl ClockTriggerRequest {
    pub fn new(state: &ClockState) -> Self {
        Self {
            schema_version: CLOCK_STATE_SCHEMA_VERSION,
            expected_scenario_revision: state.scenario_revision,
            expected_event_id: state.event_id,
        }
    }
    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_str(json)?;
        validate_version(request.schema_version)?;
        Ok(request)
    }
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        validate_version(self.schema_version)?;
        Ok(serde_json::to_string(self)?)
    }
}

fn validate_version(version: u32) -> Result<(), ProtocolError> {
    if version != CLOCK_STATE_SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchemaVersion {
            found: version,
            supported: CLOCK_STATE_SCHEMA_VERSION,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockStatePredicate {
    pub scenario_revision: u64,
    pub phase: String,
    pub event_id: Option<u64>,
    pub min_phase_tick: u64,
}

impl ClockStatePredicate {
    pub fn matches(&self, state: &ClockState) -> bool {
        state.scenario_revision == self.scenario_revision
            && state.phase == self.phase
            && self.event_id.is_none_or(|id| id == state.event_id)
            && state.phase_tick >= self.min_phase_tick
    }
}

impl ControlClient {
    pub fn clock_state_before(&self, deadline: Instant) -> Result<ClockState, ControlClientError> {
        Ok(ClockState::from_json(&self.request_before(
            &format!("{CLOCK_STATE_COMMAND}\n"),
            deadline,
        )?)?)
    }

    /// A successful response acknowledges the queued request. Wait for the next
    /// event ID to confirm it executed; a later host restart/pause can supersede it.
    pub fn clock_trigger_before(
        &self,
        request: &ClockTriggerRequest,
        deadline: Instant,
    ) -> Result<ClockState, ControlClientError> {
        Ok(ClockState::from_json(&self.request_before(
            &format!("{CLOCK_TRIGGER_COMMAND}\n{}\n", request.to_json()?),
            deadline,
        )?)?)
    }

    pub fn wait_for_clock_state(
        &self,
        predicate: &ClockStatePredicate,
        timeout: Duration,
    ) -> Result<ClockState, ControlClientError> {
        let Some(deadline) = Instant::now().checked_add(timeout) else {
            return Err(clock_failure(
                ControlFailureCode::Timeout,
                "Clock wait timeout is too large".into(),
                None,
            ));
        };
        let mut last = None;
        loop {
            match self.clock_state_before(deadline) {
                Ok(state) if state.scenario_revision != predicate.scenario_revision => {
                    return Err(clock_failure(
                        ControlFailureCode::StaleRevision,
                        "Clock instance changed while waiting".into(),
                        Some(state),
                    ));
                }
                Ok(state) if predicate.matches(&state) => return Ok(state),
                Ok(state) => last = Some(state),
                Err(ControlClientError::DeadlineElapsed) => break,
                Err(ControlClientError::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10).min(remaining));
        }
        Err(clock_failure(
            ControlFailureCode::Timeout,
            format!("Timed out waiting for Clock {predicate:?}"),
            last,
        ))
    }
}

fn clock_failure(
    code: ControlFailureCode,
    message: String,
    state: Option<ClockState>,
) -> ControlClientError {
    let mut failure = ControlFailure::new(code, message, None);
    failure.current_clock_state = state;
    ControlClientError::Failure(Box::new(failure))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_require_both_guards_and_a_supported_schema() {
        let request = ClockTriggerRequest {
            schema_version: 1,
            expected_scenario_revision: 7,
            expected_event_id: 3,
        };
        assert_eq!(
            ClockTriggerRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );
        for json in [
            r#"{"schema_version":1}"#,
            r#"{"schema_version":2,"expected_scenario_revision":7,"expected_event_id":3}"#,
        ] {
            assert!(ClockTriggerRequest::from_json(json).is_err());
        }
    }
}
