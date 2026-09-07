//! Public Clock controls. Animation ticks deliberately do not change UI revision.
use crate::{ControlClient, ControlClientError, ControlFailure, ControlFailureCode, ProtocolError};
pub use engine_common::ClockEventKind;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const CLOCK_STATE_COMMAND: &str = "clock state";
pub const CLOCK_TRIGGER_COMMAND: &str = "clock trigger";
pub const CLOCK_STATE_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockEventInfo {
    pub kind: ClockEventKind,
    pub label: String,
    pub effect: String,
    pub duration_ticks: u64,
    pub cooldown_ticks: u64,
    pub enabled: bool,
    pub automatic_ready_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub schema_version: u32,
    pub scenario_revision: u64,
    pub paused: bool,
    pub settings: engine_common::ClockSettings,
    pub profile: String,
    pub lifecycle: String,
    pub event_kind: Option<ClockEventKind>,
    pub phase: Option<String>,
    pub event_id: u64,
    /// Ticks in the event's current phase, or idle/cooldown when no event is active.
    pub phase_tick: u64,
    pub simulation_tick: u64,
    pub next_event_tick: Option<u64>,
    pub events: Vec<ClockEventInfo>,
    pub palette_rgb: [u8; 3],
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
    pub event: ClockEventKind,
    pub expected_scenario_revision: u64,
    pub expected_event_id: u64,
}

