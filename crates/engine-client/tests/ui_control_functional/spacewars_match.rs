use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn normal_spacewars_all_player_choices_persist_across_restart_and_both_renderers() {
    run_functional_test_with_backend("spacewars-player-choices", "winit-femtovg", |harness| {
        let mut state = harness.wait_until_ready();
        assert_launcher_main(&state);
        for (p1, p2, renderer) in [
            ("human", "human", "vector"),
            ("human", "rule bot", "raster"),
            ("rule bot", "rule bot", "vector"),
            ("rule bot", "human", "raster"),
        ] {
            state = harness.activate_guarded("launcher.settings", &state);
            assert_eq!(
                state
                    .controls
                    .iter()
                    .find(|c| c.id == "launcher.settings.start")
                    .unwrap()
                    .label,
                "Play World"
            );
            assert!(!control_ids(&state).contains(&"launcher.settings.spacewars.preset.next"));
            assert!(!control_ids(&state).contains(&"launcher.settings.combat.mission.next"));
            for (control, expected) in [
                ("launcher.settings.match.player-1.next", p1),
                ("launcher.settings.match.player-2.next", p2),
                ("launcher.settings.renderer.next", renderer),
            ] {
                if control_value(&state, control) != Some(expected) {
                    state = harness.activate_guarded(control, &state);
                }
                assert_eq!(control_value(&state, control), Some(expected));
            }
            for (control, expected) in [
                ("launcher.settings.match.break-interval.next", "8"),
                ("launcher.settings.match.break-duration.next", "6"),
                ("launcher.settings.match.asteroid-interval.next", "8"),
                ("launcher.settings.match.asteroid-strength.next", "Heavy"),
            ] {
                for _ in 0..4 {
                    if control_value(&state, control) == Some(expected) {
                        break;
                    }
                    state = harness.activate_guarded(control, &state);
                }
                assert_eq!(control_value(&state, control), Some(expected));
            }
            let settings_name = format!("settings-{p1}-{p2}-{renderer}.png");
            harness.capture_screenshot(&settings_name);
            let revision = state.revision;
            harness.activate_guarded("launcher.settings.start", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            terrain_lab::wait_for_terrain_frame(
                harness,
                &format!("play-{p1}-{p2}-{renderer}.png"),
                20_000,
            );
            let first_instance = state.scenario_revision;
            harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!control_ids(&state).contains(&"pause.benchmark"));
            state = harness.activate_guarded("pause.controls", &state);
            assert_eq!(state.screen, UiScreen::PauseControls);
            harness.capture_screenshot(&format!("controls-{renderer}.png"));
            state = harness.activate_guarded("pause.controls.back", &state);
            harness.activate_guarded("pause.restart", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(state.scenario_revision, first_instance);
            harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            state = harness.activate_guarded("pause.return-to-launcher", &state);
            assert_launcher_main(&state);
            state = harness.activate_guarded("launcher.settings", &state);
            assert_eq!(
                control_value(&state, "launcher.settings.match.player-1.next"),
                Some(p1)
            );
            assert_eq!(
                control_value(&state, "launcher.settings.match.player-2.next"),
                Some(p2)
            );
            for (control, expected) in [
                ("launcher.settings.match.break-interval.next", "8"),
                ("launcher.settings.match.break-duration.next", "6"),
                ("launcher.settings.match.asteroid-interval.next", "8"),
                ("launcher.settings.match.asteroid-strength.next", "Heavy"),
            ] {
                assert_eq!(control_value(&state, control), Some(expected));
            }
            state = harness.activate_guarded("launcher.settings.back", &state);
        }
    });
}

#[test]
#[ignore = "requires an explicit display and a bounded physical match; run under Xvfb"]
fn normal_spacewars_physical_round_reaches_result_and_play_again() {
    let mut settings = engine_common::Settings::default();
    settings.spacewars_match.time_limit_seconds = 60;
    run_functional_test_with_settings(
        "spacewars-finished-match",
        "winit-femtovg",
        7,
        Some(settings),
        |harness| {
            let mut state = harness.wait_until_ready();
            state = harness.activate_guarded("launcher.settings", &state);
            assert_eq!(
                control_value(&state, "launcher.settings.match.length.next"),
                Some("1 min")
            );
            for control in [
                "launcher.settings.match.player-1.next",
                "launcher.settings.match.player-2.next",
            ] {
                state = harness.activate_guarded(control, &state);
                assert_eq!(control_value(&state, control), Some("rule bot"));
            }
            harness.activate_guarded("launcher.settings.start", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            let first_instance = state.scenario_revision;
            // Both ordinary mission bots play a real, timed match. A particular
            // seed's accidental pilot death is not a stable result-screen fixture.
            // No damage, pose, ownership or outcome is injected. Keep the wall-time
            // allowance separate from the game's simulation-time limit.
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::GameOver),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                Duration::from_secs(180),
            );
            assert_timed_match_result(harness);
            assert_eq!(
                control_ids(&state),
                [
                    "game-over.play-again",
                    "game-over.new-match",
                    "game-over.return-to-launcher"
                ]
            );
            assert_eq!(control_value(&state, "game-over.play-again"), Some("7"));
            harness.capture_screenshot("physical-result.png");
            harness.activate_guarded("game-over.play-again", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(state.scenario_revision, first_instance);
            assert!(!state.paused);
            harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            assert_eq!(control_value(&state, "pause.restart"), Some("7"));
            harness.capture_screenshot("play-again-paused.png");
            harness.activate_guarded("pause.resume", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::GameOver),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                Duration::from_secs(180),
            );
            assert_timed_match_result(harness);
            assert_eq!(control_value(&state, "game-over.play-again"), Some("7"));
            harness.activate_guarded("game-over.new-match", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spacewars".into()),
                    revision_after: None,
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(control_value(&state, "pause.restart"), Some("7"));
            harness.capture_screenshot("new-match-after-result.png");
            state = harness.activate_guarded("pause.return-to-launcher", &state);
            assert_launcher_main(&state);
            state = harness.activate_guarded("launcher.settings", &state);
            for control in [
                "launcher.settings.match.player-1.next",
                "launcher.settings.match.player-2.next",
            ] {
                assert_eq!(control_value(&state, control), Some("rule bot"));
            }
        },
    );
}

fn assert_timed_match_result(harness: &FunctionalHarness) {
    let status = harness.client.request("status\n").unwrap();
    assert!(
        status.contains("match_finish_reason=Some(TimeLimit)"),
        "{status}"
    );
    assert!(status.contains("match_remaining_seconds=0.000"), "{status}");
    assert!(status.contains("autostart_session=manual"), "{status}");
}

fn wait_match(harness: &mut FunctionalHarness) -> UiState {
    harness.wait_for(
        UiStatePredicate {
            screen: Some(UiScreen::Gameplay),
            scenario: Some("spacewars".into()),
            revision_after: None,
        },
        TRANSITION_TIMEOUT,
    )
}

fn pause_match(harness: &mut FunctionalHarness, state: &UiState) -> UiState {
    harness.pause_guarded(state);
    harness.wait_for(
        UiStatePredicate {
            screen: Some(UiScreen::PauseMain),
            scenario: Some("spacewars".into()),
            revision_after: None,
        },
        TRANSITION_TIMEOUT,
    )
}

fn paused_seed(state: &UiState) -> u64 {
    control_value(state, "pause.restart")
        .unwrap()
        .parse()
        .unwrap()
}

fn relaunch(harness: &mut FunctionalHarness, extra: &[&str]) -> UiState {
    harness.child.0.kill().unwrap();
    harness.child.0.wait().unwrap();
    let _ = fs::remove_file(&harness._socket_path.0);
    let directory = harness.run_directory.as_ref().unwrap().path();
    let log = fs::OpenOptions::new()
        .append(true)
        .open(directory.join("engine-client.log"))
        .unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .arg("--config-dir")
        .arg(directory.join("config"))
        .args(extra)
        .current_dir(workspace_root())
        .env("SLINT_BACKEND", "winit-femtovg")
        .env("SPACEWARS_CONTROL_SOCKET", &harness._socket_path.0)
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stderr(Stdio::from(log.try_clone().unwrap()))
        .stdout(Stdio::from(log))
        .spawn()
        .unwrap();
    harness.child = OwnedChild(child);
    harness.last_state = None;
    harness
        .history
        .push(json!({"command":"relaunch saved settings", "extra_arguments":extra}));
    harness.wait_until_ready()
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn normal_spacewars_new_worlds_rematches_and_relaunch_preserve_settings() {
    run_functional_test_with_seed("spacewars-worlds", "winit-femtovg", u64::MAX, |harness| {
        let mut state = harness.wait_until_ready();
        assert_eq!(
            control_value(&state, "launcher.start"),
            Some("18446744073709551615")
        );
        harness.capture_screenshot("launcher-max-seed.png");
        state = harness.activate_guarded("launcher.settings", &state);
        state = harness.activate_guarded("launcher.settings.match.player-1.next", &state);
        state = harness.activate_guarded("launcher.settings.match.asteroid-interval.next", &state);
        state = harness.activate_guarded("launcher.settings.match.break-interval.next", &state);
        let choices: Vec<_> = state
            .controls
            .iter()
            .filter(|c| c.id.ends_with(".next"))
            .map(|c| (c.id.clone(), c.value.clone()))
            .collect();
        state = harness.activate_guarded("launcher.settings.back", &state);
        // Reach New Match using the same directional menu path as a gamepad.
        state = harness.press_guarded(UiAction::Up, &state);
        state = harness.press_guarded(UiAction::Right, &state);
        assert_eq!(selected_control(&state), "launcher.new-match");
        harness.press_guarded(UiAction::Confirm, &state);
        state = wait_match(harness);
        state = pause_match(harness, &state);
        let first_seed = paused_seed(&state);
        assert_ne!(first_seed, u64::MAX);
        harness.capture_screenshot("first-new-world.png");
        for action in ["pause.restart", "pause.new-match", "pause.restart"] {
            let old_seed = paused_seed(&state);
            let old_revision = state.scenario_revision;
            harness.activate_guarded(action, &state);
            state = wait_match(harness);
            assert_ne!(state.scenario_revision, old_revision);
            state = pause_match(harness, &state);
            assert_eq!(paused_seed(&state) == old_seed, action == "pause.restart");
        }
        let final_seed = paused_seed(&state);
        harness.capture_screenshot("new-world-rematched.png");
        state = harness.activate_guarded("pause.return-to-launcher", &state);
        assert_eq!(
            control_value(&state, "launcher.start")
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            final_seed
        );
        state = relaunch(harness, &[]);
        assert_launcher_main(&state);
        assert_eq!(
            control_value(&state, "launcher.start")
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            final_seed
        );
        harness.capture_screenshot("saved-world-launcher.png");
        state = harness.activate_guarded("launcher.settings", &state);
        for (id, value) in &choices {
            assert_eq!(control_value(&state, id), value.as_deref(), "{id}");
        }
        harness.activate_guarded("launcher.settings.start", &state);
        state = wait_match(harness);
        state = pause_match(harness, &state);
        assert_eq!(paused_seed(&state), final_seed);
        // An explicitly supplied seed remains reproducible, even when a different
        // world was saved by the preceding process.
        state = relaunch(
            harness,
            &["--scenario", "spacewars", "--seed", "18446744073709551615"],
        );
        if state.screen != UiScreen::Gameplay {
            state = wait_match(harness);
        }
        state = pause_match(harness, &state);
        assert_eq!(paused_seed(&state), u64::MAX);
        harness.capture_screenshot("explicit-seed-paused.png");
        state = harness.activate_guarded("pause.return-to-launcher", &state);
        assert_eq!(
            control_value(&state, "launcher.start"),
            Some("18446744073709551615")
        );
    });
}
