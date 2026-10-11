//! Diagnostic-only stage clocks on a bounded corpus of real guarded outlines.
//! These are nested wall clocks, not production CPU percentages. Capture,
//! validation and empty-hook calibration are outside throughput samples.
use super::{Context, Control, Error, HISTORY, Stamp, encode, live_retained};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        PlanMode,
        mesh::MeshBuilder,
        presentation::{self, Plan},
        retained::Cache,
        stroke_census::{
            Census,
            capture::{Capture, Outline},
        },
        stroke_culling::Control as Culling,
        stroke_output::{Bulk, StrokeWriter},
    },
    presentation_probe::Options,
    render::Viewport,
};
use lyon_tessellation::{
    ExactStrokeTessellator, LineCap, LineJoin, NoStrokeProfile, RoundJoinMode, StrokeOptions,
    StrokeProfiler, StrokeStage, VertexBuffers, path::Polygon,
};
use std::{
    fs::File,
    hint::black_box,
    io::Write,
    time::{Duration, Instant},
};

const STAGES: [StrokeStage; 5] = [
    StrokeStage::JoinGeometry,
    StrokeStage::BaseVertices,
    StrokeStage::EdgeTriangles,
    StrokeStage::JoinExtras,
    StrokeStage::FlattenedStep,
];
const NAMES: [&str; 10] = [
    "plain",
    "noop",
    "counts",
    "join-geometry",
    "base-vertices",
    "edge-triangles",
    "join-extras",
    "all-stages",
    "prepared-guard",
    "prepared-guard-checked",
];
const CONTROL: Control = Control {
    name: "bulk-static-exact",
    writer: StrokeWriter::BulkStatic,
    culling: Culling::Conservative,
    small: false,
    joins: RoundJoinMode::Exact,
};

#[derive(Default, Clone, Copy, Debug)]
struct Totals {
    calls: [u64; 5],
    elapsed: [Duration; 5],
}
#[derive(Default)]
struct Profile<const MASK: u8>(Totals);
impl<const MASK: u8> StrokeProfiler for Profile<MASK> {
    #[inline(always)]
    fn measure<R>(&mut self, stage: StrokeStage, work: impl FnOnce() -> R) -> R {
        let index = stage as usize;
        let stamp = (MASK & (1 << index) != 0).then(Instant::now);
        let result = work();
        // Stop the clock before counting/accumulation. The clock itself still
        // contributes overhead and changes code layout; report controls too.
        let duration = stamp.map(|s| s.elapsed()).unwrap_or_default();
        self.0.calls[index] += 1;
        self.0.elapsed[index] += duration;
        result
    }
}

type Buffers = VertexBuffers<[f32; 6], u32>;

fn replay<P: StrokeProfiler, const MODE: u8>(
    outlines: &[Outline],
    tess: &mut ExactStrokeTessellator,
    buffers: &mut Buffers,
    profile: &mut P,
    check: bool,
) -> Result<(), Error> {
    buffers.vertices.clear();
    buffers.indices.clear();
    for outline in outlines {
        let base = buffers.vertices.len();
        let index_base = buffers.indices.len();
        let polygon = Polygon {
            points: &outline.points,
            closed: true,
        };
        let options = StrokeOptions::default()
            .with_tolerance(0.25)
            .with_line_width(outline.stroke.width)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let mut output = Bulk::new(buffers, outline.stroke.color);
        let result = match MODE {
            0 => tess.tessellate_static(polygon.path_events(), &options, &mut output),
            1 => tess.tessellate_static_profiled(
                polygon.path_events(),
                &options,
                &mut output,
                profile,
            ),
            2 => tess.tessellate_static_invariants::<false, _>(
                polygon.path_events(),
                &options,
                &mut output,
            ),
            3 => tess.tessellate_static_invariants::<true, _>(
                polygon.path_events(),
                &options,
                &mut output,
            ),
            _ => unreachable!(),
        };
        result.map_err(|e| format!("attribution stroke: {e:?}"))?;
        if check
            && (buffers.vertices[base..]
                .iter()
                .map(|v| v.map(f32::to_bits))
                .ne(outline.vertices.iter().map(|v| v.map(f32::to_bits)))
                || buffers.indices[index_base..]
                    .iter()
                    .map(|&i| i - base as u32)
                    .ne(outline.indices.iter().copied()))
        {
            return Err("profiled stroke changed captured vertex bits or index order".into());
        }
    }
    black_box(&buffers);
    Ok(())
}

