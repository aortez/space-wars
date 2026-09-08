use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn terrain_lab_launch_pause_restart_and_both_renderers() {
    run_terrain_lifecycle("terrain-lab");
}

pub(super) fn run_terrain_lifecycle(scenario: &'static str) {
    // Slint's software backend does not draw Path items. Femtovg exercises
    // actual vector paths as well as our software raster image and text overlay.
    run_functional_test_with_backend(scenario, "winit-femtovg", |harness| {
        let ready = harness.wait_until_ready();
        let mut state = harness.activate_until_scenario(scenario, ready);
        for renderer in ["vector", "raster"] {
            state = harness.activate_guarded("launcher.settings", &state);
            if matches!(
                scenario,
                "spacewars-terrain-combat" | "spacewars-terrain-duel"
            ) {
                let interval = "launcher.settings.combat.break-interval.next";
                let duration = "launcher.settings.combat.break-duration.next";
                let mission = "launcher.settings.combat.mission.next";
                if renderer == "vector" {
                    assert_eq!(control_value(&state, mission), Some("Dogfight"));
                    state = harness.activate_guarded(mission, &state);
                    assert_eq!(control_value(&state, mission), Some("Capture"));
                    assert_eq!(control_value(&state, interval), Some("15"));
                    assert_eq!(control_value(&state, duration), Some("4"));
                    for expected in ["30", "Off", "8"] {
                        state = harness.activate_guarded(interval, &state);
                        assert_eq!(control_value(&state, interval), Some(expected));
                    }
                    state = harness.activate_guarded(duration, &state);
                    assert_eq!(control_value(&state, duration), Some("6"));
                } else {
                    // Returning through launch/restart reloads the persisted choices.
                    assert_eq!(control_value(&state, mission), Some("Capture"));
                    state = harness.activate_guarded(mission, &state);
                    assert_eq!(control_value(&state, mission), Some("Dogfight"));
                    assert_eq!(control_value(&state, interval), Some("8"));
                    assert_eq!(control_value(&state, duration), Some("6"));
                    state = harness.activate_guarded(
                        "launcher.settings.combat.break-interval.previous",
                        &state,
                    );
                    assert_eq!(control_value(&state, interval), Some("Off"));
                }
                harness.capture_screenshot(&format!("{scenario}-{renderer}-break-settings.png"));
            }
            if control_value(&state, "launcher.settings.renderer.next") != Some(renderer) {
                state = harness.activate_guarded("launcher.settings.renderer.next", &state);
            }
            assert_eq!(
                control_value(&state, "launcher.settings.renderer.next"),
                Some(renderer)
            );
            state = harness.activate_guarded("launcher.settings.back", &state);
            let revision = state.revision;
            harness.activate_guarded("launcher.start", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some(scenario.into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!state.benchmark_active);
            wait_for_terrain_frame(harness, &format!("{scenario}-{renderer}.png"));
            let first_instance = state.scenario_revision;
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some(scenario.into()),
                    revision_after: Some(pause.revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!control_ids(&state).contains(&"pause.benchmark"));
            let revision = state.revision;
            harness.activate_guarded("pause.restart", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some(scenario.into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(state.scenario_revision, first_instance);
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some(scenario.into()),
                    revision_after: Some(pause.revision),
                },
                TRANSITION_TIMEOUT,
            );
            let revision = state.revision;
            harness.activate_guarded("pause.return-to-launcher", &state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::LauncherMain),
                    scenario: None,
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert_eq!(state.selected_scenario, scenario);
        }
    });
}

fn wait_for_terrain_frame(harness: &mut FunctionalHarness, name: &str) {
    // UI state can be published before the matching frame is presented.
    let deadline = Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let path = harness.capture_screenshot(name);
        let (terrain_pixels, text_pixels) = terrain_pixel_counts(&path);
        if terrain_pixels > 20_000 && text_pixels > 150 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "terrain frame did not appear: {terrain_pixels} material pixels, {text_pixels} title/control pixels"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn terrain_pixel_counts(path: &Path) -> (usize, usize) {
    let mut reader = png::Decoder::new(File::open(path).unwrap())
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let pixels = &buffer[..info.buffer_size()];
    let terrain_pixels = pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 25 && p[0] < 115 && p[1] > 45 && p[1] < 140 && p[2] > 45 && p[2] < 150)
        .count();
    let text_pixels = pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(index, p)| {
            *index / (info.width as usize) < info.height as usize / 8
                && p[0] > 170
                && p[1] > 170
                && p[2] > 170
        })
        .count();
    (terrain_pixels, text_pixels)
}
