//! The browser only plays captured frames; simulation and both render adapters
//! are the production Rust paths. Artifact export is opt-in, assertions are not.
use super::*;
use scenario_clock::water_fixture::{DRAIN_SPLASH_CASES, DRAIN_SPLASH_SEEDS, SplashTreatment};

#[test]
fn live_clock_drain_splashes_render_and_clean_up() {
    use std::fmt::Write;
    let output =
        std::env::var_os("SPACEWARS_DRAIN_SPLASH_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut html = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Clock drain splashes</title><style>body{background:#101728;color:#eee;font:16px system-ui;margin:24px}section{display:flex;gap:16px;flex-wrap:wrap}figure{margin:0}img{max-width:100%;width:400px}a{color:#80dfff}</style><h1>Clock: varied Lively</h1><p>Actual Rain and Meltdown cycles, seed 7. Frames show a drain burst, then 0.2 and 0.4 seconds later. Captured through the native raster and vector renderers.</p>",
    );
    let mut results = Vec::new();
    for (width, height) in [(1024, 768), (800, 480), (480, 800)] {
        for kind in [ClockEventKind::Rain, ClockEventKind::Meltdown] {
            let config = ClockConfig {
                aspect_ratio: width as f32 / height as f32,
                time_format: engine_common::ClockTimeFormat::TwentyFourHour,
                event_profile: ClockEventProfile::Off,
                rain_amount: ClockRainAmount::Heavy,
                ..ClockConfig::default()
            };
            let mut state = ClockScenario::init(config, 7);
            let reading = ClockReading::new(8, 8, 0).unwrap();
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(reading),
                    ClockAction::preview_event(kind),
                ],
                Duration::ZERO,
            );
            let end = if kind == ClockEventKind::Rain {
                2520
            } else {
                510
            };
            let mut first_burst = None;
            let mut captured_burst = None;
            let capture_after = if kind == ClockEventKind::Rain {
                600
            } else {
                180
            };
            let mut bursts = 0;
            let mut redirected = 0;
            let mut suppressed = 0;
            let mut peak_parcels = 0;
            let mut captures = 0;
            let viewport = Viewport::new(width as f32, height as f32);
            let mut dry = ClockScenario::init(config, 7);
            ClockScenario::step(
                &mut dry,
                &[ClockAction::set_reading(reading)],
                Duration::ZERO,
            );
            let dry_frame = ClockScenario::render_frame(&dry);
            let dry_pixels = raster(&dry_frame, viewport);
            writeln!(html, "<h2>{} {width}×{height}</h2><section>", kind.as_str()).unwrap();
            for tick in 1..=end {
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                let stats = match kind {
                    ClockEventKind::Rain => state.rain_state().map(|s| {
                        (
                            s.drain_splash_bursts,
                            s.drain_splash_microunits,
                            s.drain_splash_suppressed,
                            s.parcels,
                        )
                    }),
                    ClockEventKind::Meltdown => state.meltdown_state().map(|s| {
                        (
                            s.drain_splash_bursts,
                            s.drain_splash_microunits,
                            s.drain_splash_suppressed,
                            s.spill_parcels,
                        )
                    }),
                    _ => unreachable!(),
                };
                if let Some((count, volume, limited, parcels)) = stats {
                    // Sample established flow instead of the first tiny crack
                    // in the drain. The full event is still checked below.
                    if count > bursts && tick >= capture_after && captured_burst.is_none() {
                        captured_burst = Some(tick);
                    }
                    bursts = count;
                    redirected = volume;
                    suppressed = limited;
                    peak_parcels = peak_parcels.max(parcels);
                    if bursts > 0 && first_burst.is_none() {
                        first_burst = Some(tick);
                    }
                }
                if !captured_burst
                    .is_some_and(|first| [first, first + 12, first + 24].contains(&tick))
                {
                    continue;
                }
                captures += 1;
                let frame = ClockScenario::render_frame(&state);
                let pixels = raster(&frame, viewport);
                let vector =
                    render::scene_primitives_from_frames(std::slice::from_ref(&frame), viewport);
                assert!(!vector.is_empty() && vector.len() <= 3600);
                assert!(
                    vector
                        .iter()
                        .all(|p| !p.commands.contains("NaN") && !p.commands.contains("inf"))
                );
                assert!(
                    pixels
                        .as_slice()
                        .iter()
                        .zip(dry_pixels.as_slice())
                        .filter(|(p, dry)| is_water(**p) && !is_water(**dry))
                        .count()
                        > 20
                );
                if let Some(path) = &output {
                    let name = format!("{}-{width}x{height}-tick-{tick}", kind.as_str());
                    write_png(&path.join(format!("{name}.png")), &pixels);
                    std::fs::write(path.join(format!("{name}.svg")), svg(&frame, viewport))
                        .unwrap();
                    writeln!(html, "<figure><a href=\"{name}.png\"><img src=\"{name}.png\"></a><figcaption>tick {tick}, burst {bursts} · <a href=\"{name}.svg\">SVG</a></figcaption></figure>").unwrap();
                }
            }
            html.push_str("</section>");
            assert!(
                bursts > 0 && redirected > 0,
                "no spray: {kind:?} {width}×{height}"
            );
            assert_eq!(captures, 3);
            assert!(state.rain_state().is_none() && state.meltdown_state().is_none());
            assert_eq!(state.floor_mode(), engine_common::ClockFloorMode::Closed);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(ClockScenario::render_frame(&state), dry_frame);
            results.push(serde_json::json!({
                "event": kind.as_str(), "width":width, "height":height,
                "seed":7, "first_burst_tick":first_burst, "bursts":bursts,
                "captured_burst_tick":captured_burst,
                "redirected_microunits":redirected, "suppressed":suppressed,
                "peak_parcels":peak_parcels,
            }));
        }
    }
    if let Some(path) = &output {
        std::fs::write(path.join("index.html"), html).unwrap();
        std::fs::write(
            path.join("cycles.json"),
            serde_json::to_string_pretty(&results).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn drain_splash_lab_renders_and_exports_comparisons() {
    let output = std::env::var_os("SPACEWARS_DRAIN_SPLASH_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut manifest = Vec::new();
    for case in DRAIN_SPLASH_CASES {
        for treatment in SplashTreatment::ALL {
            for seed in std::iter::once(None).chain(DRAIN_SPLASH_SEEDS.map(Some)) {
                if treatment == SplashTreatment::Off && seed.is_some() {
                    continue;
                }
                let suffix = seed.map_or(String::new(), |seed| format!("-seed-{seed}"));
                let name = format!("{}-{}{suffix}", case.name, treatment.name());
                let directory = output.as_ref().map(|o| o.join(&name));
                if let Some(path) = &directory {
                    std::fs::create_dir_all(path).unwrap();
                }
                let mut fixture = case.seeded_fixture(treatment, seed);
                let mut samples = Vec::new();
                let mut peak_y = f32::NEG_INFINITY;
                for tick in 0..=960 {
                    if tick > 0 {
                        fixture.step_feeding(1.0 / 60.0, tick <= 240);
                    }
                    for p in fixture.water.parcels() {
                        if p.velocity.y > 0.0 {
                            peak_y = peak_y.max(p.position.y);
                        }
                    }
                    // Eight-second loops at 30 FPS; also capture full wind-down.
                    let still = [60, 120, 240, 480, 960].contains(&tick);
                    if !(still || output.is_some() && tick <= 480 && tick % 2 == 0) {
                        continue;
                    }
                    let mut frame = fixture.frame();
                    frame.camera = Camera2::new(RenderPoint::new(0.0, -5.0), 110.0);
                    let pixels = raster(&frame, Viewport::new(640.0, 480.0));
                    let stats = fixture.water.stats();
                    if let Some(path) = &directory {
                        if tick <= 480 && tick % 2 == 0 {
                            write_png(&path.join(format!("frame-{:03}.png", tick / 2)), &pixels);
                            samples.push(serde_json::json!({
                                "t": tick as f64 / 60.0,
                                "bursts": stats.splash_bursts,
                                "spray": stats.splash_volume,
                                "parcels": stats.parcels,
                                "drained": stats.drained,
                            }));
                        }
                    }
                    if still {
                        for (layout, width, height) in [
                            ("picade", 1024.0, 768.0),
                            ("hyperpixel", 800.0, 480.0),
                            ("portrait", 480.0, 800.0),
                        ] {
                            frame.camera = Camera2::new(
                                RenderPoint::new(0.0, -5.0),
                                (110.0_f32).max(80.0 * height / width),
                            );
                            let viewport = Viewport::new(width, height);
                            let vector = render::scene_primitives_from_frames(
                                std::slice::from_ref(&frame),
                                viewport,
                            );
                            assert!(vector.len() < 2000);
                            assert!(vector.iter().all(
                                |p| !p.commands.contains("NaN") && !p.commands.contains("inf")
                            ));
                            let pixels = raster(&frame, viewport);
                            let units_per_pixel = frame.camera.height / height;
                            for (i, pixel) in pixels.as_slice().iter().enumerate() {
                                if !is_water(*pixel) {
                                    continue;
                                }
                                let point = engine_core::Vec2::new(
                                    (i % width as usize) as f32 * units_per_pixel
                                        + units_per_pixel * 0.5
                                        - width * units_per_pixel * 0.5,
                                    frame.camera.center.y + frame.camera.height * 0.5
                                        - (i / width as usize) as f32 * units_per_pixel
                                        - units_per_pixel * 0.5,
                                );
                                assert!(
                                    !fixture.inside_panel(point, units_per_pixel * 1.6),
                                    "water rendered inside panel: {name} {layout} tick={tick}"
                                );
                            }
                            // At this point the source is still replenished.
                            if tick <= 240 {
                                assert!(
                                    pixels.as_slice().iter().filter(|p| is_water(**p)).count() > 30
                                );
                            }
                            if let Some(path) = &directory {
                                write_png(&path.join(format!("{layout}-{tick}.png")), &pixels);
                                std::fs::write(
                                    path.join(format!("{layout}-{tick}.svg")),
                                    svg(&frame, viewport),
                                )
                                .unwrap();
                            }
                        }
                    }
                }
                let stats = fixture.water.stats();
                if treatment == SplashTreatment::Off || case.name == "gentle" {
                    assert_eq!(stats.splash_bursts, 0);
                } else if case.name == "heavy" || case.name == "opposed" {
                    assert!(stats.splash_bursts > 0);
                    if treatment == SplashTreatment::Lively {
                        assert!(peak_y > 0.0, "spray never clears the lip: {name}");
                    }
                }
                manifest.push(serde_json::json!({"case":case.name, "mode":treatment.name(), "seed":seed, "directory":name, "opening":case.opening, "depths":case.depths, "samples":samples}));
            }
        }
    }
    if let Some(output) = output {
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/screenshots/water/dirtsim-meltdown-splash-reference.png"),
            output.join("dirtsim-reference.png"),
        )
        .unwrap();
        let html = include_str!("splash.html")
            .replace("__MANIFEST__", &serde_json::to_string(&manifest).unwrap());
        std::fs::write(output.join("index.html"), html).unwrap();
    }
}

#[test]
#[ignore = "opt-in paired desktop timing; no FPS assertion"]
fn drain_splash_lab_profile() {
    use crate::raster::{RasterOptions, RasterRenderer};
    use crate::render::FrameLayout;
    use std::{fmt::Write, hint::black_box, time::Instant};
    let mut csv = String::from(
        "repeat,case,mode,scale,step_mean_us,scene_mean_us,raster_mean_us,vector_mean_us,step_p95_us,scene_p95_us,raster_p95_us,vector_p95_us,peak_parcels,bursts,capacity_ticks\n",
    );
    for repeat in 0..3 {
        for case in DRAIN_SPLASH_CASES {
            for scale in [1.0, 2.0] {
                let mut treatments = [
                    (SplashTreatment::Off, None),
                    (SplashTreatment::Lively, None),
                    (SplashTreatment::Lively, Some(7)),
                ];
                if repeat % 2 != 0 {
                    treatments.reverse();
                }
                for (treatment, seed) in treatments {
                    let mode = seed.map_or(treatment.name().to_owned(), |seed| {
                        format!("{}-seed-{seed}", treatment.name())
                    });
                    let viewport = Viewport::new(1024.0, 768.0);
                    let mut renderer = RasterRenderer::new();
                    let options = RasterOptions::for_scale(scale);
                    let raster_viewport =
                        Viewport::new(viewport.width * scale, viewport.height * scale);
                    let mut fixture = case.seeded_fixture(treatment, seed);
                    // Warm code and buffers, then reset the deterministic flow.
                    for _ in 0..60 {
                        fixture.step(1.0 / 60.0);
                        let frame = fixture.frame();
                        black_box(renderer.image_from_frames_with_layout(
                            &[frame],
                            raster_viewport,
                            FrameLayout::EqualHorizontal,
                            options,
                        ));
                    }
                    let mut fixture = case.seeded_fixture(treatment, seed);
                    let mut timings = Vec::with_capacity(480);
                    let mut peak = 0;
                    for tick in 0..480 {
                        let start = Instant::now();
                        fixture.step_feeding(1.0 / 60.0, tick < 240);
                        let step = start.elapsed().as_secs_f64() * 1e6;
                        let start = Instant::now();
                        let mut frame = fixture.frame();
                        frame.camera = Camera2::new(RenderPoint::new(0.0, -5.0), 110.0);
                        let scene = start.elapsed().as_secs_f64() * 1e6;
                        let start = Instant::now();
                        black_box(renderer.image_from_frames_with_layout(
                            std::slice::from_ref(&frame),
                            raster_viewport,
                            FrameLayout::EqualHorizontal,
                            options,
                        ));
                        let raster = start.elapsed().as_secs_f64() * 1e6;
                        let start = Instant::now();
                        black_box(render::scene_primitives_from_frames(
                            std::slice::from_ref(&frame),
                            viewport,
                        ));
                        let vector = start.elapsed().as_secs_f64() * 1e6;
                        timings.push([step, scene, raster, vector]);
                        peak = peak.max(fixture.water.stats().parcels);
                    }
                    let mean: [f64; 4] = std::array::from_fn(|i| {
                        timings.iter().map(|t| t[i]).sum::<f64>() / timings.len() as f64
                    });
                    let p95: [f64; 4] = std::array::from_fn(|i| {
                        let mut column: Vec<_> = timings.iter().map(|t| t[i]).collect();
                        column.sort_by(f64::total_cmp);
                        column[column.len() * 95 / 100]
                    });
                    let s = fixture.water.stats();
                    assert!(
                        (s.injected - s.pooled - s.in_flight - s.drained).abs() < s.injected * 1e-9
                    );
                    writeln!(csv, "{repeat},{},{},{scale},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{peak},{},{}", case.name,mode, mean[0],mean[1],mean[2],mean[3],p95[0],p95[1],p95[2],p95[3],s.splash_bursts,s.capacity_limited_ticks).unwrap();
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DRAIN_SPLASH_PROFILE") {
        std::fs::write(path, csv).unwrap();
    } else {
        print!("{csv}");
    }
}
