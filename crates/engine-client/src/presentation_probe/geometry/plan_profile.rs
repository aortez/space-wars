//! Matched CPU plan preparation/disposal, with and without completed GL draws.
//! A short prerecorded history isolates allocation from simulation/source work;
//! it is not a long live-match, latency, text/UI or scanout benchmark.
use super::{Context, Error, Stamp, live_retained, save_png};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        CompositePresenter, Mode, PlanMode,
        presentation::{self, Plan, scratch::Scratch},
        streaming::Policy,
    },
    presentation_probe::Options,
    render::{FrameLayout, Viewport},
};
use engine_common::RenderFrame;
use glow::HasContext;
use std::{fs::File, io::Write, time::Duration};

const HISTORY: usize = 24;
const MODES: [PlanMode; 4] = [
    PlanMode::Reference,
    PlanMode::Reserved,
    PlanMode::Scratch,
    PlanMode::Prepared,
];
// Four blocks balance positions and within-block predecessors, not all driver
// history or temperature. Use four repeats for a complete cycle.
const ORDER: [[usize; 4]; 4] = [[0, 1, 3, 2], [1, 2, 0, 3], [2, 3, 1, 0], [3, 0, 2, 1]];

fn build(
    frames: &[RenderFrame],
    v: Viewport,
    layout: FrameLayout,
    mode: PlanMode,
    scratch: &mut Scratch,
) -> Result<Plan, Error> {
    Ok(presentation::build_reusing(
        frames,
        v,
        layout,
        1.,
        true,
        mode,
        if mode.reuses_storage() {
            scratch.take()
        } else {
            None
        },
    )?)
}

fn dispose(plan: Plan, mode: PlanMode, scratch: &mut Scratch) {
    if mode.reuses_storage() {
        scratch.recycle(plan);
    } else {
        drop(plan);
    }
}

fn presenter(context: &Context) -> Result<CompositePresenter, Error> {
    let mut r = CompositePresenter::new(context.gl.clone())?;
    r.set_stream_policy(Policy::PassBucketed);
    Ok(r)
}

struct Validation {
    renderers: Vec<CompositePresenter>,
    scratch: [Scratch; 4],
}

impl Validation {
    fn new(context: &Context) -> Result<Self, Error> {
        Ok(Self {
            renderers: MODES
                .iter()
                .map(|_| presenter(context))
                .collect::<Result<_, _>>()?,
            scratch: std::array::from_fn(|_| Scratch::default()),
        })
    }

