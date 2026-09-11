use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn device_info_is_shared_read_only_scrollable_and_returns_to_its_parent() {
    run_functional_test("device-info", |harness| {
        let launcher = harness.wait_until_ready();
        let settings = harness.activate_guarded("launcher.sound", &launcher);
        let info = harness.activate_guarded("settings.device-info", &settings);
        assert_eq!(info.screen, UiScreen::LauncherInfo);
        let info = harness.wait_info_ready();
        assert!(info.active_scenario.is_none());
        assert_eq!(selected_control(&info), "info.back");
        assert!(
            info.controls
                .iter()
                .any(|control| control.id == "info.version" && !control.enabled)
        );
        assert!(control_value(&info, "info.controllers").is_some());
        #[cfg(target_os = "linux")]
        {
            assert!(!control_value(&info, "info.hostname").unwrap().is_empty());
            assert!(
                control_value(&info, "info.memory")
                    .unwrap()
                    .contains("total")
            );
            assert!(
                control_value(&info, "info.storage")
                    .unwrap()
                    .contains("available")
            );
        }
        harness.capture_screenshot("launcher-info.png");
        // Telemetry deliberately changes revisions. Navigation depends on the
        // screen, not the CPU sample that happened to be visible before a PNG.
        let info = harness.press(UiAction::Down, Some(UiScreen::LauncherInfo), None);
        assert!(
            control_value(&info, "info.scroll-offset")
                .unwrap()
                .parse::<i32>()
                .unwrap()
                < 0
        );
        let info = harness.activate("info.scroll-up", Some(UiScreen::LauncherInfo), None);
        assert_eq!(control_value(&info, "info.scroll-offset"), Some("0"));
        let settings = harness.press(UiAction::Back, Some(UiScreen::LauncherInfo), None);
        assert_eq!(settings.screen, UiScreen::LauncherSound);
        assert_eq!(selected_control(&settings), "settings.device-info");
        let launcher = harness.activate_guarded("sound.back", &settings);
        assert_eq!(selected_control(&launcher), "launcher.sound");

        let launcher = harness.activate_until_scenario("clock", launcher);
        harness.activate_guarded("launcher.start", &launcher);
        let gameplay = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        harness.pause_guarded(&gameplay);
        let pause = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        let settings = harness.activate_guarded("pause.sound", &pause);
        let info = harness.activate_guarded("settings.device-info", &settings);
        assert_eq!(info.screen, UiScreen::PauseInfo);
        let info = harness.wait_info_ready();
        assert!(info.paused);
        assert_eq!(info.scenario_revision, gameplay.scenario_revision);
        assert_eq!(
            control_value(&info, "info.scenario"),
            Some("clock · paused")
        );
        harness.capture_screenshot("pause-info.png");
        let settings = harness.activate("info.back", Some(UiScreen::PauseInfo), None);
        assert_eq!(settings.screen, UiScreen::PauseSound);
        assert!(settings.paused);
        assert_eq!(selected_control(&settings), "settings.device-info");
        let pause = harness.press_guarded(UiAction::Back, &settings);
        assert_eq!(pause.screen, UiScreen::PauseMain);
        assert_eq!(selected_control(&pause), "pause.sound");
        assert!(pause.paused);
        // Start remains the explicit shortcut out of all paused menu levels.
        let settings = harness.activate_guarded("pause.sound", &pause);
        harness.activate_guarded("settings.device-info", &settings);
        harness.press(UiAction::Start, Some(UiScreen::PauseInfo), None);
        let resumed = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        assert!(!resumed.paused);
        assert_eq!(resumed.scenario_revision, gameplay.scenario_revision);
    });
}

impl FunctionalHarness {
    fn wait_info_ready(&mut self) -> UiState {
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        loop {
            let state = self.state();
            if control_value(&state, "info.status") == Some("ready") {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "device info never became ready: {state:?}"
            );
            thread::sleep(POLL_INTERVAL);
        }
    }
}
