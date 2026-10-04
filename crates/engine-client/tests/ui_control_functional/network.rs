use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn network_settings_handle_missing_service_and_preserve_paused_session() {
    run_functional_test("network-settings", |h| {
        let launcher = h.wait_until_ready();
        let settings = h.activate_guarded("launcher.sound", &launcher);
        let mut network = h.activate_guarded("settings.network", &settings);
        assert_eq!(network.screen, UiScreen::LauncherNetwork);
        // Slint's visibility callback initializes the panel on the next UI
        // turn. Wait for its controls, not an assumed delay after activation.
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        while !control_ids(&network).contains(&"network.scan") {
            network = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::LauncherNetwork),
                    revision_after: Some(network.revision),
                    ..Default::default()
                },
                deadline.saturating_duration_since(Instant::now()),
            );
        }
        assert!(control_ids(&network).contains(&"network.scan"));
        assert!(!control_ids(&network).contains(&"network.refresh"));
        assert!(!control_ids(&network).contains(&"network.cancel"));
        h.expect_press_failure(UiAction::Start, Some(UiScreen::LauncherNetwork), None);
        h.capture_screenshot("launcher-network.png");
        let saved = h.activate("network.saved", Some(UiScreen::LauncherNetwork), None);
        assert!(control_ids(&saved).contains(&"network.refresh"));
        assert!(!control_ids(&saved).contains(&"network.forget"));
        assert!(
            !control_ids(&saved)
                .iter()
                .any(|id| id.starts_with("network.profile."))
        );
        h.capture_screenshot("launcher-saved-networks-unavailable.png");
        h.activate("network.refresh", Some(UiScreen::LauncherNetwork), None);
        let nearby = h.activate("network.back", Some(UiScreen::LauncherNetwork), None);
        assert!(control_ids(&nearby).contains(&"network.saved"));
        // Background availability updates can revise this page at any time.
        let settings = h.activate("network.back", Some(UiScreen::LauncherNetwork), None);
        assert_eq!(settings.screen, UiScreen::LauncherSound);
        assert_eq!(selected_control(&settings), "settings.network");
        let launcher = h.activate_guarded("sound.back", &settings);
        let launcher = h.activate_until_scenario("clock", launcher);
        h.activate_guarded("launcher.start", &launcher);
        let game = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        h.pause_guarded(&game);
        let pause = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        let settings = h.activate_guarded("pause.sound", &pause);
        let network = h.activate_guarded("settings.network", &settings);
        assert_eq!(network.screen, UiScreen::PauseNetwork);
        assert!(network.paused);
        assert_eq!(network.scenario_revision, game.scenario_revision);
        // Unsupported Start is rejected instead of bypassing network cleanup
        // or resuming a paused scenario.
        h.expect_press_failure(UiAction::Start, Some(UiScreen::PauseNetwork), None);
        h.capture_screenshot("pause-network.png");
        let saved = h.activate("network.saved", Some(UiScreen::PauseNetwork), None);
        assert!(saved.paused);
        assert_eq!(saved.scenario_revision, game.scenario_revision);
        assert!(control_ids(&saved).contains(&"network.refresh"));
        h.activate("network.back", Some(UiScreen::PauseNetwork), None);
        let settings = h.activate("network.back", Some(UiScreen::PauseNetwork), None);
        assert_eq!(settings.screen, UiScreen::PauseSound);
        assert!(settings.paused);
        assert_eq!(settings.scenario_revision, game.scenario_revision);
    });
}
