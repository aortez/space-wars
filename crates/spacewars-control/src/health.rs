//! Fresh UI-thread responses for a local recovery observer. These fields are
//! observations, not permission to install or a substitute for OS peer/process
//! identity, executable verification and persistent-state checks.
use crate::ProtocolError;
use serde::{Deserialize, Serialize};

pub const HEALTH_COMMAND: &str = "health";
pub const HEALTH_SCHEMA_VERSION: u32 = 1;
/// Shared runtime path on the kiosk. Unlike /tmp, /run is visible to the
/// independent device service even when it uses a private temporary directory.
pub const KIOSK_CONTROL_SOCKET: &str = "/run/spacewars-kiosk/control.sock";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthRequest {
    pub schema_version: u32,
    pub nonce: u64,
}

impl HealthRequest {
    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_str(json)?;
        version(request.schema_version)?;
        Ok(request)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthState {
    pub schema_version: u32,
    pub nonce: u64,
    pub process_id: u32,
    pub build_revision: String,
    pub ui_responses: u64,
    /// Completed DRM page-flip events, independent of optional profiling.
    /// None on backends without this evidence; never substitute UI ticks.
    pub presented_frames: Option<u64>,
}

impl HealthState {
    pub fn from_json(json: &str, nonce: u64) -> Result<Self, ProtocolError> {
        let state: Self = serde_json::from_str(json)?;
        version(state.schema_version)?;
        if state.nonce != nonce
            || state.process_id <= 1
            || state.ui_responses == 0
            || state.build_revision.is_empty()
            || state.build_revision.len() > 128
            || state.build_revision.chars().any(char::is_control)
        {
            return Err(ProtocolError::InvalidInput(
                "invalid or stale health response".into(),
            ));
        }
        Ok(state)
    }
}

fn version(found: u32) -> Result<(), ProtocolError> {
    if found != HEALTH_SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchemaVersion {
            found,
            supported: HEALTH_SCHEMA_VERSION,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_health_requests_reject_extra_fields_and_responses_reject_stale_nonce() {
        assert!(HealthRequest::from_json(r#"{"schema_version":1,"nonce":7}"#).is_ok());
        for json in [
            r#"{"schema_version":2,"nonce":7}"#,
            r#"{"schema_version":1,"nonce":7,"slot":"b"}"#,
            r#"{"schema_version":1}"#,
        ] {
            assert!(HealthRequest::from_json(json).is_err());
        }
        let state = HealthState {
            schema_version: 1,
            nonce: 7,
            process_id: 100,
            build_revision: "fixture".into(),
            ui_responses: 1,
            presented_frames: Some(30),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(HealthState::from_json(&json, 7).unwrap(), state);
        assert!(HealthState::from_json(&json, 8).is_err());
    }

    #[test]
    fn missing_presentation_evidence_remains_explicit() {
        let state = HealthState {
            schema_version: 1,
            nonce: 7,
            process_id: 100,
            build_revision: "desktop".into(),
            ui_responses: 1,
            presented_frames: None,
        };
        assert_eq!(
            HealthState::from_json(&serde_json::to_string(&state).unwrap(), 7)
                .unwrap()
                .presented_frames,
            None
        );
    }
}
