//! Attribute CPU time spent inside GL submission, buffer identities and capacities.
//! Stage clocks do not measure GPU-active time and add no intermediate GL waits.
use super::{Context, Error, Stamp, live_retained, save_png};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        CompositePresenter, Mode, presentation,
        streaming::{self, Policy as StreamPolicy},
        submit_profile::{NAMES, Profile, balanced_order},
    },
    presentation_probe::{Options, geometry},
    render::Viewport,
};
use glow::HasContext;
use std::{
    fs::File,
    io::Write,
    time::{Duration, Instant},
};

const STARTUP: u32 = 8;
const STEP: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy)]
struct Config {
    name: &'static str,
    mode: Mode,
    streams: StreamPolicy,
    detailed: bool,
}

const CONFIGS: [Config; 16] = [
    Config {
        name: "ranges",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::SharedExact,
        detailed: false,
    },
    Config {
        name: "ranges-profiled",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::SharedExact,
        detailed: true,
    },
    Config {
        name: "culled",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::SharedExact,
        detailed: false,
    },
    Config {
        name: "culled-profiled",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::SharedExact,
        detailed: true,
    },
    Config {
        name: "ranges-passes",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassExact,
        detailed: false,
    },
    Config {
        name: "ranges-passes-profiled",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassExact,
        detailed: true,
    },
    Config {
        name: "culled-passes",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassExact,
        detailed: false,
    },
    Config {
        name: "culled-passes-profiled",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassExact,
        detailed: true,
    },
    Config {
        name: "ranges-bucketed",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassBucketed,
        detailed: false,
    },
    Config {
        name: "ranges-bucketed-profiled",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassBucketed,
        detailed: true,
    },
    Config {
        name: "culled-bucketed",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassBucketed,
        detailed: false,
    },
    Config {
        name: "culled-bucketed-profiled",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassBucketed,
        detailed: true,
    },
    Config {
        name: "ranges-split",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassSplitExact,
        detailed: false,
    },
    Config {
        name: "ranges-split-profiled",
        mode: Mode::RetainedRanges,
        streams: StreamPolicy::PassSplitExact,
        detailed: true,
    },
    Config {
        name: "culled-split",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassSplitExact,
        detailed: false,
    },
    Config {
        name: "culled-split-profiled",
        mode: Mode::RetainedCulled,
        streams: StreamPolicy::PassSplitExact,
        detailed: true,
    },
];

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let validation_configs = if options.geometry_submit_api {
        &CONFIGS[4..]
    } else if options.geometry_submit_capacity {
        &CONFIGS[4..12]
    } else {
        &CONFIGS[..if options.geometry_submit_passes { 8 } else { 4 }]
    };
    let configs = validation_configs
        .iter()
        .copied()
        .filter(|c| !options.geometry_submit_plain_only || !c.detailed)
        .collect::<Vec<_>>();
    let orders = (0..options.presentation_repeats)
        .map(|r| balanced_order(configs.len(), r as usize))
        .collect::<Vec<_>>();
    let metadata = serde_json::json!({"version":3,"kind":"submission-profile", "gl":context.identity,
        "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "modes":configs.iter().map(|c|c.name).collect::<Vec<_>>(),"startup_frames":STARTUP,"in_flight":1,
        "validation_modes":validation_configs.iter().map(|c|c.name).collect::<Vec<_>>(),
        "plain_only":options.geometry_submit_plain_only,
        "timing":"source, projection, encode, submission through glFinish; simulation, text/UI and scanout excluded",
        "stages":"CPU wall time around GL API calls, not GPU-active time; no intermediate synchronization; clocks compiled out of plain controls",
        "first_use":"fresh geometry cache; GPU targets and stream objects prewarmed; not process startup",
        "buffer_control":"same vertex format/shader and actual uploaded bytes; exact buffer_data(slice), exact buffer_data(NULL used)+subdata(used), and bucketed buffer_data(NULL capacity)+subdata(used); bucketed capacity is grow-only power-of-two, minimum 4096 per nonempty buffer; 8 pairs/16 MiB reserved extra stores maximum, shared exact fallback",
        "storage_metrics":"buffer_store_bytes sums requested storage per frame; stream_capacity_bytes is end-of-frame logical capacity high-water; neither measures actual driver allocation or peak in-flight memory; upload_bytes counts actual geometry data only",
        "stage_comparison":"combined upload stages include storage+data; exact-split and bucketed split reserve from subdata upload: add reserve+upload to compare",
        "ordering":"Williams balanced Latin square: positions and within-row ordered predecessors balanced over each full mode-count repeats; cross-row transitions and longer driver history NOT controlled; no cache reset",
        "complete_order_cycles":(options.presentation_repeats as usize).is_multiple_of(configs.len()),
        "orders":orders.iter().map(|row|row.iter().map(|i| configs[*i].name).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "validation":"exact RGBA and geometry/upload/draw counts against same-policy shared-buffer path; resize, replacement, >8 passes, empty frames, mode changes, teardown and foreign VAO/VBO/EBO"});
    eprintln!("submit_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    validate(context, options, v, validation_configs)?;
    let mut trace = options
        .geometry_output
        .as_ref()
        .map(|p| File::create_new(p.join("frames.csv")))
        .transpose()?;
    let stages = NAMES.join(",");
    let streams = streaming::NAMES.join(",");
    if let Some(trace) = &mut trace {
        writeln!(
            trace,
            "fixture,mode,repeat,order,previous_mode,tick,startup,wall_ms,cpu_ms,source_ms,project_ms,encode_ms,submit_ms,finish_ms,{stages},vertices,indices,upload_bytes,draws,stream_slots,stream_capacity_bytes,{streams}"
        )?;
    }
    println!(
        "fixture,mode,repeat,order,previous_mode,frames,wall_ms,cpu_ms,source_ms,project_ms,encode_ms,submit_ms,finish_ms,{stages},p95_ms,max_ms,first_ms,startup_total_ms,vertices,indices,upload_bytes,draws,stream_slots,stream_capacity_bytes,{streams},admissions,promotions,cache_bytes"
    );
    let mut previous_mode = "validation";
    for fixture in live_retained::FIXTURES {
        for (repeat, row) in orders.iter().enumerate() {
            for (order, index) in row.iter().enumerate() {
                let config = configs[*index];
                let mut renderer = CompositePresenter::new(context.gl.clone())?;
                renderer.set_stream_policy(config.streams);
                // Match the live replay: disposable ordinary geometry allocates
                // the destinations; all measured blocks start with an empty cache.
                {
                    let s = live_retained::scenario(v, fixture)?;
                    let frames = s.render_frames(RenderBackend::Raster, v);
                    let plan =
                        presentation::build_with_retained(&frames, v, s.frame_layout(), 1., false)?;
                    renderer.draw_with_mode(&plan, Mode::Triangles)?;
                    unsafe {
                        context.gl.finish();
                    }
                }
                renderer.clear_retained();
                let mut s = live_retained::scenario(v, fixture)?;
                let mut sums = [0.; 7];
                let mut profile = Profile::default();
                let mut streaming = streaming::Stats::default();
                let mut walls = Vec::new();
                let (mut first, mut startup) = (0., 0.);
                let (mut counts, mut peak_usage) = ([0u64; 4], (0, 0));
                let mut cache = crate::gpu_scene::retained::Stats::default();
                for tick in 0..STARTUP + options.presentation_frames {
                    s.step(&[], STEP);
                    let total = Stamp::now()?;
                    let began = Instant::now();
                    let frames = s.render_frames(RenderBackend::Raster, v);
                    let source = began.elapsed();
                    let began = Instant::now();
                    let plan =
                        presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
                    let project = began.elapsed();
                    let output = if config.detailed {
                        renderer.draw_profiled::<true>(&plan, config.mode)?
                    } else {
                        renderer.draw_with_mode(&plan, config.mode)?
                    };
                    let began = Instant::now();
                    unsafe {
                        context.gl.finish();
                    }
                    let finish = began.elapsed();
                    drop(plan);
                    drop(frames);
                    let (wall, cpu) = total.elapsed()?;
                    renderer.collect_retired();
                    let usage = renderer.stream_usage();
                    let times = [
                        wall,
                        cpu,
                        ms(source),
                        ms(project),
                        ms(output.encode),
                        ms(output.submit),
                        ms(finish),
                    ];
                    let c = [
                        output.mesh.vertices as u64,
                        output.mesh.indices as u64,
                        output.mesh.upload_bytes,
                        output.mesh.draws,
                    ];
                    if !config.detailed && output.mesh.submission.0.iter().any(|d| !d.is_zero()) {
                        return Err("unprofiled submission recorded stage clocks".into());
                    }
                    if tick == 0 {
                        first = wall;
                    }
                    if tick < STARTUP {
                        startup += wall;
                    } else {
                        for (a, b) in sums.iter_mut().zip(times) {
                            *a += b;
                        }
                        for (a, b) in counts.iter_mut().zip(c) {
                            *a += b;
                        }
                        profile.accumulate(output.mesh.submission);
                        streaming.accumulate(output.mesh.streaming);
                        cache.accumulate(output.mesh.retained);
                        peak_usage = (peak_usage.0.max(usage.0), peak_usage.1.max(usage.1));
                        walls.push(wall);
                    }
                    if let Some(trace) = &mut trace {
                        writeln!(
                            trace,
                            "{},{},{repeat},{order},{previous_mode},{tick},{},{},{},{},{},{},{}",
                            fixture.name,
                            config.name,
                            tick < STARTUP,
                            csv(times),
                            csv(output.mesh.submission.0.map(ms)),
                            c.map(|n| n.to_string()).join(","),
                            usage.0,
                            usage.1,
                            output.mesh.streaming.0.map(|n| n.to_string()).join(",")
                        )?;
                    }
                }
                let n = f64::from(options.presentation_frames);
                println!(
                    "{},{},{repeat},{order},{previous_mode},{},{},{},{:.6},{:.6},{first:.6},{startup:.6},{},{},{},{},{},{},{}",
                    fixture.name,
                    config.name,
                    options.presentation_frames,
                    csv(sums.map(|v| v / n)),
                    csv(profile.0.map(|d| ms(d) / n)),
                    geometry::percentile(&walls, 95),
                    geometry::percentile(&walls, 100),
                    csv(counts.map(|v| v as f64 / n)),
                    peak_usage.0,
                    peak_usage.1,
                    csv(streaming.0.map(|v| v as f64 / n)),
                    cache.admissions,
                    cache.promotions,
                    cache.bytes
                );
                previous_mode = config.name;
            }
        }
    }
    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}
fn csv<const N: usize>(v: [f64; N]) -> String {
    v.map(|x| format!("{x:.6}")).join(",")
}

fn validate(
    context: &Context,
    options: &Options,
    viewport: Viewport,
    configs: &[Config],
) -> Result<(), Error> {
    let g = &context.gl;
    let foreign = ForeignStreams::new(g)?;
    let mut reference = [
        CompositePresenter::new(g.clone())?,
        CompositePresenter::new(g.clone())?,
    ];
    let mut tested = configs
        .iter()
        .map(|_| CompositePresenter::new(g.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let (mut checks, mut overflow) = (0, 0);
    for fixture in live_retained::FIXTURES {
        for (resize, v) in [viewport, Viewport::new(641., 361.), viewport]
            .into_iter()
            .enumerate()
        {
            let mut s = live_retained::scenario(v, fixture)?;
            for tick in 0..24 {
                s.step(&[], STEP);
                let frames = s.render_frames(RenderBackend::Raster, v);
                let mut plan =
                    presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
                if tick == 13 {
                    let extra =
                        presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
                    plan.passes.extend(extra.passes);
                    if plan.passes.len() <= 8 {
                        return Err("overflow fixture needs more than eight passes".into());
                    }
                    overflow += 1;
                }
                if tick == 16 {
                    plan.passes.clear();
                }
                let mut pixels = Vec::new();
                let mut counts = Vec::new();
                for (r, mode) in reference
                    .iter_mut()
                    .zip([Mode::RetainedRanges, Mode::RetainedCulled])
                {
                    let o = r.draw_with_mode(&plan, mode)?;
                    pixels.push(r.readback()?);
                    counts.push((
                        o.mesh.vertices,
                        o.mesh.indices,
                        o.mesh.upload_bytes,
                        o.mesh.draws,
                    ));
                    r.collect_retired();
                }
                for (r, c) in tested.iter_mut().zip(configs) {
                    r.set_stream_policy(if tick == 20 {
                        StreamPolicy::SharedExact
                    } else {
                        c.streams
                    });
                    let o = if c.detailed {
                        r.draw_profiled::<true>(&plan, c.mode)?
                    } else {
                        r.draw_with_mode(&plan, c.mode)?
                    };
                    let actual = r.readback()?;
                    let i = usize::from(c.mode == Mode::RetainedCulled);
                    if actual != pixels[i]
                        || (
                            o.mesh.vertices,
                            o.mesh.indices,
                            o.mesh.upload_bytes,
                            o.mesh.draws,
                        ) != counts[i]
                    {
                        return Err(format!(
                            "submission parity failed {} {} resize={resize} tick={tick}",
                            fixture.name, c.name
                        )
                        .into());
                    }
                    let (slots, bytes) = r.stream_usage();
                    if slots > 8 || bytes > 16 * 1024 * 1024 + 36_000_000 {
                        return Err("stream pool exceeded slot/payload bounds".into());
                    }
                    if c.streams == StreamPolicy::SharedExact || tick == 20 {
                        if slots != 0 {
                            return Err("disabled stream pool retained objects".into());
                        }
                    } else if tick == 13 && o.mesh.streaming.0[4] == 0 {
                        return Err("overflow fixture did not exercise shared fallback".into());
                    }
                    r.collect_retired();
                    foreign.check(g)?;
                    checks += 1;
                    if resize == 0
                        && tick == 23
                        && !c.detailed
                        && let Some(path) = &options.geometry_output
                    {
                        save_png(
                            &path.join(format!("{}-{}.png", fixture.name, c.name)),
                            v.width as u32,
                            v.height as u32,
                            png::ColorType::Rgba,
                            &actual,
                        )?;
                    }
                }
            }
        }
    }
    for r in &mut tested {
        r.retire();
        r.collect_retired();
    }
    drop(tested);
    drop(reference);
    foreign.check(g)?;
    eprintln!(
        "validated submission exact_image_and_geometry_checks={checks} overflow_histories={overflow} foreign_stream_bindings=preserved"
    );
    Ok(())
}

/// A deliberately foreign VAO owns its EBO. Every draw, pool toggle and teardown
/// must preserve it, as well as the independent ARRAY_BUFFER binding.
pub(super) struct ForeignStreams<'a> {
    gl: &'a glow::Context,
    old_vao: Option<glow::NativeVertexArray>,
    old_array: Option<glow::NativeBuffer>,
    vao: Option<glow::NativeVertexArray>,
    array: Option<glow::NativeBuffer>,
    element: Option<glow::NativeBuffer>,
}
impl<'a> ForeignStreams<'a> {
    pub(super) fn new(g: &'a glow::Context) -> Result<Self, Error> {
        unsafe {
            let mut r = Self {
                gl: g,
                old_vao: g.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                old_array: g.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
                vao: None,
                array: None,
                element: None,
            };
            r.vao = Some(g.create_vertex_array()?);
            r.array = Some(g.create_buffer()?);
            r.element = Some(g.create_buffer()?);
            g.bind_vertex_array(r.vao);
            g.bind_buffer(glow::ARRAY_BUFFER, r.array);
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, r.element);
            Ok(r)
        }
    }
    pub(super) fn check(&self, g: &glow::Context) -> Result<(), Error> {
        unsafe {
            if g.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING) != self.vao
                || g.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING) != self.array
                || g.get_parameter_buffer(glow::ELEMENT_ARRAY_BUFFER_BINDING) != self.element
            {
                return Err("submission leaked foreign VAO/VBO/EBO bindings".into());
            }
        }
        Ok(())
    }
}
impl Drop for ForeignStreams<'_> {
    fn drop(&mut self) {
        unsafe {
            self.gl.bind_vertex_array(self.old_vao);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, self.old_array);
            if let Some(v) = self.vao {
                self.gl.delete_vertex_array(v);
            }
            for b in [self.array, self.element].into_iter().flatten() {
                self.gl.delete_buffer(b);
            }
        }
    }
}
