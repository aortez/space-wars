use super::*;

#[test]
fn falling_shares_visible_course_and_moving_floor_in_production_adapters() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    use engine_common::{ClockEventKind, ClockFloorMode, ClockRainAmount, ClockTimeFormat};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        for arena in ["course", "panels"] {
            let mut scene = scenario(viewport);
            let mut settings = scene.state.settings();
            settings.time_format = ClockTimeFormat::TwelveHour;
            settings.rain_amount = ClockRainAmount::Heavy;
            scene.step(&[ClockAction::configure(settings)], Duration::ZERO);
            if arena == "panels" {
                scene.step(
                    &[ClockAction::preview_event(ClockEventKind::Rain)],
                    Duration::ZERO,
                );
                for _ in 0..480 {
                    scene.step(&[], Duration::from_nanos(16_666_667));
                }
            }
            scene.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
            for _ in 0..90 {
                scene.step(&[], Duration::from_nanos(16_666_667));
            }
            let session = scene.state.player_duck_session();
            let base = scene.state.body_count();
            scene.step(
                &[ClockAction::preview_event(ClockEventKind::Falling)],
                Duration::ZERO,
            );
            assert!(scene.state.body_count() > base);
            for elapsed in 0..=300 {
                if [0, 120, 210, 270, 300].contains(&elapsed) {
                    assert_eq!(scene.state.player_duck_session(), session);
                    assert_eq!(scene.state.floor_mode(), ClockFloorMode::EventOwned);
                    if elapsed >= 210 {
                        assert_eq!(scene.state.body_count(), base);
                    }
                    let frames = scene.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
                    let pixels = raster(&frames[0], viewport);
                    let vector = svg(&frames[0], viewport);
                    assert!(!vector.contains("NaN") && !vector.contains("inf"));
                    if let Some(output) = &output {
                        let name = format!("player-falling-{arena}-{name}-{elapsed}");
                        write_png(&output.join(format!("{name}.png")), &pixels);
                        std::fs::write(output.join(format!("{name}.svg")), vector).unwrap();
                    }
                    assert!(
                        pixels
                            .as_slice()
                            .iter()
                            // Falling bars can push the duck into the frame's
                            // shadow. Require visible yellow, not its unshaded RGB.
                            .filter(|p| p.r > 230 && p.g > 180 && p.b < 50)
                            .count()
                            >= 4,
                        "visible duck: {arena} {name} tick {elapsed}: {:?}",
                        scene.state.player_duck_state()
                    );
                }
                if elapsed < 300 {
                    scene.step(&[], Duration::from_nanos(16_666_667));
                }
            }
            assert_eq!(scene.state.event_kind(), None);
        }
    }
}