fn selected<const MASK: u8>(
    outlines: &[Outline],
    tess: &mut ExactStrokeTessellator,
    buffers: &mut Buffers,
    check: bool,
) -> Result<Totals, Error> {
    let mut profile = Profile::<MASK>::default();
    replay::<_, 1>(outlines, tess, buffers, &mut profile, check)?;
    Ok(profile.0)
}

fn sample(
    mode: usize,
    outlines: &[Outline],
    tess: &mut ExactStrokeTessellator,
    buffers: &mut Buffers,
    check: bool,
) -> Result<Totals, Error> {
    match mode {
        0 | 1 => {
            if mode == 0 {
                replay::<_, 0>(outlines, tess, buffers, &mut NoStrokeProfile, check)?;
            } else {
                replay::<_, 1>(outlines, tess, buffers, &mut NoStrokeProfile, check)?;
            }
            Ok(Totals::default())
        }
        2 => selected::<0>(outlines, tess, buffers, check),
        3 => selected::<1>(outlines, tess, buffers, check),
        4 => selected::<2>(outlines, tess, buffers, check),
        5 => selected::<4>(outlines, tess, buffers, check),
        6 => selected::<8>(outlines, tess, buffers, check),
        7 => selected::<31>(outlines, tess, buffers, check),
        8 | 9 => {
            if mode == 8 {
                replay::<_, 2>(outlines, tess, buffers, &mut NoStrokeProfile, check)?;
            } else {
                replay::<_, 3>(outlines, tess, buffers, &mut NoStrokeProfile, check)?;
            }
            Ok(Totals::default())
        }
        _ => unreachable!(),
    }
}

// Count reuse opportunities outside every throughput/region timing. The
// fixed-width options are identical at every round join within one outline.
fn invariant_census(captures: &[Capture]) -> Result<serde_json::Value, Error> {
    let (mut tess, mut buffers) = (ExactStrokeTessellator::new(), Buffers::new());
    let mut joins = [0_u64; 5];
    let mut fallbacks = [0_u64; 5];
    let mut repeated_guards = 0;
    let mut repeated_steps = 0;
    for outline in captures.iter().flat_map(|c| &c.outlines) {
        let before = tess.round_join_stats();
        replay::<_, 0>(
            std::slice::from_ref(outline),
            &mut tess,
            &mut buffers,
            &mut NoStrokeProfile,
            true,
        )?;
        let after = tess.round_join_stats();
        let fallback = after.fallbacks - before.fallbacks;
        let count = after.shortcuts - before.shortcuts + fallback;
        joins[count.min(4) as usize] += 1;
        fallbacks[fallback.min(4) as usize] += 1;
        repeated_guards += count.saturating_sub(1);
        repeated_steps += fallback.saturating_sub(1);
    }
    let totals = tess.round_join_stats();
    Ok(
        serde_json::json!({"histogram_buckets":["0","1","2","3","4+"],
        "outlines_by_round_joins":joins,"outlines_by_fallbacks":fallbacks,
        "round_joins":totals.shortcuts+totals.fallbacks,"fallbacks":totals.fallbacks,
        "repeated_fixed_width_guards":repeated_guards,"repeated_flattening_steps":repeated_steps}),
    )
}

fn capture(plans: &[Plan]) -> Result<Vec<Capture>, Error> {
    let (mut cache, mut mesh) = (Cache::default(), MeshBuilder::default());
    for plan in plans {
        encode(plan, &mut cache, &mut mesh, CONTROL, false)?;
    }
    let mut captures = Vec::new();
    let mut bytes = 0;
    for plan in plans {
        mesh.stroke_census = Some(Box::new(Census::default()));
        mesh.stroke_census.as_mut().unwrap().capture = Some(Capture::default());
        let (profile, _, _, _) = encode(plan, &mut cache, &mut mesh, CONTROL, true)?;
        let census = mesh.stroke_census.take().unwrap();
        let c = census.capture.unwrap();
        if let Some(error) = c.error {
            return Err(error.into());
        }
        if c.outlines.len() as u64 != profile.fallback_strokes
            || c.outlines
                .iter()
                .map(|o| o.vertices.len() as u64)
                .sum::<u64>()
                != profile.guarded_stroke.vertices
            || c.outlines
                .iter()
                .map(|o| o.indices.len() as u64)
                .sum::<u64>()
                != profile.guarded_stroke.indices
        {
            return Err("capture omitted a guarded stroke or changed its output counts".into());
        }
        bytes += c.bytes;
        if bytes > 128 * 1024 * 1024 {
            return Err("stroke history exceeds 128 MiB budget".into());
        }
        captures.push(c);
    }
    Ok(captures)
}

