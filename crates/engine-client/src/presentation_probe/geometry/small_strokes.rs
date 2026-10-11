//! Matched outline controls, including bounded single-pass polygon output.
//! Image/vertex validation stays outside uninstrumented throughput samples.
use super::{
    Context, Control, Error, HISTORY, Stamp, decisions, encode, live_retained, presenter, save_png,
};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        CompositePresenter, Mode, PlanMode,
        mesh::MeshBuilder,
        presentation::{self, Clip, Pass, Plan},
        retained::Cache,
        stroke_culling::Control as Culling,
        stroke_output::StrokeWriter,
    },
    presentation_probe::Options,
    render::Viewport,
};
use glow::HasContext;
use lyon_tessellation::{RoundJoinMode, RoundJoinStats};
use std::{fs::File, io::Write, time::Duration};

#[path = "broad_strokes.rs"]
mod broad_strokes;
#[path = "stroke_encoder.rs"]
mod encoder;
#[path = "small_strokes_quality.rs"]
mod quality;
#[path = "round_join_check.rs"]
mod round_join_check;

fn modes(
    exact: bool,
    bulk: bool,
    concrete_output: bool,
    polygon: bool,
    edges: bool,
    prepared: bool,
    single_pass: bool,
) -> [Control; 2] {
    if polygon || edges || prepared || single_pass {
        let candidate = if single_pass {
            StrokeWriter::BulkSinglePass
        } else if prepared {
            StrokeWriter::BulkPrepared
        } else if edges {
            StrokeWriter::BulkEdges
        } else {
            StrokeWriter::BulkPolygon
        };
        return [StrokeWriter::BulkStatic, candidate].map(|writer| Control {
            name: writer.name(),
            writer,
            culling: Culling::Conservative,
            small: false,
            joins: RoundJoinMode::Exact,
        });
    }
    let writer = if concrete_output {
        StrokeWriter::BulkStatic
    } else if bulk {
        StrokeWriter::Bulk
    } else {
        StrokeWriter::Stock
    };
    [
        Control {
            name: if concrete_output {
                "reference-bulk-static"
            } else if bulk {
                "reference-bulk"
            } else {
                "reference"
            },
            writer,
            culling: Culling::Conservative,
            small: false,
            joins: RoundJoinMode::Reference,
        },
        Control {
            name: if concrete_output {
                "round-exact-bulk-static"
            } else if bulk {
                "round-exact-bulk"
            } else if exact {
                "round-exact"
            } else {
                "small-bevel"
            },
            writer,
            culling: Culling::Conservative,
            small: !exact,
            joins: if exact {
                RoundJoinMode::Exact
            } else {
                RoundJoinMode::Reference
            },
        },
    ]
}

fn selected_modes(options: &Options) -> [Control; 2] {
    let mut pair = modes(
        options.geometry_round_joins,
        options.geometry_stroke_bulk,
        options.geometry_stroke_static,
        options.geometry_stroke_polygon,
        options.geometry_stroke_edges,
        options.geometry_stroke_prepared,
        options.geometry_stroke_single_pass,
    );
    if let Some(selection) = options.geometry_stroke_selection.as_deref() {
        pair[1].writer = match selection {
            "all" => StrokeWriter::BulkSinglePass,
            "layouts" => StrokeWriter::BulkSinglePassLayouts,
            "arcs" => StrokeWriter::BulkSinglePassArcs,
            "classify" => StrokeWriter::BulkSinglePassClassify,
            "prefix" => StrokeWriter::BulkSinglePassPrefix,
            _ => unreachable!("clap validates stroke selection"),
        };
        pair[1].name = pair[1].writer.name();
        if selection == "prefix" {
            pair[0].writer = StrokeWriter::BulkSinglePass;
            pair[0].name = pair[0].writer.name();
        }
    }
    pair
}

