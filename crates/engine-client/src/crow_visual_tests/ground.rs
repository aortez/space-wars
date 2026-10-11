//! Native dry/rain and Shy/Hardy comparisons of the same seeded visit.
use super::*;
use engine_common::{
    ClockCrowPhase, ClockCrowWaterTolerance, ClockFont, ClockFontSettings, ClockRainAmount,
};

#[test]
fn crow_ground_and_rain_render_and_export_playback() {
    render_visits("SPACEWARS_CROW_GROUND_ARTIFACTS", false);
}

#[test]
fn crow_water_tolerance_renders_and_exports_playback() {
    render_visits("SPACEWARS_CROW_TOLERANCE_ARTIFACTS", true);
}

fn render_visits(output_var: &str, compare_tolerance: bool) {
    let output = std::env::var_os(output_var).map(std::path::PathBuf::from);
    // Seed 5 visits adjacent digit tops, so the rain comparison shows hopping
    // through puddles instead of returning to the isolated tip of the "1".
    // Seed 13 chooses the occasional ground visit on both landscape layouts.
    let seed = if compare_tolerance { 5 } else { 13 };
    let mut cases = Vec::new();
    for (width, height) in [(1024, 768), (800, 480), (480, 800)] {
        for second in [false, true] {
            let rain = compare_tolerance || second;
            let tolerance = if compare_tolerance && second {
                ClockCrowWaterTolerance::Hardy
            } else {
                ClockCrowWaterTolerance::Shy
            };
            let mode = if compare_tolerance {
                if second { "hardy" } else { "shy" }
            } else if second {
                "rain"
            } else {
                "dry"
            };
            let name = format!("{width}x{height}-{mode}");
            let directory = output.as_ref().map(|path| path.join(&name));
            if let Some(path) = &directory {
                std::fs::create_dir_all(path).unwrap();
            }
            let viewport = Viewport::new(width as f32, height as f32);
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: ClockEventProfile::Off,
                    rain_amount: ClockRainAmount::Heavy,
                    crow_water_tolerance: tolerance,
                    fonts: ClockFontSettings {
                        selected: ClockFont::Matrix,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                seed,
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
            let mut ground = false;
            let mut reacted_to_water = false;
            let mut saw_peck_frame = false;
            for tick in 0..=1320 {
                if rain && tick == 180 {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::preview_event(ClockEventKind::Rain)],
                        Duration::ZERO,
                    );
                }
                let crow = state.crow_state();
                let peck = crow
                    .is_some_and(|c| c.phase == ClockCrowPhase::Pecking && c.phase_tick % 28 == 16);
                ground |= crow.is_some_and(|c| c.ground_visits > 0);
                reacted_to_water |= crow.is_some_and(|c| c.wet_departures > 0 || c.escapes > 0);
                if tick % 120 == 0 || peck || directory.is_some() && tick % 2 == 0 {
                    let frame = ClockScenario::render_frame(&state);
                    let pixels = raster(&frame, viewport);
                    let vector = svg(&frame, viewport);
                    assert!(!vector.contains("NaN") && !vector.contains("inf"));
                    if let Some(path) = &directory {
                        if tick % 2 == 0 {
                            write_png(&path.join(format!("frame-{:04}.png", tick / 2)), &pixels);
                        }
                        if tick % 120 == 0 {
                            std::fs::write(path.join(format!("tick-{tick}.svg")), &vector).unwrap();
                        }
                        if peck && !saw_peck_frame {
                            write_png(&path.join("peck.png"), &pixels);
                            std::fs::write(path.join("peck.svg"), vector).unwrap();
                        }
                    }
                    saw_peck_frame |= peck;
                }
                if tick % 2 == 0 {
                    samples.push(serde_json::json!({"tick":tick,"crow":crow}));
                }
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            if !rain && width > height {
                assert!(ground && saw_peck_frame, "{name}: missing ground/peck");
            }
            if rain && tolerance == ClockCrowWaterTolerance::Shy {
                assert!(reacted_to_water, "{name}: missing water response");
            }
            assert!(state.crow_state().is_none());
            cases.push(
                serde_json::json!({"name":name,"width":width,"height":height,"seed":seed,"samples":samples}),
            );
        }
    }
    if let Some(path) = output {
        std::fs::write(
            path.join("manifest.json"),
            serde_json::to_string_pretty(&cases).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("index.html"), include_str!("ground.html")).unwrap();
    }
}
