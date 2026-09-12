//! Freeze real two-bot matches before paired raster-scale measurements. Ablations
//! deliberately change pixels to identify work; they are not production changes.
//! The terrain-culling comparison instead requires identical full-UI pixels.
use super::*;
use crate::{client_scenarios::ScenarioStartMode, host, raster as rasterizer, render as scene};
use engine_common::{RenderFrame, RenderPrimitive};
use std::{collections::BTreeMap, fs::File, io::Write, path::Path};

const TERRAIN_LAYER: i32 = -10;

pub(super) fn run(
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(windows.clone())))?;
    let uis = [crate::MainWindow::new()?, crate::MainWindow::new()?];
    for ui in &uis {
        ui.set_launcher_scenario("spacewars".into());
        ui.set_raster_visible(true);
        ui.show()?;
        ui.window().set_size(PhysicalSize::new(width, height));
    }
    let windows = windows.borrow();
    let viewport = scene::Viewport::new(width as f32, height as f32);
    let mut settings = engine_common::Settings::default();
    settings.spacewars.player_1_controller = engine_common::SpacewarsController::RuleBot;
    settings.spacewars.player_2_controller = engine_common::SpacewarsController::RuleBot;
    let mut scenario = host::HostedScenario::new(
        "spacewars",
        42,
        &settings,
        viewport,
        ScenarioStartMode::Normal,
    )?;
    let mut ticks = options.presentation_match_ticks.clone();
    ticks.sort_unstable();
    ticks.dedup();
    let mut current_tick = 0;
    if let Some(directory) = &options.presentation_output {
        std::fs::create_dir_all(directory)?;
    }
    println!(
        "tick,variant,scale,width,height,primitives,text_rows,repeat,frames,detail,scene_ms,raster_ms,buffers_ms,clear_ms,player_ms,starfield_ms,world_ms,sun_planets_ms,spaceports_ms,effects_ms,ships_ms,debris_ms,particles_ms,other_player_ms,overview_refresh_ms,overview_live_ms,overview_blit_ms,other_frames_ms,image_ms,publish_ms,draw_ms,dirty_ms,text_ms,hud_ms,sun_corona_ms,checksum"
    );
    for tick in ticks {
        while current_tick < tick {
            scenario.step(&[], Duration::from_nanos(16_666_667));
            current_tick += 1;
        }
        let frames = scenario.render_frames(host::RenderBackend::Raster, viewport);
        let layout = scenario.frame_layout();
        assert_eq!(layout, scene::FrameLayout::PlayerViewsWithMinimaps);
        let rows = scene::raster_text_overlay(&frames, viewport, layout);
        if let Some(directory) = &options.presentation_output {
            let mut file = File::create_new(directory.join(format!("tick-{tick}-frames.toml")))?;
            file.write_all(toml::to_string(&BTreeMap::from([("frames", &frames)]))?.as_bytes())?;
            save_counts(
                &directory.join(format!("tick-{tick}-counts.csv")),
                &frames,
                viewport,
                layout,
            )?;
        }
        let reference = options.presentation_terrain_culling.then(|| {
            let reference = scenario
                .render_frames_reference(viewport)
                .expect("Spacewars must expose its uncropped scene reference");
            assert_eq!(
                rows,
                scene::raster_text_overlay(&reference, viewport, layout)
            );
            reference
        });
        if let (Some(directory), Some(reference)) = (&options.presentation_output, &reference) {
            let mut file =
                File::create_new(directory.join(format!("tick-{tick}-unculled-frames.toml")))?;
            file.write_all(toml::to_string(&BTreeMap::from([("frames", reference)]))?.as_bytes())?;
            save_counts(
                &directory.join(format!("tick-{tick}-unculled-counts.csv")),
                reference,
                viewport,
                layout,
            )?;
        }
        let variants: &[&str] = if options.presentation_terrain_culling {
            &["complete", "unculled"]
        } else if options.presentation_raster_ablation {
            &[
                "complete",
                "no-terrain-stroke",
                "no-terrain-fill",
                "no-terrain",
                "no-hud-backing",
                "no-corona",
            ]
        } else {
            &["complete"]
        };
        let mut references: BTreeMap<(&str, u32), Vec<Xrgb>> = BTreeMap::new();
        for repeat in 0..options.presentation_repeats {
            for offset in 0..variants.len() {
                let variant = &variants[if repeat % 2 == 0 {
                    offset
                } else {
                    variants.len() - 1 - offset
                }];
                let mut fixture = if *variant == "unculled" {
                    reference.as_ref().unwrap().clone()
                } else {
                    frames.clone()
                };
                for frame in fixture.iter_mut().take(frames.len() / 2) {
                    for layer in &mut frame.layers {
                        if (*variant == "no-hud-backing" && layer.z == 15)
                            || (*variant == "no-corona" && layer.z == -22)
                        {
                            layer.primitives.clear();
                        }
                        if layer.z != TERRAIN_LAYER {
                            continue;
                        }
                        if *variant == "no-terrain" {
                            layer.primitives.clear();
                        }
                        for primitive in &mut layer.primitives {
                            if let RenderPrimitive::Polygon(polygon) = primitive {
                                if *variant == "no-terrain-stroke" {
                                    polygon.stroke = None;
                                }
                                if *variant == "no-terrain-fill" {
                                    polygon.fill = None;
                                }
                            }
                        }
                    }
                }
                let mut rasters = [
                    rasterizer::RasterRenderer::new(),
                    rasterizer::RasterRenderer::new(),
                ];
                let mut pixels = [
                    vec![Xrgb::default(); (width * height) as usize],
                    vec![Xrgb::default(); (width * height) as usize],
                ];
                for index in if repeat % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let scale = index as u32 + 1;
                    let internal =
                        scene::Viewport::new((width * scale) as f32, (height * scale) as f32);
                    let mut scene_time = Duration::ZERO;
                    let mut raster_time = Duration::ZERO;
                    let mut publish_time = Duration::ZERO;
                    let mut draw_time = Duration::ZERO;
                    let mut dirty_time = Duration::ZERO;
                    let mut text_time = Duration::ZERO;
                    let mut timings = rasterizer::RasterTimings::default();
                    for frame in 0..options.presentation_frames + 10 {
                        // Time construction separately, then draw the same frozen
                        // fixture at both scales. Include disposal of the draw list.
                        let started = Instant::now();
                        drop(black_box(if *variant == "unculled" {
                            scenario.render_frames_reference(viewport).unwrap()
                        } else {
                            scenario.render_frames(host::RenderBackend::Raster, viewport)
                        }));
                        let scene_elapsed = started.elapsed();
                        let started = Instant::now();
                        let raster = rasters[index].image_from_frames_with_layout_timed(
                            &fixture,
                            internal,
                            layout,
                            rasterizer::RasterOptions::for_scale(scale as f32),
                        );
                        let raster_elapsed = started.elapsed();
                        let raster_timings = raster.timings;
                        let started = Instant::now();
                        // Keep the previous image alive until publication, as in
                        // the real host, to exercise triple-buffer ownership.
                        host::update_raster_text_overlay(&uis[index], rows.clone());
                        uis[index].set_raster_frame(raster.image);
                        let publish_elapsed = started.elapsed();
                        MEASUREMENTS.with(|m| {
                            *m.borrow_mut() = [Measurement::default(); DRAW_DIAGNOSTIC_COUNT]
                        });
                        windows[index].request_redraw();
                        let draw_elapsed = render(
                            &windows[index],
                            &mut pixels[index],
                            width as usize,
                            options.presentation_detail,
                        );
                        if frame >= 10 {
                            scene_time += scene_elapsed;
                            raster_time += raster_elapsed;
                            publish_time += publish_elapsed;
                            draw_time += draw_elapsed;
                            timings += raster_timings;
                            MEASUREMENTS.with(|m| {
                                let m = m.borrow();
                                dirty_time += m[DrawDiagnostic::DirtyRegion as usize].elapsed;
                                text_time += m[DrawDiagnostic::Text as usize].elapsed;
                            });
                        }
                    }
                    if let Some(reference) = references.get(&(*variant, scale)) {
                        assert!(
                            *reference == pixels[index],
                            "frozen pixels changed between repeats"
                        );
                    } else {
                        references.insert((*variant, scale), pixels[index].clone());
                    }
                    if options.presentation_terrain_culling {
                        let other = if *variant == "unculled" {
                            "complete"
                        } else {
                            "unculled"
                        };
                        if let Some(reference) = references.get(&(other, scale)) {
                            assert!(
                                *reference == pixels[index],
                                "terrain culling changed pixels at tick {tick}, scale {scale}"
                            );
                        }
                    }
                    let checksum = pixels[index].iter().fold(0xcbf29ce484222325u64, |h, p| {
                        (h ^ u64::from(p.0)).wrapping_mul(0x100000001b3)
                    });
                    let durations = [
                        scene_time,
                        raster_time,
                        timings.buffers,
                        timings.clear,
                        timings.player_views,
                        timings.player_starfield,
                        timings.player_world,
                        timings.player_sun_planets,
                        timings.player_spaceports,
                        timings.player_effects,
                        timings.player_ships,
                        timings.player_debris,
                        timings.player_particles,
                        timings.player_other,
                        timings.overview_refresh,
                        timings.overview_live,
                        timings.overview_blit,
                        timings.other_frames,
                        timings.image,
                        publish_time,
                        draw_time,
                        dirty_time,
                        text_time,
                        timings.player_hud,
                        timings.player_sun_corona,
                    ];
                    let times = durations
                        .map(|d| {
                            format!(
                                "{:.6}",
                                d.as_secs_f64() * 1000.0 / f64::from(options.presentation_frames)
                            )
                        })
                        .join(",");
                    println!(
                        "{tick},{variant},{scale},{width},{height},{},{},{repeat},{},{},{times},{checksum:016x}",
                        rasterizer::primitive_count(&fixture),
                        rows.len(),
                        options.presentation_frames,
                        options.presentation_detail
                    );
                    if repeat == 0 {
                        if let Some(directory) = &options.presentation_output {
                            save_png(
                                &directory.join(format!("tick-{tick}-{variant}-{scale}x.png")),
                                width,
                                height,
                                &pixels[index],
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn save_counts(
    path: &Path,
    frames: &[RenderFrame],
    viewport: scene::Viewport,
    layout: scene::FrameLayout,
) -> std::io::Result<()> {
    let mut file = File::create_new(path)?;
    writeln!(
        file,
        "scale,view,z,submitted,visible,visible_polygons,visible_polygon_vertices"
    )?;
    for scale in [1, 2] {
        let viewports = scene::frame_viewports(
            scene::Viewport::new(
                viewport.width * scale as f32,
                viewport.height * scale as f32,
            ),
            frames.len(),
            layout,
        );
        for (view, (frame, viewport)) in frames.iter().zip(viewports).enumerate() {
            for layer in &frame.layers {
                let visible: Vec<_> = layer
                    .primitives
                    .iter()
                    .filter(|p| rasterizer::primitive_visible(frame.camera, viewport, p))
                    .collect();
                let polygons: Vec<_> = visible
                    .iter()
                    .filter_map(|p| {
                        if let RenderPrimitive::Polygon(p) = p {
                            Some(p)
                        } else {
                            None
                        }
                    })
                    .collect();
                writeln!(
                    file,
                    "{scale},{view},{},{},{},{},{}",
                    layer.z,
                    layer.primitives.len(),
                    visible.len(),
                    polygons.len(),
                    polygons.iter().map(|p| p.points.len()).sum::<usize>()
                )?;
            }
        }
    }
    Ok(())
}

fn save_png(
    path: &Path,
    width: u32,
    height: u32,
    pixels: &[Xrgb],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(File::create_new(path)?, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<_> = pixels
        .iter()
        .flat_map(|p| [(p.0 >> 16) as u8, (p.0 >> 8) as u8, p.0 as u8])
        .collect();
    encoder.write_header()?.write_image_data(&bytes)?;
    Ok(())
}
