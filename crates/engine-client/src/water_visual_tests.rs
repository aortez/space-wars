//! Headless captures of the water edge test bed through both production adapters.
use crate::{
    render::{self, Viewport},
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{
    Camera2, ClockEventKind, ClockEventProfile, ClockRainAmount, RenderPoint, Scenario,
};
use scenario_clock::water_fixture::{OpposedFixture, Profile, WaterFixture};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use slint::{Rgb8Pixel, SharedPixelBuffer};
use std::time::Duration;

// Four pixels per world unit, showing both lips and the outgoing jet. These
// checks always run in normal CI; no window, artifact export, or golden PNGs.
fn opposed_pixels(depths: [f64; 2], mixing: bool) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut fixture = OpposedFixture::new(depths, mixing, false);
    for _ in 0..90 {
        fixture.step(1.0 / 60.0, [true; 2]);
    }
    let mut frame = fixture.frame();
    frame.camera = Camera2::new(RenderPoint::new(0.0, -35.0), 100.0);
    raster(&frame, Viewport::new(480.0, 400.0))
}

fn is_water(pixel: Rgb8Pixel) -> bool {
    // Includes the blue fill and cyan highlight, not the gray supports or sky.
    pixel.b > 180 && pixel.g > 100 && pixel.b.saturating_sub(pixel.r) > 80
}

/// Scan world y=-40..-60, below the junction but above the receiving pool.
/// Every row must contain one connected jet, also connected to the next row.
/// Allow one pixel of discretization, not large holes or two crossing streams.
fn outgoing_jet(
    pixels: &SharedPixelBuffer<Rgb8Pixel>,
    max_width: usize,
) -> Result<Vec<f32>, String> {
    let mut centers = Vec::new();
    let mut previous: Option<(usize, usize)> = None;
    for row in 220..=300 {
        let wet: Vec<_> = (120..360)
            .filter(|x| is_water(pixels.as_slice()[row * 480 + x]))
            .collect();
        let (Some(&first), Some(&last)) = (wet.first(), wet.last()) else {
            return Err(format!("jet missing at row {row}"));
        };
        let width = last - first + 1;
        if width > max_width || width - wet.len() > 1 {
            return Err(format!(
                "split or over-wide jet at row {row}: span={width} wet={}",
                wet.len()
            ));
        }
        if let Some((old_first, old_last)) = previous
            && (first > old_last + 1 || old_first > last + 1)
        {
            return Err(format!(
                "jet disconnected between rows {} and {row}",
                row - 1
            ));
        }
        previous = Some((first, last));
        centers.push(wet.iter().sum::<usize>() as f32 / wet.len() as f32);
    }
    Ok(centers)
}

fn assert_both_sources_visible(pixels: &SharedPixelBuffer<Rgb8Pixel>) {
    // World x=+-22, y=2: inside both source pools, just behind the spill lips.
    for x in [152, 328] {
        assert!(
            is_water(pixels.as_slice()[52 * 480 + x]),
            "source absent at x={x}"
        );
    }
}

#[test]
fn water_equal_streams_render_a_connected_downward_jet() {
    let mixed = opposed_pixels([5.0, 5.0], true);
    assert_both_sources_visible(&mixed);
    let centers = outgoing_jet(&mixed, 16).expect("equal streams must form one connected jet");
    assert!(
        centers.iter().all(|x| (*x - 240.0).abs() <= 6.0),
        "equal jet must stay centered: {centers:?}"
    );

    // Negative control: the former crossing streams must not satisfy the same
    // image contract, even though their volume and replay checks still pass.
    let unmixed = opposed_pixels([5.0, 5.0], false);
    assert_both_sources_visible(&unmixed);
    assert!(
        outgoing_jet(&unmixed, 16).is_err(),
        "pass-through control unexpectedly looks merged"
    );
}

#[test]
fn water_unequal_streams_render_a_connected_deflected_jet() {
    let mixed = opposed_pixels([15.0, 5.0], true);
    assert_both_sources_visible(&mixed);
    let centers = outgoing_jet(&mixed, 48).expect("unequal streams must form one connected jet");
    assert!(
        centers.iter().all(|x| *x > 256.0 && *x < 352.0),
        "stronger left stream must push the jet right: {centers:?}"
    );
    assert!(
        centers.last().unwrap() - centers.first().unwrap() >= 8.0,
        "jet must continue deflecting as it falls: {centers:?}"
    );

    let unmixed = opposed_pixels([15.0, 5.0], false);
    assert_both_sources_visible(&unmixed);
    assert!(
        outgoing_jet(&unmixed, 48).is_err(),
        "pass-through control unexpectedly looks merged"
    );
}

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
