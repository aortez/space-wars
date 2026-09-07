use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn terrain_lab_launch_pause_restart_and_both_renderers() {
    // Slint's software backend does not draw Path items. Femtovg exercises
    // actual vector paths as well as our software raster image and text overlay.
    run_functional_test_with_backend("terrain-lab-lifecycle", "winit-femtovg", |harness| {
        let ready = harness.wait_until_ready();
        let mut state = harness.activate_until_scenario("terrain-lab", ready);
        for renderer in ["vector", "raster"] {
            state = harness.activate_guarded("launcher.settings", &state);
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
                    scenario: Some("terrain-lab".into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!state.benchmark_active);
            let screenshot = harness.capture_screenshot(&format!("terrain-lab-{renderer}.png"));
            assert_terrain_visible(&screenshot);
            let first_instance = state.scenario_revision;
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("terrain-lab".into()),
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
                    scenario: Some("terrain-lab".into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(state.scenario_revision, first_instance);
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("terrain-lab".into()),
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
            assert_eq!(state.selected_scenario, "terrain-lab");
        }
    });
}

fn assert_terrain_visible(path: &Path) {
    let mut reader = png::Decoder::new(File::open(path).unwrap())
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let pixels = &buffer[..info.buffer_size()];
    let terrain_pixels = pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 25 && p[0] < 115 && p[1] > 45 && p[1] < 140 && p[2] > 45 && p[2] < 100)
        .count();
    assert!(
        terrain_pixels > 20_000,
        "missing terrain: {terrain_pixels} material pixels"
    );
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
    assert!(
        text_pixels > 150,
        "missing title and controls: {text_pixels} text pixels"
    );
}
