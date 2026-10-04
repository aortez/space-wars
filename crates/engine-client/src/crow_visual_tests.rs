//! Production Clock/render captures. The HTML viewer only plays recorded frames.
use crate::{
    render::Viewport,
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::time::Duration;

mod ground;

#[test]
fn crow_flight_renders_and_exports_playback() {
    let output = std::env::var_os("SPACEWARS_CROW_FLIGHT_ARTIFACTS").map(std::path::PathBuf::from);
    let mut cases = Vec::new();
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
            42,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Crow),
            ],
            Duration::ZERO,
        );
        let mut samples = Vec::new();
        let mut saw_perch = false;
        let mut saw_leaving = false;
        for tick in 0..=1320 {
            if tick == 480 {
                // Remove the occupied support through normal Clock inputs.
                for (hour, minute) in [(5, 55), (2, 22), (1, 11)] {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::set_reading(
                            ClockReading::new(hour, minute, 0).unwrap(),
                        )],
                        Duration::ZERO,
                    );
                }
            }
            if tick == 900 {
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
                    Duration::ZERO,
                );
            }
            let crow = state.crow_state();
            saw_perch |= crow.is_some_and(|c| c.phase.as_str() == "perched");
            saw_leaving |= crow.is_some_and(|c| c.phase.as_str() == "leaving");
            // A small raster/vector smoke sample always runs. Full playback is opt-in.
            if tick % 120 == 0 || directory.is_some() && tick % 2 == 0 {
                let frame = ClockScenario::render_frame(&state);
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(path) = &directory {
                    write_png(&path.join(format!("frame-{:04}.png", tick / 2)), &pixels);
                    if tick % 120 == 0 {
                        std::fs::write(path.join(format!("tick-{tick}.svg")), vector).unwrap();
                    }
                }
            }
            if tick % 2 == 0 {
                samples.push(serde_json::json!({"tick":tick,"crow":crow}));
            }
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(
            saw_perch && saw_leaving,
            "{name}: missing landing/departure"
        );
        assert!(state.crow_state().is_none());
        cases
            .push(serde_json::json!({"name":name,"width":width,"height":height,"samples":samples}));
    }
    if let Some(path) = &output {
        std::fs::write(
            path.join("manifest.json"),
            serde_json::to_string_pretty(&cases).unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("index.html"),
            include_str!("crow_visual_tests/flight.html"),
        )
        .unwrap();
    }
}
