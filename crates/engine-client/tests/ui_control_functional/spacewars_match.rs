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
    run_functional_test_with_seed("spacewars-finished-match", "winit-femtovg", 7, |harness| {
        let mut state = harness.wait_until_ready();
        state = harness.activate_guarded("launcher.settings", &state);
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
        // Both ordinary mission bots play this recorded seed. No damage, pose,
        // ownership or outcome is injected to make the result screen appear.
        state = harness.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::GameOver),
                scenario: Some("spacewars".into()),
                revision_after: None,
            },
            Duration::from_secs(180),
        );
        assert_eq!(
            control_ids(&state),
            ["game-over.play-again", "game-over.return-to-launcher"]
        );
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
        harness.capture_screenshot("play-again-paused.png");
        state = harness.activate_guarded("pause.return-to-launcher", &state);
        assert_launcher_main(&state);
        state = harness.activate_guarded("launcher.settings", &state);
        for control in [
            "launcher.settings.match.player-1.next",
            "launcher.settings.match.player-2.next",
        ] {
            assert_eq!(control_value(&state, control), Some("rule bot"));
        }
    });
}
