//! Complete encoder attribution on the same moving history as the stroke probe.
//! Nested clocks are diagnostics, not a replacement for uninstrumented controls.
use super::{
    Context, Control, Error, HISTORY, PlanMode, RenderBackend, Stamp, Validation, Viewport,
    decisions, encode, live_retained, modes, presentation,
};
use crate::{
    gpu_scene::{
        encode_profile::EncodeProfile,
        mesh::MeshBuilder,
        presentation::Plan,
        retained::{Cache, Policy},
        stroke_single_pass::Stats as Attempts,
    },
    presentation_probe::Options,
};
use std::{fs::File, io::Write, time::Duration};

fn configure(mesh: &mut MeshBuilder, mode: Control) {
    mesh.set_stroke_writer(mode.writer);
    mesh.stroke_culling = mode.culling;
    mesh.set_round_join_mode(mode.joins);
}

fn counts(s: Attempts) -> [u64; 7] {
    [
        s.cached,
        s.prepared,
        s.refused[0],
        s.refused[1],
        s.refused[2],
        s.refused[3],
        s.refused[4],
    ]
}

fn validate_cpu(
    plan: &Plan,
    states: &mut [(Cache, MeshBuilder); 4],
    controls: [Control; 2],
) -> Result<(), Error> {
    for (i, (cache, mesh)) in states.iter_mut().enumerate() {
        cache.begin_frame();
        configure(mesh, controls[i / 2]);
    }
    for pass in &plan.passes {
        let scene = pass.scene();
        let mut attempts = [Attempts::default(); 4];
        for (i, (cache, mesh)) in states.iter_mut().enumerate() {
            let before = mesh.single_pass_stats();
            let p = if i % 2 == 1 {
                cache.rebuild_profiled_clipped(
                    scene,
                    &scene.retained,
                    mesh,
                    Policy::HybridCulled,
                    pass.clip(),
                )?
            } else {
                cache.rebuild_clipped(
                    scene,
                    &scene.retained,
                    mesh,
                    Policy::HybridCulled,
                    pass.clip(),
                )?;
                EncodeProfile::default()
            };
            attempts[i] = mesh.single_pass_stats().delta(before);
            if !attempts[i].bounded()
                || cache.stats.entries > 128
                || cache.stats.bytes > 16 * 1024 * 1024
                || p.ordinary_topology
                    .iter()
                    .zip(counts(attempts[i]))
                    .any(|(a, b)| *a > b)
            {
                return Err(
                    "encoder attribution exceeded budget or ordinary-attempt totals".into(),
                );
            }
        }
        for i in 1..4 {
            super::super::same_mesh(&states[0].1, &states[i].1)?;
            if decisions(states[0].0.stats) != decisions(states[i].0.stats) {
                return Err("encoder instrumentation changed cache decisions".into());
            }
        }
        if attempts[0] != attempts[1] || attempts[2] != attempts[3] {
            return Err("encoder instrumentation changed adapter attempts".into());
        }
    }
    Ok(())
}

const STAGES: [&str; 18] = [
    "resolve",
    "exact_expand",
    "scalable_expand",
    "fallback_fill",
    "fallback_stroke",
    "ordinary",
    "validate",
    "stroke_visibility",
    "guard_prepare",
    "guard_fill",
    "guard_stroke",
    "guard_finish",
    "ordinary_prepare",
    "ordinary_fill",
    "ordinary_stroke",
    "ordinary_finish",
    "offscreen_stroke",
    "unattributed",
];

fn stages(p: &EncodeProfile, wall_ms: f64) -> [f64; 18] {
    let g = &p.guarded_stroke;
    let o = &p.ordinary_geometry;
    let mut out = [0.; 18];
    for (out, duration) in out.iter_mut().zip([
        p.resolve,
        p.exact_expand,
        p.scalable_expand,
        p.fallback_fill,
        p.fallback_stroke,
        p.ordinary,
        p.validate,
        p.stroke_visibility,
        g.prepare,
        g.fill,
        g.stroke,
        g.finish,
        o.prepare,
        o.fill,
        o.stroke,
        o.finish,
        p.offscreen_stroke,
    ]) {
        *out = duration.as_secs_f64() * 1000.;
    }
    // Only the first eight spans are disjoint. Nested draw/offscreen spans must
    // not be added again. Residual includes traversal, clocks and unlabelled work.
    out[17] = wall_ms - out[..8].iter().sum::<f64>();
    out
}

