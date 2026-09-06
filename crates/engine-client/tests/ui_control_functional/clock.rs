use super::*;
use spacewars_control::{ClockEventKind, ClockState, ClockStatePredicate, ClockTriggerRequest};

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn live_clock_controls_preserve_events_preview_and_persist_across_restart_and_relaunch() {
    run_functional_test("clock-live-controls", |harness| {
        let state = harness.wait_until_ready();
        let state = harness.activate_until_scenario("clock", state);
        harness.activate_guarded("launcher.start", &state);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, state.revision);
        let initial = harness.clock_state();
        harness.clock_trigger(&initial);
        harness.clock_wait(&initial, "falling", 1, 30);
        // The same operation as the on-face touchscreen affordance.
        harness.activate_guarded("gameplay.clock-controls", &gameplay);
        let mut page = harness.wait_clock_screen(UiScreen::PauseClock, gameplay.revision);
        let paused = harness.clock_state();
        assert!(paused.paused && paused.body_count > 4);
        page = harness.change_clock_setting("pause.clock.time-format.next", "12-hour", &page);
        page = harness.change_clock_setting("pause.clock.event-profile.previous", "Off", &page);
        page = harness.change_clock_setting("pause.clock.falling", "Off", &page);
        page = harness.change_clock_setting("pause.clock.color-cycle", "Off", &page);
        let configured = harness.clock_state();
        assert_eq!(configured.scenario_revision, initial.scenario_revision);
        assert_eq!(configured.event_id, paused.event_id);
        assert_eq!(configured.phase_tick, paused.phase_tick);
        assert_eq!(configured.simulation_tick, paused.simulation_tick);
        assert_eq!(configured.body_count, paused.body_count);
        assert_eq!(configured.collider_count, paused.collider_count);
        assert_eq!(configured.profile, "off");
        assert!(configured.events.iter().all(|event| !event.enabled));
        assert_eq!(
            configured.settings.time_format,
            engine_common::ClockTimeFormat::TwelveHour
        );
        assert_eq!(
            harness
                .expect_activate_failure(
                    "pause.clock.preview",
                    Some(UiScreen::PauseClock),
                    Some(page.revision - 1)
                )
                .code,
            ControlFailureCode::StaleRevision
        );
        page = harness.activate_guarded("pause.clock.preview-event.next", &page);
        assert_eq!(
            control_value(&page, "pause.clock.preview-event.next"),
            Some("Color Cycle")
        );
        harness.capture_screenshot("clock-live-controls.png");
        harness.activate_guarded("pause.clock.preview", &page);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, page.revision);
        let color = harness.clock_wait(&configured, "cycling", 2, 35);
        assert_eq!(color.scenario_revision, initial.scenario_revision);
        assert_eq!((color.body_count, color.collider_count), (0, 0));
        assert!(!color.paused);
        assert_eq!(color.settings, configured.settings);
        harness.capture_screenshot("clock-live-preview.png");

        // Controller-style navigation reaches the full-width Clock button.
        harness.pause_guarded(&gameplay);
        let menu = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        let menu = harness.press_guarded(UiAction::Down, &menu);
        let menu = harness.press_guarded(UiAction::Down, &menu);
        assert_eq!(menu.selected_control.as_deref(), Some("pause.clock"));
        let page = harness.press_guarded(UiAction::Confirm, &menu);
        assert_eq!(page.screen, UiScreen::PauseClock);
        let page = harness.press_guarded(UiAction::Down, &page);
        let page = harness.press_guarded(UiAction::Down, &page);
        let page = harness.press_guarded(UiAction::Right, &page);
        assert_eq!(
            page.selected_control.as_deref(),
            Some("pause.clock.color-cycle")
        );
        let page = harness.activate_guarded("pause.clock.preview-event.previous", &page);
        harness.activate_guarded("pause.clock.preview", &page);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, page.revision);
        let falling = harness.clock_wait(&color, "falling", 3, 30);
        assert!(falling.body_count > 4 && falling.collider_count <= 100);

        harness.pause_guarded(&gameplay);
        let menu = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        harness.activate_guarded("pause.restart", &menu);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, menu.revision);
        let restarted = harness.clock_state();
        assert_ne!(restarted.scenario_revision, initial.scenario_revision);
        assert_eq!(restarted.settings, configured.settings);
        assert_eq!(restarted.event_id, 0);
        assert_eq!((restarted.body_count, restarted.collider_count), (0, 0));
        harness.pause_guarded(&gameplay);
        let menu = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        harness.activate_guarded("pause.return-to-launcher", &menu);
        let launcher = harness.wait_clock_screen(UiScreen::LauncherMain, menu.revision);
        let page = harness.activate_guarded("launcher.settings", &launcher);
        assert_eq!(
            control_value(&page, "launcher.settings.clock.time-format.next"),
            Some("12-hour")
        );
        assert_eq!(
            control_value(&page, "launcher.settings.clock.event-profile.next"),
            Some("Off")
        );
        harness.activate_guarded("launcher.settings.start", &page);
        harness.wait_clock_screen(UiScreen::Gameplay, page.revision);
        let relaunched = harness.clock_state();
        assert_ne!(relaunched.scenario_revision, restarted.scenario_revision);
        assert_eq!(relaunched.settings, configured.settings);
        let saved: engine_common::Settings = toml::from_str(
            &fs::read_to_string(harness.run_path().join("config/settings.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved.clock, configured.settings);
    });
}

