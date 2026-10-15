#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_comparison_launch_pause_restart_and_both_renderers() {
    for scenario in [
        "spacewars-surface-blocks",
        "spacewars-surface-contour",
        "spacewars-surface-round",
    ] {
        super::terrain_lab::run_terrain_lifecycle(scenario);
    }
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn spacewars_terrain_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_pilot_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-ai");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_recovery_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-recovery");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_combat_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-combat");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-duel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_jetpack_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-jetpack");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_travel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-travel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_travel_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-travel-duel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_arena_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-arena");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_arena_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-arena-duel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn planetary_tank_fixture_motion_support_and_lifecycle() {
    use super::*;
    for scenario in [
        "planetary-tank-fixture",
        "planetary-tank-fixture-angular",
        "planetary-tank-comparison",
        "planetary-tank-comparison-angular",
    ] {
        run_functional_test_with_seed("planetary-tanks", "winit-femtovg", 42, |h| {
            let ready = h.wait_until_ready();
            let mut launcher = h.activate_until_scenario(scenario, ready);
            for renderer in ["vector", "raster"] {
                let mut settings = h.activate_guarded("launcher.settings", &launcher);
                if control_value(&settings, "launcher.settings.renderer.next") != Some(renderer) {
                    settings = h.activate_guarded("launcher.settings.renderer.next", &settings);
                }
                assert_eq!(
                    control_value(&settings, "launcher.settings.renderer.next"),
                    Some(renderer)
                );
                launcher = h.activate_guarded("launcher.settings.back", &settings);
                h.activate_guarded("launcher.start", &launcher);
                let gameplay = h.wait_for(
                    UiStatePredicate {
                        screen: Some(UiScreen::Gameplay),
                        scenario: Some(scenario.into()),
                        ..Default::default()
                    },
                    TRANSITION_TIMEOUT,
                );
                if scenario.contains("comparison") {
                    h.capture_screenshot(&format!("{scenario}-{renderer}-close-comparison.png"));
                }
                for (tick, label) in [
                    (60, "drive"),
                    (180, "brake"),
                    (300, "gap"),
                    (430, "fall"),
                    (600, "cleanup"),
                ]
                .into_iter()
                .filter(|_| !scenario.contains("comparison"))
                {
                    let diagnostics = performance::wait_status(h, |s| {
                        s.get("tank_fixture_tick")
                            .and_then(|v| v.parse::<usize>().ok())
                            .is_some_and(|n| n >= tick)
                    });
                    assert_eq!(diagnostics["tank_fixture_pilots"], "1");
                    let sample: serde_json::Value =
                        serde_json::from_str(&diagnostics["tank_fixture_latest"]).unwrap();
                    assert_eq!(sample["tanks"].as_array().unwrap().len(), 2);
                    if label == "gap" {
                        assert_eq!(sample["tanks"][0]["blocked"], true);
                    }
                    if label == "cleanup" {
                        assert_eq!(diagnostics["tank_fixture_passed"], "true");
                        assert_eq!(diagnostics["tank_fixture_actors"], "0");
                    }
                    h.capture_screenshot(&format!("{scenario}-{renderer}-{label}.png"));
                    if label == "brake" {
                        h.pause_guarded(&gameplay);
                        let pause = h.wait_for(
                            UiStatePredicate {
                                screen: Some(UiScreen::PauseMain),
                                ..Default::default()
                            },
                            TRANSITION_TIMEOUT,
                        );
                        let frozen =
                            performance::wait_status(h, |_| true)["tank_fixture_tick"].clone();
                        h.capture_screenshot(&format!("{scenario}-{renderer}-pause.png"));
                        assert_eq!(
                            performance::wait_status(h, |_| true)["tank_fixture_tick"],
                            frozen
                        );
                        h.activate_guarded("pause.resume", &pause);
                        h.wait_for(
                            UiStatePredicate {
                                screen: Some(UiScreen::Gameplay),
                                ..Default::default()
                            },
                            TRANSITION_TIMEOUT,
                        );
                    }
                }
                let current = h.client.ui_state().unwrap();
                h.pause_guarded(&current);
                let pause = h.wait_for(
                    UiStatePredicate {
                        screen: Some(UiScreen::PauseMain),
                        ..Default::default()
                    },
                    TRANSITION_TIMEOUT,
                );
                h.activate_guarded("pause.return-to-launcher", &pause);
                launcher = h.wait_for(
                    UiStatePredicate {
                        screen: Some(UiScreen::LauncherMain),
                        ..Default::default()
                    },
                    TRANSITION_TIMEOUT,
                );
            }
        });
    }
}