struct Validation<'a> {
    renderers: [CompositePresenter; 2],
    caches: [Cache; 2],
    meshes: [MeshBuilder; 2],
    quality: quality::Report<'a>,
    applied: u64,
    edges_applied: u64,
    single_applied: u64,
    modes: [Control; 2],
    exact_check: Option<round_join_check::Validation>,
}
impl<'a> Validation<'a> {
    fn new(context: &Context, options: &'a Options) -> Result<Self, Error> {
        let modes = selected_modes(options);
        Ok(Self {
            renderers: [presenter(context, modes[0])?, presenter(context, modes[1])?],
            caches: std::array::from_fn(|_| Cache::default()),
            meshes: std::array::from_fn(|_| MeshBuilder::default()),
            quality: quality::Report::new(
                options.geometry_output.as_deref(),
                modes[1].name,
                if options.geometry_stroke_single_pass {
                    "single_pass_strokes"
                } else if options.geometry_round_joins {
                    "join_shortcuts"
                } else {
                    "small_strokes"
                },
            )?,
            applied: 0,
            edges_applied: 0,
            single_applied: 0,
            modes,
            exact_check: options.geometry_round_joins.then(Default::default),
        })
    }
    fn check(&mut self, plan: &Plan, label: &str, save: bool) -> Result<[Vec<u8>; 2], Error> {
        let mut pixels: [Vec<u8>; 2] = Default::default();
        let mut stats = [Default::default(); 2];
        let mut applied = [0; 2];
        let before_edges = self.meshes[1].edge_stats();
        for i in 0..2 {
            let before = self.meshes[i].small_strokes_used;
            let before_joins = self.meshes[i].round_join_stats();
            let before_single = self.meshes[i].single_pass_stats();
            let (_, vertices, indices, s) = encode(
                plan,
                &mut self.caches[i],
                &mut self.meshes[i],
                self.modes[i],
                false,
            )?;
            let small = self.meshes[i].small_strokes_used - before;
            let joins = delta(self.meshes[i].round_join_stats(), before_joins);
            let single = self.meshes[i].single_pass_stats().delta(before_single);
            let r = &mut self.renderers[i];
            let before = r.small_strokes_used();
            let before_joins = r.round_join_stats();
            let before_single = r.single_pass_stats();
            let output = r.draw_with_mode(plan, Mode::RetainedCulled)?;
            applied[i] = r.small_strokes_used() - before;
            let gpu_joins = delta(r.round_join_stats(), before_joins);
            let gpu_single = r.single_pass_stats().delta(before_single);
            if (vertices, indices, small) != (output.mesh.vertices, output.mesh.indices, applied[i])
                || joins != gpu_joins
                || single != gpu_single
                || !single.bounded()
                || decisions(s) != decisions(output.mesh.retained)
            {
                return Err(format!(
                    "small stroke CPU/GPU path disagreement: {label} {}",
                    self.modes[i].name
                )
                .into());
            }
            if s.entries > 128 || s.bytes > 16 * 1024 * 1024 {
                return Err("retained cache budget exceeded".into());
            }
            stats[i] = decisions(s);
            if self.exact_check.is_some() {
                applied[i] = joins.shortcuts;
            }
            if self.modes[i].writer.single_pass() {
                applied[i] = single.cached + single.prepared + single.skipped.iter().sum::<u64>();
                self.single_applied += applied[i];
                eprintln!(
                    "single_pass_check={label} cached={} prepared={} refused={:?} layouts={} bytes={} hits={} misses={} evictions={} skipped={:?}",
                    single.cached,
                    single.prepared,
                    single.refused,
                    single.layouts.entries,
                    single.layouts.bytes,
                    single.layouts.hits,
                    single.layouts.misses,
                    single.layouts.evictions,
                    single.skipped
                );
            }
            pixels[i] = r.readback()?;
            r.collect_retired();
        }
        let expected_reference = if self.modes[0].joins == RoundJoinMode::Exact {
            applied[1]
        } else {
            0
        };
        // Single-pass output bypasses some ordinary join calls; those counters
        // are intentionally not equal. Exact output and own counters are checked.
        if stats[0] != stats[1]
            || (!self.modes[1].writer.single_pass() && applied[0] != expected_reference)
        {
            return Err("small outline control changed cache/reuse decisions".into());
        }
        if let Some(check) = &mut self.exact_check {
            let before = check.reused_prefix_vertices();
            check.check(plan)?;
            eprintln!(
                "prefix_reuse_check={label} reused={}",
                check.reused_prefix_vertices() - before
            );
            if pixels[0] != pixels[1] {
                return Err(format!("exact round-join pixels differ: {label}").into());
            }
        }
        if self.modes[1].writer == StrokeWriter::BulkEdges {
            let a = self.meshes[0].edge_stats();
            let b = self.meshes[1].edge_stats();
            if a != Default::default() {
                return Err("reference unexpectedly reused an edge".into());
            }
            let reused = b.reused - before_edges.reused;
            self.edges_applied += reused;
            eprintln!(
                "edge_reuse_check={label} reused={} missed={}",
                reused,
                b.missed - before_edges.missed
            );
        }
        self.applied += applied[1];
        self.quality.check(plan, label, &pixels, applied[1], save)?;
        Ok(pixels)
    }
    fn reset(&mut self) {
        for (r, mode) in self.renderers.iter_mut().zip(self.modes) {
            r.retire();
            r.set_stream_policy(crate::gpu_scene::streaming::Policy::PassBucketed);
            r.set_stroke_culling(mode.culling);
            r.set_stroke_writer(mode.writer);
            r.set_small_strokes(mode.small);
            r.set_round_join_mode(mode.joins);
        }
        self.caches = std::array::from_fn(|_| Cache::default());
        self.meshes = std::array::from_fn(|_| MeshBuilder::default());
        self.quality.clear_previous();
        if let Some(check) = &mut self.exact_check {
            *check = Default::default();
        }
    }
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if options.geometry_stroke_encoder_profile {
        return encoder::run(context, options, width, height);
    }
    if !options.geometry_stroke_cases.is_empty() {
        return broad_strokes::run(context, options, width, height);
    }
    let viewport = Viewport::new(width as f32, height as f32);
    let modes = selected_modes(options);
    let metadata = serde_json::json!({"version":10,"kind":if options.geometry_stroke_single_pass { "single-pass-stroke-renderer" } else if options.geometry_stroke_prepared { "prepared-stroke-guard" } else if options.geometry_stroke_edges { "adjacent-edge-reuse" } else if options.geometry_stroke_polygon { "direct-polygon-input" } else if options.geometry_round_joins { "exact-round-join-shortcut" } else { "small-opaque-outline-approximation" }, "gl":context.identity,
        "stroke_writer":modes[0].writer.name(),
        "polygon_input_comparison":options.geometry_stroke_polygon,
        "edge_reuse_comparison":options.geometry_stroke_edges,
        "prepared_guard_comparison":options.geometry_stroke_prepared,
        "single_pass_comparison":options.geometry_stroke_single_pass,
        "single_pass_selection":options.geometry_stroke_selection.as_deref().unwrap_or("all"),
        "selection_scope":"same classifier in every control; excluded emitters use ordinary output before any layout lookup/compilation or fresh-arc emission; no source reordering or per-piece clocks; not an offline oracle or a cheap acceptance predictor",
        "prefix_scope":"prefix pairs original single-pass with same-frame certified base-position reuse; both emitters and fallback unchanged; other selections still pair with bulk-static; reused-prefix-vertices counts avoided regeneration, not cached screen positions",
        "prefix_accounting":"checked-only: per-state prefix_reuse_check coverage in stderr outside timing; timed CSV topology_reused_prefix_vertices is zero/unmeasured, not evidence of no reuse; no count payload in common mesh results",
        "single_pass_limits":"64 input points, 128 layouts / 512 KiB key-index payload, 642 scratch positions / 3072 scratch indices; merged/fine-arc/unsupported input uses bulk-static; lazy allocation, discarded on writer exit",
        "width":width,"height":height,"history_frames":HISTORY,"warmup_frames":HISTORY,
        "frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "modes":modes.map(|m|m.name),"geometry":"retained-culled","streams":"bucketed","plan":"prepared",
        "scope":if options.geometry_stroke_single_pass { "uncached closed polygons: bounded topology layouts or fresh coarse arcs from computed corner geometry; current-f32 points and ordinary fallback; circles/lines, retained templates, culling and guards unchanged" } else if options.geometry_stroke_prepared { "fixed-width guard bounds prepared once per uncached polygon/line stroke; both path-event input, exact joins and concrete bulk output; circles and retained templates unchanged" } else if options.geometry_stroke_edges { "one-edge per-stroke cache with exact endpoint keys; both path-event input, exact joins and concrete bulk output; circles and retained templates unchanged" } else if options.geometry_stroke_polygon { "uncached polygons/lines: direct endpoint input versus path-event iterator; both exact joins/concrete bulk output; circles and cached templates unchanged" } else if options.geometry_round_joins { "uncached round joins with guaranteed zero subdivisions; conservative angular/numeric bounds, fixed widths, no folds; cached templates stay reference; unchanged geometry styles and cache budgets/guards" } else { "uncached opaque same-color fill/stroke polygons, 3..64 points, <=1px projected extent, 0.001..1px stroke; bevel joins only; stock cached templates and cache budgets/guards" },
        "quality":if options.geometry_round_joins { "exact GPU pixels plus vertex bits, indices, batches and retained-cache decisions across fifteen writers x reference/exact/checked CPU paths; both single-pass paths checked against ordinary Lyon, own cache counters/bounds verified independently of ordinary join counters; selected-or-skipped classifications counted in quality rows" } else { "lossy experiment, not an exact-pixel control; differences and signed temporal error recorded; changes outside eligible envelopes forbidden; no automatic visual-quality approval" },
        "timing":"paired warm CPU encoding or completed GL drawing; source/projection/simulation/text/UI/readback/scanout excluded; no per-piece clocks"});
    eprintln!("small_strokes_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let mut validation = Validation::new(context, options)?;
    println!("{HEADER}");
    for fixture in live_retained::FIXTURES {
        validation.reset();
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
            validation.check(plan, &format!("{}-{i:03}", fixture.name), i == HISTORY - 1)?;
        }
        let frozen = history.last().unwrap();
        if fixture.name == "close-up-zoom" {
            validation.reset();
            let mut cold = None;
            for i in 0..12 {
                let plan = plans.last().unwrap();
                let pixels =
                    validation.check(plan, &format!("hold-{i:03}"), [0, 3, 11].contains(&i))?;
                if i == 0 {
                    cold = Some(pixels);
                } else if i == 11 {
                    let expected = cold.as_ref().unwrap();
                    validation.quality.check_history(
                        plan,
                        "hold-reference-history",
                        &[expected[0].clone(), pixels[0].clone()],
                    )?;
                    validation.quality.check_history(
                        plan,
                        "hold-approx-history",
                        &[expected[1].clone(), pixels[1].clone()],
                    )?;
                }
            }
            validation.reset();
            let mut first = None;
            // Frozen world, smoothly out and back through the selection threshold.
            // Repeat each first/last frame to test cold/warm cache history too.
            for step in 0..=120 {
                let phase = step.min(120 - step) as f32 / 60.;
                let mut frames = frozen.clone();
                for (f, original) in frames.iter_mut().zip(frozen) {
                    f.camera.height = original.camera.height * (0.7 + 0.6 * phase);
                }
                let plan = build(&frames, viewport)?;
                let pixels = validation.check(&plan, &format!("zoom-{step:03}"), step % 30 == 0)?;
                if step == 0 {
                    first = Some(validation.check(&plan, "zoom-warm-start", true)?);
                }
                if step == 120 {
                    let expected = first.as_ref().unwrap();
                    // Cached expansion can differ at antialiased edges; the
                    // return comparison is measured explicitly, not assumed exact.
                    validation.quality.check_history(
                        &plan,
                        "zoom-return-reference-history",
                        &[expected[0].clone(), pixels[0].clone()],
                    )?;
                    validation.quality.check_history(
                        &plan,
                        "zoom-return-approx-history",
                        &[expected[1].clone(), pixels[1].clone()],
                    )?;
                }
            }
        }
        validation.quality.clear_previous();
        for v in [Viewport::new(641., 361.), viewport] {
            for zoom in [44., 640.] {
                let mut frames = frozen.clone();
                for f in &mut frames {
                    f.camera.height = zoom;
                }
                validation.check(
                    &build(&frames, v)?,
                    &format!("{}-resize-{}-zoom-{zoom}", fixture.name, v.width),
                    true,
                )?;
            }
        }
        measure(context, options, modes, fixture.name, &plans)?;
    }
    validation.reset();
    for step in 0..=60 {
        validation.check(
            &synthetic(viewport, step),
            &format!("synthetic-{step:03}"),
            [0, 1, 5, 12, 15, 30, 45, 48, 55, 60].contains(&step),
        )?;
    }
    // Production terrain generations, reveal, excavation and replacement.
    for name in ["terrain-reveal-zoom", "terrain-edit-replace"] {
        validation.reset();
        let mut source = super::super::terrain::StreamSource::new(name)?;
        for step in 0..80 {
            let v = if (64..72).contains(&step) {
                Viewport::new(641., 361.)
            } else {
                viewport
            };
            let scene = source.generate(step, v)?;
            let plan = Plan {
                width: scene.width,
                height: scene.height,
                passes: vec![Pass::Direct {
                    clip: Clip::new(v, (scene.width, scene.height)).unwrap(),
                    scene,
                }],
            };
            validation.check(
                &plan,
                &format!("{name}-{step:03}"),
                [0, 15, 16, 31, 32, 47, 48, 63, 64, 72, 79].contains(&step),
            )?;
        }
    }
    if validation.applied == 0 {
        return Err("small outline control was not exercised".into());
    }
    if options.geometry_stroke_edges && validation.edges_applied == 0 {
        return Err("edge reuse control was not exercised".into());
    }
    if options.geometry_stroke_single_pass && validation.single_applied == 0 {
        return Err("single-pass control was not exercised".into());
    }
    eprintln!(
        "small_strokes_validation frames={} applied={} max_rgb_mae={:.6} max_channel_error={} max_temporal_error_mae={:.6}",
        validation.quality.frames,
        validation.applied,
        validation.quality.max_mae,
        validation.quality.max_error,
        validation.quality.max_temporal
    );
    Ok(())
}

