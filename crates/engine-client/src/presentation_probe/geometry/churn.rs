//! Admission stress using production terrain artwork and prescribed edit/visibility
//! history. Every frame is timed, including cold starts and world replacements.
use super::{Case, Context, Error, Producer, Stamp, Viewport, save_png};
use crate::{
    gpu_scene::{
        TrianglePresenter, plan,
        retained::{ADMISSION_WORK_LIMIT, Policy, Stats},
    },
    presentation_probe::{Options, geometry},
    render::FrameLayout,
};
use glow::HasContext;
use std::{fs::File, io::Write, time::Instant};

const CASES: [Case; 2] = [
    Case {
        name: "terrain-reveal-zoom",
        bodies: 4,
        contour: true,
        zoom: true,
        edits: false,
        visibility: true,
    },
    Case {
        name: "terrain-edit-replace",
        bodies: 2,
        contour: true,
        zoom: false,
        edits: true,
        visibility: true,
    },
];

pub(in crate::presentation_probe::geometry::gpu) struct StreamSource(Producer);

impl StreamSource {
    pub(in crate::presentation_probe::geometry::gpu) fn new(name: &str) -> Result<Self, Error> {
        let case = CASES
            .iter()
            .find(|c| c.name == name)
            .ok_or("unknown terrain history")?;
        Ok(Self(Producer::new(*case)?))
    }

    pub(in crate::presentation_probe::geometry::gpu) fn generate(
        &mut self,
        step: usize,
        viewport: Viewport,
    ) -> Result<plan::Scene, Error> {
        Ok(scene(&mut self.0, step, viewport, true)?.0)
    }
}
const MODES: [(&str, Option<Policy>); 12] = [
    ("triangles", None),
    ("retained-adaptive", Some(Policy::Adaptive)),
    ("retained-outlines", Some(Policy::HybridOutlines)),
    ("retained-paced", Some(Policy::HybridPaced)),
    ("retained-memoized", Some(Policy::HybridMemoized)),
    ("retained-scan", Some(Policy::HybridScan)),
    ("retained-ranges", Some(Policy::HybridRanges)),
    ("retained-culled", Some(Policy::HybridCulled)),
    ("retained-resident", Some(Policy::HybridResident)),
    (
        "retained-resident-passes",
        Some(Policy::HybridResidentPasses),
    ),
    (
        "retained-resident-buffers",
        Some(Policy::HybridResidentBuffers),
    ),
    (
        "retained-resident-uniforms",
        Some(Policy::HybridResidentUniforms),
    ),
];

