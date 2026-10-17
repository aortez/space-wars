use super::*;

const TOGGLE: &str = "launcher.scenarios.show_all";

fn stored(h: &FunctionalHarness) -> engine_common::Settings {
    toml::from_str(&fs::read_to_string(h.run_path().join("config/settings.toml")).unwrap()).unwrap()
}

fn restart(h: &mut FunctionalHarness, state: &UiState, arguments: &[&str]) -> UiState {
    h.activate_guarded("launcher.quit", state);
    let deadline = Instant::now() + TRANSITION_TIMEOUT;
    while h.child.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "client did not quit");
        thread::sleep(POLL_INTERVAL);
    }
    let log = File::create(h.run_path().join("restarted-client.log")).unwrap();
    h.child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_engine-client"))
            .arg("--config-dir")
            .arg(h.run_path().join("config"))
            .args(arguments)
            .env("SPACEWARS_CONTROL_SOCKET", &h._socket_path.0)
            .env("SLINT_BACKEND", "winit-software")
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!(
                    "unix:path={}",
                    h.run_path().join("absent-system-bus").display()
                ),
            )
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::null())
            .stderr(Stdio::from(log.try_clone().unwrap()))
            .stdout(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    h.wait_until_ready()
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn main_catalog_wraps_and_toggle_preserves_setup_and_extra_return() {
    run_functional_test("launcher-catalog", |h| {
        let mut state = h.wait_until_ready();
        assert_eq!(control_value(&state, TOGGLE), Some("Off"));
        state = h.activate_guarded("launcher.scenario.previous", &state);
        assert_eq!(state.selected_scenario, "clock");
        for expected in [
            "spacewars",
            "scorched-earth",
            "pizza",
            "falling",
            "nes",
            "clock",
        ] {
            state = h.activate_guarded("launcher.scenario.next", &state);
            assert_eq!(state.selected_scenario, expected);
        }
        state = h.activate_guarded("launcher.scenario.previous", &state);
        assert_eq!(state.selected_scenario, "nes");
        state = h.activate_guarded(TOGGLE, &state);
        assert_eq!(state.selected_scenario, "nes");
        assert_eq!(control_value(&state, TOGGLE), Some("On"));
        assert_eq!(selected_control(&state), TOGGLE);
        assert_eq!(state.active_scenario, None);
        let mut visited = BTreeSet::new();
        loop {
            assert!(visited.insert(state.selected_scenario.clone()));
            state = h.activate_guarded("launcher.scenario.next", &state);
            if state.selected_scenario == "nes" {
                break;
            }
        }
        assert!(visited.contains("spacewars-classic"));
        assert!(visited.contains("garden-lab"));
        assert!(!visited.contains("null"));
        state = h.activate_until_scenario("spacewars-classic", state);
        state = h.activate_guarded(TOGGLE, &state);
        assert_eq!(state.selected_scenario, "spacewars");
        assert_eq!(selected_control(&state), TOGGLE);
        assert_eq!(control_value(&state, "launcher.start"), Some("4242"));
        assert_eq!(state.active_scenario, None);
        let settings = h.activate_guarded("launcher.settings", &state);
        assert!(control_ids(&settings).contains(&"launcher.settings.match.length.next"));
        state = h.activate_guarded("launcher.settings.back", &settings);
        state = h.activate_guarded(TOGGLE, &state);
        assert_eq!(state.selected_scenario, "spacewars");
        assert_eq!(control_value(&state, "launcher.start"), Some("4242"));
        state = h.activate_until_scenario("garden-lab", state);
        h.activate_guarded("launcher.start", &state);
        let gameplay = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        h.pause_guarded(&gameplay);
        let pause = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        state = h.activate_guarded("pause.return-to-launcher", &pause);
        assert_eq!(state.selected_scenario, "garden-lab");
        assert_eq!(control_value(&state, TOGGLE), Some("On"));
        state = restart(h, &state, &[]);
        assert_eq!(state.selected_scenario, "garden-lab");
        assert_eq!(control_value(&state, TOGGLE), Some("On"));
        assert!(stored(h).launcher.show_all);
        assert_eq!(stored(h).launch.seed, 4242);
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn saved_extra_automatically_expands_and_persists_the_catalog() {
    let mut settings = engine_common::Settings::default();
    settings.launch.scenario = "spacewars-classic".into();
    run_functional_test_with_settings(
        "launcher-saved-extra",
        "winit-software",
        4242,
        Some(settings),
        |h| {
            let state = h.wait_until_ready();
            assert_eq!(state.selected_scenario, "spacewars-classic");
            assert_eq!(control_value(&state, TOGGLE), Some("On"));
            let state = restart(h, &state, &[]);
            assert_eq!(state.selected_scenario, "spacewars-classic");
            assert!(stored(h).launcher.show_all);
        },
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn direct_extra_and_hidden_launches_do_not_depend_on_the_picker_filter() {
    for scenario in ["garden-lab", "null"] {
        run_functional_test("launcher-direct-selection", |h| {
            let launcher = h.wait_until_ready();
            assert_eq!(control_value(&launcher, TOGGLE), Some("Off"));
            restart(
                h,
                &launcher,
                &[
                    "--scenario",
                    scenario,
                    "--seed",
                    "4242",
                    "--renderer",
                    "raster",
                ],
            );
            let gameplay = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            assert_eq!(gameplay.active_scenario.as_deref(), Some(scenario));
            h.pause_guarded(&gameplay);
            let pause = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let launcher = h.activate_guarded("pause.return-to-launcher", &pause);
            assert_eq!(
                launcher.selected_scenario,
                if scenario == "null" {
                    "spacewars"
                } else {
                    scenario
                }
            );
            assert_eq!(
                control_value(&launcher, TOGGLE),
                Some(if scenario == "null" { "Off" } else { "On" })
            );
        });
    }
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn filter_save_failure_uses_app_settings_retry_and_persists_off_on_restart() {
    run_functional_test("launcher-filter-save", |h| {
        let launcher = h.wait_until_ready();
        let saved_seed = stored(h).launch.seed;
        let path = h.run_path().join("config/settings.toml");
        let backup = h.run_path().join("settings-backup.toml");
        fs::rename(&path, &backup).unwrap();
        fs::create_dir(&path).unwrap();
        let launcher = h.activate_guarded(TOGGLE, &launcher);
        assert_eq!(control_value(&launcher, TOGGLE), Some("On"));
        assert_eq!(launcher.active_scenario, None);
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        let failed_launcher = loop {
            let state = h.state();
            assert_eq!(state.screen, UiScreen::LauncherMain);
            if state.error.is_some() {
                break state;
            }
            assert!(
                Instant::now() < deadline,
                "launcher did not report the save failure"
            );
            thread::sleep(POLL_INTERVAL);
        };
        assert!(
            failed_launcher
                .error
                .as_deref()
                .unwrap()
                .contains("could not save settings")
        );
        assert_eq!(control_value(&failed_launcher, TOGGLE), Some("On"));
        assert_eq!(selected_control(&failed_launcher), TOGGLE);
        h.capture_screenshot("launcher-save-error.png");
        h.activate_guarded("launcher.sound", &failed_launcher);
        let failed = h.wait_sound_save("error");
        assert!(failed.error.is_some());
        fs::remove_dir(&path).unwrap();
        fs::rename(&backup, &path).unwrap();
        h.activate_guarded("sound.retry", &failed);
        let saved = h.wait_sound_save("saved");
        assert!(stored(h).launcher.show_all);
        let launcher = h.activate_guarded("sound.back", &saved);
        assert!(launcher.error.is_none());
        assert_eq!(control_value(&launcher, TOGGLE), Some("On"));
        let launcher = h.activate_guarded(TOGGLE, &launcher);
        let launcher = restart(h, &launcher, &[]);
        assert_eq!(control_value(&launcher, TOGGLE), Some("Off"));
        assert!(!stored(h).launcher.show_all);
        assert_eq!(stored(h).launch.scenario, "spacewars");
        assert_eq!(stored(h).launch.seed, saved_seed);
    });
}
