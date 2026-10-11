//! Isolated throughput and exact-output checks for topology reuse. No live
//! renderer integration: captured guarded strokes are the entire timed scope.
use super::*;
use lyon_tessellation::{
    StrokeTopologyReject, StrokeTopologySource, StrokeTopologyTessellator, math::point,
};

#[path = "stroke_adapter.rs"]
mod adapter;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Plain,
    Topology,
    SinglePass,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Topology => "topology",
            Self::SinglePass => "single-pass",
        }
    }
}

const MODES: [Mode; 3] = [Mode::Plain, Mode::Topology, Mode::SinglePass];

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    reused: u64,
    prepared: u64,
    refused: [u64; 5],
}

fn reject_index(reason: StrokeTopologyReject) -> usize {
    match reason {
        StrokeTopologyReject::Unsupported => 0,
        StrokeTopologyReject::Merge => 1,
        StrokeTopologyReject::Arc => 2,
        StrokeTopologyReject::NonFinite => 3,
        StrokeTopologyReject::Budget => 4,
    }
}

#[derive(Default)]
struct Replay {
    topology: StrokeTopologyTessellator,
    fallback: ExactStrokeTessellator,
    buffers: Buffers,
}

impl Replay {
    fn run(&mut self, outlines: &[Outline], mode: Mode, check: bool) -> Result<Counts, Error> {
        if mode == Mode::Plain {
            replay::<_, 0>(
                outlines,
                &mut self.fallback,
                &mut self.buffers,
                &mut NoStrokeProfile,
                check,
            )?;
            return Ok(Counts::default());
        }
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let mut counts = Counts::default();
        for outline in outlines {
            let base = self.buffers.vertices.len();
            let first_index = self.buffers.indices.len();
            let options = StrokeOptions::default()
                .with_tolerance(0.25)
                .with_line_width(outline.stroke.width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round);
            let polygon = Polygon {
                points: &outline.points,
                closed: true,
            };
            let result = if mode == Mode::SinglePass {
                self.topology.tessellate_single_pass(polygon, &options)
            } else {
                self.topology.tessellate(polygon, &options)
            };
            match result {
                Ok(mesh) => {
                    match mesh.source {
                        StrokeTopologySource::Layout => counts.reused += 1,
                        StrokeTopologySource::Prepared => counts.prepared += 1,
                    }
                    let c = outline.stroke.color;
                    let color = [c.r * c.a, c.g * c.a, c.b * c.a, c.a];
                    self.buffers.vertices.extend(
                        mesh.positions
                            .iter()
                            .map(|p| [p.x, p.y, color[0], color[1], color[2], color[3]]),
                    );
                    self.buffers
                        .indices
                        .extend(mesh.indices.iter().map(|&i| i + base as u32));
                }
                Err(reason) => {
                    counts.refused[reject_index(reason)] += 1;
                    self.fallback
                        .tessellate_static(
                            Polygon {
                                points: &outline.points,
                                closed: true,
                            }
                            .path_events(),
                            &options,
                            &mut Bulk::new(&mut self.buffers, outline.stroke.color),
                        )
                        .map_err(|e| format!("topology fallback: {e:?}"))?;
                }
            }
            if check
                && (self.buffers.vertices[base..]
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .ne(outline.vertices.iter().map(|v| v.map(f32::to_bits)))
                    || self.buffers.indices[first_index..]
                        .iter()
                        .map(|&i| i - base as u32)
                        .ne(outline.indices.iter().copied()))
            {
                return Err(format!(
                    "topology output changed for {:?}, width={}, counts={counts:?}",
                    outline.points, outline.stroke.width
                )
                .into());
            }
        }
        black_box(&self.buffers);
        Ok(counts)
    }
}

