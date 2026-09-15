//! Headless captures of the water edge test bed through both production adapters.
use crate::{
    render::{self, Viewport},
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockRainAmount, Scenario};
use scenario_clock::water_fixture::{OpposedFixture, Profile, WaterFixture};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::time::Duration;

#[test]
fn water_edge_lab_captures_production_renderers() {
    let output = std::env::var_os("SPACEWARS_WATER_EDGE_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>Water edge lab</title><style>body{background:#151926;color:#eee;font:16px system-ui}img{max-width:100%}a{color:#8de}</style><h1>Water edge lab</h1><p>Production raster PNGs and vector SVGs. The steps/ramp fixtures expose the current stepped-bed approximation; this pass fixes the free outfall.</p>",
    );
    for profile in [Profile::Ledge, Profile::Steps, Profile::Ramp] {
        for mirrored in [false, true] {
            for depth in [1.0, 5.0, 15.0] {
                let mut fixture = WaterFixture::new(depth, mirrored, profile);
                for tick in 1..=120 {
                    fixture.step(1.0 / 60.0, tick <= 60);
                    if ![30, 60, 120].contains(&tick) {
                        continue;
                    }
                    let frame = fixture.frame();
                    let viewport = Viewport::new(1200.0, 660.0);
                    let pixels = raster(&frame, viewport);
                    let vector = render::scene_primitives_from_frames(
                        std::slice::from_ref(&frame),
                        viewport,
                    );
                    assert!(
                        vector
                            .iter()
                            .all(|p| !p.commands.contains("NaN") && !p.commands.contains("inf"))
                    );
                    assert!(vector.len() <= 1200);
                    assert!(pixels.as_slice().iter().any(|p| p.b > 180 && p.g > 100));
                    if matches!(profile, Profile::Ledge) && tick <= 60 {
                        // A near-lip scan in actual output pixels. Leave one
                        // pixel for discretization; this is not an SVG oracle.
                        let surface = fixture.water.pools()[0].columns().next().unwrap().surface;
                        let top = ((30.0 - surface) * 6.0).ceil() as usize;
                        let x = if mirrored { 898 } else { 301 };
                        let wet = (top..180)
                            .filter(|y| {
                                let p = pixels.as_slice()[y * 1200 + x];
                                p.b > 180 && p.g > 100
                            })
                            .count();
                        assert!(
                            wet as f64 / (180 - top) as f64 >= 0.8,
                            "raster lip lost thickness: {mirrored} {depth} {tick}"
                        );
                    }
                    let name = format!(
                        "{}-{}-depth-{depth}-tick-{tick}",
                        profile.name(),
                        if mirrored { "left" } else { "right" }
                    );
                    if let Some(output) = &output {
                        write_png(&output.join(format!("{name}.png")), &pixels);
                        std::fs::write(output.join(format!("{name}.svg")), svg(&frame, viewport))
                            .unwrap();
                        html.push_str(&format!("<h2>{name}</h2><a href='{name}.svg'>Vector SVG</a><br><img src='{name}.png'>"));
                    }
                }
            }
        }
    }
    for (depths, name) in [
        ([5.0, 5.0], "equal-5"),
        ([15.0, 15.0], "equal-15"),
        ([15.0, 5.0], "unequal"),
    ] {
        for mixing in [false, true] {
            let mut fixture = OpposedFixture::new(depths, mixing, false);
            for tick in 1..=180 {
                fixture.step(1.0 / 60.0, [true, tick <= 90]);
                if ![30, 60, 90, 180].contains(&tick) {
                    continue;
                }
                let frame = fixture.frame();
                let viewport = Viewport::new(1200.0, 660.0);
                if let Some(output) = &output {
                    let name = format!("opposed-{name}-mix-{mixing}-tick-{tick}");
                    write_png(
                        &output.join(format!("{name}.png")),
                        &raster(&frame, viewport),
                    );
                    std::fs::write(output.join(format!("{name}.svg")), svg(&frame, viewport))
                        .unwrap();
                    html.push_str(&format!("<h2>{name}</h2><a href='{name}.svg'>Vector SVG</a><br><img src='{name}.png'>"));
                }
            }
            assert_eq!(fixture.water.stats().spill_merges > 0, mixing);
        }
    }
    if let Some(output) = &output {
        // Include the actual event as well as isolated transport fixtures.
        // Artifact-only: the full event's logic is covered in scenario-clock.
        for (w, h) in [(1024.0, 768.0), (800.0, 480.0)] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    event_profile: ClockEventProfile::Off,
                    rain_amount: ClockRainAmount::Heavy,
                    ..ClockConfig::default()
                },
                7,
            );
            state.set_aspect_ratio(w / h);
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
                    ClockAction::preview_event(ClockEventKind::Rain),
                ],
                Duration::ZERO,
            );
            for tick in 1..=1200 {
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                if ![300, 600, 900, 1200].contains(&tick) {
                    continue;
                }
                let frame = ClockScenario::render_frame(&state);
                let viewport = Viewport::new(w, h);
                let name = format!("clock-heavy-rain-{w}x{h}-tick-{tick}");
                write_png(
                    &output.join(format!("{name}.png")),
                    &raster(&frame, viewport),
                );
                std::fs::write(output.join(format!("{name}.svg")), svg(&frame, viewport)).unwrap();
                html.push_str(&format!(
                    "<h2>{name}</h2><a href='{name}.svg'>Vector SVG</a><br><img src='{name}.png'>"
                ));
            }
        }
        std::fs::write(output.join("index.html"), html).unwrap();
    }
}
