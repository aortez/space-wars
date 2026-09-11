use super::*;
use std::collections::BTreeMap;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn fps_counter_is_shared_by_scene_and_native_video_lifecycles() {
    run_functional_test("performance-overlay", |harness| {
        let launcher = harness.wait_until_ready();
        let preferences = harness.activate_guarded("launcher.sound", &launcher);
        harness.activate_guarded("settings.fps-counter", &preferences);
        let preferences = harness.wait_sound_save("saved");
        assert_eq!(
            control_value(&preferences, "settings.fps-counter"),
            Some("on")
        );
        harness.capture_screenshot("app-settings.png");
        let mut launcher = harness.activate_guarded("sound.back", &preferences);

        // Clock uses CPU raster images, Falling uses the realtime NES worker,
        // and Spacewars exercises the scene/vector path with its own HUD.
        for scenario in ["clock", "falling", "spacewars"] {
            launcher = harness.activate_until_scenario(scenario, launcher);
            if scenario == "spacewars" {
                let settings = harness.activate_guarded("launcher.settings", &launcher);
                assert_eq!(
                    control_value(&settings, "launcher.settings.renderer.next"),
                    Some("raster")
                );
                let settings =
                    harness.activate_guarded("launcher.settings.renderer.next", &settings);
                launcher = harness.activate_guarded("launcher.settings.back", &settings);
            }
            harness.activate_guarded("launcher.start", &launcher);
            let gameplay = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some(scenario.into()),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let status = wait_status(harness, |status| {
                status
                    .get("performance_overlay_text")
                    .is_some_and(|text| text.starts_with("FPS ") && !text.contains("--"))
            });
            assert_eq!(status["performance_overlay_enabled"], "true");
            assert_eq!(status["fps_source"], "submitted-frames");
            for rate in ["fps", "ups"] {
                let value: f64 = status[rate].parse().unwrap();
                assert!(value.is_finite() && value >= 0.0);
            }
            harness.capture_screenshot(&format!("{scenario}-overlay.png"));
            harness.pause_guarded(&gameplay);
            let pause = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            wait_status(harness, |status| {
                status.get("performance_overlay_text").map(String::as_str) == Some("Paused")
            });

            let preferences = harness.activate_guarded("pause.sound", &pause);
            assert_eq!(
                control_value(&preferences, "settings.fps-counter"),
                Some("on")
            );
            harness.activate_guarded("settings.fps-counter", &preferences);
            let preferences = harness.wait_sound_save("saved");
            wait_status(harness, |status| {
                status
                    .get("performance_overlay_enabled")
                    .map(String::as_str)
                    == Some("false")
            });
            harness.activate_guarded("settings.fps-counter", &preferences);
            let preferences = harness.wait_sound_save("saved");
            let pause = harness.activate_guarded("sound.back", &preferences);
            harness.activate_guarded("pause.restart", &pause);
            let restarted = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(restarted.scenario_revision, gameplay.scenario_revision);
            wait_status(harness, |status| {
                status
                    .get("performance_overlay_text")
                    .is_some_and(|text| text.starts_with("FPS ") && !text.contains("--"))
            });
            harness.pause_guarded(&restarted);
            let pause = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            harness.activate_guarded("pause.return-to-launcher", &pause);
            launcher = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::LauncherMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let status = wait_status(harness, |status| {
                status
                    .get("performance_overlay_text")
                    .is_some_and(String::is_empty)
            });
            assert_eq!(
                status["performance_overlay_enabled"], "true",
                "returning to launcher does not reset the preference"
            );
        }
    });
}

fn wait_status(
    harness: &mut FunctionalHarness,
    ready: impl Fn(&BTreeMap<String, String>) -> bool,
) -> BTreeMap<String, String> {
    let deadline = Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let response = harness
            .client
            .request_before("status\n", deadline.min(request_deadline()))
            .unwrap();
        let status = response
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        if ready(&status) {
            harness
                .history
                .push(json!({"command": "status", "response": response}));
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "counter did not reach expected state: {status:?}"
        );
        thread::sleep(POLL_INTERVAL);
    }
}