fn transformed_checks(captures: &[Capture]) -> Result<usize, Error> {
    let (mut reference, mut candidate) = (Replay::default(), Replay::default());
    let mut checked = 0;
    for outline in captures.iter().flat_map(|c| &c.outlines) {
        let origin = outline.points[0];
        for (scale, angle, offset) in [
            (0.99, 0.0_f32, [0., 0.]),
            (1.01, 0., [0., 0.]),
            (1., 0.01, [0., 0.]),
            (1., 0., [0.125, -0.25]),
            (1., 0., [111.123, -87.456]),
        ] {
            let (s, c) = angle.sin_cos();
            let mut transformed = Outline {
                points: outline
                    .points
                    .iter()
                    .map(|p| {
                        let x = (p.x - origin.x) * scale;
                        let y = (p.y - origin.y) * scale;
                        point(
                            c * x - s * y + origin.x + offset[0],
                            s * x + c * y + origin.y + offset[1],
                        )
                    })
                    .collect(),
                stroke: outline.stroke,
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            reference.run(std::slice::from_ref(&transformed), Mode::Plain, false)?;
            transformed.vertices.clone_from(&reference.buffers.vertices);
            transformed.indices.clone_from(&reference.buffers.indices);
            for mode in [Mode::Topology, Mode::SinglePass] {
                candidate.run(std::slice::from_ref(&transformed), mode, true)?;
            }
            checked += 1;
        }
    }
    Ok(checked)
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if options.geometry_stroke_adapter {
        return adapter::run(context, options, width, height);
    }
    let viewport = Viewport::new(width as f32, height as f32);
    let mut metadata = serde_json::json!({"kind":"stroke-topology", "version":2,
        "gl":context.identity,"width":width,"height":height,"history_frames":HISTORY,
        "frames":options.presentation_frames,"repeats":options.presentation_repeats,"fixtures":[],
        "controls": MODES.map(Mode::name),
        "scope":"isolated stroke replay: current-f32 transition checks, cache lookup, fresh vertices, copied cached indices, prepared/ordinary fallback; no projection/simulation/text/UI/GPU timing",
        "limits":"64 points per candidate, 128 FIFO layouts, 512 KiB key/index payload; single-pass adds at most 642 scratch vertices and 3072 indices, arcs with tolerance >= radius/2 only; merged paths use ordinary fallback",
        "quality":"exact vertex bits and index order on every captured and synthetically transformed stroke for BOTH candidates, outside timing; transformed_checks counts cases per candidate",
        "order":"rotate policy order and reverse alternate repeats; six repeats cover all six orders; warm complete 24-frame history before each timing block"});
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    println!(
        "fixture,mode,repeat,frame,wall_ms,cpu_ms,strokes,vertices,indices,reused,prepared,unsupported,merged,arc,nonfinite,budget,cache_hits,cache_misses,cache_evictions,cache_entries,cache_bytes"
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
        let mut validation = Replay::default();
        let mut counts = Vec::new();
        for c in &captures {
            validation.run(&c.outlines, Mode::Plain, true)?;
            counts.push([
                validation.run(&c.outlines, Mode::Topology, true)?,
                validation.run(&c.outlines, Mode::SinglePass, true)?,
            ]);
        }
        let changed = transformed_checks(&captures)?;
        let cache = validation.topology.stats();
        let row = serde_json::json!({"name":fixture.name,
            "strokes":captures.iter().map(|c|c.outlines.len()).collect::<Vec<_>>(),
            "accepted":counts.iter().map(|c|c[0].reused).collect::<Vec<_>>(),
            "refused":counts.iter().map(|c|c[0].refused).collect::<Vec<_>>(),
            "single_pass_accepted":counts.iter().map(|c|c[1].reused).collect::<Vec<_>>(),
            "single_pass_prepared":counts.iter().map(|c|c[1].prepared).collect::<Vec<_>>(),
            "single_pass_refused":counts.iter().map(|c|c[1].refused).collect::<Vec<_>>(),
            "refusal_order":["unsupported","merged","arc","nonfinite","budget"],
            "transformed_checks":changed,"cache_entries":cache.entries,"cache_bytes":cache.bytes});
        eprintln!("topology_fixture={row}");
        metadata["fixtures"].as_array_mut().unwrap().push(row);
        for repeat in 0..options.presentation_repeats {
            let mut order = (0..3)
                .map(|i| MODES[(i + repeat as usize) % 3])
                .collect::<Vec<_>>();
            if repeat % 2 != 0 {
                order.reverse();
            }
            for mode in order {
                let mut state = Replay::default();
                for c in &captures {
                    state.run(&c.outlines, mode, false)?;
                }
                let mut rows = Vec::new();
                for frame in 0..options.presentation_frames as usize {
                    let c = &captures[frame % HISTORY];
                    let before = state.topology.stats();
                    let stamp = Stamp::now()?;
                    let actual = state.run(&c.outlines, mode, false)?;
                    let (wall, cpu) = stamp.elapsed()?;
                    if mode != Mode::Plain {
                        let index = if mode == Mode::Topology { 0 } else { 1 };
                        if actual != counts[frame % HISTORY][index] {
                            return Err("topology selection changed during timing".into());
                        }
                    }
                    let after = state.topology.stats();
                    if after.entries > 128 || after.bytes > 512 * 1024 {
                        return Err("topology cache exceeds budget".into());
                    }
                    rows.push((
                        wall,
                        cpu,
                        actual,
                        state.buffers.vertices.len(),
                        state.buffers.indices.len(),
                        c.outlines.len(),
                        before,
                        after,
                    ));
                }
                for (frame, (wall, cpu, c, v, i, n, b, a)) in rows.into_iter().enumerate() {
                    println!(
                        "{},{},{repeat},{frame},{wall:.6},{cpu:.6},{n},{v},{i},{},{},{},{},{},{},{},{},{},{},{},{}",
                        fixture.name,
                        mode.name(),
                        c.reused,
                        c.prepared,
                        c.refused[0],
                        c.refused[1],
                        c.refused[2],
                        c.refused[3],
                        c.refused[4],
                        a.hits - b.hits,
                        a.misses - b.misses,
                        a.evictions - b.evictions,
                        a.entries,
                        a.bytes
                    );
                }
            }
        }
    }
    eprintln!("stroke_topology={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{RenderColor, Stroke};

    #[test]
    fn captured_merge_fold_translation_and_cap_transitions_match_reference() {
        // Counterexamples from the blind-template audit: decisions must be
        // recomputed from today's projected f32 points, never yesterday's key.
        let captures = vec![Capture {
            outlines: [
                vec![
                    point(271.32242, 535.57996),
                    point(271.34363, 533.79584),
                    point(271.1863, 533.73505),
                    point(270.5834, 535.2939),
                ],
                vec![
                    point(1005.36536, 546.7102),
                    point(1005.1515, 546.41016),
                    point(1004.6779, 546.7476),
                ],
                vec![
                    point(186.77505, 215.63063),
                    point(187.02226, 215.72528),
                    point(186.87161, 215.37845),
                ],
                vec![
                    point(752.3717, 341.4065),
                    point(752.3513, 341.3778),
                    point(752.30286, 341.4123),
                ],
            ]
            .into_iter()
            .map(|points| Outline {
                points,
                stroke: Stroke::new(RenderColor::rgba(0.1, 0.4, 0.9, 0.5), 0.75),
                vertices: Vec::new(),
                indices: Vec::new(),
            })
            .collect(),
            ..Default::default()
        }];
        assert_eq!(transformed_checks(&captures).unwrap(), 20);
    }

    #[test]
    fn topology_replay_preserves_prefix_color_and_merging_fallback() {
        let mut outlines = Vec::new();
        let mut reference = Replay::default();
        for points in [
            vec![point(0., 0.), point(1., 0.), point(1., 1.), point(0., 1.)],
            vec![point(0., 0.), point(3., 0.), point(0.2, 0.1)],
            vec![point(0., 0.); 3],
        ] {
            let mut o = Outline {
                points,
                stroke: Stroke::new(RenderColor::rgba(0.3, 0.2, 0.9, 0.4), 0.75),
                vertices: vec![],
                indices: vec![],
            };
            reference
                .run(std::slice::from_ref(&o), Mode::Plain, false)
                .unwrap();
            o.vertices.clone_from(&reference.buffers.vertices);
            o.indices.clone_from(&reference.buffers.indices);
            outlines.push(o);
        }
        for mode in [Mode::Topology, Mode::SinglePass] {
            let mut candidate = Replay::default();
            for _ in 0..3 {
                let counts = candidate.run(&outlines, mode, true).unwrap();
                assert_eq!(counts.reused, 1);
                if mode == Mode::SinglePass {
                    assert_eq!(counts.prepared, 1);
                    assert_eq!(counts.refused, [0, 1, 0, 0, 0]);
                } else {
                    assert_eq!(counts.prepared, 0);
                    assert_eq!(counts.refused, [0, 1, 1, 0, 0]);
                }
            }
            assert_eq!(candidate.topology.stats().hits, 2);
        }
    }
}
