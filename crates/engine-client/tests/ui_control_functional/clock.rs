use super::*;
use spacewars_control::{ClockState, ClockStatePredicate, ClockTriggerRequest};

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn falling_digits_recover_and_survive_pause_restart_and_relaunch() {
    run_functional_test("clock-falling-digits", |harness| {
        let state = harness.wait_until_ready();
        let state = harness.activate_until_scenario("clock", state);
        let state = harness.activate_guarded("launcher.settings", &state);
        assert_eq!(
            control_value(&state, "launcher.settings.clock.event-profile.next"),
            Some("Calm")
        );
        let state =
            harness.activate_guarded("launcher.settings.clock.event-profile.previous", &state);
        assert_eq!(
            control_value(&state, "launcher.settings.clock.event-profile.next"),
            Some("Off")
        );
        let state = harness.activate_guarded("launcher.settings.clock.time-format.next", &state);
        harness.capture_screenshot("clock-settings.png");
        harness.activate_guarded("launcher.settings.start", &state);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, state.revision);
        let initial = harness.clock_state();
        assert_eq!(initial.profile, "off");
        assert_eq!(initial.next_event_tick, None);
        assert_eq!(initial.event_id, 0);
        assert_eq!((initial.body_count, initial.collider_count), (0, 0));
        let idle = harness.clock_wait(&initial, "idle", 0, 60);
        harness.capture_screenshot("clock-idle.png");
        harness.clock_trigger(&idle);
        let falling = harness.clock_wait(&idle, "falling", 1, 30);
        assert!(falling.body_count > 4 && falling.body_count <= 32);
        assert!(falling.collider_count > falling.body_count && falling.collider_count <= 100);
        harness.capture_screenshot("clock-falling.png");
        // Animation/time diagnostics must not invalidate unrelated UI guards.
        assert_eq!(harness.state(), gameplay);
        harness.clock_trigger_failure(&idle, ControlFailureCode::StaleRevision);
        harness.clock_trigger_failure(&falling, ControlFailureCode::ActionUnavailable);

        harness.pause_guarded(&gameplay);
        let paused_ui = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        let paused = harness.clock_state();
        assert!(paused.paused);
        assert_eq!(paused.phase, "falling");
        harness.clock_trigger_failure(&paused, ControlFailureCode::WrongScreen);
        let predicate = ClockStatePredicate {
            scenario_revision: paused.scenario_revision,
            phase: paused.phase.clone(),
            event_id: Some(paused.event_id),
            min_phase_tick: paused.phase_tick + 1,
        };
        let error = harness
            .client
            .wait_for_clock_state(&predicate, Duration::from_millis(250))
            .unwrap_err();
        harness.record_error("clock wait (paused animation must not advance)", &error);
        let failure = error.failure().unwrap();
        assert_eq!(failure.code, ControlFailureCode::Timeout);
        assert_eq!(failure.current_clock_state.as_ref(), Some(&paused));
        assert_eq!(harness.clock_state(), paused);
        harness.capture_screenshot("clock-paused.png");

        harness.activate_guarded("pause.resume", &paused_ui);
        harness.wait_clock_screen(UiScreen::Gameplay, paused_ui.revision);
        harness.clock_wait(&falling, "falling", 1, 175);
        harness.capture_screenshot("clock-floor.png");
        let reforming = harness.clock_wait(&falling, "reforming", 1, 35);
        assert_eq!((reforming.body_count, reforming.collider_count), (0, 0));
        harness.capture_screenshot("clock-reforming.png");
        let recovered = harness.clock_wait(&falling, "idle", 1, 0);
        assert!(recovered.can_trigger);
        assert_eq!((recovered.body_count, recovered.collider_count), (0, 0));
        let [hour, minute, _] = recovered.reading.unwrap();
        let hour = match hour % 12 {
            0 => 12,
            h => h,
        };
        assert_eq!(
            recovered.display_digits,
            [
                (hour >= 10).then_some(hour / 10),
                Some(hour % 10),
                Some(minute / 10),
                Some(minute % 10)
            ]
        );
        harness.capture_screenshot("clock-recovered.png");

        harness.clock_trigger(&recovered);
        let second = harness.clock_wait(&recovered, "falling", 2, 30);
        let gameplay = harness.state();
        harness.pause_guarded(&gameplay);
        let paused_ui = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        harness.activate_guarded("pause.restart", &paused_ui);
        let restarted_ui = harness.wait_clock_screen(UiScreen::Gameplay, paused_ui.revision);
        let restarted = harness.clock_state();
        assert_ne!(restarted.scenario_revision, second.scenario_revision);
        assert_eq!(restarted.event_id, 0);
        assert_eq!(restarted.body_count, 0);
        harness.clock_trigger_failure(&second, ControlFailureCode::StaleRevision);
        let error = harness
            .client
            .wait_for_clock_state(
                &ClockStatePredicate {
                    scenario_revision: second.scenario_revision,
                    phase: "idle".into(),
                    event_id: None,
                    min_phase_tick: 0,
                },
                TRANSITION_TIMEOUT,
            )
            .unwrap_err();
        harness.record_error("clock wait (stale instance)", &error);
        assert_eq!(
            error.failure().unwrap().code,
            ControlFailureCode::StaleRevision
        );

        harness.pause_guarded(&restarted_ui);
        let paused_ui = harness.wait_clock_screen(UiScreen::PauseMain, restarted_ui.revision);
        harness.activate_guarded("pause.return-to-launcher", &paused_ui);
        let launcher = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::LauncherMain),
                revision_after: Some(paused_ui.revision),
                ..UiStatePredicate::default()
            },
            TRANSITION_TIMEOUT,
        );
        let error = harness
            .client
            .clock_state_before(request_deadline())
            .unwrap_err();
        harness.record_error("clock state (launcher)", &error);
        assert_eq!(
            error.failure().unwrap().code,
            ControlFailureCode::ControlUnavailable
        );
        harness.clock_trigger_failure(&restarted, ControlFailureCode::ControlUnavailable);
        harness.activate_guarded("launcher.start", &launcher);
        harness.wait_clock_screen(UiScreen::Gameplay, launcher.revision);
        let relaunched = harness.clock_state();
        assert_ne!(relaunched.scenario_revision, restarted.scenario_revision);
        assert_eq!(relaunched.profile, "off");
        assert_eq!((relaunched.event_id, relaunched.body_count), (0, 0));
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn demo_profile_automatically_runs_multiple_bounded_events() {
    run_functional_test("clock-demo-events", |harness| {
        let state = harness.wait_until_ready();
        let state = harness.activate_until_scenario("clock", state);
        let state = harness.activate_guarded("launcher.settings", &state);
        let state = harness.activate_guarded("launcher.settings.clock.event-profile.next", &state);
        assert_eq!(
            control_value(&state, "launcher.settings.clock.event-profile.next"),
            Some("Demo")
        );
        harness.activate_guarded("launcher.settings.start", &state);
        harness.wait_clock_screen(UiScreen::Gameplay, state.revision);
        let initial = harness.clock_state();
        assert_eq!(initial.profile, "demo");
        assert!((360..=600).contains(&initial.next_event_tick.unwrap()));
        for event_id in 1..=2 {
            let falling = harness.clock_wait(&initial, "falling", event_id, 30);
            assert!((5..=32).contains(&falling.body_count));
            assert!(falling.collider_count <= 100);
            let recovered = harness.clock_wait(&initial, "idle", event_id, 0);
            assert_eq!((recovered.body_count, recovered.collider_count), (0, 0));
            let next_wait = recovered.next_event_tick.unwrap() - recovered.simulation_tick;
            assert!((350..=600).contains(&next_wait));
        }
        harness.capture_screenshot("clock-demo-recovered.png");
    });
}

