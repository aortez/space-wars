use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn controllers_are_reachable_from_launcher_and_pause_without_resuming() {
    run_functional_test("controllers", |h| {
        let launcher = h.wait_until_ready();
        let settings = h.activate_guarded("launcher.sound", &launcher);
        let controllers = h.activate_guarded("settings.controllers", &settings);
        assert_eq!(controllers.screen, UiScreen::LauncherControllers);
        assert!(control_ids(&controllers).contains(&"controllers.back"));
        assert!(control_value(&controllers, "controllers.detail").is_some());
        assert!(control_value(&controllers, "controllers.players").is_some());
        assert!(control_ids(&controllers).contains(&"controllers.reset-players"));
        // Works without attached hardware too; resetting player preferences
        // must not leave setup, resume gameplay, or reset button profiles.
        let controllers = h.activate_guarded("controllers.reset-players", &controllers);
        assert_eq!(controllers.screen, UiScreen::LauncherControllers);
        h.capture_screenshot("launcher-controllers.png");
        let settings = h.activate_guarded("controllers.back", &controllers);
        assert_eq!(settings.screen, UiScreen::LauncherSound);
        assert_eq!(selected_control(&settings), "settings.controllers");
        // New row participates in controller-only navigation and scrolling.
        let back = h.press_guarded(UiAction::Down, &settings);
        assert_eq!(selected_control(&back), "sound.back");
        let settings = h.press_guarded(UiAction::Up, &back);
        assert_eq!(selected_control(&settings), "settings.controllers");
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
        let paused = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        let settings = h.activate_guarded("pause.sound", &paused);
        let controllers = h.activate_guarded("settings.controllers", &settings);
        assert_eq!(controllers.screen, UiScreen::PauseControllers);
        assert!(controllers.paused);
        let controllers = h.activate_guarded("controllers.reset-players", &controllers);
        assert!(controllers.paused);
        assert_eq!(controllers.scenario_revision, game.scenario_revision);
        h.capture_screenshot("pause-controllers.png");
        let settings = h.press_guarded(UiAction::Back, &controllers);
        assert_eq!(settings.screen, UiScreen::PauseSound);
        assert!(settings.paused);
        assert_eq!(settings.scenario_revision, game.scenario_revision);
    });
}
