//! Benchmark-only comparison of direct topology replay and the real adapter.
//! Cohorts are classified/captured/validated outside all throughput samples.
use super::*;
use crate::gpu_scene::{plan::Shape, stroke_single_pass};
use lyon_tessellation::StrokeTopologyStats;

const COHORTS: [&str; 8] = [
    "all",
    "layout",
    "prepared",
    "unsupported",
    "merged",
    "fine-arc",
    "nonfinite",
    "budget",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Plain,
    Direct,
    Adapter,
}
const CONTROLS: [Control; 3] = [Control::Plain, Control::Direct, Control::Adapter];
impl Control {
    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Direct => "direct",
            Self::Adapter => "adapter",
        }
    }
}

#[derive(Default)]
struct Adapter {
    // Match the renderer's lazy boxed state, not a reimplementation of append().
    state: Option<Box<stroke_single_pass::State>>,
    fallback: ExactStrokeTessellator,
    buffers: Buffers,
}

fn options(outline: &Outline) -> StrokeOptions {
    StrokeOptions::default()
        .with_tolerance(0.25)
        .with_line_width(outline.stroke.width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

fn check_outline(
    buffers: &Buffers,
    outline: &Outline,
    base: usize,
    first: usize,
) -> Result<(), Error> {
    if buffers.vertices[base..]
        .iter()
        .map(|v| v.map(f32::to_bits))
        .ne(outline.vertices.iter().map(|v| v.map(f32::to_bits)))
        || buffers.indices[first..]
            .iter()
            .map(|&i| i - base as u32)
            .ne(outline.indices.iter().copied())
    {
        return Err("adapter changed exact colored vertex bits or rebased index order".into());
    }
    Ok(())
}

impl Adapter {
    fn stats(&self) -> stroke_single_pass::Stats {
        self.state
            .as_ref()
            .map_or_else(Default::default, |s| s.stats())
    }

    fn run(&mut self, outlines: &[Outline], check: bool) -> Result<(), Error> {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        for outline in outlines {
            let base = self.buffers.vertices.len();
            let first = self.buffers.indices.len();
            let options = options(outline);
            if !self.state.get_or_insert_with(Default::default).append(
                &outline.points,
                &options,
                outline.stroke.color,
                if check {
                    crate::gpu_scene::StrokeWriter::BulkSinglePassChecked
                } else {
                    crate::gpu_scene::StrokeWriter::BulkSinglePass
                },
                &mut self.buffers,
            )? {
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
                    .map_err(|e| format!("adapter replay fallback: {e:?}"))?;
            }
            if check {
                check_outline(&self.buffers, outline, base, first)?;
            }
        }
        black_box(&self.buffers);
        Ok(())
    }
}

#[derive(Default)]
struct Runner {
    direct: Replay,
    adapter: Adapter,
}
impl Runner {
    fn run(
        &mut self,
        outlines: &[Outline],
        control: Control,
        check: bool,
    ) -> Result<Counts, Error> {
        match control {
            Control::Plain => self.direct.run(outlines, Mode::Plain, check),
            Control::Direct => self.direct.run(outlines, Mode::SinglePass, check),
            Control::Adapter => {
                self.adapter.run(outlines, check)?;
                // Read cumulative counts after timing, not once per stroke.
                Ok(Counts::default())
            }
        }
    }
    fn layouts(&self, control: Control) -> StrokeTopologyStats {
        match control {
            Control::Adapter => self.adapter.stats().layouts,
            _ => self.direct.topology.stats(),
        }
    }
    fn buffers(&self, control: Control) -> &Buffers {
        match control {
            Control::Adapter => &self.adapter.buffers,
            _ => &self.direct.buffers,
        }
    }
}

fn adapter_counts(stats: stroke_single_pass::Stats) -> Counts {
    Counts {
        reused: stats.cached,
        prepared: stats.prepared,
        refused: stats.refused,
    }
}

fn classify(tessellator: &mut StrokeTopologyTessellator, outline: &Outline) -> usize {
    match tessellator.tessellate_single_pass(
        Polygon {
            points: &outline.points,
            closed: true,
        },
        &options(outline),
    ) {
        Ok(mesh) => match mesh.source {
            StrokeTopologySource::Layout => 1,
            StrokeTopologySource::Prepared => 2,
        },
        Err(reason) => 3 + reject_index(reason),
    }
}

fn classes(captures: &[Capture]) -> Vec<Vec<usize>> {
    let mut tessellator = StrokeTopologyTessellator::new();
    captures
        .iter()
        .map(|c| {
            c.outlines
                .iter()
                .map(|o| classify(&mut tessellator, o))
                .collect()
        })
        .collect()
}

fn subset(captures: &[Capture], classes: &[Vec<usize>], cohort: usize) -> Vec<Capture> {
    captures
        .iter()
        .zip(classes)
        .map(|(c, kinds)| Capture {
            outlines: c
                .outlines
                .iter()
                .zip(kinds)
                .filter(|(_, kind)| cohort == 0 || **kind == cohort)
                .map(|(o, _)| Outline {
                    points: o.points.clone(),
                    stroke: o.stroke,
                    vertices: o.vertices.clone(),
                    indices: o.indices.clone(),
                })
                .collect(),
            // Subsets cannot exceed the already checked original history budget.
            ..Default::default()
        })
        .collect()
}

fn clock_capture(name: &str, viewport: Viewport) -> Result<Vec<Capture>, Error> {
    let fixture = crate::presentation_probe::geometry::fixtures::build(name, viewport)?;
    let plan = presentation::build_reusing(
        fixture.at(0),
        viewport,
        crate::render::FrameLayout::EqualHorizontal,
        1.,
        true,
        PlanMode::Prepared,
        None,
    )?;
    let scene = plan
        .single_scene()
        .ok_or("Clock capture requires one full scene")?;
    // These production fixtures contain no retained geometry; only ordinary
    // projected polygon strokes enter the adapter. Do not silently flatten hints.
    if !scene.retained.is_empty() || !scene.reuse.is_empty() {
        return Err("unexpected Clock retention".into());
    }
    let mut mesh = MeshBuilder::default();
    mesh.set_stroke_writer(StrokeWriter::BulkStatic);
    mesh.set_round_join_mode(RoundJoinMode::Exact);
    let mut capture = Capture::default();
    for draw in &scene.draws {
        if draw.stroke.is_none() {
            continue;
        }
        match &draw.shape {
            Shape::Polygon(_) => {
                mesh.clear();
                mesh.append_draw_with_styles(draw, None, draw.stroke)?;
                capture.record(draw, &mesh.buffers.vertices, &mesh.buffers.indices, 0);
            }
            Shape::LocalPolygon(_) => {
                return Err("Clock fixture needs projected outline capture".into());
            }
            Shape::Circle { .. } | Shape::Line { .. } => {}
        }
    }
    if let Some(error) = capture.error {
        return Err(error.into());
    }
    if capture.outlines.is_empty() {
        return Err("Clock outline corpus is empty".into());
    }
    Ok(vec![capture])
}

fn validate(
    captures: &[Capture],
    cohort: usize,
) -> Result<(Vec<Counts>, serde_json::Value), Error> {
    let (mut reference, mut direct, mut adapter) =
        (Runner::default(), Runner::default(), Runner::default());
    let mut expected = Vec::new();
    for traversal in 0..2 {
        for (frame, capture) in captures.iter().enumerate() {
            reference.run(&capture.outlines, Control::Plain, true)?;
            let before_direct = direct.layouts(Control::Direct);
            let counts = direct.run(&capture.outlines, Control::Direct, true)?;
            let before_adapter = adapter.adapter.stats();
            adapter.run(&capture.outlines, Control::Adapter, true)?;
            let stats = adapter.adapter.stats().delta(before_adapter);
            if counts != adapter_counts(stats)
                || !stats.bounded()
                || stats.checked != counts.reused + counts.prepared
            {
                return Err("adapter selection/checked coverage differs from direct replay".into());
            }
            let after_direct = direct.layouts(Control::Direct);
            if after_direct.entries != stats.layouts.entries
                || after_direct.bytes != stats.layouts.bytes
                || after_direct.hits - before_direct.hits != stats.layouts.hits
                || after_direct.misses - before_direct.misses != stats.layouts.misses
                || after_direct.evictions - before_direct.evictions != stats.layouts.evictions
            {
                return Err("adapter layout cache differs from direct replay".into());
            }
            let selected = match cohort {
                0 => capture.outlines.len() as u64,
                1 => counts.reused,
                2 => counts.prepared,
                _ => counts.refused[cohort - 3],
            };
            if selected != capture.outlines.len() as u64
                || counts.reused + counts.prepared + counts.refused.iter().sum::<u64>() != selected
            {
                return Err("cohort classification changed during replay".into());
            }
            if traversal == 0 {
                expected.push(counts);
            } else if counts != expected[frame] {
                return Err("cohort history changed selection".into());
            }
        }
    }
    let stats = adapter.adapter.stats();
    let metadata = serde_json::json!({"name":COHORTS[cohort],
        "strokes": captures.iter().map(|c| c.outlines.len()).collect::<Vec<_>>(),
        "vertices": captures.iter().map(|c| c.outlines.iter().map(|o| o.vertices.len()).sum::<usize>()).collect::<Vec<_>>(),
        "indices": captures.iter().map(|c| c.outlines.iter().map(|o| o.indices.len()).sum::<usize>()).collect::<Vec<_>>(),
        "cached":expected.iter().map(|c| c.reused).collect::<Vec<_>>(),
        "prepared":expected.iter().map(|c| c.prepared).collect::<Vec<_>>(),
        "refused":expected.iter().map(|c| c.refused).collect::<Vec<_>>(),
        "cache_entries":stats.layouts.entries,"cache_bytes":stats.layouts.bytes,
        "checked_strokes":stats.checked});
    Ok((expected, metadata))
}

fn measure(
    options: &Options,
    name: &str,
    cohort: &str,
    captures: &[Capture],
    expected: &[Counts],
) -> Result<(), Error> {
    for repeat in 0..options.presentation_repeats {
        let mut order = (0..3)
            .map(|i| CONTROLS[(i + repeat as usize) % 3])
            .collect::<Vec<_>>();
        if repeat % 2 != 0 {
            order.reverse();
        }
        for control in order {
            let mut runner = Runner::default();
            for c in captures {
                runner.run(&c.outlines, control, false)?;
            }
            let mut samples = Vec::new();
            for frame in 0..options.presentation_frames as usize {
                let index = frame % captures.len();
                let capture = &captures[index];
                let before_layouts = runner.layouts(control);
                let before_adapter = runner.adapter.stats();
                let stamp = Stamp::now()?;
                let mut actual = runner.run(&capture.outlines, control, false)?;
                let (wall, cpu) = stamp.elapsed()?;
                if control == Control::Adapter {
                    actual = adapter_counts(runner.adapter.stats().delta(before_adapter));
                }
                let layouts = runner.layouts(control);
                if control != Control::Plain && actual != expected[index] {
                    return Err("timed adapter selection changed".into());
                }
                if layouts.entries > 128 || layouts.bytes > 512 * 1024 {
                    return Err("timed adapter exceeded cache bounds".into());
                }
                let buffers = runner.buffers(control);
                let vertices = capture
                    .outlines
                    .iter()
                    .map(|o| o.vertices.len())
                    .sum::<usize>();
                let indices = capture
                    .outlines
                    .iter()
                    .map(|o| o.indices.len())
                    .sum::<usize>();
                if buffers.vertices.len() != vertices || buffers.indices.len() != indices {
                    return Err("timed adapter geometry counts changed".into());
                }
                samples.push((
                    wall,
                    cpu,
                    actual,
                    vertices,
                    indices,
                    capture.outlines.len(),
                    before_layouts,
                    layouts,
                ));
            }
            for (frame, (wall, cpu, c, v, i, n, before, after)) in samples.into_iter().enumerate() {
                println!(
                    "{name},{cohort},{},{repeat},{frame},{wall:.6},{cpu:.6},{n},{v},{i},{},{},{},{},{},{},{},{},{},{},{},{}",
                    control.name(),
                    c.reused,
                    c.prepared,
                    c.refused[0],
                    c.refused[1],
                    c.refused[2],
                    c.refused[3],
                    c.refused[4],
                    after.hits - before.hits,
                    after.misses - before.misses,
                    after.evictions - before.evictions,
                    after.entries,
                    after.bytes
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "stroke_adapter_tests.rs"]
mod tests;

fn fixture(
    options: &Options,
    metadata: &mut serde_json::Value,
    name: &str,
    captures: Vec<Capture>,
) -> Result<(), Error> {
    let classified = classes(&captures);
    let mut cohorts = Vec::new();
    for (index, name_cohort) in COHORTS.iter().enumerate() {
        if !classified
            .iter()
            .flatten()
            .any(|&c| index == 0 || index == c)
        {
            continue;
        }
        let selected = subset(&captures, &classified, index);
        let (counts, data) = validate(&selected, index)?;
        measure(options, name, name_cohort, &selected, &counts)?;
        cohorts.push(data);
    }
    let row = serde_json::json!({"name":name,"history_frames":captures.len(),
        "capture":"Spacewars: actual guarded strokes; Clock: ordinary production projected polygon strokes",
        "strokes":captures.iter().map(|c| c.outlines.len()).collect::<Vec<_>>(),"cohorts":cohorts});
    eprintln!("adapter_fixture={row}");
    metadata["fixtures"].as_array_mut().unwrap().push(row);
    Ok(())
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let viewport = Viewport::new(width as f32, height as f32);
    let mut metadata = serde_json::json!({"version":1,"kind":"stroke-adapter", "gl":context.identity,
        "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "controls":CONTROLS.map(Control::name),"cohorts":COHORTS,"fixtures":[],
        "scope":"warm CPU stroke replay only; actual State::append (lazy boxed state, budget/check branches, cumulative counters) vs existing direct replay and plain Lyon; no fill/point projection/retained expansion/GPU/UI/simulation",
        "validation":"two complete cold/warm traversals, exact colored vertex bits and indices per stroke against capture; adapter checked mode covers all accepted strokes; decisions/cache resources match direct; no per-stroke clocks; classification/counter snapshots/oracles outside timing",
        "limits":"same 128 layouts / 512 KiB payload; original capture max 8192 draws / 8 MiB per frame, 128 MiB per history, one subset clone at a time",
        "order":"six repeats cover all six control orders; each control warms entire selected history; filtered cohorts retain original within-frame order and frame boundaries, but change interleaving, so their timings are not additive full-frame costs"});
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    println!(
        "fixture,cohort,mode,repeat,frame,wall_ms,cpu_ms,strokes,vertices,indices,reused,prepared,unsupported,merged,arc,nonfinite,budget,cache_hits,cache_misses,cache_evictions,cache_entries,cache_bytes"
    );
    for source in live_retained::FIXTURES {
        let mut scenario = live_retained::scenario(viewport, source)?;
        let layout = scenario.frame_layout();
        let plans = (0..HISTORY)
            .map(|_| {
                scenario.step(&[], Duration::from_nanos(16_666_667));
                presentation::build_reusing(
                    &scenario.render_frames(RenderBackend::Raster, viewport),
                    viewport,
                    layout,
                    1.,
                    true,
                    PlanMode::Prepared,
                    None,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        fixture(options, &mut metadata, source.name, capture(&plans)?)?;
    }
    for name in ["clock-idle", "clock-rain"] {
        fixture(options, &mut metadata, name, clock_capture(name, viewport)?)?;
    }
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    eprintln!("stroke_adapter={metadata}");
    Ok(())
}
