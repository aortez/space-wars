use super::*;
use engine_common::{ClockEventKind, ClockEventProfile, ClockRainAmount, ClockTimeFormat};

#[test]
fn rain_is_visible_in_both_render_paths_and_leaves_the_clock_face_readable() {
    for viewport in [
        Viewport::new(800.0, 480.0),
        Viewport::new(1024.0, 768.0),
        Viewport::new(480.0, 800.0),
    ] {
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            time_format: ClockTimeFormat::TwelveHour,
            rain_amount: ClockRainAmount::Heavy,
            ..ClockConfig::default()
        };
        let mut state = ClockScenario::init(config, 0);
        let reading = ClockReading::new(12, 0, 0).unwrap();
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(reading),
                ClockAction::preview_event(ClockEventKind::Rain),
            ],
            Duration::ZERO,
        );
        let mut reference = ClockScenario::init(config, 0);
        ClockScenario::step(
            &mut reference,
            &[ClockAction::set_reading(reading)],
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
        let duration = scenario_clock::EVENT_CATALOG[ClockEventKind::Rain as usize].duration_ticks;
        let mut saw_duck = false;
        for tick in 1..=duration {
            scenario.step(&[], Duration::from_nanos(16_666_667));
            saw_duck |= scenario
                .state
                .rain_state()
                .is_some_and(|r| r.duck_position_milli.is_some());
            if ![
                60, 360, 480, 540, 660, 900, 1100, 1230, 2400, 2470, duration,
            ]
            .contains(&tick)
            {
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
                    <= 800
            );
            assert_eq!(
                frames[0]
                    .layers
                    .iter()
                    .filter(|l| l.z >= 2)
                    .collect::<Vec<_>>(),
                normal
                    .layers
                    .iter()
                    .filter(|l| l.z >= 2)
                    .collect::<Vec<_>>(),
                "rain never replaces/obscures the clock face"
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
            if tick == duration {
                assert_eq!(frames[0], normal);
                assert_eq!(pixels.as_bytes(), normal_pixels.as_bytes());
            } else if tick == 900 {
                assert_ne!(pixels.as_bytes(), normal_pixels.as_bytes());
            }
            if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                let file = std::fs::File::create(directory.join(format!(
                    "rain-{tick}-{}x{}.png",
                    viewport.width, viewport.height
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
        assert!(saw_duck);
        assert_eq!(
            (scenario.state.body_count(), scenario.state.collider_count()),
            (0, 0)
        );
    }
}
