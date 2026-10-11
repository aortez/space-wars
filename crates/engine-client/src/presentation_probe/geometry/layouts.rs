//! Integration checks exercise the live composite renderer without scanout.
//! Analytic interior probes supplement the coarse whole-image raster comparison.
use super::{Context, Error, Stamp};
use crate::{
    client_scenarios::{self, RenderBackend, ScenarioStartMode},
    gpu_scene::{CompositePresenter, presentation, target_counts},
    presentation_probe::{Options, geometry},
    raster::{RasterOptions, RasterRenderer},
    render::{self, FrameLayout, Viewport},
};
use engine_common::{
    Camera2, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive, Settings,
    SpacewarsController,
};
use glow::HasContext;
use glutin::prelude::*;
use std::{fs::File, io::Write, time::Duration};

struct Fixture {
    name: String,
    frames: Vec<RenderFrame>,
    layout: FrameLayout,
}

fn rectangle(frame: &mut RenderFrame, z: i32, x: f32, y: f32, w: f32, h: f32, color: RenderColor) {
    frame.push_primitive(
        z,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            vec![
                RenderPoint::new(x, y),
                RenderPoint::new(x + w, y),
                RenderPoint::new(x + w, y + h),
                RenderPoint::new(x, y + h),
            ],
            color,
        )),
    );
}

fn synthetic() -> Fixture {
    let camera = Camera2::new(RenderPoint::ZERO, 100.0);
    let mut left = RenderFrame::new(camera);
    let mut right = left.clone();
    // Both extend well beyond their pane. Wrong scissor paints over its neighbor.
    rectangle(
        &mut left,
        0,
        -500.0,
        -500.0,
        1000.0,
        1000.0,
        RenderColor::RED,
    );
    rectangle(
        &mut right,
        0,
        -500.0,
        -500.0,
        1000.0,
        1000.0,
        RenderColor::GREEN,
    );
    let mut map = RenderFrame::new(camera);
    // Deliberately add these out of Z order. Both cover the central map probe.
    rectangle(
        &mut map,
        3,
        -30.0,
        -30.0,
        60.0,
        60.0,
        RenderColor::rgba(0.0, 0.0, 1.0, 0.5),
    );
    rectangle(
        &mut map,
        -3,
        -30.0,
        -30.0,
        60.0,
        60.0,
        RenderColor::rgba(1.0, 0.0, 0.0, 0.5),
    );
    let empty = RenderFrame::new(camera);
    let mut hud = RenderFrame::new(camera);
    rectangle(&mut hud, 10, -10.0, 25.0, 20.0, 10.0, RenderColor::WHITE);
    Fixture {
        name: "clip-alpha-overlay".into(),
        frames: vec![left, right, map, empty, hud],
        layout: FrameLayout::PlayerViewsWithMinimaps,
    }
}

fn fixtures(v: Viewport) -> Result<Vec<Fixture>, Error> {
    let mut cases = vec![synthetic()];
    let mut settings = Settings::default();
    settings.spacewars.player_1_controller = SpacewarsController::RuleBot;
    settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
    let mut scenario = client_scenarios::registration("spacewars")
        .unwrap()
        .create_with_asset(
            42,
            &settings,
            v,
            ScenarioStartMode::Normal,
            &client_scenarios::ScenarioAsset::None,
        )?;
    let mut tick = 0;
    for target in [120, 1800] {
        while tick < target {
            scenario.step(&[], Duration::from_nanos(16_666_667));
            tick += 1;
        }
        let mut frames = scenario.render_frames(RenderBackend::Raster, v);
        for f in &mut frames {
            for l in &mut f.layers {
                l.primitives
                    .retain(|p| !matches!(p, RenderPrimitive::Text(_)));
            }
        }
        cases.push(Fixture {
            name: format!("spacewars-{target}"),
            frames,
            layout: scenario.frame_layout(),
        });
    }
    Ok(cases)
}

fn software(f: &Fixture, v: Viewport) -> Vec<u8> {
    RasterRenderer::new()
        .image_from_frames_with_layout(&f.frames, v, f.layout, RasterOptions::default())
        .to_rgb8()
        .unwrap()
        .as_bytes()
        .to_vec()
}