fn measure(
    plans: &[Plan],
    controls: [Control; 2],
    options: &Options,
    fixture: &str,
) -> Result<(), Error> {
    for repeat in 0..options.presentation_repeats as usize {
        for index in super::super::ORDER[repeat % super::super::ORDER.len()] {
            let mode = controls[index / 2];
            let detailed = index % 2 == 1;
            let (mut cache, mut mesh) = (Cache::default(), MeshBuilder::default());
            for plan in plans {
                encode(plan, &mut cache, &mut mesh, mode, detailed)?;
            }
            let mut rows = Vec::with_capacity(options.presentation_frames as usize);
            for frame in 0..options.presentation_frames as usize {
                let before = mesh.single_pass_stats();
                let stamp = Stamp::now()?;
                let (p, vertices, indices, retained) = encode(
                    &plans[frame % plans.len()],
                    &mut cache,
                    &mut mesh,
                    mode,
                    detailed,
                )?;
                let (wall, cpu) = stamp.elapsed()?;
                let attempts = mesh.single_pass_stats().delta(before);
                if !attempts.bounded()
                    || retained.entries > 128
                    || retained.bytes > 16 * 1024 * 1024
                {
                    return Err("encoder timing exceeded cache budget".into());
                }
                rows.push((wall, cpu, p, vertices, indices, retained, attempts));
            }
            for (frame, (wall, cpu, p, v, ind, retained, a)) in rows.into_iter().enumerate() {
                // Uninstrumented rows have zero diagnostic fields, not an
                // invented attribution of their whole time to the residual.
                let times = if detailed { stages(&p, wall) } else { [0.; 18] }
                    .map(|t| format!("{t:.6}"))
                    .join(",");
                let attempts = counts(a).map(|n| n.to_string()).join(",");
                let ordinary = p.ordinary_topology.map(|n| n.to_string()).join(",");
                let geometry = [
                    p.exact_vertices,
                    p.scalable_vertices,
                    p.fallback_fills,
                    p.fallback_strokes,
                    p.ordinary_draws,
                    p.guarded_stroke.points,
                    p.guarded_stroke.vertices,
                    p.guarded_stroke.indices,
                    p.ordinary_geometry.points,
                    p.ordinary_geometry.vertices,
                    p.ordinary_geometry.indices,
                ]
                .map(|n| n.to_string())
                .join(",");
                println!(
                    "{fixture},{},{},{repeat},{frame},{wall:.6},{cpu:.6},{v},{ind},{},{},{},{},{times},{geometry},{attempts},{ordinary},{},{},{},{},{}",
                    if detailed { "profiled" } else { "plain" },
                    mode.name,
                    v * 24 + ind * 4,
                    retained.admissions,
                    retained.entries,
                    retained.bytes,
                    a.layouts.hits,
                    a.layouts.misses,
                    a.layouts.evictions,
                    a.layouts.entries,
                    a.layouts.bytes
                );
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
    let viewport = Viewport::new(width as f32, height as f32);
    let controls = modes(true, true, true, false, false, false, true);
    let metadata = serde_json::json!({
        "version":1,"kind":"single-pass-encoder-profile","gl":context.identity,
        "width":width,"height":height,"history_frames":HISTORY,"warmup_frames":HISTORY,
        "frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "modes":controls.map(|c|c.name),"order":super::super::ORDER,
        "fixtures":live_retained::FIXTURES.iter().map(|f|f.name).collect::<Vec<_>>(),
        "timing":"complete CPU geometry encoding only; simulation, source, projection, GL, UI and scanout excluded",
        "instrumentation":"plain paths compile out nested clocks/counter reads; profiled spans use elapsed wall time, not per-stage thread CPU; profile/plain comparison measures perturbation",
        "spans":"first eight stages disjoint; guarded and ordinary draw spans and offscreen time nested; residual includes traversal, clocks and unlabelled work",
        "topology_order":["cached","prepared","unsupported","merged","arc","nonfinite","budget"],
        "ordinary_topology":"subset of total adapter attempts; excludes guarded terrain strokes",
        "validation":"per-pass plain/profiled and static/single-pass exact mesh bits, indices, batches, cache decisions and adapter counts; existing checked writer and GPU pixels on moving/resize/zoom histories"
    });
    eprintln!("stroke_encoder_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let stages = STAGES.map(|s| format!("{s}_ms")).join(",");
    println!(
        "fixture,path,mode,repeat,frame,wall_ms,cpu_ms,vertices,indices,upload_bytes,admissions,entries,cache_bytes,{stages},exact_vertices,scalable_vertices,fallback_fills,fallback_strokes,ordinary_draws,guard_points,guard_vertices,guard_indices,ordinary_points,ordinary_vertices,ordinary_indices,cached,prepared,unsupported,merged,arc,nonfinite,budget,ordinary_cached,ordinary_prepared,ordinary_unsupported,ordinary_merged,ordinary_arc,ordinary_nonfinite,ordinary_budget,layout_hits,layout_misses,layout_evictions,layout_entries,layout_bytes"
    );
    let mut validation = Validation::new(context, options)?;
    let mut checked = 0;
    for fixture in live_retained::FIXTURES {
        validation.reset();
        let mut states = std::array::from_fn(|_| (Cache::default(), MeshBuilder::default()));
        let mut scenario = live_retained::scenario(viewport, fixture)?;
        let layout = scenario.frame_layout();
        let history: Vec<_> = (0..HISTORY)
            .map(|_| {
                scenario.step(&[], Duration::from_nanos(16_666_667));
                scenario.render_frames(RenderBackend::Raster, viewport)
            })
            .collect();
        let build = |frames: &[_], v| {
            presentation::build_reusing(frames, v, layout, 1., true, PlanMode::Prepared, None)
        };
        let plans = history
            .iter()
            .map(|f| build(f, viewport))
            .collect::<Result<Vec<_>, _>>()?;
        for (i, plan) in plans.iter().enumerate() {
            validate_cpu(plan, &mut states, controls)?;
            validation.check(plan, &format!("{}-{i:03}", fixture.name), i == HISTORY - 1)?;
            checked += 1;
        }
        let mut changed = history.last().unwrap().clone();
        for v in [Viewport::new(641., 361.), viewport] {
            for height in [44., 640.] {
                for frame in &mut changed {
                    frame.camera.height = height;
                }
                let plan = build(&changed, v)?;
                validate_cpu(&plan, &mut states, controls)?;
                validation.check(
                    &plan,
                    &format!("{}-resize-{}-zoom-{height}", fixture.name, v.width),
                    true,
                )?;
                checked += 1;
            }
        }
        measure(&plans, controls, options, fixture.name)?;
    }
    eprintln!(
        "encoder_validated_states={checked} exact_gpu_frames={} single_pass_applied={}",
        validation.quality.frames, validation.single_applied
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn encoder_profile_requires_complete_single_pass_controls_and_rejects_other_fixtures() {
        let flags = [
            "test",
            "--benchmark-presentation",
            "--presentation-geometry",
            "--geometry-stroke-profile",
            "--geometry-round-joins",
            "--geometry-stroke-bulk",
            "--geometry-stroke-static",
            "--geometry-stroke-single-pass",
            "--geometry-stroke-encoder-profile",
        ];
        assert!(crate::Args::try_parse_from(flags).is_ok());
        assert!(
            crate::Args::try_parse_from(["test", "--geometry-stroke-encoder-profile"]).is_err()
        );
        assert!(
            crate::Args::try_parse_from(
                flags
                    .into_iter()
                    .chain(["--geometry-stroke-cases", "clock-idle"])
            )
            .is_err()
        );
    }

    #[test]
    fn residual_excludes_nested_spans_and_keeps_clock_overhead_visible() {
        let mut p = EncodeProfile::default();
        p.fallback_stroke = Duration::from_millis(3);
        p.ordinary = Duration::from_millis(2);
        p.guarded_stroke.stroke = Duration::from_millis(2);
        p.ordinary_geometry.fill = Duration::from_millis(1);
        p.offscreen_stroke = Duration::from_millis(1);
        let s = stages(&p, 8.);
        assert_eq!(s[17], 3.);
        assert_eq!(s[10], 2.);
        assert_eq!(s[13], 1.);
        let mut total = EncodeProfile::default();
        p.ordinary_topology = [1, 2, 3, 4, 5, 6, 7];
        total.accumulate(p);
        assert_eq!(total.ordinary_topology, [1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(total.ordinary_geometry.fill, Duration::from_millis(1));
    }
}