    fn check(
        &mut self,
        frames: &[RenderFrame],
        v: Viewport,
        layout: FrameLayout,
    ) -> Result<Vec<u8>, Error> {
        let mut expected = None;
        let mut pixels = None;
        for ((mode, r), scratch) in MODES
            .into_iter()
            .zip(&mut self.renderers)
            .zip(&mut self.scratch)
        {
            let plan = build(frames, v, layout, mode, scratch)?;
            let out = r.draw_with_mode(&plan, Mode::RetainedCulled)?;
            let counts = (
                plan.shapes(),
                out.mesh.vertices,
                out.mesh.indices,
                out.mesh.draws,
                out.mesh.upload_bytes,
                out.mesh.retained.admissions,
            );
            let actual = r.readback()?;
            if let Some(expected) = &expected {
                if *expected != counts || pixels.as_ref() != Some(&actual) {
                    return Err(
                        format!("plan {} changed pixels or draw counts", mode.name()).into(),
                    );
                }
            } else {
                expected = Some(counts);
                pixels = Some(actual);
            }
            dispose(plan, mode, scratch);
            r.collect_retired();
        }
        if self
            .scratch
            .iter()
            .any(|s| s.discards != 0 || s.high_water_bytes > presentation::scratch::LIMIT_BYTES)
        {
            return Err("production plan unexpectedly exceeded scratch budget".into());
        }
        Ok(pixels.unwrap())
    }
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let metadata = serde_json::json!({"version":2,"kind":"plan-preparation-profile",
        "gl":context.identity,"width":width,"height":height,
        "frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "history_frames":HISTORY,"history":"24 consecutive production ticks, cyclic replay; one full cycle prewarmed per block",
        "timing":"plan construction plus disposal/recycling; completed mode also draws and glFinish; wall and thread CPU",
        "prepared":"scratch storage plus once-per-camera bounds and once-per-hint sin/cos; reference arithmetic order and checks unchanged",
        "simulation_source_text_ui_scanout_timed":false,"modes":MODES.map(|m|m.name()),
        "geometry":"retained-culled","streams":"bucketed","scratch_limit_bytes":presentation::scratch::LIMIT_BYTES,
        "validation":"exact pixels and draw counts across moving frames, native/odd/native resize, 44/640 camera heights and scenario replacements"});
    eprintln!("plan_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    println!(
        "fixture,path,mode,repeat,frame,shapes,wall_ms,cpu_ms,scratch_high_water_bytes,scratch_reuses,scratch_discards"
    );
    // Keep validation resources across scenario replacement and size changes.
    let mut validation = Validation::new(context)?;
    for fixture in live_retained::FIXTURES {
        let mut scenario = live_retained::scenario(v, fixture)?;
        let layout = scenario.frame_layout();
        let history: Vec<_> = (0..HISTORY)
            .map(|_| {
                scenario.step(&[], Duration::from_nanos(16_666_667));
                scenario.render_frames(RenderBackend::Raster, v)
            })
            .collect();
        if history.first().unwrap()[0].camera == history.last().unwrap()[0].camera
            && history.first().unwrap()[1].camera == history.last().unwrap()[1].camera
        {
            return Err(format!("{} history has no camera movement", fixture.name).into());
        }
        let heights: Vec<_> = history
            .iter()
            .map(|frames| {
                frames
                    .iter()
                    .take(2)
                    .map(|f| f.camera.height)
                    .collect::<Vec<_>>()
            })
            .collect();
        if let Some(path) = &options.geometry_output {
            File::create_new(path.join(format!("{}-cameras.json", fixture.name)))?
                .write_all(serde_json::to_string_pretty(&heights)?.as_bytes())?;
        }
        for (i, frames) in history.iter().enumerate() {
            let pixels = validation.check(frames, v, layout)?;
            if i == HISTORY - 1
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
        }
        // Explicit adapter stress: same production artwork, freshly projected
        // with abrupt zoom/size changes. These are validation-only, not timings.
        let mut zoomed = history.last().unwrap().clone();
        for viewport in [Viewport::new(641., 361.), v] {
            for height in [44., 640.] {
                for frame in &mut zoomed {
                    frame.camera.height = height;
                }
                validation.check(&zoomed, viewport, layout)?;
            }
        }
        eprintln!(
            "validated {}: 28 frames x 4 policies, exact pixels/counts",
            fixture.name
        );
        for completed in [false, true] {
            let path = if completed { "completed" } else { "cpu-plan" };
            for repeat in 0..options.presentation_repeats as usize {
                for index in ORDER[repeat % ORDER.len()] {
                    let mode = MODES[index];
                    let mut scratch = Scratch::default();
                    let mut r = completed.then(|| presenter(context)).transpose()?;
                    for frames in &history {
                        let plan = build(frames, v, layout, mode, &mut scratch)?;
                        if let Some(r) = &mut r {
                            r.draw_with_mode(&plan, Mode::RetainedCulled)?;
                        }
                        dispose(plan, mode, &mut scratch);
                    }
                    unsafe {
                        context.gl.finish();
                    }
                    let mut samples = Vec::new();
                    for i in 0..options.presentation_frames as usize {
                        let stamp = Stamp::now()?;
                        let plan = build(&history[i % HISTORY], v, layout, mode, &mut scratch)?;
                        let shapes = std::hint::black_box(&plan).shapes();
                        if let Some(r) = &mut r {
                            r.draw_with_mode(&plan, Mode::RetainedCulled)?;
                            unsafe {
                                context.gl.finish();
                            }
                        }
                        dispose(plan, mode, &mut scratch);
                        let (wall, cpu) = stamp.elapsed()?;
                        samples.push((
                            shapes,
                            wall,
                            cpu,
                            scratch.high_water_bytes,
                            scratch.reuses,
                            scratch.discards,
                        ));
                    }
                    // CSV I/O and final retained-capacity release are outside
                    // frame timing. Every block starts with new storage/history.
                    for (i, (shapes, wall, cpu, bytes, reuses, discards)) in
                        samples.into_iter().enumerate()
                    {
                        println!(
                            "{},{path},{},{repeat},{i},{shapes},{wall:.6},{cpu:.6},{bytes},{reuses},{discards}",
                            fixture.name,
                            mode.name()
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
