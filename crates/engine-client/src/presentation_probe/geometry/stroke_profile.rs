//! Matched stroke output controls on a bounded moving history. Projection/source
//! work is outside timing. CPU-only and completed-GL samples are separate;
//! detailed per-piece clocks are excluded from both plain controls.
use super::{Context, Error, Stamp, live_retained, save_png};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        CompositePresenter, Mode, PlanMode,
        encode_profile::EncodeProfile,
        mesh::MeshBuilder,
        presentation::{self, Plan},
        retained::{Cache, Policy, Stats},
        streaming,
        stroke_census::Census,
        stroke_culling::{self, Control as Culling},
        stroke_output::StrokeWriter,
    },
    presentation_probe::Options,
    render::Viewport,
};
use glow::HasContext;
use std::{fs::File, io::Write, time::Duration};

#[path = "stroke_attribution.rs"]
mod attribution;
#[path = "small_strokes.rs"]
mod small_strokes;

const HISTORY: usize = 24;
#[derive(Clone, Copy)]
struct Control {
    name: &'static str,
    writer: StrokeWriter,
    culling: Culling,
    small: bool,
    joins: lyon_tessellation::RoundJoinMode,
}

fn controls(rejection: bool) -> [Control; 2] {
    [
        Control {
            name: StrokeWriter::Stock.name(),
            writer: StrokeWriter::Stock,
            culling: Culling::Off,
            small: false,
            joins: lyon_tessellation::RoundJoinMode::Reference,
        },
        if rejection {
            Control {
                name: "reject",
                writer: StrokeWriter::Stock,
                culling: Culling::Conservative,
                small: false,
                joins: lyon_tessellation::RoundJoinMode::Reference,
            }
        } else {
            Control {
                name: StrokeWriter::Bulk.name(),
                writer: StrokeWriter::Bulk,
                culling: Culling::Off,
                small: false,
                joins: lyon_tessellation::RoundJoinMode::Reference,
            }
        },
    ]
}
// Four plain/profiled blocks balance positions and within-block predecessors.
const ORDER: [[usize; 4]; 4] = [[0, 1, 3, 2], [1, 2, 0, 3], [2, 3, 1, 0], [3, 0, 2, 1]];

fn presenter(context: &Context, control: Control) -> Result<CompositePresenter, Error> {
    let mut r = CompositePresenter::new(context.gl.clone())?;
    r.set_stream_policy(streaming::Policy::PassBucketed);
    r.set_stroke_writer(control.writer);
    r.set_stroke_culling(control.culling);
    r.set_small_strokes(control.small);
    r.set_round_join_mode(control.joins);
    Ok(r)
}

fn decisions(mut s: Stats) -> Stats {
    s.build_ns = 0;
    s.max_build_ns = 0;
    s.stroke_bound_tests = 0;
    s.stroke_bound_unknown = 0;
    s.stroke_bound_rejections = 0;
    s
}

fn encode(
    plan: &Plan,
    cache: &mut Cache,
    mesh: &mut MeshBuilder,
    control: Control,
    detailed: bool,
) -> Result<(EncodeProfile, usize, usize, Stats), Error> {
    mesh.stroke_writer = control.writer;
    mesh.stroke_culling = control.culling;
    mesh.small_strokes = control.small;
    mesh.set_round_join_mode(control.joins);
    cache.begin_frame();
    let mut stages = EncodeProfile::default();
    let (mut vertices, mut indices) = (0, 0);
    let mut stats = Stats::default();
    for pass in &plan.passes {
        let scene = pass.scene();
        if detailed {
            stages.accumulate(cache.rebuild_profiled_clipped(
                scene,
                &scene.retained,
                mesh,
                Policy::HybridCulled,
                pass.clip(),
            )?);
        } else {
            cache.rebuild_clipped(
                scene,
                &scene.retained,
                mesh,
                Policy::HybridCulled,
                pass.clip(),
            )?;
        }
        std::hint::black_box(&mesh.buffers);
        vertices += mesh.buffers.vertices.len();
        indices += mesh.buffers.indices.len();
        stats.accumulate(cache.stats);
    }
    Ok((stages, vertices, indices, stats))
}

