use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn spaceling_lab_launch_pause_restart_and_both_renderers() {
    run_functional_test("spaceling-lab-lifecycle", |harness| {
        let ready = harness.wait_until_ready();
        let mut state = harness.activate_until_scenario("spaceling-lab", ready);
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
                    scenario: Some("spaceling-lab".into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!state.benchmark_active);
            let screenshot = harness.capture_screenshot(&format!("spaceling-lab-{renderer}.png"));
            if renderer == "raster" {
                assert_raster_lab_visible(&screenshot);
            }
            let first_instance = state.scenario_revision;
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spaceling-lab".into()),
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
                    scenario: Some("spaceling-lab".into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(state.scenario_revision, first_instance);
            let pause = harness.pause_guarded(&state);
            state = harness.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    scenario: Some("spaceling-lab".into()),
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
            assert_eq!(state.selected_scenario, "spaceling-lab");
        }
    });
}

fn assert_raster_lab_visible(path: &Path) {
    let mut decoder = png::Decoder::new(File::open(path).unwrap());
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).unwrap();
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => panic!("unexpected screenshot color type {other:?}"),
    };
    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut suit_pixels = 0;
    let mut diagnostics_pixels = 0;
    for (index, pixel) in buffer[..frame.buffer_size()]
        .chunks_exact(channels)
        .enumerate()
    {
        let x = index % width;
        let y = index / width;
        if (width * 45 / 100..width * 55 / 100).contains(&x)
            && (height * 40 / 100..height * 60 / 100).contains(&y)
            && pixel[0] > 200
            && pixel[1] > 70
            && pixel[1] < 190
            && pixel[2] < 110
        {
            suit_pixels += 1;
        }
        if y > height * 85 / 100 && pixel[0] < 150 && pixel[1] > 175 && pixel[2] > 140 {
            diagnostics_pixels += 1;
        }
    }
    // Broad semantic regions/colors, not a renderer-specific golden image or
    // an assertion about an exact tick/font/anti-aliasing pattern.
    assert!(
        suit_pixels > 40,
        "missing character: {suit_pixels} suit pixels"
    );
    assert!(
        diagnostics_pixels > 40,
        "missing HUD: {diagnostics_pixels} cyan text pixels"
    );
}