fn scene(
    producer: &mut Producer,
    step: usize,
    v: Viewport,
    hinted: bool,
) -> Result<(plan::Scene, u64), Error> {
    let (frame, groups, rebuilt) = producer.generate(step, v, hinted)?;
    let mut scene = plan::build(&[frame], v, FrameLayout::EqualHorizontal, 1.)?;
    // Same explicit source/pose validation as the earlier terrain experiment.
    if !groups.iter().all(|p| p.matches(&scene)) {
        return Err("churn metadata disagrees with source".into());
    }
    scene.retained = groups;
    Ok((scene, rebuilt))
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let modes: Vec<_> = MODES
        .into_iter()
        .filter(|(_, p)| p.is_none_or(|p| p.supported(&context.gl)))
        .collect();
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let metadata = serde_json::json!({"version":2,"kind":"terrain-admission-churn","gl":context.identity,
        "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "modes":modes.iter().map(|m| m.0).collect::<Vec<_>>(),"cases":CASES.map(|c|c.name),"admission_work_limit":ADMISSION_WORK_LIMIT,
        "warmup_frames":0,"gpu_target_prewarmed":true,"in_flight":1,
        "timing":"source construction, validation, projection, mesh encoding through glFinish; all frames included",
        "edit_reconstruction_timed":false,"simulation_ui_scanout_timed":false,
        "phases":"16-frame phases: all, odd chunks, even chunks, all; edits/remove/remove/replacement; 64-frame cycle"});
    eprintln!("churn_probe={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    validate(context, options, v)?;
    println!(
        "fixture,mode,repeat,frames,render_wall_ms,render_cpu_ms,source_project_ms,encode_ms,submit_ms,wall_p95_ms,wall_max_ms,first_wall_ms,first_build_ms,startup_wall_total_ms,definition_builds,hits,admissions,promotions,evictions,fallbacks,entries,cache_bytes,build_ms,build_max_ms,build_work,deferred,oversized,upload_bytes,build_reuses"
    );
    for case in CASES {
        for repeat in 0..options.presentation_repeats {
            for offset in 0..modes.len() {
                let (name, policy) = modes[(repeat as usize + offset) % modes.len()];
                let mut producer = Producer::new(case)?;
                let mut renderer = TrianglePresenter::new(context.gl.clone())?;
                renderer.draw(&plan::Scene {
                    width,
                    height,
                    draws: vec![],
                    reuse: vec![],
                    retained: vec![],
                })?;
                unsafe {
                    context.gl.finish();
                }
                renderer.set_retention(policy);
                let mut walls = Vec::new();
                let mut sums = [0.; 5];
                let mut first = [0.; 2];
                let mut startup = 0.;
                let mut cache = Stats::default();
                let (mut builds, mut uploads) = (0, 0);
                let mut trace = options
                    .geometry_output
                    .as_ref()
                    .map(|path| {
                        File::create_new(
                            path.join(format!("{}-{name}-{repeat}-frames.csv", case.name)),
                        )
                    })
                    .transpose()?;
                if let Some(trace) = &mut trace {
                    writeln!(
                        trace,
                        "frame,phase,wall_ms,cpu_ms,build_ms,max_build_ms,work,admissions,deferred,oversized,definitions,entries,bytes,build_reuses"
                    )?;
                }
                for step in 0..options.presentation_frames as usize {
                    let started = Stamp::now()?;
                    let source = Instant::now();
                    let (scene, definitions) = scene(&mut producer, step, v, policy.is_some())?;
                    let source_ms = source.elapsed().as_secs_f64() * 1000.;
                    let output = renderer.draw(&scene)?;
                    unsafe {
                        context.gl.finish();
                    }
                    drop(scene);
                    let (wall, cpu) = started.elapsed()?;
                    renderer.collect_retired();
                    let c = output.mesh.retained;
                    if c.entries > 128 || c.bytes > 16 * 1024 * 1024 {
                        return Err("churn cache exceeded resident limits".into());
                    }
                    if step == 0 {
                        first = [wall, c.build_ns as f64 / 1e6];
                    }
                    if step < 8 {
                        startup += wall;
                    }
                    walls.push(wall);
                    for (sum, value) in sums.iter_mut().zip([
                        wall,
                        cpu,
                        source_ms,
                        output.encode.as_secs_f64() * 1000.,
                        output.submit.as_secs_f64() * 1000.,
                    ]) {
                        *sum += value;
                    }
                    builds += definitions;
                    uploads += output.mesh.upload_bytes;
                    cache.accumulate(c);
                    if let Some(trace) = &mut trace {
                        writeln!(
                            trace,
                            "{step},{},{wall:.6},{cpu:.6},{:.6},{:.6},{},{},{},{},{definitions},{},{},{}",
                            step / 16 % 4,
                            c.build_ns as f64 / 1e6,
                            c.max_build_ns as f64 / 1e6,
                            c.build_work,
                            c.admissions,
                            c.deferred,
                            c.oversized,
                            c.entries,
                            c.bytes,
                            c.build_reuses
                        )?;
                    }
                }
                let n = f64::from(options.presentation_frames);
                println!(
                    "{},{name},{repeat},{},{},{:.6},{:.6},{:.6},{:.6},{startup:.6},{builds},{},{},{},{},{},{},{},{:.6},{:.6},{},{},{},{:.3},{}",
                    case.name,
                    options.presentation_frames,
                    sums.map(|s| format!("{:.6}", s / n)).join(","),
                    geometry::percentile(&walls, 95),
                    geometry::percentile(&walls, 100),
                    first[0],
                    first[1],
                    cache.hits,
                    cache.admissions,
                    cache.promotions,
                    cache.evictions,
                    cache.fallbacks,
                    cache.entries,
                    cache.bytes,
                    cache.build_ns as f64 / 1e6 / n,
                    cache.max_build_ns as f64 / 1e6,
                    cache.build_work,
                    cache.deferred,
                    cache.oversized,
                    uploads as f64 / n,
                    cache.build_reuses
                );
            }
        }
    }
    Ok(())
}

fn validate(context: &Context, options: &Options, v: Viewport) -> Result<(), Error> {
    let mut checked = 0;
    let mut worst = (0.0_f64, 0.0_f64);
    let mut paced = Stats::default();
    for case in CASES {
        let mut producer = Producer::new(case)?;
        let mut reference = TrianglePresenter::new(context.gl.clone())?;
        let mut cached = MODES[1..]
            .iter()
            .filter(|(_, p)| p.is_none_or(|p| p.supported(&context.gl)))
            .map(|&(name, policy)| {
                let mut renderer = TrianglePresenter::new(context.gl.clone())?;
                renderer.set_retention(policy);
                Ok((name, renderer))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut previous_rgb = None;
        let mut changed_boundaries = 0;
        for step in 0..80 {
            let current_v = if (64..72).contains(&step) {
                Viewport::new(641., 361.)
            } else {
                v
            };
            let (scene, _) = scene(&mut producer, step, current_v, true)?;
            reference.draw(&scene)?;
            let expected = reference.readback()?;
            let rgb: Vec<_> = expected
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            if step > 0 && step % 16 == 0 && previous_rgb.as_ref() != Some(&rgb) {
                changed_boundaries += 1;
            }
            let mut ranges_pixels = None;
            for (name, renderer) in &mut cached {
                let output = renderer.draw(&scene)?;
                let actual = renderer.readback()?;
                if *name == "retained-culled" && ranges_pixels.as_ref() != Some(&actual) {
                    return Err(
                        format!("culling changed churn pixels: {} step={step}", case.name).into(),
                    );
                }
                if *name == "retained-ranges" {
                    ranges_pixels = Some(actual.clone());
                }
                let error = geometry::difference(&rgb, &actual)?;
                worst.0 = worst.0.max(error.0);
                worst.1 = worst.1.max(error.1);
                let failed = error.0 > 0.05 || error.1 > 0.001;
                if (failed || [0, 1, 15, 16, 31, 32, 47, 48, 63, 64, 71, 72, 79].contains(&step))
                    && let Some(path) = &options.geometry_output
                {
                    for (label, data) in [("actual", &actual), ("reference", &expected)] {
                        save_png(
                            &path.join(format!("{}-{step}-{name}-{label}.png", case.name)),
                            current_v.width as u32,
                            current_v.height as u32,
                            png::ColorType::Rgba,
                            data,
                        )?;
                    }
                }
                if failed {
                    return Err(format!(
                        "churn parity {} {name} step={step}: {error:?}",
                        case.name
                    )
                    .into());
                }
                if *name == "retained-paced" {
                    paced.accumulate(output.mesh.retained);
                }
                if output.mesh.retained.entries > 128
                    || output.mesh.retained.bytes > 16 * 1024 * 1024
                {
                    return Err("churn validation cache limit".into());
                }
                renderer.collect_retired();
                checked += 1;
            }
            reference.collect_retired();
            previous_rgb = Some(rgb);
        }
        if changed_boundaries != 4 {
            return Err("churn fixture did not visibly change at every boundary".into());
        }
    }
    if paced.deferred == 0
        || paced.admissions == 0
        || paced.hits == 0
        || paced.promotions == 0
        || paced.evictions == 0
    {
        return Err("churn fixture missed admission/reuse/defer/promotion/eviction paths".into());
    }
    eprintln!(
        "validated churn frames={checked} max_mae={:.8} max_different={:.8} paced_deferred={} admissions={} promotions={} evictions={}",
        worst.0, worst.1, paced.deferred, paced.admissions, paced.promotions, paced.evictions
    );
    Ok(())
}