impl ClockTriggerRequest {
    pub fn new(state: &ClockState, event: ClockEventKind) -> Self {
        Self {
            schema_version: CLOCK_STATE_SCHEMA_VERSION,
            event,
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
    pub lifecycle: Option<String>,
    pub event_kind: Option<ClockEventKind>,
    pub phase: Option<String>,
    pub event_id: Option<u64>,
    pub min_phase_tick: u64,
}

impl ClockStatePredicate {
    pub fn matches(&self, state: &ClockState) -> bool {
        state.scenario_revision == self.scenario_revision
            && self
                .lifecycle
                .as_ref()
                .is_none_or(|lifecycle| *lifecycle == state.lifecycle)
            && self
                .event_kind
                .is_none_or(|kind| Some(kind) == state.event_kind)
            && self
                .phase
                .as_ref()
                .is_none_or(|phase| Some(phase) == state.phase.as_ref())
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

    /// Wait for a matching state. A timeout includes the last successful reply,
    /// or no state if the deadline elapsed before any reply was received.
    pub fn wait_for_clock_state(
        &self,
        predicate: &ClockStatePredicate,
        timeout: Duration,
    ) -> Result<ClockState, ControlClientError> {
        wait_for_clock_state_with(
            predicate,
            timeout,
            Instant::now,
            |deadline| self.clock_state_before(deadline),
            std::thread::sleep,
        )
    }
}

// Keep the actual polling loop testable with scripted replies and a virtual
// monotonic clock; timeout coverage must not depend on socket/thread scheduling.
fn wait_for_clock_state_with(
    predicate: &ClockStatePredicate,
    timeout: Duration,
    mut now: impl FnMut() -> Instant,
    mut request: impl FnMut(Instant) -> Result<ClockState, ControlClientError>,
    mut sleep: impl FnMut(Duration),
) -> Result<ClockState, ControlClientError> {
    let Some(deadline) = now().checked_add(timeout) else {
        return Err(clock_failure(
            ControlFailureCode::Timeout,
            "Clock wait timeout is too large".into(),
            None,
        ));
    };
    let mut last = None;
    loop {
        match request(deadline) {
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
        let remaining = deadline.saturating_duration_since(now());
        if remaining.is_zero() {
            break;
        }
        sleep(Duration::from_millis(10).min(remaining));
    }
    Err(clock_failure(
        ControlFailureCode::Timeout,
        format!("Timed out waiting for Clock {predicate:?}"),
        last,
    ))
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
    use std::cell::Cell;

    fn clock_state() -> ClockState {
        ClockState {
            schema_version: CLOCK_STATE_SCHEMA_VERSION,
            scenario_revision: 7,
            paused: false,
            settings: engine_common::ClockSettings::default(),
            profile: "demo".into(),
            lifecycle: "active".into(),
            event_kind: Some(ClockEventKind::ColorCycle),
            phase: Some("cycling".into()),
            event_id: 3,
            phase_tick: 60,
            simulation_tick: 420,
            next_event_tick: None,
            events: vec![],
            palette_rgb: [170, 140, 255],
            body_count: 0,
            collider_count: 0,
            reading: Some([12, 34, 56]),
            display_digits: [Some(1), Some(2), Some(3), Some(4)],
            can_trigger: false,
            trigger_pending: false,
        }
    }

    #[test]
    fn event_state_round_trips_and_waits_match_kind_phase_and_lifecycle_separately() {
        let mut state = clock_state();
        assert_eq!(
            ClockState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        let predicate = ClockStatePredicate {
            scenario_revision: 7,
            lifecycle: Some("active".into()),
            event_kind: Some(ClockEventKind::ColorCycle),
            phase: Some("cycling".into()),
            event_id: Some(3),
            min_phase_tick: 60,
        };
        assert!(predicate.matches(&state));
        for mismatch in [
            ClockStatePredicate {
                scenario_revision: 8,
                ..predicate.clone()
            },
            ClockStatePredicate {
                lifecycle: Some("idle".into()),
                ..predicate.clone()
            },
            ClockStatePredicate {
                event_kind: Some(ClockEventKind::Falling),
                ..predicate.clone()
            },
            ClockStatePredicate {
                phase: Some("reforming".into()),
                ..predicate.clone()
            },
            ClockStatePredicate {
                event_id: Some(4),
                ..predicate.clone()
            },
            ClockStatePredicate {
                min_phase_tick: 61,
                ..predicate.clone()
            },
        ] {
            assert!(!mismatch.matches(&state));
        }
        state.lifecycle = "idle".into();
        state.event_kind = None;
        state.phase = None;
        assert_eq!(
            ClockState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        assert!(!predicate.matches(&state));
        assert!(
            ClockStatePredicate {
                lifecycle: Some("idle".into()),
                event_kind: None,
                phase: None,
                ..predicate
            }
            .matches(&state)
        );
    }

    #[test]
    fn triggers_require_both_guards_and_a_supported_schema() {
        let request = ClockTriggerRequest {
            schema_version: CLOCK_STATE_SCHEMA_VERSION,
            event: ClockEventKind::Falling,
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
            r#"{"schema_version":1,"event":"falling","expected_scenario_revision":7,"expected_event_id":3}"#,
            r#"{"schema_version":2,"event":"unknown","expected_scenario_revision":7,"expected_event_id":3}"#,
        ] {
            assert!(ClockTriggerRequest::from_json(json).is_err());
        }
    }

    fn after(state: &ClockState) -> ClockStatePredicate {
        ClockStatePredicate {
            scenario_revision: state.scenario_revision,
            lifecycle: Some(state.lifecycle.clone()),
            event_kind: state.event_kind,
            phase: state.phase.clone(),
            event_id: Some(state.event_id),
            min_phase_tick: state.phase_tick + 1,
        }
    }

    struct WaitRun {
        result: Result<ClockState, ControlClientError>,
        requests: usize,
        sleeps: Vec<Duration>,
    }

    fn scripted_wait(
        predicate: &ClockStatePredicate,
        timeout: Duration,
        replies: impl IntoIterator<Item = (Duration, Result<ClockState, ControlClientError>)>,
    ) -> WaitRun {
        let start = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let mut replies = replies.into_iter();
        let mut requests = 0;
        let mut sleeps = Vec::new();
        let result = wait_for_clock_state_with(
            predicate,
            timeout,
            || start + elapsed.get(),
            |deadline| {
                assert_eq!(deadline, start + timeout, "polls share one deadline");
                // Like request_before(), an elapsed deadline does no transport I/O.
                if elapsed.get() >= timeout {
                    return Err(ControlClientError::DeadlineElapsed);
                }
                requests += 1;
                let (delay, reply) = replies.next().expect("unexpected extra request");
                elapsed.set(elapsed.get() + delay);
                reply
            },
            |duration| {
                sleeps.push(duration);
                elapsed.set(elapsed.get() + duration);
            },
        );
        assert!(
            replies.next().is_none(),
            "not all expected requests were made"
        );
        WaitRun {
            result,
            requests,
            sleeps,
        }
    }

    fn timeout_errors() -> [ControlClientError; 3] {
        [
            ControlClientError::DeadlineElapsed,
            std::io::Error::from(std::io::ErrorKind::TimedOut).into(),
            std::io::Error::from(std::io::ErrorKind::WouldBlock).into(),
        ]
    }

    #[test]
    fn wait_timeout_before_any_reply_has_no_snapshot() {
        let budget = Duration::from_millis(250);
        for error in timeout_errors() {
            let run = scripted_wait(&after(&clock_state()), budget, [(budget, Err(error))]);
            let failure = run.result.unwrap_err().into_failure().unwrap();
            assert_eq!(failure.code, ControlFailureCode::Timeout);
            assert_eq!(failure.current_clock_state, None);
            assert_eq!(run.requests, 1);
            assert!(run.sleeps.is_empty());
        }
    }

    #[test]
    fn wait_timeout_retains_the_latest_reply_for_all_timeout_paths() {
        let first = clock_state();
        let mut latest = first.clone();
        latest.phase_tick += 1;
        latest.simulation_tick += 1;
        for error in timeout_errors() {
            let run = scripted_wait(
                &after(&latest),
                Duration::from_millis(25),
                [
                    (Duration::ZERO, Ok(first.clone())),
                    (Duration::ZERO, Ok(latest.clone())),
                    (Duration::from_millis(5), Err(error)),
                ],
            );
            let failure = run.result.unwrap_err().into_failure().unwrap();
            assert_eq!(failure.code, ControlFailureCode::Timeout);
            assert_eq!(failure.current_clock_state.as_ref(), Some(&latest));
            assert_eq!(run.requests, 3);
            assert_eq!(run.sleeps, [Duration::from_millis(10); 2]);
        }
    }

    #[test]
    fn wait_keeps_a_reply_that_consumes_the_remaining_budget() {
        let state = clock_state();
        let budget = Duration::from_millis(25);
        let run = scripted_wait(&after(&state), budget, [(budget, Ok(state.clone()))]);
        let failure = run.result.unwrap_err().into_failure().unwrap();
        assert_eq!(failure.code, ControlFailureCode::Timeout);
        assert_eq!(failure.current_clock_state, Some(state));
        assert_eq!(run.requests, 1);
        assert!(run.sleeps.is_empty());
    }

    #[test]
    fn wait_caps_retry_sleeps_to_the_shared_deadline() {
        let state = clock_state();
        let run = scripted_wait(
            &after(&state),
            Duration::from_millis(25),
            (0..3).map(|_| (Duration::ZERO, Ok(state.clone()))),
        );
        let failure = run.result.unwrap_err().into_failure().unwrap();
        assert_eq!(failure.code, ControlFailureCode::Timeout);
        assert_eq!(failure.current_clock_state, Some(state));
        assert_eq!(run.requests, 3);
        assert_eq!(run.sleeps, [10, 10, 5].map(Duration::from_millis));
    }

    #[test]
    fn wait_returns_a_matching_reply_without_an_extra_request() {
        let first = clock_state();
        let mut matching = first.clone();
        matching.phase_tick += 1;
        let run = scripted_wait(
            &after(&first),
            Duration::from_millis(25),
            [
                (Duration::from_millis(2), Ok(first)),
                (Duration::from_millis(2), Ok(matching.clone())),
            ],
        );
        assert_eq!(run.result.unwrap(), matching);
        assert_eq!(run.requests, 2);
        assert_eq!(run.sleeps, [Duration::from_millis(10)]);
    }

    #[test]
    fn wait_rejects_a_new_instance_with_its_snapshot() {
        let first = clock_state();
        let mut restarted = first.clone();
        restarted.scenario_revision += 1;
        let run = scripted_wait(
            &after(&first),
            Duration::from_millis(25),
            [
                (Duration::ZERO, Ok(first)),
                (Duration::ZERO, Ok(restarted.clone())),
            ],
        );
        let failure = run.result.unwrap_err().into_failure().unwrap();
        assert_eq!(failure.code, ControlFailureCode::StaleRevision);
        assert_eq!(failure.current_clock_state, Some(restarted));
        assert_eq!(run.requests, 2);
        assert_eq!(run.sleeps, [Duration::from_millis(10)]);
    }

    #[test]
    fn wait_does_not_relabel_other_errors_as_timeouts() {
        for error in [
            ControlClientError::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset)),
            ControlClientError::ServerMessage("Clock unavailable".into()),
            clock_failure(ControlFailureCode::WrongScreen, "paused".into(), None),
        ] {
            let expected = error.to_string();
            let run = scripted_wait(
                &after(&clock_state()),
                Duration::from_millis(25),
                [(Duration::ZERO, Err(error))],
            );
            assert_eq!(run.result.unwrap_err().to_string(), expected);
            assert_eq!(run.requests, 1);
            assert!(run.sleeps.is_empty());
        }
    }

    #[test]
    fn wait_with_zero_or_overflowing_budget_does_not_request_a_state() {
        for budget in [Duration::ZERO, Duration::MAX] {
            let run = scripted_wait(&after(&clock_state()), budget, []);
            let failure = run.result.unwrap_err().into_failure().unwrap();
            assert_eq!(failure.code, ControlFailureCode::Timeout);
            assert_eq!(failure.current_clock_state, None);
            assert_eq!(run.requests, 0);
            assert!(run.sleeps.is_empty());
        }
    }
}