fn probes(rgb: &[u8], actual: &[u8], v: Viewport) -> Result<(), Error> {
    let panes = render::frame_viewports(v, 5, FrameLayout::PlayerViewsWithMinimaps);
    let center = |p: Viewport| {
        (
            p.x.round() as usize + p.width.ceil() as usize / 2,
            p.y.round() as usize + p.height.ceil() as usize / 2,
        )
    };
    let mut positions = vec![
        (10, 10),
        (v.width as usize - 10, 10),
        (v.width as usize / 2, (v.height * 0.2) as usize),
    ];
    for p in &panes[2..4] {
        positions.push(center(*p)); // Whole-group opacity and empty dark backing.
        positions.push((p.x.round() as usize + 2, p.y.round() as usize + 2)); // No square backing.
        let c = center(*p);
        positions.push((c.0, p.y.round() as usize + 3)); // Empty area inside circular backing.
    }
    for (x, y) in positions {
        let p = y * v.width as usize + x;
        for channel in 0..3 {
            if rgb[p * 3 + channel].abs_diff(actual[p * 4 + channel]) > 2 {
                return Err(format!(
                    "layout interior probe ({x},{y}) differs: {:?} vs {:?}",
                    &rgb[p * 3..p * 3 + 3],
                    &actual[p * 4..p * 4 + 3]
                )
                .into());
            }
        }
    }
    Ok(())
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if width < 320 || height < 240 {
        return Err("layout probe requires at least 320x240".into());
    }
    let v = Viewport::new(width as f32, height as f32);
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let metadata = serde_json::json!({"version":1, "kind":"layouts", "gl":context.identity, "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,"in_flight":1,"scanout":false,"simulation_timed":false,"text_ui_timed":false,"timing":"synchronous completed render frame (glFinish), not presentation FPS"});
    eprintln!("layout_probe={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let cases = fixtures(v)?;
    let api = slint::GraphicsAPI::NativeOpenGL {
        get_proc_address: &|name| context.display.get_proc_address(name),
    };
    crate::gpu_scene::validate_retirement(
        &api,
        &presentation::build(&cases[0].frames, v, cases[0].layout, 1.0)?,
    )?;
    eprintln!(
        "validated failed-resize retention, layout replacement, CPU fallback and context teardown"
    );
    let counts_before = target_counts();
    let mut renderer = CompositePresenter::new(context.gl.clone())?;
    // Reuse one renderer across native/half/odd/native sizes, empty maps and one/
    // two camera layouts. Old images remain owned until replacement submission.
    for (step, size) in [
        v,
        Viewport::new(v.width / 2.0, v.height / 2.0),
        Viewport::new(641.0, 361.0),
        v,
    ]
    .into_iter()
    .enumerate()
    {
        for case in &cases {
            let reference = software(case, size);
            let plan = presentation::build(&case.frames, size, case.layout, 1.0)?;
            for circles in [false, true] {
                unsafe {
                    context.gl.viewport(9, 10, 11, 12);
                    context.gl.scissor(1, 2, 3, 4);
                    context.gl.enable(glow::SCISSOR_TEST);
                    context.gl.active_texture(glow::TEXTURE1);
                }
                renderer.draw(&plan, circles)?;
                unsafe {
                    let mut viewport = [0; 4];
                    let mut scissor = [0; 4];
                    context
                        .gl
                        .get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
                    context
                        .gl
                        .get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
                    if viewport != [9, 10, 11, 12]
                        || scissor != [1, 2, 3, 4]
                        || !context.gl.is_enabled(glow::SCISSOR_TEST)
                        || context.gl.get_parameter_i32(glow::ACTIVE_TEXTURE)
                            != glow::TEXTURE1 as i32
                    {
                        return Err("compositor did not restore caller GL state".into());
                    }
                }
                let rgba = renderer.readback()?;
                let (mae, different) = geometry::compare(&reference, &rgba)?;
                if case.name == "clip-alpha-overlay"
                    && let Err(error) = probes(&reference, &rgba, size)
                {
                    if let Some(path) = &options.geometry_output {
                        super::save_png(
                            &path.join(format!("failed-{step}-{circles}.png")),
                            size.width as u32,
                            size.height as u32,
                            png::ColorType::Rgba,
                            &rgba,
                        )?;
                    }
                    return Err(error);
                }
                renderer.collect_retired();
                if target_counts().1 != counts_before.1 + 3 {
                    return Err("composite targets grew beyond main + two minimaps".into());
                }
                eprintln!(
                    "validated {} {}x{} circles={circles}: mae={mae:.6} different={different:.6} targets=3",
                    case.name, size.width, size.height
                );
                if step == 0
                    && let Some(path) = &options.geometry_output
                {
                    let label = if circles { "circles" } else { "triangles" };
                    super::save_png(
                        &path.join(format!("{}-{label}.png", case.name)),
                        width,
                        height,
                        png::ColorType::Rgba,
                        &rgba,
                    )?;
                    if !circles {
                        super::save_png(
                            &path.join(format!("{}-raster.png", case.name)),
                            width,
                            height,
                            png::ColorType::Rgb,
                            &reference,
                        )?;
                    }
                }
            }
        }
        let single = Fixture {
            name: "single".into(),
            frames: vec![cases[0].frames[0].clone()],
            layout: FrameLayout::EqualHorizontal,
        };
        let plan = presentation::build(&single.frames, size, single.layout, 1.0)?;
        renderer.draw(&plan, false)?;
        renderer.collect_retired();
        if target_counts().1 != counts_before.1 + 1 {
            return Err("unused minimap target not released".into());
        }
        // Invalid allocation must not change the last resolved image.
        let before = renderer.readback()?;
        let invalid = presentation::Plan {
            width: u32::MAX,
            height: 1,
            passes: Vec::new(),
        };
        if renderer.draw(&invalid, false).is_ok() || renderer.readback()? != before {
            return Err("failed resize lost last image".into());
        }
    }
    renderer.retire();
    renderer.collect_retired();
    if target_counts().1 != counts_before.1 {
        return Err("retirement leaked targets".into());
    }
    println!(
        "case,mode,repeat,frames,completed_wall_ms,cpu_ms,project_wall_ms,encode_wall_ms,submit_wall_ms,draws,upload_bytes,targets"
    );
    for case in &cases {
        // Fixed snapshot, but build/project/tessellate/dispose every measured frame.
        for repeat in 0..options.presentation_repeats {
            for offset in 0..2 {
                let circles = (repeat + offset) % 2 == 1;
                let mut project = Duration::ZERO;
                let mut encode = Duration::ZERO;
                let mut submit = Duration::ZERO;
                let plan = presentation::build(&case.frames, v, case.layout, 1.0)?;
                for _ in 0..8 {
                    renderer.draw(&plan, circles)?;
                }
                renderer.collect_retired();
                unsafe {
                    context.gl.finish();
                }
                let allocations = target_counts().0;
                let mut draws = 0;
                let mut upload = 0;
                let total = Stamp::now()?;
                for _ in 0..options.presentation_frames {
                    let started = std::time::Instant::now();
                    let plan = presentation::build(&case.frames, v, case.layout, 1.0)?;
                    project += started.elapsed();
                    let output = renderer.draw(&plan, circles)?;
                    encode += output.encode;
                    submit += output.submit;
                    draws += output.mesh.draws;
                    upload += output.mesh.upload_bytes;
                    unsafe {
                        context.gl.finish();
                    }
                }
                let (wall, cpu) = total.elapsed()?;
                if target_counts().0 != allocations {
                    return Err("steady layout allocated more targets".into());
                }
                let n = f64::from(options.presentation_frames);
                let ms = |d: Duration| d.as_secs_f64() * 1000.0 / n;
                println!(
                    "{},{},{repeat},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{}",
                    case.name,
                    if circles { "circles" } else { "triangles" },
                    options.presentation_frames,
                    wall / n,
                    cpu / n,
                    ms(project),
                    ms(encode),
                    ms(submit),
                    draws as f64 / n,
                    upload as f64 / n,
                    target_counts().1 - counts_before.1
                );
            }
        }
    }
    drop(renderer);
    if target_counts().1 != counts_before.1 {
        return Err("compositor teardown leaked targets".into());
    }
    Ok(())
}