struct Validation {
    renderers: Vec<CompositePresenter>,
    caches: [Cache; 3],
    meshes: [MeshBuilder; 3],
    rejection: bool,
}
impl Validation {
    fn new(context: &Context, rejection: bool) -> Result<Self, Error> {
        let modes = controls(rejection);
        Ok(Self {
            renderers: (0..2)
                .map(|i| presenter(context, modes[i]))
                .collect::<Result<_, _>>()?,
            caches: std::array::from_fn(|_| Cache::default()),
            meshes: std::array::from_fn(|i| {
                let mut mesh = MeshBuilder::default();
                mesh.stroke_writer = modes[i.min(1)].writer;
                mesh.stroke_culling = if i == 2 {
                    Culling::Checked
                } else {
                    modes[i].culling
                };
                mesh
            }),
            rejection,
        })
    }
    fn check(&mut self, plan: &Plan) -> Result<Vec<u8>, Error> {
        for c in &mut self.caches {
            c.begin_frame();
        }
        let mut expected_rejections = [0; 2];
        for pass in &plan.passes {
            let scene = pass.scene();
            for i in 0..if self.rejection { 3 } else { 2 } {
                self.caches[i].rebuild_clipped(
                    scene,
                    &scene.retained,
                    &mut self.meshes[i],
                    Policy::HybridCulled,
                    pass.clip(),
                )?;
            }
            let [a, b, checked] = &self.meshes;
            if self.rejection {
                same_mesh(b, checked)?;
                visible_subsequence(a, b, pass.clip())?;
                if decisions(self.caches[1].stats) != decisions(self.caches[2].stats)
                    || self.caches[1].stats.stroke_bound_rejections
                        != self.caches[2].stats.stroke_bound_rejections
                {
                    return Err("checked rejection changed cache decisions".into());
                }
            } else {
                same_mesh(a, b)?;
            }
            for (expected, c) in expected_rejections.iter_mut().zip(&self.caches) {
                *expected += c.stats.stroke_bound_rejections;
            }
            if decisions(self.caches[0].stats) != decisions(self.caches[1].stats) {
                return Err(
                    "stroke writer changed mesh bits/order/batches or cache decisions".into(),
                );
            }
        }
        let mut reference = None;
        let mut counts: Option<(usize, usize, u64, u64, Stats)> = None;
        for (index, r) in self.renderers.iter_mut().enumerate() {
            let out = r.draw_with_mode(plan, Mode::RetainedCulled)?;
            let actual = r.readback()?;
            let count = (
                out.mesh.vertices,
                out.mesh.indices,
                out.mesh.draws,
                out.mesh.upload_bytes,
                decisions(out.mesh.retained),
            );
            if out.mesh.retained.stroke_bound_rejections != expected_rejections[index] {
                return Err("GPU path did not apply checked CPU rejection decisions".into());
            }
            if reference.is_some() && reference.as_ref() != Some(&actual) {
                return Err("stroke control changed pixels".into());
            }
            if let Some(old) = counts {
                let same = if self.rejection {
                    count.0 <= old.0
                        && count.1 <= old.1
                        && count.2 <= old.2
                        && count.3 <= old.3
                        && count.4 == old.4
                } else {
                    count == old
                };
                if !same {
                    return Err("stroke writer changed pixels/draws/uploads/cache decisions".into());
                }
            }
            reference = Some(actual);
            counts = Some(count);
            r.collect_retired();
        }
        Ok(reference.unwrap())
    }
}

fn same_mesh(a: &MeshBuilder, b: &MeshBuilder) -> Result<(), Error> {
    if !a
        .buffers
        .vertices
        .iter()
        .flatten()
        .map(|f| f.to_bits())
        .eq(b.buffers.vertices.iter().flatten().map(|f| f.to_bits()))
        || a.buffers.indices != b.buffers.indices
        || a.batches != b.batches
    {
        return Err("stroke control changed exact checked mesh".into());
    }
    Ok(())
}