const HEADER: &str = "fixture,path,mode,repeat,frame,wall_ms,cpu_ms,vertices,indices,upload_bytes,admissions,small_strokes,join_shortcuts,join_fallbacks,topology_cached,topology_prepared,topology_unsupported,topology_merged,topology_arc,topology_nonfinite,topology_budget,topology_hits,topology_misses,topology_evictions,topology_entries,topology_bytes,topology_skipped_layouts,topology_skipped_arcs,topology_reused_prefix_vertices";

fn measure(
    context: &Context,
    options: &Options,
    modes: [Control; 2],
    fixture_name: &str,
    plans: &[Plan],
) -> Result<(), Error> {
    for completed in [false, true] {
        for repeat in 0..options.presentation_repeats as usize {
            for offset in 0..2 {
                let mode = modes[(repeat + offset) % 2];
                let mut cache = Cache::default();
                let mut mesh = MeshBuilder::default();
                let mut r = completed.then(|| presenter(context, mode)).transpose()?;
                for plan in plans.iter().cycle().take(plans.len().max(HISTORY)) {
                    if let Some(r) = &mut r {
                        r.draw_with_mode(plan, Mode::RetainedCulled)?;
                    } else {
                        encode(plan, &mut cache, &mut mesh, mode, false)?;
                    }
                }
                // SAFETY: The probe owns the current context.
                unsafe {
                    context.gl.finish();
                }
                let mut samples = Vec::new();
                for i in 0..options.presentation_frames as usize {
                    let plan = &plans[i % plans.len()];
                    let before_single = r
                        .as_ref()
                        .map_or_else(|| mesh.single_pass_stats(), |r| r.single_pass_stats());
                    let stamp = Stamp::now()?;
                    let counts = if let Some(r) = &mut r {
                        let before = r.small_strokes_used();
                        let before_joins = r.round_join_stats();
                        let out = r.draw_with_mode(plan, Mode::RetainedCulled)?;
                        unsafe {
                            context.gl.finish();
                        }
                        (
                            out.mesh.vertices,
                            out.mesh.indices,
                            out.mesh.upload_bytes,
                            out.mesh.retained.admissions,
                            r.small_strokes_used() - before,
                            delta(r.round_join_stats(), before_joins),
                        )
                    } else {
                        let before = mesh.small_strokes_used;
                        let before_joins = mesh.round_join_stats();
                        let (_, v, ind, stats) = encode(plan, &mut cache, &mut mesh, mode, false)?;
                        (
                            v,
                            ind,
                            (v * 24 + ind * 4) as u64,
                            stats.admissions,
                            mesh.small_strokes_used - before,
                            delta(mesh.round_join_stats(), before_joins),
                        )
                    };
                    let (wall, cpu) = stamp.elapsed()?;
                    let single = r
                        .as_ref()
                        .map_or_else(|| mesh.single_pass_stats(), |r| r.single_pass_stats())
                        .delta(before_single);
                    if !single.bounded() {
                        return Err("single-pass cache budget exceeded".into());
                    }
                    samples.push((wall, cpu, counts, single));
                }
                for (i, (wall, cpu, (v, ind, bytes, admissions, small, joins), s)) in
                    samples.into_iter().enumerate()
                {
                    println!(
                        "{},{},{},{repeat},{i},{wall:.6},{cpu:.6},{v},{ind},{bytes},{admissions},{small},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        fixture_name,
                        if completed { "completed" } else { "cpu-encode" },
                        mode.name,
                        joins.shortcuts,
                        joins.fallbacks,
                        s.cached,
                        s.prepared,
                        s.refused[0],
                        s.refused[1],
                        s.refused[2],
                        s.refused[3],
                        s.refused[4],
                        s.layouts.hits,
                        s.layouts.misses,
                        s.layouts.evictions,
                        s.layouts.entries,
                        s.layouts.bytes,
                        s.skipped[0],
                        s.skipped[1],
                        s.reused_prefix_vertices
                    );
                }
            }
        }
    }
    Ok(())
}