impl FunctionalHarness {
    fn change_clock_setting(&mut self, id: &str, value: &str, state: &UiState) -> UiState {
        let pending = self.activate_guarded(id, state);
        let applied = self.wait_clock_screen(UiScreen::PauseClock, pending.revision);
        assert_eq!(control_value(&applied, id), Some(value));
        assert!(applied.controls.iter().all(|control| control.enabled));
        assert_eq!(applied.error, None);
        applied
    }
}

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
        assert_eq!(paused.phase.as_deref(), Some("falling"));
        harness.clock_trigger_failure(&paused, ControlFailureCode::WrongScreen);
        let predicate = ClockStatePredicate {
            scenario_revision: paused.scenario_revision,
            lifecycle: Some("active".into()),
            event_kind: paused.event_kind,
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
                    lifecycle: Some("idle".into()),
                    event_kind: None,
                    phase: None,
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
        assert_eq!(
            initial
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
            ClockEventKind::ALL
        );
        assert!(initial.events.iter().all(|event| event.enabled));
        assert!((360..=600).contains(&initial.next_event_tick.unwrap()));
        let mut previous = None;
        for event_id in 1..=2 {
            let active = harness.clock_wait(&initial, "active", event_id, 30);
            assert_ne!(active.event_kind, previous);
            previous = active.event_kind;
            if active.event_kind == Some(ClockEventKind::Falling) {
                assert!((5..=32).contains(&active.body_count));
                assert!(active.collider_count <= 100);
            } else {
                assert_eq!(active.event_kind, Some(ClockEventKind::ColorCycle));
                assert_eq!((active.body_count, active.collider_count), (0, 0));
            }
            let recovered = harness.clock_wait(&initial, "idle", event_id, 0);
            assert_eq!((recovered.body_count, recovered.collider_count), (0, 0));
            let next_wait = recovered.next_event_tick.unwrap() - recovered.simulation_tick;
            assert!((350..=600).contains(&next_wait));
        }
        harness.capture_screenshot("clock-demo-recovered.png");
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn color_cycle_preview_preserves_time_and_resets_after_pause_restart_and_relaunch() {
    run_functional_test("clock-color-cycle", |harness| {
        let state = harness.wait_until_ready();
        let state = harness.activate_until_scenario("clock", state);
        let mut state = harness.activate_guarded("launcher.settings", &state);
        state = harness.activate_guarded("launcher.settings.clock.event-profile.next", &state);
        for id in [
            "launcher.settings.clock.falling.next",
            "launcher.settings.clock.color-cycle.next",
        ] {
            assert_eq!(control_value(&state, id), Some("On"));
            state = harness.activate_guarded(id, &state);
            assert_eq!(control_value(&state, id), Some("Off"));
        }
        harness.capture_screenshot("clock-event-switches.png");
        harness.activate_guarded("launcher.settings.start", &state);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, state.revision);
        let initial = harness.clock_state();
        assert_eq!(initial.profile, "demo");
        assert!(initial.events.iter().all(|event| !event.enabled));
        assert_eq!(initial.next_event_tick, None);
        harness.clock_trigger_event(&initial, ClockEventKind::ColorCycle);
        let cycling = harness.clock_wait(&initial, "cycling", 1, 72);
        assert_eq!(cycling.event_kind, Some(ClockEventKind::ColorCycle));
        assert_eq!((cycling.body_count, cycling.collider_count), (0, 0));
        assert_ne!(cycling.palette_rgb, initial.palette_rgb);
        let [hour, minute, _] = cycling.reading.unwrap();
        assert_eq!(
            cycling.display_digits,
            [
                Some(hour / 10),
                Some(hour % 10),
                Some(minute / 10),
                Some(minute % 10)
            ]
        );
        assert_eq!(harness.state(), gameplay);
        harness.capture_screenshot("clock-color-cycle.png");
        // Cross-event requests must obey the same busy/stale/paused guards.
        harness.clock_trigger_failure(&initial, ControlFailureCode::StaleRevision);
        harness.clock_trigger_failure(&cycling, ControlFailureCode::ActionUnavailable);
        harness.pause_guarded(&gameplay);
        let pause_ui = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        let paused = harness.clock_state();
        harness.clock_trigger_failure(&paused, ControlFailureCode::WrongScreen);
        let error = harness
            .client
            .wait_for_clock_state(
                &ClockStatePredicate {
                    scenario_revision: paused.scenario_revision,
                    lifecycle: Some("active".into()),
                    event_kind: Some(ClockEventKind::ColorCycle),
                    phase: Some("cycling".into()),
                    event_id: Some(1),
                    min_phase_tick: paused.phase_tick + 1,
                },
                Duration::from_millis(250),
            )
            .unwrap_err();
        harness.record_error("color wait (paused animation must not advance)", &error);
        assert_eq!(error.failure().unwrap().code, ControlFailureCode::Timeout);
        assert_eq!(harness.clock_state(), paused);
        harness.activate_guarded("pause.resume", &pause_ui);
        let recovered = harness.clock_wait(&initial, "idle", 1, 0);
        assert_eq!(recovered.palette_rgb, initial.palette_rgb);
        assert_eq!(recovered.event_kind, None);
        assert_eq!(recovered.next_event_tick, None);
        harness.capture_screenshot("clock-color-recovered.png");

        harness.clock_trigger_event(&recovered, ClockEventKind::ColorCycle);
        let second = harness.clock_wait(&initial, "cycling", 2, 60);
        let gameplay = harness.state();
        harness.pause_guarded(&gameplay);
        let pause_ui = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        harness.activate_guarded("pause.restart", &pause_ui);
        let gameplay = harness.wait_clock_screen(UiScreen::Gameplay, pause_ui.revision);
        let restarted = harness.clock_state();
        assert_ne!(restarted.scenario_revision, second.scenario_revision);
        assert_eq!(restarted.event_id, 0);
        assert_eq!(restarted.palette_rgb, initial.palette_rgb);
        assert_eq!((restarted.body_count, restarted.collider_count), (0, 0));
        harness.clock_trigger_failure(&second, ControlFailureCode::StaleRevision);

        harness.pause_guarded(&gameplay);
        let pause_ui = harness.wait_clock_screen(UiScreen::PauseMain, gameplay.revision);
        harness.activate_guarded("pause.return-to-launcher", &pause_ui);
        let launcher = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::LauncherMain),
                revision_after: Some(pause_ui.revision),
                ..UiStatePredicate::default()
            },
            TRANSITION_TIMEOUT,
        );
        let settings = harness.activate_guarded("launcher.settings", &launcher);
        for id in [
            "launcher.settings.clock.falling.next",
            "launcher.settings.clock.color-cycle.next",
        ] {
            assert_eq!(control_value(&settings, id), Some("Off"));
        }
        harness.activate_guarded("launcher.settings.start", &settings);
        harness.wait_clock_screen(UiScreen::Gameplay, settings.revision);
        let relaunched = harness.clock_state();
        assert_ne!(relaunched.scenario_revision, restarted.scenario_revision);
        assert_eq!(relaunched.event_id, 0);
        assert_eq!(relaunched.palette_rgb, initial.palette_rgb);
        assert!(relaunched.events.iter().all(|event| !event.enabled));
        assert_eq!(relaunched.next_event_tick, None);
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
            lifecycle: Some(
                if ["idle", "active", "cooldown"].contains(&phase) {
                    phase
                } else {
                    "active"
                }
                .into(),
            ),
            event_kind: None,
            phase: (!["idle", "active", "cooldown"].contains(&phase)).then(|| phase.into()),
            event_id: Some(event_id),
            min_phase_tick,
        };
        let result = self
            .client
            .wait_for_clock_state(&predicate, Duration::from_secs(30));
        self.require_clock(&format!("clock wait {predicate:?}"), result)
    }

    fn clock_trigger(&mut self, state: &ClockState) {
        self.clock_trigger_event(state, ClockEventKind::Falling);
    }

    fn clock_trigger_event(&mut self, state: &ClockState, event: ClockEventKind) {
        let request = ClockTriggerRequest::new(state, event);
        let result = self
            .client
            .clock_trigger_before(&request, request_deadline());
        let accepted = self.require_clock(&format!("clock trigger {request:?}"), result);
        assert!(accepted.trigger_pending);
        assert!(!accepted.can_trigger);
    }

    fn clock_trigger_failure(&mut self, state: &ClockState, code: ControlFailureCode) {
        let request = ClockTriggerRequest::new(state, ClockEventKind::Falling);
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
