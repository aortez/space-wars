use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn sound_controls_persist_across_scenarios_restart_and_process_restart() {
    run_functional_test("sound-controls", |harness| {
        let launcher = harness.wait_until_ready();
        let sound = harness.activate_guarded("launcher.sound", &launcher);
        assert_eq!(sound.screen, UiScreen::LauncherSound);
        assert_eq!(control_value(&sound, "sound.volume.next"), Some("25%"));
        harness.activate_guarded("sound.volume.previous", &sound);
        let mut sound = harness.wait_sound_save("saved");
        // Cabinet amplifiers need useful stops below 10%, including durable
        // persistence across the same error/retry and restart paths.
        for percent in (9..=19).rev() {
            harness.activate_guarded("sound.volume.previous", &sound);
            sound = harness.wait_sound_save("saved");
            assert_eq!(
                control_value(&sound, "sound.volume.next"),
                Some(format!("{percent}%").as_str())
            );
        }
        let sound = harness.activate_guarded("sound.mute", &sound);
        assert_eq!(control_value(&sound, "sound.mute"), Some("on"));
        let sound = harness.wait_sound_save("saved");
        harness.capture_screenshot("launcher-sound.png");
        let launcher = harness.activate_guarded("sound.back", &sound);
        let launcher = harness.activate_until_scenario("falling", launcher);
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
        let sound = harness.activate_guarded("pause.sound", &pause);
        assert_eq!(sound.screen, UiScreen::PauseSound);
        assert!(sound.paused);
        assert_eq!(control_value(&sound, "sound.volume.next"), Some("9%"));
        assert_eq!(control_value(&sound, "sound.mute"), Some("on"));

        // Force a durable-save error without permissions assumptions.
        let path = harness.run_path().join("config/settings.toml");
        let backup = harness.run_path().join("settings-backup.toml");
        fs::rename(&path, &backup).unwrap();
        fs::create_dir(&path).unwrap();
        harness.activate_guarded("sound.volume.next", &sound);
        let failed = harness.wait_sound_save("error");
        assert_eq!(control_value(&failed, "sound.volume.next"), Some("10%"));
        assert!(failed.error.is_some());
        assert!(failed.paused);
        harness.capture_screenshot("sound-save-error.png");
        fs::remove_dir(&path).unwrap();
        fs::rename(&backup, &path).unwrap();
        harness.activate_guarded("sound.retry", &failed);
        let saved = harness.wait_sound_save("saved");
        assert!(saved.error.is_none());
        let stored: engine_common::Settings =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored.audio.master_volume, 0.10);
        assert!(stored.audio.muted);
        assert_eq!(stored.launch.scenario, "falling");
        harness.capture_screenshot("pause-sound.png");

        let pause = harness.activate_guarded("sound.back", &saved);
        assert_eq!(pause.screen, UiScreen::PauseMain);
        assert!(pause.paused, "Back must not resume a loud game");
        harness.activate_guarded("pause.restart", &pause);
        let gameplay = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        assert_ne!(gameplay.scenario_revision, saved.scenario_revision);
        harness.pause_guarded(&gameplay);
        let pause = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        harness.activate_guarded("pause.return-to-launcher", &pause);
        let launcher = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::LauncherMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
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
        let sound = harness.activate_guarded("pause.sound", &pause);
        assert_eq!(
            sound.screen,
            UiScreen::PauseSound,
            "Clock has a different pause menu layout"
        );
        assert_eq!(control_value(&sound, "sound.mute"), Some("on"));
        assert_eq!(control_value(&sound, "sound.volume.next"), Some("10%"));
        let pause = harness.activate_guarded("sound.back", &sound);
        harness.activate_guarded("pause.return-to-launcher", &pause);
        let launcher = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::LauncherMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        // Graceful exit/restart using the same isolated config and socket.
        harness.activate_guarded("launcher.quit", &launcher);
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        while harness.child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "client did not quit");
            thread::sleep(POLL_INTERVAL);
        }
        let log = File::create(harness.run_path().join("restarted-client.log")).unwrap();
        harness.child = OwnedChild(
            Command::new(env!("CARGO_BIN_EXE_engine-client"))
                .arg("--config-dir")
                .arg(harness.run_path().join("config"))
                .env("SPACEWARS_CONTROL_SOCKET", &harness._socket_path.0)
                .env("SLINT_BACKEND", "winit-software")
                .env_remove("WAYLAND_DISPLAY")
                .stdin(Stdio::null())
                .stderr(Stdio::from(log.try_clone().unwrap()))
                .stdout(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let launcher = harness.wait_until_ready();
        let sound = harness.activate_guarded("launcher.sound", &launcher);
        assert_eq!(control_value(&sound, "sound.volume.next"), Some("10%"));
        assert_eq!(control_value(&sound, "sound.mute"), Some("on"));
        assert_eq!(control_value(&sound, "sound.save-status"), Some("saved"));
    });
}

impl FunctionalHarness {
    fn wait_sound_save(&mut self, expected: &str) -> UiState {
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        loop {
            let state = self.state();
            if control_value(&state, "sound.save-status") == Some(expected) {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "settings save did not reach {expected}: {state:?}"
            );
            thread::sleep(POLL_INTERVAL);
        }
    }
}
