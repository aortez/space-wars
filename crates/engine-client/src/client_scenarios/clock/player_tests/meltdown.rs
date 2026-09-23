use super::*;

#[test]
fn meltdown_shares_player_blocks_water_and_floor_in_production_adapters() {
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
                &[ClockAction::preview_event(ClockEventKind::Meltdown)],
                Duration::ZERO,
            );
            for elapsed in 0..=510 {
                if [0, 65, 120, 180, 300, 450, 510].contains(&elapsed) {
                    assert_eq!(scene.state.player_duck_session(), session);
                    assert_eq!(scene.state.floor_mode(), ClockFloorMode::EventOwned);
                    assert!(scene.state.body_count() <= 128 && scene.state.collider_count() <= 128);
                    if elapsed >= 420 {
                        assert_eq!(scene.state.body_count(), base);
                    }
                    let frames = scene.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
                    let pixels = raster(&frames[0], viewport);
                    let vector = svg(&frames[0], viewport);
                    assert!(!vector.contains("NaN") && !vector.contains("inf"));
                    assert!(
                        pixels
                            .as_slice()
                            .iter()
                            .any(|p| p.r > 240 && p.g > 220 && p.b < 50),
                        "visible duck: {arena} {name} tick {elapsed}"
                    );
                    if let Some(output) = &output {
                        let name = format!("player-meltdown-{arena}-{name}-{elapsed}");
                        write_png(&output.join(format!("{name}.png")), &pixels);
                        std::fs::write(output.join(format!("{name}.svg")), vector).unwrap();
                    }
                }
                if elapsed < 510 {
                    scene.step(&[], Duration::from_nanos(16_666_667));
                }
            }
            assert_eq!(scene.state.event_kind(), None);
        }
    }
}
