//! Source construction/destruction and completed geometry on moving production
//! states. Simulation steps are outside each timing; no prerecorded source data.
use super::{Context, Error, Stamp, live_retained, save_png};
use crate::{
    client_scenarios::{ClientScenario, RenderBackend},
    gpu_scene::{
        CompositePresenter, Mode, PlanMode,
        presentation::{self, scratch::Scratch},
        source_control::LIMIT_BYTES,
        streaming::Policy,
    },
    presentation_probe::Options,
    render::Viewport,
};
use engine_common::{RenderFrame, RenderPointPool};
use glow::HasContext;
use std::{fs::File, io::Write, time::Duration};

const DT: Duration = Duration::from_nanos(16_666_667);
const WARMUP: usize = 16;

fn frames(
    s: &dyn ClientScenario,
    v: Viewport,
    pool: &mut Option<RenderPointPool>,
) -> Result<Vec<RenderFrame>, Error> {
    match pool {
        Some(pool) => s
            .render_frames_pooled(RenderBackend::Raster, v, pool)
            .ok_or_else(|| "fixture has no pooled producer".into()),
        None => Ok(s.render_frames(RenderBackend::Raster, v)),
    }
}

fn dispose(frames: Vec<RenderFrame>, pool: &mut Option<RenderPointPool>) {
    if let Some(pool) = pool {
        pool.recycle(frames);
    }
}

fn presenter(context: &Context) -> Result<CompositePresenter, Error> {
    let mut r = CompositePresenter::new(context.gl.clone())?;
    r.set_stream_policy(Policy::PassBucketed);
    Ok(r)
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let metadata = serde_json::json!({"version":1,"kind":"source-points-profile","gl":context.identity,
        "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "warmup_ticks":WARMUP,"timing":"source construction and disposal/recycling; completed path also includes scratch projection, draw, glFinish and plan recycling; wall and thread CPU",
        "simulation_text_ui_scanout_timed":false,"source_timed":true,"geometry":"retained-culled","streams":"bucketed","plan":"scratch",
        "source_pool_limit_bytes":LIMIT_BYTES,"order":"AB/BA alternating repeats, fresh matched scenario per policy block",
        "validation":"ordinary frame contents plus exact pixels/counts, same moving state, three viewport shapes, resources retained across scenario replacement"});
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    eprintln!("source_profile={metadata}");
    println!(
        "fixture,path,mode,repeat,frame,primitives,p1_height,p2_height,wall_ms,cpu_ms,pool_high_water_bytes,pool_reuses,pool_misses,pool_discards"
    );
    let mut validators = [presenter(context)?, presenter(context)?];
    let mut validation_pool = Some(RenderPointPool::new(LIMIT_BYTES));
    for fixture in live_retained::FIXTURES {
        let mut s = live_retained::scenario(v, fixture)?;
        for step in 0..24 {
            s.step(&[], DT);
            for viewport in [v, Viewport::new(641., 361.), Viewport::new(361., 641.)] {
                let ordinary = frames(&*s, viewport, &mut None)?;
                let pooled = frames(&*s, viewport, &mut validation_pool)?;
                if ordinary != pooled {
                    return Err(
                        format!("{} step={step}: source frame changed", fixture.name).into(),
                    );
                }
                let mut expected = None;
                for (frames, r) in [&ordinary, &pooled].into_iter().zip(&mut validators) {
                    let plan = presentation::build_with_retained(
                        frames,
                        viewport,
                        s.frame_layout(),
                        1.,
                        true,
                    )?;
                    let out = r.draw_with_mode(&plan, Mode::RetainedCulled)?;
                    let pixels = r.readback()?;
                    let counts = (
                        plan.shapes(),
                        out.mesh.vertices,
                        out.mesh.indices,
                        out.mesh.draws,
                        out.mesh.upload_bytes,
                        out.mesh.retained.admissions,
                    );
                    if let Some((ref previous, ref previous_counts)) = expected {
                        if *previous != pixels || *previous_counts != counts {
                            return Err(format!(
                                "{} step={step}: pixels or draw counts changed",
                                fixture.name
                            )
                            .into());
                        }
                    } else {
                        if step == 23
                            && viewport == v
                            && let Some(path) = &options.geometry_output
                        {
                            save_png(
                                &path.join(format!("{}.png", fixture.name)),
                                width,
                                height,
                                png::ColorType::Rgba,
                                &pixels,
                            )?;
                        }
                        expected = Some((pixels, counts));
                    }
                    r.collect_retired();
                }
                dispose(pooled, &mut validation_pool);
            }
        }
        eprintln!(
            "validated {}: 72 paired source/pixel/count checks",
            fixture.name
        );
        for completed in [false, true] {
            let path = if completed { "completed" } else { "cpu-source" };
            for repeat in 0..options.presentation_repeats {
                for offset in 0..2 {
                    let pooled = (repeat + offset) % 2 != 0;
                    let name = if pooled { "pooled" } else { "reference" };
                    let mut pool = pooled.then(|| RenderPointPool::new(LIMIT_BYTES));
                    let mut s = live_retained::scenario(v, fixture)?;
                    let mut r = completed.then(|| presenter(context)).transpose()?;
                    let mut scratch = Scratch::default();
                    let mut samples = Vec::new();
                    for i in 0..WARMUP + options.presentation_frames as usize {
                        s.step(&[], DT);
                        let stamp = Stamp::now()?;
                        let frames = frames(&*s, v, &mut pool)?;
                        let count = std::hint::black_box(&frames)
                            .iter()
                            .flat_map(|f| &f.layers)
                            .map(|l| l.primitives.len())
                            .sum::<usize>();
                        let heights = [frames[0].camera.height, frames[1].camera.height];
                        let plan = if completed {
                            Some(presentation::build_reusing(
                                &frames,
                                v,
                                s.frame_layout(),
                                1.,
                                true,
                                PlanMode::Scratch,
                                scratch.take(),
                            )?)
                        } else {
                            None
                        };
                        dispose(frames, &mut pool);
                        if let (Some(plan), Some(r)) = (plan, &mut r) {
                            r.draw_with_mode(&plan, Mode::RetainedCulled)?;
                            unsafe {
                                context.gl.finish();
                            }
                            scratch.recycle(plan);
                        }
                        let (wall, cpu) = stamp.elapsed()?;
                        if i >= WARMUP {
                            samples.push((
                                i - WARMUP,
                                count,
                                heights,
                                wall,
                                cpu,
                                pool.as_ref().map_or((0, 0, 0, 0), |p| {
                                    (p.high_water_bytes, p.reuses, p.misses, p.discards)
                                }),
                            ));
                        }
                    }
                    for (i, count, h, wall, cpu, (bytes, reuses, misses, discards)) in samples {
                        println!(
                            "{},{path},{name},{repeat},{i},{count},{},{},{wall:.6},{cpu:.6},{bytes},{reuses},{misses},{discards}",
                            fixture.name, h[0], h[1]
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
