use super::*;
use crate::thruster_visual_tests::{raster, svg, write_png};
use engine_common::{ClockEventKind, ClockEventProfile, ClockTimeFormat};

#[test]
fn centered_clock_and_symmetric_framing_render_across_formats_and_layouts() {
    let output = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        for (format_name, time_format, hour) in [
            ("24h", ClockTimeFormat::TwentyFourHour, 23),
            ("12h", ClockTimeFormat::TwelveHour, 20),
        ] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: ClockEventProfile::Off,
                    time_format,
                    ..Default::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(hour, 58, 0).unwrap(),
                )],
                Duration::ZERO,
            );
            let mut scenario = ClockClientScenario {
                state,
                last_emitted_reading: Cell::new(None),
                benchmark: None,
            };
            let normal = ClockScenario::render_frame(&scenario.state);
            let normal_pixels = raster(&normal, viewport);
            let width = normal_pixels.width() as usize;
            let height = normal_pixels.height() as usize;
            let at = |x, y| normal_pixels.as_slice()[y * width + x];
            // Both bands occupy 8%, with open background immediately inside.
            let band = at(width / 2, height * 4 / 100);
            assert_eq!(band, at(width / 2, height * 96 / 100));
            assert_ne!(band, at(width / 2, height * 9 / 100));
            assert_ne!(band, at(width / 2, height * 91 / 100));
            // Opposite polygon edges can round to adjacent raster rows.
            let edge_differences = (0..height * 8 / 100)
                .filter(|&y| at(width / 2, y) != at(width / 2, height - 1 - y))
                .count();
            assert!(edge_differences <= 2, "{name}: asymmetric framing");
            // Bright cyan excludes dim guides, the frame and the AM/PM label.
            let lit: Vec<_> = normal_pixels
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(_, p)| p.r >= 60 && p.g >= 200 && p.b >= 200)
                .map(|(i, _)| (i % width, i / width))
                .collect();
            let top = lit.iter().map(|p| p.1).min().unwrap();
            let bottom = lit.iter().map(|p| p.1).max().unwrap();
            assert!(
                (top + bottom).abs_diff(height - 1) <= 1,
                "{name} {format_name}"
            );
            if time_format == ClockTimeFormat::TwentyFourHour {
                let left = lit.iter().map(|p| p.0).min().unwrap();
                let right = lit.iter().map(|p| p.0).max().unwrap();
                assert!((left + right).abs_diff(width - 1) <= 1);
            }
            assert_eq!(
                (scenario.state.body_count(), scenario.state.collider_count()),
                (0, 0)
            );

            for (label, event) in [
                ("idle", None),
                ("rain", Some(ClockEventKind::Rain)),
                ("falling", Some(ClockEventKind::Falling)),
                ("meltdown", Some(ClockEventKind::Meltdown)),
                ("marquee", Some(ClockEventKind::Marquee)),
            ] {
                if let Some(event) = event {
                    scenario.preview_clock_event(event);
                    for _ in 0..120 {
                        scenario.step(&[], Duration::from_nanos(16_666_667));
                    }
                }
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                let pixels = raster(&frames[0], viewport);
                let vector = svg(&frames[0], viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                // Events must not fade the canopy with the face or draw rain
                // into the band. Only the topmost full interior rows are used
                // to avoid edge antialiasing at fractional device pixels.
                for y in 0..height * 7 / 100 {
                    assert_eq!(
                        &pixels.as_slice()[y * width..(y + 1) * width],
                        &normal_pixels.as_slice()[y * width..(y + 1) * width],
                        "{name} {format_name} {label} row={y}"
                    );
                }
                if let Some(output) = &output {
                    let stem = format!("layout-{name}-{format_name}-{label}");
                    write_png(&output.join(format!("{stem}.png")), &pixels);
                    std::fs::write(output.join(format!("{stem}.svg")), vector).unwrap();
                }
            }
        }
    }
}
