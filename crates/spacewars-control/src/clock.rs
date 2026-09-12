//! Public Clock controls. Animation ticks deliberately do not change UI revision.
use crate::{ControlClient, ControlClientError, ControlFailure, ControlFailureCode, ProtocolError};
pub use engine_common::{ClockEventKind, ClockMarqueeMessage};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const CLOCK_STATE_COMMAND: &str = "clock state";
pub const CLOCK_TRIGGER_COMMAND: &str = "clock trigger";
pub const CLOCK_MESSAGE_COMMAND: &str = "clock message";
pub const CLOCK_STATE_SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockEventInfo {
    pub kind: ClockEventKind,
    pub label: String,
    pub effect: String,
    pub trigger: engine_common::ClockEventTrigger,
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
    pub meltdown: Option<engine_common::ClockMeltdownState>,
    pub duck: Option<engine_common::ClockDuckState>,
    pub marquee: Option<engine_common::ClockMarqueeState>,
    pub digit_slide: Option<engine_common::ClockDigitSlideState>,
    pub reading: Option<[u8; 3]>,
    /// Latest target digits, including during a fall. Blank 12-hour slots are null.
    pub display_digits: [Option<u8>; 4],
    pub can_trigger: bool,
    pub trigger_pending: bool,
    pub settings_pending: bool,
    /// The settings are effective for this session, but persistence failed.
    pub settings_error: Option<String>,
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

/// Change only the saved marquee message, leaving all other Clock settings and
/// the active animation untouched. Requires a paused Clock and a fresh message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockMessageRequest {
    pub schema_version: u32,
    pub message: ClockMarqueeMessage,
    pub expected_scenario_revision: u64,
    pub expected_message: ClockMarqueeMessage,
}

impl ClockMessageRequest {
    pub fn new(state: &ClockState, message: ClockMarqueeMessage) -> Self {
        Self {
            schema_version: CLOCK_STATE_SCHEMA_VERSION,
            message,
            expected_scenario_revision: state.scenario_revision,
            expected_message: state.settings.marquee_message,
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

    /// Acknowledges a queued settings change. Use wait_for_clock_message to
    /// confirm application and persistence at the next host boundary.
    pub fn clock_message_before(
        &self,
        request: &ClockMessageRequest,
        deadline: Instant,
    ) -> Result<ClockState, ControlClientError> {
        Ok(ClockState::from_json(&self.request_before(
            &format!("{CLOCK_MESSAGE_COMMAND}\n{}\n", request.to_json()?),
            deadline,
        )?)?)
    }

    pub fn wait_for_clock_message(
        &self,
        request: &ClockMessageRequest,
        timeout: Duration,
    ) -> Result<ClockState, ControlClientError> {
        let state = wait_for_clock_with(
            request.expected_scenario_revision,
            "marquee message to be applied",
            |state| message_applied(request, state),
            timeout,
            Instant::now,
            |deadline| self.clock_state_before(deadline),
            std::thread::sleep,
        )?;
        if let Some(error) = &state.settings_error {
            return Err(clock_failure(
                ControlFailureCode::ActionUnavailable,
                error.clone(),
                Some(state),
            ));
        }
        Ok(state)
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
    now: impl FnMut() -> Instant,
    request: impl FnMut(Instant) -> Result<ClockState, ControlClientError>,
    sleep: impl FnMut(Duration),
) -> Result<ClockState, ControlClientError> {
    wait_for_clock_with(
        predicate.scenario_revision,
        &format!("{predicate:?}"),
        |state| predicate.matches(state),
        timeout,
        now,
        request,
        sleep,
    )
}

fn message_applied(request: &ClockMessageRequest, state: &ClockState) -> bool {
    !state.settings_pending && state.settings.marquee_message == request.message
}

fn wait_for_clock_with(
    scenario_revision: u64,
    description: &str,
    matches: impl Fn(&ClockState) -> bool,
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
            Ok(state) if state.scenario_revision != scenario_revision => {
                return Err(clock_failure(
                    ControlFailureCode::StaleRevision,
                    "Clock instance changed while waiting".into(),
                    Some(state),
                ));
            }
            Ok(state) if matches(&state) => return Ok(state),
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
        format!("Timed out waiting for Clock {description}"),
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
            meltdown: None,
            duck: None,
            marquee: None,
            digit_slide: None,
            reading: Some([12, 34, 56]),
            display_digits: [Some(1), Some(2), Some(3), Some(4)],
            can_trigger: false,
            trigger_pending: false,
            settings_pending: false,
            settings_error: None,
        }
    }

    #[test]
    fn message_protocol_validates_text_versions_and_required_guards() {
        let state = clock_state();
        let request = ClockMessageRequest::new(&state, "hello, pi!".parse().unwrap());
        let json = request.to_json().unwrap();
        assert!(json.contains("HELLO, PI!"));
        assert_eq!(ClockMessageRequest::from_json(&json).unwrap(), request);
        for invalid in ["", "   ", "a\nb", "é", &"A".repeat(33)] {
            let mut value = serde_json::to_value(&request).unwrap();
            value["message"] = invalid.into();
            assert!(ClockMessageRequest::from_json(&value.to_string()).is_err());
        }
        for field in [
            "schema_version",
            "expected_scenario_revision",
            "expected_message",
            "message",
        ] {
            let mut value = serde_json::to_value(&request).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(ClockMessageRequest::from_json(&value.to_string()).is_err());
        }
        let mut stale = request;
        stale.schema_version -= 1;
        assert!(stale.to_json().is_err());
        assert!(ClockMessageRequest::from_json(&serde_json::to_string(&stale).unwrap()).is_err());
    }

    #[test]
    fn message_wait_checks_the_applied_value_not_just_queue_acknowledgement() {
        let mut state = clock_state();
        let request = ClockMessageRequest::new(&state, "HELLO!".parse().unwrap());
        assert!(!message_applied(&request, &state));
        state.settings.marquee_message = request.message;
        state.settings_pending = true;
        assert!(!message_applied(&request, &state));
        state.settings_pending = false;
        assert!(message_applied(&request, &state));
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
            r#"{"schema_version":3,"event":"falling","expected_scenario_revision":7,"expected_event_id":3}"#,
            r#"{"schema_version":4,"event":"falling","expected_scenario_revision":7,"expected_event_id":3}"#,
            r#"{"schema_version":5,"event":"falling","expected_scenario_revision":7,"expected_event_id":3}"#,
        ] {
            assert!(ClockTriggerRequest::from_json(json).is_err());
        }
    }

