use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn spaceling_lab_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "spaceling-lab",
        "spaceling-lab-lifecycle",
        assert_raster_lab_visible,
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_sortie_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "surface-sortie",
        "surface-sortie-lifecycle",
        assert_raster_sortie_visible,
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_sortie_orbit_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "surface-sortie-orbit",
        "surface-sortie-orbit-lifecycle",
        assert_raster_sortie_visible,
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_sortie_generated_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "surface-sortie-generated",
        "surface-sortie-generated-lifecycle",
        assert_raster_sortie_visible,
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_sortie_world_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "surface-sortie-world",
        "surface-sortie-world-lifecycle",
        assert_raster_sortie_visible,
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_expedition_launch_pause_restart_and_both_renderers() {
    lab_lifecycle(
        "surface-expedition",
        "surface-expedition-lifecycle",
        assert_raster_sortie_visible,
    );
}

fn lab_lifecycle(scenario: &str, test_name: &'static str, assert_visible: fn(&Path)) {
    run_functional_test(test_name, |harness| {
        let ready = harness.wait_until_ready();
        let mut state = harness.activate_until_scenario(scenario, ready);
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
                    scenario: Some(scenario.into()),
                    revision_after: Some(revision),
                },
                TRANSITION_TIMEOUT,
            );
            assert!(!state.benchmark_active);
            let screenshot = harness.capture_screenshot(&format!("{scenario}-{renderer}.png"));
            if renderer == "raster" {
                assert_visible(&screenshot);
            }
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

fn assert_raster_sortie_visible(path: &Path) {
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
    let mut ship_pixels = 0;
    let mut diagnostics_pixels = 0;
    let mut minimap_planet_pixels = 0;
    let mut outpost_pixels = 0;
    let mut outpost_hud_pixels = 0;
    for (index, pixel) in buffer[..frame.buffer_size()]
        .chunks_exact(channels)
        .enumerate()
    {
        let y = index / width;
        let x = index % width;
        if y > height / 5 && y < height * 4 / 5 && pixel[0] > 200 && pixel[1] < 80 && pixel[2] < 80
        {
            ship_pixels += 1;
        }
        // The cyan transfer row is in the bottom quarter. Landing diagnostics
        // below it change from orange to cyan as the initial ship settles.
        if y > height * 3 / 4 && pixel[0] < 150 && pixel[1] > 175 && pixel[2] > 140 {
            diagnostics_pixels += 1;
        }
        if pixel[0] > 200 && pixel[1] > 160 && pixel[1] < 230 && pixel[2] < 100 {
            if y > height / 3 && y < height * 3 / 4 {
                outpost_pixels += 1;
            }
            if y > height * 4 / 5 {
                outpost_hud_pixels += 1;
            }
        }
        // The overview sits below the top HUD at the right. Its planet stays
        // cyan even though the main scene uses a dark-blue surface.
        if x > width * 3 / 4
            && y >= height / 4
            && y <= height / 2
            && pixel[0] < 100
            && pixel[1] > 140
            && pixel[2] > 110
        {
            minimap_planet_pixels += 1;
        }
    }
    assert!(ship_pixels > 100, "missing initial ship: {ship_pixels}");
    assert!(
        outpost_pixels > 40,
        "missing amber outpost: {outpost_pixels}"
    );
    assert!(
        outpost_hud_pixels > 40,
        "missing outpost capture HUD: {outpost_hud_pixels}"
    );
    assert!(
        minimap_planet_pixels > 40,
        "missing minimap planet: {minimap_planet_pixels}"
    );
    assert!(
        diagnostics_pixels > 40,
        "missing sortie HUD: {diagnostics_pixels}"
    );
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
