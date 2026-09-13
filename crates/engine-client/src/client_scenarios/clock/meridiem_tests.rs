use super::*;
use engine_common::{ClockEventKind, ClockEventProfile, ClockTimeFormat};

#[test]
fn meridiem_events_reach_both_render_paths_and_recover_latest_time_pixels() {
    for viewport in [
        Viewport::new(800.0, 480.0),
        Viewport::new(1024.0, 768.0),
        Viewport::new(480.0, 800.0),
    ] {
        for (kind, recovery) in [
            (ClockEventKind::Falling, scenario_clock::FALLING_TICKS),
            (
                ClockEventKind::Meltdown,
                scenario_clock::MELTING_TICKS + scenario_clock::DRAINING_TICKS,
            ),
        ] {
            for (hour, next_hour) in [(11, 12), (23, 0)] {
                let config = ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    time_format: ClockTimeFormat::TwelveHour,
                    event_profile: ClockEventProfile::Off,
                    ..ClockConfig::default()
                };
                let mut state = ClockScenario::init(config, 42);
                ClockScenario::step(
                    &mut state,
                    &[
                        ClockAction::set_reading(ClockReading::new(hour, 59, 0).unwrap()),
                        ClockAction::preview_event(kind),
                    ],
                    Duration::ZERO,
                );
                let next_reading = ClockReading::new(next_hour, 0, 0).unwrap();
                let mut reference = ClockScenario::init(config, 42);
                ClockScenario::step(
                    &mut reference,
                    &[ClockAction::set_reading(next_reading)],
                    Duration::ZERO,
                );
                let normal = ClockScenario::render_frame(&reference);
                let mut scenario = ClockClientScenario {
                    state,
                    last_emitted_reading: Cell::new(None),
                    benchmark: None,
                };
                let mut renderer = crate::raster::RasterRenderer::new();
                let normal_pixels = renderer
                    .image_from_frames_with_layout(
                        std::slice::from_ref(&normal),
                        viewport,
                        scenario.frame_layout(),
                        crate::raster::RasterOptions::default(),
                    )
                    .to_rgb8()
                    .unwrap();
                let duration = recovery + scenario_clock::REFORMING_TICKS;
                for tick in 0..=duration {
                    if tick > 0 {
                        scenario.step(&[], Duration::from_nanos(16_666_667));
                    }
                    if tick == 60 {
                        scenario.step(&[ClockAction::set_reading(next_reading)], Duration::ZERO);
                    }
                    if ![0, 60, recovery, recovery + 45, duration - 1, duration].contains(&tick) {
                        continue;
                    }
                    let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(
                        frames,
                        scenario.render_frames(RenderBackend::Vector, viewport)
                    );
                    let presentation = crate::render::scene_presentation_from_frames_with_layout(
                        &frames,
                        viewport,
                        scenario.frame_layout(),
                    );
                    assert!(!presentation.main_primitives.is_empty());
                    assert!(
                        frames[0]
                            .layers
                            .iter()
                            .map(|l| l.primitives.len())
                            .sum::<usize>()
                            <= 850
                    );
                    let pixels = renderer
                        .image_from_frames_with_layout(
                            &frames,
                            viewport,
                            scenario.frame_layout(),
                            crate::raster::RasterOptions::default(),
                        )
                        .to_rgb8()
                        .unwrap();
                    let label_pixels = pixels
                        .as_slice()
                        .iter()
                        .filter(|p| {
                            p.r.abs_diff(133) <= 1
                                && p.g.abs_diff(184) <= 1
                                && p.b.abs_diff(196) <= 1
                        })
                        .count();
                    if [0, 60, duration].contains(&tick) {
                        assert!(
                            label_pixels > 0,
                            "missing AM/PM: {kind:?}, tick={tick}, {viewport:?}"
                        );
                    }
                    if kind == ClockEventKind::Meltdown && tick == recovery {
                        assert_eq!(
                            label_pixels, 0,
                            "no anchored label while its material is water"
                        );
                    }
                    if tick == duration
                        || (kind == ClockEventKind::Meltdown && tick == duration - 1)
                    {
                        assert_eq!(
                            pixels.as_bytes(),
                            normal_pixels.as_bytes(),
                            "latest label must recover exactly"
                        );
                    }
                    if tick == duration {
                        assert_eq!(frames[0], normal);
                    }
                    if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                        let directory = std::path::PathBuf::from(directory);
                        std::fs::create_dir_all(&directory).unwrap();
                        let file = std::fs::File::create(directory.join(format!(
                            "meridiem-{}-from{hour}-{tick}-{}x{}.png",
                            kind.as_str(),
                            viewport.width,
                            viewport.height,
                        )))
                        .unwrap();
                        let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .unwrap()
                            .write_image_data(pixels.as_bytes())
                            .unwrap();
                    }
                }
            }
        }
    }
}