/// Every omitted reference triangle must be wholly outside one expanded clip
/// edge. Kept triangles preserve exact vertex bits and original painter order.
fn visible_subsequence(
    a: &MeshBuilder,
    b: &MeshBuilder,
    clip: presentation::Clip,
) -> Result<(), Error> {
    let mut kept = b.buffers.indices.chunks_exact(3).peekable();
    for tri in a.buffers.indices.chunks_exact(3) {
        let vertices: [_; 3] = std::array::from_fn(|i| a.buffers.vertices[tri[i] as usize]);
        if let Some(next) = kept.peek()
            && vertices.iter().flatten().map(|v| v.to_bits()).eq(next
                .iter()
                .flat_map(|i| b.buffers.vertices[*i as usize])
                .map(f32::to_bits))
        {
            kept.next();
        } else if !stroke_culling::vertices_outside(&vertices, clip) {
            return Err("stroke rejection removed or reordered a visible triangle".into());
        }
    }
    if kept.next().is_some() {
        return Err("stroke rejection added geometry".into());
    }
    Ok(())
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if options.geometry_stroke_attribution {
        return attribution::run(context, options, width, height);
    }
    if options.geometry_small_strokes || options.geometry_round_joins {
        return small_strokes::run(context, options, width, height);
    }
    let viewport = Viewport::new(width as f32, height as f32);
    let rejection = options.geometry_stroke_rejection;
    let modes = controls(rejection);
    let metadata = serde_json::json!({"version":3,"kind":if rejection {"stroke-rejection-profile"} else {"stroke-writer-profile"},
        "gl":context.identity,"width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "history_frames":HISTORY,"warmup_frames":HISTORY,"history":"24 consecutive production ticks, cyclic replay",
        "modes":modes.map(|m|m.name),"geometry":"retained-culled","streams":"bucketed","plan":"prepared",
        "scope":if rejection {"guarded polygon stroke rejection only; stock writer; fills, cached meshes and guards unchanged"} else {"uncached stroke output only; fills and cached template construction stay stock"},
        "timing":"warm CPU mesh encoding; completed path adds GL submission and glFinish; wall and thread CPU, no per-piece clocks except cpu-profiled",
        "simulation_source_projection_text_ui_scanout_timed":false,
        "census":"when output directory is supplied: separate 96-frame diagnostic replay after warmup, outside all timing blocks; fixed-size point/extent/actual-output-clip buckets; nested tessellation clocks include clock overhead; clip-candidate is not proof of visible pixels",
        "validation":if rejection {"independent checked tessellation of every rejection; exact retained triangle order/bits, no visible reference triangles removed; exact GPU pixels and cache decisions across moving, native/odd/native resize, 44/640 zoom and scenario replacement"} else {"exact vertex bits/indices/batches/cache decisions and pixels/draws/uploads across moving, native/odd/native resize, 44/640 zoom, and scenario replacement"}});
    eprintln!("stroke_writer_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let mut census_csv = options
        .geometry_output
        .as_ref()
        .map(|path| File::create_new(path.join("stroke-census.csv")))
        .transpose()?;
    if let Some(file) = &mut census_csv {
        writeln!(file, "{}", Census::HEADER)?;
    }
    println!(
        "fixture,path,mode,repeat,frame,wall_ms,cpu_ms,exact_expand_ms,scalable_expand_ms,fallback_stroke_ms,stroke_prepare_ms,stroke_tessellate_ms,ordinary_ms,validate_ms,vertices,indices,upload_bytes,admissions,bound_tests,bound_unknown,bound_rejections"
    );
    let mut validation = Validation::new(context, rejection)?;
    for fixture in live_retained::FIXTURES {
        let mut scenario = live_retained::scenario(viewport, fixture)?;
        let layout = scenario.frame_layout();
        let history: Vec<_> = (0..HISTORY)
            .map(|_| {
                scenario.step(&[], Duration::from_nanos(16_666_667));
                scenario.render_frames(RenderBackend::Raster, viewport)
            })
            .collect();
        if history.first().unwrap()[0].camera == history.last().unwrap()[0].camera
            && history.first().unwrap()[1].camera == history.last().unwrap()[1].camera
        {
            return Err(format!("{} history has no camera movement", fixture.name).into());
        }
        let build = |frames: &[_], v| {
            presentation::build_reusing(frames, v, layout, 1., true, PlanMode::Prepared, None)
        };
        let plans = history
            .iter()
            .map(|f| build(f, viewport))
            .collect::<Result<Vec<_>, _>>()?;
        for (i, plan) in plans.iter().enumerate() {
            let pixels = validation.check(plan)?;
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
        let mut zoomed = history.last().unwrap().clone();
        for v in [Viewport::new(641., 361.), viewport] {
            for height in [44., 640.] {
                for f in &mut zoomed {
                    f.camera.height = height;
                }
                validation.check(&build(&zoomed, v)?)?;
            }
        }
        eprintln!(
            "validated {}: 28 frames x 2 modes; checked CPU geometry and exact GPU pixels",
            fixture.name
        );
        if let Some(file) = &mut census_csv {
            for control in modes {
                let mut cache = Cache::default();
                let mut mesh = MeshBuilder::default();
                for plan in &plans {
                    encode(plan, &mut cache, &mut mesh, control, false)?;
                }
                mesh.stroke_census = Some(Box::default());
                let mut totals = [0; 4];
                for plan in plans.iter().cycle().take(HISTORY * 4) {
                    let (p, _, _, _) = encode(plan, &mut cache, &mut mesh, control, true)?;
                    for (sum, value) in totals.iter_mut().zip([
                        p.fallback_strokes,
                        p.guarded_stroke.points,
                        p.guarded_stroke.vertices,
                        p.guarded_stroke.indices,
                    ]) {
                        *sum += value;
                    }
                }
                if mesh.stroke_census.as_ref().unwrap().totals() != totals {
                    return Err("stroke census did not account for every guarded stroke".into());
                }
                mesh.stroke_census.as_ref().unwrap().write_csv(
                    file,
                    fixture.name,
                    control.name,
                    HISTORY * 4,
                )?;
            }
        }
        for completed in [false, true] {
            for repeat in 0..options.presentation_repeats as usize {
                let order = if completed {
                    if repeat % 2 == 0 {
                        vec![0, 2]
                    } else {
                        vec![2, 0]
                    }
                } else {
                    ORDER[repeat % ORDER.len()].to_vec()
                };
                for control in order {
                    let index = control / 2;
                    let detailed = control % 2 != 0;
                    let path = if completed {
                        "completed"
                    } else if detailed {
                        "cpu-profiled"
                    } else {
                        "cpu-encode"
                    };
                    let mut cache = Cache::default();
                    let mut mesh = MeshBuilder::default();
                    let mut r = completed
                        .then(|| presenter(context, modes[index]))
                        .transpose()?;
                    for plan in &plans {
                        if let Some(r) = &mut r {
                            r.draw_with_mode(plan, Mode::RetainedCulled)?;
                        } else {
                            encode(plan, &mut cache, &mut mesh, modes[index], detailed)?;
                        }
                    }
                    // SAFETY: Probe owns the current context for its lifetime.
                    unsafe {
                        context.gl.finish();
                    }
                    let mut samples = Vec::new();
                    for i in 0..options.presentation_frames as usize {
                        let plan = &plans[i % HISTORY];
                        let stamp = Stamp::now()?;
                        let (stages, vertices, indices, bytes, stats) = if let Some(r) = &mut r {
                            let out = r.draw_with_mode(plan, Mode::RetainedCulled)?;
                            unsafe {
                                context.gl.finish();
                            }
                            (
                                EncodeProfile::default(),
                                out.mesh.vertices,
                                out.mesh.indices,
                                out.mesh.upload_bytes,
                                out.mesh.retained,
                            )
                        } else {
                            let (p, v, i, s) =
                                encode(plan, &mut cache, &mut mesh, modes[index], detailed)?;
                            (p, v, i, (v * 24 + i * 4) as u64, s)
                        };
                        let (wall, cpu) = stamp.elapsed()?;
                        samples.push((wall, cpu, stages, vertices, indices, bytes, stats));
                    }
                    for (i, (wall, cpu, p, v, ind, bytes, stats)) in samples.into_iter().enumerate()
                    {
                        let times = [
                            p.exact_expand,
                            p.scalable_expand,
                            p.fallback_stroke,
                            p.guarded_stroke.prepare,
                            p.guarded_stroke.stroke,
                            p.ordinary,
                            p.validate,
                        ]
                        .map(|d| format!("{:.6}", d.as_secs_f64() * 1000.))
                        .join(",");
                        println!(
                            "{fixture},{path},{mode},{repeat},{i},{wall:.6},{cpu:.6},{times},{v},{ind},{bytes},{admissions},{tests},{unknown},{rejected}",
                            fixture = fixture.name,
                            mode = modes[index].name,
                            admissions = stats.admissions,
                            tests = stats.stroke_bound_tests,
                            unknown = stats.stroke_bound_unknown,
                            rejected = stats.stroke_bound_rejections,
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