impl FunctionalHarness {
    fn clock_state(&mut self) -> ClockState {
        let result = self.client.clock_state_before(request_deadline());
        self.require_clock("clock state", result)
    }

    fn clock_wait(
        &mut self,
        state: &ClockState,
        phase: &str,
        event_id: u64,
        min_phase_tick: u64,
    ) -> ClockState {
        let predicate = ClockStatePredicate {
            scenario_revision: state.scenario_revision,
            phase: phase.into(),
            event_id: Some(event_id),
            min_phase_tick,
        };
        let result = self
            .client
            .wait_for_clock_state(&predicate, Duration::from_secs(30));
        self.require_clock(&format!("clock wait {predicate:?}"), result)
    }

    fn clock_trigger(&mut self, state: &ClockState) {
        let request = ClockTriggerRequest::new(state);
        let result = self
            .client
            .clock_trigger_before(&request, request_deadline());
        let accepted = self.require_clock(&format!("clock trigger {request:?}"), result);
        assert!(accepted.trigger_pending);
        assert!(!accepted.can_trigger);
    }

    fn clock_trigger_failure(&mut self, state: &ClockState, code: ControlFailureCode) {
        let request = ClockTriggerRequest::new(state);
        let error = self
            .client
            .clock_trigger_before(&request, request_deadline())
            .unwrap_err();
        self.record_error(
            &format!("clock trigger {request:?} (expected rejection)"),
            &error,
        );
        assert_eq!(error.failure().unwrap().code, code);
    }

    fn require_clock(
        &mut self,
        command: &str,
        result: Result<ClockState, ControlClientError>,
    ) -> ClockState {
        match result {
            Ok(state) => {
                self.history.push(json!({ "elapsed_ms": self.elapsed_ms(), "command": command, "outcome": "ok", "clock_state": state }));
                state
            }
            Err(error) => {
                self.record_error(command, &error);
                panic!("{command} failed: {error}");
            }
        }
    }

    fn wait_clock_screen(&mut self, screen: UiScreen, revision: u64) -> UiState {
        self.wait_for(
            UiStatePredicate {
                screen: Some(screen),
                scenario: Some("clock".into()),
                revision_after: Some(revision),
            },
            TRANSITION_TIMEOUT,
        )
    }
}