    #[test]
    fn meltdown_diagnostics_and_named_trigger_round_trip() {
        let mut state = clock_state();
        state.event_kind = Some(ClockEventKind::Meltdown);
        state.phase = Some("draining".into());
        state.meltdown = Some(engine_common::ClockMeltdownState {
            initial_cells: 70,
            water_columns: 60,
            pooled_microunits: 30_000_000,
            spilling_microunits: 5_000_000,
            displaced_microunits: 2_000_000,
            spill_parcels: 12,
            capacity_limited_ticks: 3,
            drained_microunits: 35_000_000,
            ..Default::default()
        });
        assert_eq!(
            ClockState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        let request = ClockTriggerRequest::new(&state, ClockEventKind::Meltdown);
        assert_eq!(
            ClockTriggerRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );
        let mut legacy = serde_json::to_value(&state).unwrap();
        let material = legacy["meltdown"].as_object_mut().unwrap();
        for field in [
            "spilling_microunits",
            "displaced_microunits",
            "spill_parcels",
            "capacity_limited_ticks",
        ] {
            material.remove(field);
        }
        let restored: ClockState = serde_json::from_value(legacy).unwrap();
        let material = restored.meltdown.unwrap();
        assert_eq!(material.spilling_microunits, 0);
        assert_eq!(material.displaced_microunits, 0);
        assert_eq!(material.spill_parcels, 0);
        assert_eq!(material.capacity_limited_ticks, 0);
    }

    #[test]
    fn duck_diagnostics_and_named_trigger_round_trip() {
        let mut state = clock_state();
        state.event_kind = Some(ClockEventKind::Duck);
        state.phase = Some("resetting".into());
        state.duck = Some(engine_common::ClockDuckState {
            left_to_right: false,
            position_milli: None,
            grounded: false,
            jumps: 3,
            cleared_obstacles: 3,
            obstacle_count: 3,
            entrance_open_milli: 0,
            exit_open_milli: 700,
            outcome: Some(engine_common::ClockDuckOutcome::Exited),
            navigation: Some(engine_common::ClockDuckNavigationState {
                jump_profile: engine_common::ClockDuckJumpProfile::Flowing,
                course_seed: 42,
                behavior: engine_common::ClockDuckBehavior::Exiting,
                facing_right: false,
                wall_tags: [2, 2],
                calibrated_jumps: 2,
                speed_samples: 9,
                jump_height_milli: Some(35_000),
                flight_ticks: Some(51),
                run_speed_milli: Some(160_000),
                target_obstacle: None,
                spawned_ticks: 1650,
                exit_visible: true,
                body_radius_milli: 8000,
                planning: Some(engine_common::ClockDuckPlanningState {
                    pattern: engine_common::ClockDuckCoursePattern::Shortcut,
                    surface_count: 5,
                    support: Some(1),
                    plan: Some(engine_common::ClockDuckPlanState {
                        source: 1,
                        target: 2,
                        takeoff_milli: [-100_000, -150_000],
                        landing_milli: [0, -140_000],
                        flight_ticks: 45,
                        cruise_speed_milli: 120_000,
                        running_takeoff: true,
                        next_target: Some(3),
                    }),
                    confirmed_landings: 10,
                    undershoots: 1,
                    overshoots: 0,
                    wrong_surface_landings: 0,
                    rejected_plans: 1,
                    rejection: Some(engine_common::ClockDuckRejection::OutOfRange),
                    acceleration_milli: Some(960_000),
                    generation_attempts: 2,
                    fallback_course: false,
                    running_jumps: 10,
                    flowing_fallbacks: 1,
                    moving_landings: 10,
                    skipped_platforms: 2,
                }),
            }),
        });
        assert_eq!(
            ClockState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        let request = ClockTriggerRequest::new(&state, ClockEventKind::Duck);
        assert_eq!(
            ClockTriggerRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );
        // The two-profile diagnostics remain additive to the careful planner.
        let mut careful_payload = serde_json::to_value(&state).unwrap();
        let navigation = careful_payload["duck"]["navigation"]
            .as_object_mut()
            .unwrap();
        navigation.remove("jump_profile");
        let planning = navigation["planning"].as_object_mut().unwrap();
        for field in [
            "running_jumps",
            "flowing_fallbacks",
            "moving_landings",
            "skipped_platforms",
            "pattern",
        ] {
            planning.remove(field);
        }
        let plan = planning["plan"].as_object_mut().unwrap();
        plan.remove("running_takeoff");
        plan.remove("next_target");
        let older = ClockState::from_json(&careful_payload.to_string()).unwrap();
        let navigation = older.duck.unwrap().navigation.unwrap();
        assert_eq!(
            navigation.jump_profile,
            engine_common::ClockDuckJumpProfile::Careful
        );
        let planning = navigation.planning.unwrap();
        assert_eq!(
            planning.pattern,
            engine_common::ClockDuckCoursePattern::Platforms
        );
        assert_eq!(planning.skipped_platforms, 0);
        assert_eq!(
            (
                planning.running_jumps,
                planning.flowing_fallbacks,
                planning.moving_landings
            ),
            (0, 0, 0)
        );
        assert!(!planning.plan.unwrap().running_takeoff);
        assert_eq!(planning.plan.unwrap().next_target, None);
        // Navigation is an additive schema-8 field; older clients/recordings
        // can still exchange the original Duck diagnostics.
        let mut old_payload = serde_json::to_value(&state).unwrap();
        old_payload["duck"]
            .as_object_mut()
            .unwrap()
            .remove("navigation");
        state.duck.as_mut().unwrap().navigation = None;
        assert_eq!(
            ClockState::from_json(&old_payload.to_string()).unwrap(),
            state
        );
    }

    #[test]
    fn digit_slide_diagnostics_trigger_metadata_and_guards_round_trip() {
        let mut state = clock_state();
        state.event_kind = Some(ClockEventKind::DigitSlide);
        state.phase = Some("sliding".into());
        state.digit_slide = Some(engine_common::ClockDigitSlideState {
            from_digits: [Some(2), Some(3), Some(5), Some(9)],
            to_digits: [Some(0); 4],
            changed_slots: [true; 4],
            progress_milli: 500,
            preview: false,
        });
        state.events.push(ClockEventInfo {
            kind: ClockEventKind::DigitSlide,
            label: "Digit Slide".into(),
            effect: "digit-geometry".into(),
            trigger: engine_common::ClockEventTrigger::TimeChange,
            duration_ticks: 48,
            cooldown_ticks: 120,
            enabled: true,
            automatic_ready_at_tick: 0,
        });
        assert_eq!(
            ClockState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        let request = ClockTriggerRequest::new(&state, ClockEventKind::DigitSlide);
        assert_eq!(
            ClockTriggerRequest::from_json(&request.to_json().unwrap()).unwrap(),
            request
        );
        for version in 1..CLOCK_STATE_SCHEMA_VERSION {
            let mut stale = request.clone();
            stale.schema_version = version;
            assert!(
                ClockTriggerRequest::from_json(&serde_json::to_string(&stale).unwrap()).is_err()
            );
        }
    }

    #[test]
    fn marquee_diagnostics_settings_and_named_trigger_round_trip() {
        for preset in engine_common::ClockMarqueePreset::ALL {
            let mut state = clock_state();
            state.settings.marquee_preset = preset;
            state.event_kind = Some(ClockEventKind::Marquee);
            state.phase = Some("presenting".into());
            state.marquee = Some(engine_common::ClockMarqueeState {
                preset,
                content: "SPACE WARS".into(),
                cell_count: 150,
                group_count: 10,
                progress_milli: 400,
                scrolling: true,
                waving: true,
                rotation_target: None,
                lighting: "sweep".into(),
            });
            assert_eq!(
                ClockState::from_json(&state.to_json().unwrap()).unwrap(),
                state
            );
            let request = ClockTriggerRequest::new(&state, ClockEventKind::Marquee);
            assert_eq!(
                ClockTriggerRequest::from_json(&request.to_json().unwrap()).unwrap(),
                request
            );
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