#[cfg(test)]
#[path = "stroke_reuse_audit.rs"]
mod reuse_audit;

#[path = "stroke_topology.rs"]
mod topology;

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if options.geometry_stroke_topology {
        return topology::run(context, options, width, height);
    }
    let viewport = Viewport::new(width as f32, height as f32);
    let modes: Vec<usize> = if options.geometry_stroke_invariants {
        vec![0, 8, 9]
    } else {
        (0..NAMES.len()).collect()
    };
    let mut metadata = serde_json::json!({"kind":"stroke-attribution", "version":2,
        "gl":context.identity,"width":width,"height":height,"history_frames":HISTORY,
        "frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "controls":modes.iter().map(|&i|NAMES[i]).collect::<Vec<_>>(),"invariants_only":options.geometry_stroke_invariants,"fixtures":[],"capture":"all actual guarded polygon strokes after conservative rejection; 8192 draws / 8 MiB per frame, 128 MiB per history; overflow fails, never truncates",
        "scope":"warm isolated fixed-width bulk-static/exact tessellation; real moving projected inputs; no renderer/projection/cache/culling/GPU/UI/simulation in throughput samples",
        "stage_timing":"nested wall clocks, inclusive of clock read overhead; pure join geometry; base vertex construction plus output writes; edge triangles; mixed join extras; flattened steps; remaining outer time includes setup/merge/close plus hook overhead",
        "warning":"single-stage and all-stage clocks perturb execution; not percentages of uninstrumented frame CPU; empty-hook floor is diagnostic, not a subtractable correction",
        "order":"rotate controls each repeat, reverse alternate repeats; exact vertex bits and rebased indices checked outside timing"});
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let mut calibration = Vec::new();
    for _ in 0..if options.geometry_stroke_invariants {
        0
    } else {
        8
    } {
        let mut profile = Profile::<31>::default();
        for _ in 0..10_000 {
            for stage in STAGES {
                profile.measure(stage, || black_box(()));
            }
        }
        calibration.push(profile.0.elapsed.iter().sum::<Duration>().as_secs_f64() * 1e9 / 50_000.);
    }
    metadata["empty_hook_ns"] = serde_json::json!(calibration);
    println!(
        "fixture,mode,repeat,frame,wall_ms,cpu_ms,strokes,vertices,indices,geometry_calls,base_calls,edge_calls,extra_calls,flat_calls,geometry_ms,base_ms,edge_ms,extra_ms,flat_ms"
    );
    for fixture in live_retained::FIXTURES {
        let mut scenario = live_retained::scenario(viewport, fixture)?;
        let layout = scenario.frame_layout();
        let mut plans = Vec::new();
        for _ in 0..HISTORY {
            scenario.step(&[], Duration::from_nanos(16_666_667));
            plans.push(presentation::build_reusing(
                &scenario.render_frames(RenderBackend::Raster, viewport),
                viewport,
                layout,
                1.,
                true,
                PlanMode::Prepared,
                None,
            )?);
        }
        let captures = capture(&plans)?;
        let (mut tess, mut buffers) = (ExactStrokeTessellator::new(), Buffers::new());
        let mut calls = Vec::new();
        for c in &captures {
            let before = tess.round_join_stats();
            let reference = sample(2, &c.outlines, &mut tess, &mut buffers, true)?;
            let after = tess.round_join_stats();
            let expected = (
                after.shortcuts - before.shortcuts,
                after.fallbacks - before.fallbacks,
            );
            for mode in 0..NAMES.len() {
                let before = tess.round_join_stats();
                let totals = sample(mode, &c.outlines, &mut tess, &mut buffers, true)?;
                let after = tess.round_join_stats();
                if expected
                    != (
                        after.shortcuts - before.shortcuts,
                        after.fallbacks - before.fallbacks,
                    )
                {
                    return Err("stroke join decisions differ".into());
                }
                if (2..8).contains(&mode) && totals.calls != reference.calls {
                    return Err("stroke stage counts differ".into());
                }
            }
            calls.push(reference.calls);
        }
        let row = serde_json::json!({"name":fixture.name, "capture_bytes":captures.iter().map(|c|c.bytes).sum::<usize>(),
            "strokes":captures.iter().map(|c|c.outlines.len()).collect::<Vec<_>>(),
            "stage_calls":calls,"invariants":invariant_census(&captures)?,"checked":"every captured stroke x all ten controls, exact vertex bits and indices"});
        eprintln!("attribution_fixture={row}");
        metadata["fixtures"].as_array_mut().unwrap().push(row);
        for repeat in 0..options.presentation_repeats as usize {
            let mut order: Vec<_> = (0..modes.len())
                .map(|i| modes[(i + repeat) % modes.len()])
                .collect();
            if repeat % 2 != 0 {
                order.reverse();
            }
            for mode in order {
                // Warm each code path and reused output allocation before timing.
                for c in &captures {
                    sample(mode, &c.outlines, &mut tess, &mut buffers, false)?;
                }
                let mut rows = Vec::with_capacity(options.presentation_frames as usize);
                for frame in 0..options.presentation_frames as usize {
                    let c = &captures[frame % captures.len()];
                    let stamp = Stamp::now()?;
                    let totals = sample(mode, &c.outlines, &mut tess, &mut buffers, false)?;
                    let (wall, cpu) = stamp.elapsed()?;
                    if (2..8).contains(&mode) && totals.calls != calls[frame % captures.len()] {
                        return Err("timed call counts changed".into());
                    }
                    rows.push((
                        wall,
                        cpu,
                        totals,
                        c.outlines.len(),
                        buffers.vertices.len(),
                        buffers.indices.len(),
                    ));
                }
                // Keep stdout formatting/IO out of the block, not just timers.
                for (frame, (wall, cpu, totals, strokes, vertices, indices)) in
                    rows.into_iter().enumerate()
                {
                    let ns = totals.elapsed.map(|d| d.as_secs_f64() * 1000.);
                    println!(
                        "{},{},{repeat},{frame},{wall:.6},{cpu:.6},{},{},{},{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6}",
                        fixture.name,
                        NAMES[mode],
                        strokes,
                        vertices,
                        indices,
                        totals.calls[0],
                        totals.calls[1],
                        totals.calls[2],
                        totals.calls[3],
                        totals.calls[4],
                        ns[0],
                        ns[1],
                        ns[2],
                        ns[3],
                        ns[4]
                    );
                }
            }
        }
    }
    eprintln!("stroke_attribution={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn census_counts_per_outline_reuse_and_preparation_tracks_changed_widths() {
        use engine_common::{RenderColor, Stroke};
        use lyon_tessellation::math::point;
        let (mut tess, mut buffers) = (ExactStrokeTessellator::new(), Buffers::new());
        let mut capture = Capture::default();
        for width in [0.75, 4., 0.75] {
            let mut outline = Outline {
                points: vec![
                    point(0., 0.),
                    point(20., 0.),
                    point(20., 20.),
                    point(0., 20.),
                ],
                stroke: Stroke::new(RenderColor::WHITE, width),
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            sample(
                0,
                std::slice::from_ref(&outline),
                &mut tess,
                &mut buffers,
                false,
            )
            .unwrap();
            outline.vertices.clone_from(&buffers.vertices);
            outline.indices.clone_from(&buffers.indices);
            capture.outlines.push(outline);
        }
        for mode in [0, 8, 9] {
            sample(mode, &capture.outlines, &mut tess, &mut buffers, true).unwrap();
        }
        let census = invariant_census(&[capture]).unwrap();
        assert_eq!(
            census["outlines_by_round_joins"],
            serde_json::json!([0, 0, 0, 0, 3])
        );
        assert_eq!(
            census["outlines_by_fallbacks"],
            serde_json::json!([2, 0, 0, 0, 1])
        );
        assert_eq!(census["round_joins"], 12);
        assert_eq!(census["fallbacks"], 4);
        assert_eq!(census["repeated_fixed_width_guards"], 9);
        assert_eq!(census["repeated_flattening_steps"], 3);
    }

    #[test]
    fn timing_selection_counts_untimed_regions_and_preserves_error_results() {
        let mut p = Profile::<1>::default();
        for stage in STAGES {
            let result: Result<(), &str> = p.measure(stage, || Err("output full"));
            assert_eq!(result, Err("output full"));
        }
        assert_eq!(p.0.calls, [1; 5]);
        assert_eq!(p.0.elapsed[1..], [Duration::ZERO; 4]);
        assert_eq!(size_of::<NoStrokeProfile>(), 0);
    }
}