fn delta(after: RoundJoinStats, before: RoundJoinStats) -> RoundJoinStats {
    RoundJoinStats {
        shortcuts: after.shortcuts - before.shortcuts,
        fallbacks: after.fallbacks - before.fallbacks,
    }
}

fn synthetic(viewport: Viewport, step: usize) -> Plan {
    use crate::gpu_scene::plan::{Draw, Scene, Shape};
    use engine_common::{RenderColor, RenderPoint, Stroke};
    let shapes: [&[(f32, f32)]; 4] = [
        &[(0., 0.), (1., 0.), (0.2, 1.)],
        &[(0., 0.), (1., 0.), (1., 1.), (0., 1.)],
        &[(0., 0.), (1., 0.), (0.3, 0.4), (1., 1.), (0., 1.)],
        &[(0., 0.), (1., 0.), (0.01, 0.02)],
    ];
    let phase = step.min(60 - step) as f32 / 30.;
    let angle = phase * std::f32::consts::TAU;
    let (s, c) = angle.sin_cos();
    let mut draws = Vec::new();
    for i in 0..1200 {
        let origin = RenderPoint::new(20. + (i % 60) as f32 * 2.25, 20. + (i / 60) as f32 * 2.25);
        let scale = 0.25 + phase * 1.15;
        let color = if i % 2 == 0 {
            RenderColor::rgb(0.53, 0.49, 0.25)
        } else {
            RenderColor::rgb(0.22, 0.36, 0.45)
        };
        draws.push(Draw {
            shape: Shape::Polygon(
                shapes[i % 4]
                    .iter()
                    .map(|&(x, y)| {
                        RenderPoint::new(
                            origin.x + (x * c - y * s) * scale,
                            origin.y + (x * s + y * c) * scale,
                        )
                    })
                    .collect(),
            ),
            fill: Some(color),
            stroke: Some(Stroke::new(color, 0.75)),
        });
    }
    let scene = Scene {
        width: viewport.width as u32,
        height: viewport.height as u32,
        draws,
        reuse: vec![],
        retained: vec![],
    };
    Plan {
        width: scene.width,
        height: scene.height,
        passes: vec![Pass::Direct {
            clip: Clip::new(viewport, (scene.width, scene.height)).unwrap(),
            scene,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn selection_is_explicit_and_uses_the_standard_matched_controls() {
        let flags = [
            "test",
            "--benchmark-presentation",
            "--presentation-geometry",
            "--geometry-stroke-profile",
            "--geometry-round-joins",
            "--geometry-stroke-bulk",
            "--geometry-stroke-static",
            "--geometry-stroke-single-pass",
        ];
        assert!(
            crate::Args::try_parse_from(["test", "--geometry-stroke-selection", "layouts"])
                .is_err()
        );
        for (selection, writer) in [
            ("all", StrokeWriter::BulkSinglePass),
            ("layouts", StrokeWriter::BulkSinglePassLayouts),
            ("arcs", StrokeWriter::BulkSinglePassArcs),
            ("classify", StrokeWriter::BulkSinglePassClassify),
            ("prefix", StrokeWriter::BulkSinglePassPrefix),
        ] {
            let args = crate::Args::try_parse_from(
                flags
                    .into_iter()
                    .chain(["--geometry-stroke-selection", selection]),
            )
            .unwrap();
            let pair = selected_modes(&args.presentation);
            assert_eq!(
                pair[0].writer,
                if selection == "prefix" {
                    StrokeWriter::BulkSinglePass
                } else {
                    StrokeWriter::BulkStatic
                }
            );
            assert_eq!(pair[1].writer, writer);
            for mode in pair {
                assert_eq!(mode.joins, RoundJoinMode::Exact);
                assert_eq!(mode.culling, Culling::Conservative);
                assert!(!mode.small);
            }
        }
        assert!(
            crate::Args::try_parse_from(
                flags
                    .into_iter()
                    .chain(["--geometry-stroke-selection", "unknown"])
            )
            .is_err()
        );
        assert!(
            crate::Args::try_parse_from(flags.into_iter().chain([
                "--geometry-stroke-selection",
                "layouts",
                "--geometry-stroke-encoder-profile"
            ]))
            .is_err()
        );
    }

    #[test]
    fn polygon_comparison_changes_only_input_and_keeps_optimized_output_and_joins() {
        let pair = modes(true, true, true, true, false, false, false);
        assert_eq!(pair[0].writer, StrokeWriter::BulkStatic);
        assert_eq!(pair[1].writer, StrokeWriter::BulkPolygon);
        for mode in pair {
            assert_eq!(mode.joins, RoundJoinMode::Exact);
            assert_eq!(mode.culling, Culling::Conservative);
            assert!(!mode.small);
        }
        let existing = modes(true, true, true, false, false, false, false);
        assert!(
            existing
                .iter()
                .all(|m| m.writer == StrokeWriter::BulkStatic)
        );
        assert_eq!(existing[0].joins, RoundJoinMode::Reference);
        assert_eq!(existing[1].joins, RoundJoinMode::Exact);
    }

    #[test]
    fn edge_comparison_keeps_path_event_input_concrete_output_and_exact_joins() {
        let pair = modes(true, true, true, false, true, false, false);
        assert_eq!(pair[0].writer, StrokeWriter::BulkStatic);
        assert_eq!(pair[1].writer, StrokeWriter::BulkEdges);
        for mode in pair {
            assert_eq!(mode.joins, RoundJoinMode::Exact);
            assert_eq!(mode.culling, Culling::Conservative);
            assert!(!mode.small);
        }
    }

    #[test]
    fn prepared_comparison_changes_only_fixed_width_guard_preparation() {
        let pair = modes(true, true, true, false, false, true, false);
        assert_eq!(pair[0].writer, StrokeWriter::BulkStatic);
        assert_eq!(pair[1].writer, StrokeWriter::BulkPrepared);
        for mode in pair {
            assert_eq!(mode.joins, RoundJoinMode::Exact);
            assert_eq!(mode.culling, Culling::Conservative);
            assert!(!mode.small);
        }
    }

    #[test]
    fn single_pass_comparison_keeps_culling_and_ordinary_fallback_control() {
        let pair = modes(true, true, true, false, false, false, true);
        assert_eq!(pair[0].writer, StrokeWriter::BulkStatic);
        assert_eq!(pair[1].writer, StrokeWriter::BulkSinglePass);
        for mode in pair {
            assert_eq!(mode.joins, RoundJoinMode::Exact);
            assert_eq!(mode.culling, Culling::Conservative);
            assert!(!mode.small);
        }
    }
}
