use super::*;

#[test]
fn seamless_player_entry_preserves_both_render_adapters_and_opens_into_the_live_scene() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    use engine_common::{ClockEventKind, ClockTimeFormat};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (device, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        for (name, kind, elapsed) in [
            ("falling", ClockEventKind::Falling, 90),
            ("falling", ClockEventKind::Falling, 299),
            ("meltdown", ClockEventKind::Meltdown, 80),
            ("meltdown", ClockEventKind::Meltdown, 200),
            ("meltdown", ClockEventKind::Meltdown, 509),
        ] {
            let mut scene = scenario(viewport);
            let mut settings = scene.state.settings();
            settings.time_format = ClockTimeFormat::TwelveHour;
            scene.step(
                &[
                    ClockAction::configure(settings),
                    ClockAction::preview_event(kind),
                ],
                Duration::ZERO,
            );
            for _ in 0..elapsed {
                scene.step(&[], Duration::from_nanos(16_666_667));
            }
            let id = scene.state.event_id();
            let before = scene.render_frames(RenderBackend::Raster, viewport);
            let before_pixels = raster(&before[0], viewport);
            // Exercise the actual client input translation, not only the
            // scenario action. Reading/physics remain paused at the join point.
            let (input, _) = crate::input::new_shared_input();
            input.borrow_mut().request_clock_player_duck(1);
            // Ignore wall-clock sampling here: the replay has a fixed reading,
            // and changing it during the capture would test a different action.
            let actions: Vec<_> = scene
                .map_input(&mut input.borrow_mut(), false)
                .into_iter()
                .filter(|a| {
                    matches!(
                        ClockAction::decode(a),
                        Some(ClockAction::TogglePlayerDuck(1))
                    )
                })
                .collect();
            assert_eq!(actions.len(), 1);
            scene.step(&actions, Duration::ZERO);
            assert_eq!(scene.state.event_kind(), Some(kind));
            assert_eq!(scene.state.event_id(), id);
            let after = scene.render_frames(RenderBackend::Raster, viewport);
            assert!(
                raster(&after[0], viewport).as_slice() == before_pixels.as_slice(),
                "no scene reset, terrain pop or material jump: {name} / {device} / {elapsed}"
            );
            for (stage, frames) in [("before", before), ("joined", after)] {
                assert!(!svg(&frames[0], viewport).contains("NaN"));
                if let Some(output) = &output {
                    let file = format!("join-{name}-{device}-{elapsed}-{stage}");
                    write_png(
                        &output.join(format!("{file}.png")),
                        &raster(&frames[0], viewport),
                    );
                    std::fs::write(
                        output.join(format!("{file}.svg")),
                        svg(&frames[0], viewport),
                    )
                    .unwrap();
                }
            }
            for tick in 1..=90 {
                scene.step(&[], Duration::from_nanos(16_666_667));
                if [40, 90].contains(&tick) {
                    let frames = scene.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
                    let pixels = raster(&frames[0], viewport);
                    assert!(
                        pixels
                            .as_slice()
                            .iter()
                            .any(|p| p.r > 240 && p.g > 220 && p.b < 50),
                        "duck visible after joining {name} at {elapsed} on {device}"
                    );
                    if let Some(output) = &output {
                        let file = format!("join-{name}-{device}-{elapsed}-after-{tick}");
                        write_png(&output.join(format!("{file}.png")), &pixels);
                        std::fs::write(
                            output.join(format!("{file}.svg")),
                            svg(&frames[0], viewport),
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
