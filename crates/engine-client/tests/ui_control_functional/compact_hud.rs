use super::*;

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn compact_hud_renders_with_bottom_instruments_in_both_backends() {
    // Exact device sizes are covered by the deterministic software-window
    // fixture. This black-box check exercises real desktop vector Path drawing.
    let (name, width, height) = ("hud-desktop", 1280, 720);
    let mut settings = engine_common::Settings::default();
    settings.video.width = width;
    settings.video.height = height;
    settings.video.show_fps = true;
    settings.audio.muted = true;
    settings.spacewars.player_1_controller = engine_common::SpacewarsController::Human;
    settings.spacewars.player_2_controller = engine_common::SpacewarsController::Human;
    run_functional_test_with_settings(name, "winit-femtovg", 42, Some(settings), |h| {
        let ready = h.wait_until_ready();
        let mut launcher = h.activate_until_scenario("spacewars", ready);
        for renderer in ["vector", "raster"] {
            let mut settings = h.activate_guarded("launcher.settings", &launcher);
            if control_value(&settings, "launcher.settings.renderer.next") != Some(renderer) {
                settings = h.activate_guarded("launcher.settings.renderer.next", &settings);
            }
            assert_eq!(
                control_value(&settings, "launcher.settings.renderer.next"),
                Some(renderer)
            );
            h.activate_guarded("launcher.settings.start", &settings);
            let gameplay = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("spacewars".into()),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let capture = format!("{renderer}.png");
            terrain_lab::wait_for_terrain_frame(h, &capture, 3_000);
            let path = h.capture_screenshot(&capture);
            let mut reader = png::Decoder::new(File::open(&path).unwrap())
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!((info.width, info.height), (width, height));
            // Long, colored meter fills beside the outside maps must be in
            // the bottom instrument band, not the old shaded top strips.
            for player in 0..2 {
                let colored = pixels[..info.buffer_size()]
                    .chunks_exact(4)
                    .enumerate()
                    .filter(|(index, p)| {
                        let x = *index as u32 % width;
                        let y = *index as u32 / width;
                        y > height - 145
                            && y < height - 25
                            && x / (width / 2) == player
                            && x > 155
                            && x < width - 155
                            && if player == 0 {
                                p[0] > 200 && p[1] < 80
                            } else {
                                p[1] > 200 && p[0] < 80
                            }
                    })
                    .count();
                assert!(
                    colored > 200,
                    "{name} {renderer} P{} missing bottom vitals: {colored}",
                    player + 1
                );
            }
            h.pause_guarded(&gameplay);
            let pause = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            launcher = h.activate_guarded("pause.return-to-launcher", &pause);
        }
    });
}
