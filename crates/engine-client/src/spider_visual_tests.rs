//! Production Clock/render captures of the spider's visit: the raster and
//! vector adapters drawing the spider, its line and the hatch through whole
//! visits in the device-sized views. A small sample of frames always runs;
//! every other frame is written out when `SPACEWARS_SPIDER_VISIT_ARTIFACTS`
//! names a directory, for the docs' pictures.
use crate::{
    render::Viewport,
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::{collections::BTreeSet, time::Duration};

#[test]
fn spider_visit_renders_in_every_view_and_exports_frames() {
    let output = std::env::var_os("SPACEWARS_SPIDER_VISIT_ARTIFACTS").map(std::path::PathBuf::from);
    for (width, height) in [(1024, 768), (800, 480), (480, 800)] {
        let name = format!("{width}x{height}");
        let directory = output.as_ref().map(|path| path.join(&name));
        if let Some(path) = &directory {
            std::fs::create_dir_all(path).unwrap();
        }
        let viewport = Viewport::new(width as f32, height as f32);
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            7,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(12, 39, 58).unwrap()),
                ClockAction::preview_event(ClockEventKind::Spider),
            ],
            Duration::ZERO,
        );
        let mut phases = BTreeSet::new();
        for tick in 0..=40 * 60 {
            let Some(spider) = state.spider_state() else {
                break;
            };
            phases.insert(spider.phase.as_str());
            // The host feeds the time every second; the minute changes two
            // seconds in, while the spider is on its way down.
            if tick % 60 == 0 {
                let seconds = (58 + tick / 60) as u8;
                let reading = ClockReading::new(12, 39 + seconds / 60, seconds % 60).unwrap();
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::set_reading(reading)],
                    Duration::ZERO,
                );
            }
            // A small raster/vector sample always runs; the frames for a
            // film are opt-in.
            if tick % 120 == 0 || directory.is_some() && tick % 2 == 0 {
                let frame = ClockScenario::render_frame(&state);
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(path) = &directory {
                    write_png(&path.join(format!("frame-{:04}.png", tick / 2)), &pixels);
                    if tick % 120 == 0 {
                        std::fs::write(
                            path.join(format!("tick-{tick}-{}.svg", spider.phase.as_str())),
                            vector,
                        )
                        .unwrap();
                    }
                }
            }
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        for phase in ["descending", "crawling", "swinging", "leaving"] {
            assert!(phases.contains(phase), "{name}: no {phase}: {phases:?}");
        }
        assert!(state.spider_state().is_none(), "{name}: still visiting");
        assert_eq!(state.spider_accounting().departed, 1, "{name}");
    }
}
