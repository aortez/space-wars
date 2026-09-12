//! Paired text-model publication with frozen real match geometry. Simulation,
//! geometry rasterization, comparisons and CSV output are outside draw timers.
use super::*;
use crate::{client_scenarios::ScenarioStartMode, host, render};
use slint::{ModelRc, VecModel};

pub(super) fn run(
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(windows.clone())))?;
    let uis = [crate::MainWindow::new()?, crate::MainWindow::new()?];
    let viewport = render::Viewport::new(width as f32, height as f32);
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
    for _ in 0..120 {
        scenario.step(&[], Duration::from_nanos(16_666_667));
    }
    let frames = scenario.render_frames(host::RenderBackend::Raster, viewport);
    let rows = render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
    assert!(!rows.is_empty());
    let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
        &frames,
        render::Viewport::new(width as f32 * 2.0, height as f32 * 2.0),
        scenario.frame_layout(),
        crate::raster::RasterOptions::for_scale(2.0),
    );
    for ui in &uis {
        ui.set_launcher_scenario("spacewars".into());
        ui.set_raster_visible(true);
        ui.set_raster_frame(image.clone());
        ui.show()?;
        ui.window().set_size(PhysicalSize::new(width, height));
    }
    let windows = windows.borrow();
    let mut pixels = [
        vec![Xrgb::default(); (width * height) as usize],
        vec![Xrgb::default(); (width * height) as usize],
    ];
    println!(
        "fixture,model,width,height,rows,repeat,frames,publish_ms,draw_ms,dirty_ms,text_ms,checksum"
    );
    for changing in [false, true] {
        for repeat in 0..options.presentation_repeats {
            let mut publish_times = [Duration::ZERO; 2];
            let mut draw_times = [Duration::ZERO; 2];
            let mut dirty_times = [Duration::ZERO; 2];
            let mut text_times = [Duration::ZERO; 2];
            for frame in 0..options.presentation_frames + 10 {
                let mut next = rows.clone();
                if changing {
                    // Exercise one changing HUD label, movement, style and row
                    // insertion/removal; the other real match labels stay fixed.
                    next[0].text = format!("Recovery {:03}", frame % 100).into();
                    next[0].text_x += (frame % 9) as f32;
                    next[0].font_size += (frame % 3) as f32;
                    if frame % 4 < 2 {
                        next.pop();
                    }
                    if frame % 11 == 0 {
                        next.clear();
                    }
                }
                for index in if repeat % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let next = next.clone();
                    let started = Instant::now();
                    if index == 0 {
                        uis[index].set_primitives(ModelRc::new(VecModel::from(next)));
                    } else {
                        host::update_raster_text_overlay(&uis[index], next);
                    }
                    let publish_time = started.elapsed();
                    MEASUREMENTS.with(|m| {
                        *m.borrow_mut() = [Measurement::default(); DRAW_DIAGNOSTIC_COUNT]
                    });
                    windows[index].request_redraw();
                    let draw_time = render(
                        &windows[index],
                        &mut pixels[index],
                        width as usize,
                        options.presentation_detail,
                    );
                    if frame >= 10 {
                        publish_times[index] += publish_time;
                        draw_times[index] += draw_time;
                        MEASUREMENTS.with(|m| {
                            let m = m.borrow();
                            dirty_times[index] += m[DrawDiagnostic::DirtyRegion as usize].elapsed;
                            text_times[index] += m[DrawDiagnostic::Text as usize].elapsed;
                        });
                    }
                }
                assert!(
                    pixels[0] == pixels[1],
                    "text publication changed pixels at repeat {repeat}, frame {frame}, changing={changing}"
                );
                black_box(&pixels);
            }
            for index in 0..2 {
                let checksum = pixels[index].iter().fold(0xcbf29ce484222325u64, |h, p| {
                    (h ^ u64::from(p.0)).wrapping_mul(0x100000001b3)
                });
                let ms =
                    |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(options.presentation_frames);
                println!(
                    "{},{},{width},{height},{},{repeat},{},{:.6},{:.6},{:.6},{:.6},{checksum:016x}",
                    if changing { "changing" } else { "frozen" },
                    if index == 0 { "replace" } else { "retain" },
                    rows.len(),
                    options.presentation_frames,
                    ms(publish_times[index]),
                    ms(draw_times[index]),
                    ms(dirty_times[index]),
                    ms(text_times[index])
                );
            }
        }
    }
    Ok(())
}
