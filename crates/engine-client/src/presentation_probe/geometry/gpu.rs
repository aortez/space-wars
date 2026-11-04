use super::{
    compare,
    fixtures::{self, Fixture},
    percentile,
};
use crate::{
    gpu_scene::{GeometryMode, TrianglePresenter, plan},
    presentation_probe::Options,
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use glow::HasContext;
use glutin::{
    api::egl::{device::Device, display::Display},
    config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, Version},
    prelude::*,
};
use nix::time::{ClockId, clock_gettime};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::Write,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};
#[path = "admission_check.rs"]
mod admission_check;
#[path = "build_profile.rs"]
mod build_profile;
#[path = "encode_profile.rs"]
mod encode_profile;
#[path = "layouts.rs"]
mod layouts;
#[path = "live_retained.rs"]
mod live_retained;
#[path = "plan_profile.rs"]
mod plan_profile;
#[path = "source_profile.rs"]
mod source_profile;
#[path = "stream_history.rs"]
mod stream_history;
#[path = "stream_input.rs"]
mod stream_input;
#[path = "stroke_profile.rs"]
mod stroke_profile;
#[path = "submit_profile.rs"]
mod submit_profile;
#[path = "terrain.rs"]
mod terrain;
#[path = "timer.rs"]
mod timer;
#[path = "zoom.rs"]
mod zoom;
type Error = Box<dyn std::error::Error>;
const WARMUP: u32 = 16;

struct Context {
    gl: Rc<glow::Context>,
    // Drop GL users before dropping the context that makes their deletion valid.
    current: glutin::api::egl::context::PossiblyCurrentContext,
    display: Display,
    identity: [String; 3],
}
impl Context {
    fn new(node: &Path) -> Result<Self, Error> {
        let device = Device::query_devices()?
            .find(|d| d.drm_render_device_node_path() == Some(node))
            .ok_or_else(|| {
                format!(
                    "EGL did not enumerate render node {}; refusing implicit fallback",
                    node.display()
                )
            })?;
        let display = unsafe { Display::with_device(&device, None)? };
        let template = ConfigTemplateBuilder::new()
            .with_api(Api::GLES2)
            .with_surface_type(ConfigSurfaceTypes::PBUFFER)
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(0)
            .build();
        let config = unsafe { display.find_configs(template)? }
            .min_by_key(|c| c.num_samples())
            .ok_or("no offscreen EGL configuration")?;
        let attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 1))))
            .build(None);
        let current =
            unsafe { display.create_context(&config, &attributes)? }.make_current_surfaceless()?;
        let gl = Rc::new(unsafe {
            glow::Context::from_loader_function_cstr(|n| display.get_proc_address(n))
        });
        let identity = unsafe {
            [
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
            ]
        };
        let renderer = identity[1].to_lowercase();
        // This probe explicitly requests GLES3.1. Record actual stage limits:
        // ES3.1 alone does not guarantee vertex-stage SSBO access.
        unsafe {
            eprintln!(
                "geometry_storage_limits: vertex={}, combined={}, bindings={}, block_bytes={}, alignment={}",
                gl.get_parameter_i32(glow::MAX_VERTEX_SHADER_STORAGE_BLOCKS),
                gl.get_parameter_i32(glow::MAX_COMBINED_SHADER_STORAGE_BLOCKS),
                gl.get_parameter_i32(glow::MAX_SHADER_STORAGE_BUFFER_BINDINGS),
                gl.get_parameter_i64(glow::MAX_SHADER_STORAGE_BLOCK_SIZE),
                gl.get_parameter_i32(glow::SHADER_STORAGE_BUFFER_OFFSET_ALIGNMENT)
            );
        }
        if ["llvmpipe", "softpipe", "swrast"]
            .iter()
            .any(|s| renderer.contains(s))
        {
            return Err(format!("software GL renderer rejected: {}", identity[1]).into());
        }
        Ok(Self {
            gl,
            current,
            display,
            identity,
        })
    }
}

fn cpu_time() -> Result<Duration, Error> {
    let t = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)?;
    Ok(Duration::new(
        t.tv_sec().try_into()?,
        t.tv_nsec().try_into()?,
    ))
}
struct Stamp {
    wall: Instant,
    cpu: Duration,
}
impl Stamp {
    fn now() -> Result<Self, Error> {
        Ok(Self {
            wall: Instant::now(),
            cpu: cpu_time()?,
        })
    }
    fn elapsed(&self) -> Result<(f64, f64), Error> {
        Ok((
            self.wall.elapsed().as_secs_f64() * 1000.0,
            cpu_time()?
                .checked_sub(self.cpu)
                .ok_or("CPU clock reset")?
                .as_secs_f64()
                * 1000.0,
        ))
    }
}
#[derive(Default)]
struct Samples {
    wall: Vec<f64>,
    cpu: Vec<f64>,
}
impl Samples {
    fn record(&mut self, stamp: &Stamp) -> Result<(), Error> {
        let (wall, cpu) = stamp.elapsed()?;
        self.wall.push(wall);
        self.cpu.push(cpu);
        Ok(())
    }
    fn fields(&self, prefix: &str, fields: &mut Vec<(String, String)>) {
        for (name, value) in [
            ("wall_avg", mean(&self.wall)),
            ("cpu_avg", mean(&self.cpu)),
            ("wall_p50", percentile(&self.wall, 50)),
            ("wall_p95", percentile(&self.wall, 95)),
            ("wall_p99", percentile(&self.wall, 99)),
        ] {
            fields.push((format!("{prefix}_{name}_ms"), format!("{value:.6}")));
        }
    }
}
fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

// A bounded queue. Every frame gets a fence; at most `depth` can be in flight.
// Flush starts work without waiting, and only fence waits measure completion.
struct Pending {
    gl: Rc<glow::Context>,
    fence: glow::NativeFence,
    slot: usize,
}
impl Pending {
    fn wait(&self) -> Result<(), Error> {
        let start = Instant::now();
        loop {
            let result = unsafe { self.gl.client_wait_sync(self.fence, 0, 10_000_000) };
            match result {
                glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => return Ok(()),
                glow::TIMEOUT_EXPIRED if start.elapsed() < Duration::from_secs(10) => {}
                _ => {
                    return Err(format!(
                        "geometry completion fence failed/timed out: 0x{result:x}"
                    )
                    .into());
                }
            }
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_sync(self.fence);
        }
    }
}

#[derive(Default)]
struct Block {
    produce: Samples,
    prepare: Samples,
    tessellate: Samples,
    submit: Samples,
    frame: Samples,
    wait_wall: f64,
    wait_cpu: f64,
    wall: f64,
    cpu: f64,
    gpu: Vec<f64>,
    disjoint: bool,
    vertices: usize,
    indices: usize,
    upload: u64,
    samples: i32,
    draws: u64,
    instances: usize,
    template_bytes: usize,
    circle_vertex_work: u64,
    mesh_instances: usize,
    mesh_hits: u64,
    mesh_misses: u64,
    mesh_evictions: u64,
    mesh_fallbacks: u64,
    mesh_entries: usize,
    mesh_bytes: usize,
    expansion_wall: f64,
}

fn drain_one(
    queue: &mut VecDeque<Pending>,
    timer: Option<&timer::Timer>,
    block: &mut Block,
) -> Result<(), Error> {
    let pending = queue.pop_front().ok_or("empty completion queue")?;
    let stamp = Stamp::now()?;
    pending.wait()?;
    let (wall, cpu) = stamp.elapsed()?;
    block.wait_wall += wall;
    block.wait_cpu += cpu;
    if let Some(timer) = timer {
        block.gpu.push(timer.milliseconds(pending.slot));
    }
    Ok(())
}

fn block(
    renderer: &mut TrianglePresenter,
    gl: &Rc<glow::Context>,
    timer: Option<&timer::Timer>,
    fixture: &Fixture,
    viewport: Viewport,
    frames: u32,
    depth: usize,
) -> Result<Block, Error> {
    let mut result = Block::default();
    let mut queue = VecDeque::with_capacity(depth);
    if let Some(timer) = timer {
        timer.disjoint();
    } // Reset invalidation before this block.
    let total = Stamp::now()?;
    for i in 0..frames as usize {
        if queue.len() == depth {
            drain_one(&mut queue, timer, &mut result)?;
        }
        let frame = Stamp::now()?;
        let generated = if fixture.generates_frames() {
            let produce = Stamp::now()?;
            let generated = fixture.generate(i, renderer.geometry_mode().reusable_meshes());
            result.produce.record(&produce)?;
            generated
        } else {
            None
        };
        let prepare = Stamp::now()?;
        let scene = fixture.project(i, viewport, generated.as_ref())?;
        result.prepare.record(&prepare)?;
        let tessellate = Stamp::now()?;
        renderer.prepare(&scene)?;
        result.tessellate.record(&tessellate)?;
        let slot = i % depth;
        if let Some(timer) = timer {
            timer.begin(slot);
        }
        let submit = Stamp::now()?;
        // End an active query even if rendering fails, before unwinding owners.
        let draw = renderer.draw_prepared();
        if let Some(timer) = timer {
            timer.end();
        }
        let output = draw?;
        let fence = unsafe { gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0)? };
        queue.push_back(Pending {
            gl: gl.clone(),
            fence,
            slot,
        });
        unsafe {
            gl.flush();
        }
        result.submit.record(&submit)?;
        result.vertices = output.mesh.vertices;
        result.indices = output.mesh.indices;
        result.upload += output.mesh.upload_bytes;
        result.samples = output.mesh.samples;
        result.draws += output.mesh.draws;
        result.instances = output.mesh.circle_instances;
        result.template_bytes = output.mesh.circle_template_bytes;
        result.circle_vertex_work += output.mesh.circle_vertex_work as u64;
        result.mesh_instances = output.mesh.mesh_instances;
        let cache = output.mesh.reusable;
        result.mesh_hits += cache.hits;
        result.mesh_misses += cache.misses;
        result.mesh_evictions += cache.evictions;
        result.mesh_fallbacks += cache.fallbacks;
        result.mesh_entries = cache.entries;
        result.mesh_bytes = cache.bytes;
        result.expansion_wall += output.mesh.expansion_wall.as_secs_f64() * 1000.0;
        drop(output);
        drop(scene);
        drop(generated);
        result.frame.record(&frame)?;
    }
    while !queue.is_empty() {
        drain_one(&mut queue, timer, &mut result)?;
    }
    (result.wall, result.cpu) = total.elapsed()?;
    result.disjoint = timer.is_some_and(|timer| timer.disjoint());
    if result.disjoint {
        result.gpu.clear();
    }
    let error = unsafe { gl.get_error() };
    if error != glow::NO_ERROR {
        return Err(format!("geometry GL error 0x{error:04x}").into());
    }
    if result.samples != 4 || result.draws < u64::from(frames) {
        return Err("unexpected scene draw/sample count".into());
    }
    Ok(result)
}

fn save_png(
    path: &Path,
    width: u32,
    height: u32,
    channels: png::ColorType,
    pixels: &[u8],
) -> Result<(), Error> {
    let mut encoder = png::Encoder::new(File::create_new(path)?, width, height);
    encoder.set_color(channels);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}
fn rss_kib() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}
fn order(count: usize, repeat: u32) -> Vec<usize> {
    if repeat.is_multiple_of(2) {
        (0..count).collect()
    } else {
        (0..count).rev().collect()
    }
}

fn geometry_mode(name: &str) -> Result<GeometryMode, Error> {
    Ok(match name {
        "triangles" => GeometryMode::Triangles,
        "triangles-split" => GeometryMode::TrianglesSplit,
        "triangles-programs" => GeometryMode::TrianglesPrograms,
        "circles" => GeometryMode::Circles,
        "circles-expanded" => GeometryMode::CirclesExpanded,
        "circles-uniforms" => GeometryMode::CirclesUniforms,
        "circles-bindings" => GeometryMode::CirclesBindings,
        "circles-submission" => GeometryMode::CirclesSubmission,
        "circles-unified" => GeometryMode::CirclesUnified,
        "circles-unified-instanced" => GeometryMode::CirclesUnifiedInstanced,
        "circles-unified-layout" => GeometryMode::CirclesUnifiedLayout,
        "circles-unified-vao" => GeometryMode::CirclesUnifiedVao,
        "meshes-streamed" => GeometryMode::MeshesStreamed,
        "meshes" => GeometryMode::Meshes,
        "meshes-expanded" => GeometryMode::MeshesExpanded,
        _ => return Err(format!("unknown geometry mode: {name}").into()),
    })
}

fn baseline_difference(
    mode: GeometryMode,
    baseline: &[u8],
    rgba: &[u8],
) -> Result<(f64, f64), Error> {
    // Draw splitting is a control, not a new approximation: require exact pixels.
    if mode.split_triangles() && baseline != rgba {
        return Err("split-draw control changed baseline pixels".into());
    }
    let rgb: Vec<u8> = baseline
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    let difference = super::compare(&rgb, rgba)?;
    // Reusing a polygon is not the circle renderer's geometric approximation.
    // Allow tiny local-vs-world tessellation edge differences, not the coarse
    // independent software-reference tolerance used for other experiments.
    if mode.reusable_meshes() && (difference.0 > 0.05 || difference.1 > 0.001) {
        return Err(format!("reusable polygon baseline mismatch: {difference:?}").into());
    }
    Ok(difference)
}

// These modes change submission only. They must match the original instanced
// renderer exactly, not merely pass the coarse triangle/software quality gate.
fn submission_difference(
    renderer: &mut TrianglePresenter,
    scene: &plan::Scene,
    mode: GeometryMode,
    rgba: &[u8],
) -> Result<(f64, f64), Error> {
    if mode.circle_submission_control() {
        renderer.set_geometry_mode(GeometryMode::Circles);
        renderer.draw(scene)?;
        if renderer.readback()? != rgba {
            return Err("circle submission control changed reference pixels".into());
        }
    }
    if mode.reusable_meshes() {
        renderer.set_geometry_mode(GeometryMode::MeshesStreamed);
        renderer.draw(scene)?;
        return mesh_reference_difference(mode, &renderer.readback()?, rgba);
    }
    Ok((0.0, 0.0))
}

fn mesh_reference_difference(
    mode: GeometryMode,
    reference: &[u8],
    actual: &[u8],
) -> Result<(f64, f64), Error> {
    if mode != GeometryMode::MeshesExpanded {
        if reference != actual {
            return Err("retained mesh changed streamed-template reference pixels".into());
        }
        return Ok((0.0, 0.0));
    }
    // Moving transforms from GLSL to Rust can change subpixel rounding (including
    // multiply/add contraction). Keep exact checks for the existing GPU controls;
    // allow only tiny edge differences for CPU expansion, with measured errors.
    let rgb: Vec<_> = reference
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    let difference = super::difference(&rgb, actual)?;
    if difference.0 > 0.01 || difference.1 > 0.0002 {
        return Err(
            format!("expanded polygon differs from instanced reference: {difference:?}").into(),
        );
    }
    Ok(difference)
}

// Frozen production captures alone cannot catch stale per-instance data. Check
// every precomputed animation frame in each selected mode before accepting timings.
fn validate_motion(
    renderer: &mut TrianglePresenter,
    fixture: &Fixture,
    viewport: Viewport,
    modes: &[String],
    output: Option<&Path>,
) -> Result<(), Error> {
    for i in 0..fixture.sequence.len() {
        let scene = fixture.reference_scene(i, viewport)?;
        renderer.set_geometry_mode(GeometryMode::Triangles);
        renderer.draw(&scene)?;
        let baseline = renderer.readback()?;
        for name in modes
            .iter()
            .filter(|s| *s != "triangles" || fixture.local_geometry())
        {
            let mode = geometry_mode(name)?;
            let scene = fixture.scene_mode(i, viewport, mode.reusable_meshes())?;
            renderer.set_geometry_mode(mode);
            renderer.draw(&scene)?;
            let rgba = renderer.readback()?;
            let parity = baseline_difference(
                if fixture.local_geometry() {
                    GeometryMode::Meshes
                } else {
                    mode
                },
                &baseline,
                &rgba,
            )
            .and_then(|_| submission_difference(renderer, &scene, mode, &rgba));
            if let Err(error) = parity {
                if let Some(path) = output {
                    for (mode, pixels) in [("triangles", &baseline), (name.as_str(), &rgba)] {
                        save_png(
                            &path.join(format!("{}-motion-{i}-{mode}-failed.png", fixture.name)),
                            scene.width,
                            scene.height,
                            png::ColorType::Rgba,
                            pixels,
                        )?;
                    }
                }
                return Err(format!("{} {name} animation frame {i}: {error}", fixture.name).into());
            }
        }
    }
    eprintln!(
        "validated {}: {} animated frames, modes={modes:?}",
        fixture.name,
        fixture.sequence.len()
    );
    Ok(())
}

// Exercise program uniforms and VAO ownership across target resize and mode
// switches, before all timers. Readback completes use of the old target before
// its retired owner is collected.
fn validate_submission_resize(
    renderer: &mut TrianglePresenter,
    fixture: &Fixture,
    viewport: Viewport,
    modes: &[String],
) -> Result<(), Error> {
    for scale in [1.0, 0.5, 1.0] {
        let viewport = Viewport::new(viewport.width * scale, viewport.height * scale);
        let scene = fixture.scene(0, viewport)?;
        for name in modes {
            let mode = geometry_mode(name)?;
            if !mode.circle_submission_control()
                && mode != GeometryMode::TrianglesPrograms
                && !mode.reusable_meshes()
            {
                continue;
            }
            renderer.set_geometry_mode(mode);
            renderer.draw(&scene)?;
            let actual = renderer.readback()?;
            renderer.set_geometry_mode(if mode.circle_submission_control() {
                GeometryMode::Circles
            } else if mode.reusable_meshes() {
                GeometryMode::MeshesStreamed
            } else {
                GeometryMode::Triangles
            });
            renderer.draw(&scene)?;
            let reference = renderer.readback()?;
            if mode.reusable_meshes() {
                mesh_reference_difference(mode, &reference, &actual)?;
            } else if reference != actual {
                return Err(
                    format!("{name} changed reference pixels at resize scale {scale}").into(),
                );
            }
            renderer.collect_retired();
        }
    }
    eprintln!("validated submission controls across native/half/native target sizes");
    Ok(())
}

// A separate reference presenter is essential here: rebuilding a reference in
// the retained presenter would erase its history and hide invalidation bugs.
fn validate_retained_motion(
    renderer: &mut TrianglePresenter,
    gl: &Rc<glow::Context>,
    fixture: &Fixture,
    viewport: Viewport,
    mode: GeometryMode,
) -> Result<(), Error> {
    let mut reference = TrianglePresenter::new(gl.clone())?;
    reference.set_geometry_mode(GeometryMode::MeshesStreamed);
    renderer.set_geometry_mode(mode);
    for scale in [1.0, 0.5, 1.0] {
        let viewport = Viewport::new(viewport.width * scale, viewport.height * scale);
        for i in 0..64 {
            let scene = fixture.scene(i, viewport)?;
            let output = renderer.draw(&scene)?;
            let actual = renderer.readback()?;
            reference.draw(&scene)?;
            mesh_reference_difference(mode, &reference.readback()?, &actual).map_err(|error| {
                format!(
                    "{} {mode:?} history at scale {scale}, frame {i}: {error}",
                    fixture.name
                )
            })?;
            if output.mesh.mesh_instances != scene.reusable_polygons()
                || output.mesh.reusable.bytes > crate::gpu_scene::reusable::MAX_BYTES
                || output.mesh.reusable.entries > crate::gpu_scene::reusable::MAX_ENTRIES
                || (mode == GeometryMode::MeshesExpanded && output.mesh.draws != 1)
            {
                return Err(
                    "reusable fixture unexpectedly fell back or exceeded its cache budget".into(),
                );
            }
            renderer.collect_retired();
            reference.collect_retired();
        }
    }
    eprintln!(
        "validated {} {mode:?} history: 192 frames, native/half/native, {} rebuilt-template parity",
        fixture.name,
        if mode == GeometryMode::MeshesExpanded {
            "tight edge-tolerant"
        } else {
            "exact"
        }
    );
    Ok(())
}

pub(in crate::presentation_probe) fn run(
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    // Select GPU before generating expensive fixtures. No settings, sockets or UI.
    let context = Context::new(&options.geometry_device)?;
    let _keep_current = &context.current;
    if options.geometry_admission_check {
        return admission_check::run(&context, options, width, height);
    }
    if options.geometry_stroke_profile {
        return stroke_profile::run(&context, options, width, height);
    }
    if options.geometry_source_profile {
        return source_profile::run(&context, options, width, height);
    }
    if options.geometry_plan_profile {
        return plan_profile::run(&context, options, width, height);
    }
    if options.geometry_stream_history {
        return stream_history::run(&context, options, width, height);
    }
    if options.geometry_layouts {
        return layouts::run(&context, options, width, height);
    }
    if options.geometry_terrain {
        return terrain::run(&context, options, width, height);
    }
    if options.geometry_live_retained {
        return live_retained::run(&context, options, width, height);
    }
    if options.geometry_churn {
        return terrain::run_churn(&context, options, width, height);
    }
    if options.geometry_build_profile {
        return build_profile::run(&context, options, width, height);
    }
    if options.geometry_encode_profile {
        return encode_profile::run(&context, options, width, height);
    }
    if options.geometry_submit_profile {
        return submit_profile::run(&context, options, width, height);
    }
    let mut renderer = TrianglePresenter::new(context.gl.clone())?;
    let timer = timer::Timer::new(
        context.gl.clone(),
        |n| context.display.get_proc_address(n),
        options.geometry_in_flight as usize,
        !options.geometry_no_gpu_timing,
    )?;
    let viewport = Viewport::new(width as f32, height as f32);
    // Pizza's direction fixtures select their seed through real admission and
    // report it per fixture; do not mislabel those captures with the lab default.
    let seed = (!options
        .geometry_cases
        .iter()
        .any(|case| case.starts_with("pizza-event-")))
    .then_some(7);
    let metadata = serde_json::json!({"version": 9, "gl": context.identity, "device": options.geometry_device, "width": width, "height": height, "seed": seed,
        "warmup_frames": WARMUP, "frames": options.presentation_frames, "repeats": options.presentation_repeats, "in_flight": options.geometry_in_flight,
        "cases": options.geometry_cases, "modes": options.geometry_modes, "gpu_queries": timer.is_some(), "simulation_timed": false, "producer_timed_cases": options.geometry_cases.iter().filter(|s| s.starts_with("ships-")).collect::<Vec<_>>(), "fresh_presenter_per_block": true, "text_ui_timed": false, "scanout": false, "paced": false, "gpu_memory_measured": false});
    eprintln!("geometry_probe={metadata}");
    if let Some(path) = &options.geometry_output {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir(path)?; // Existing artifacts must never be overwritten.
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let fixtures = options
        .geometry_cases
        .iter()
        .map(|name| {
            eprintln!("preparing {name}; simulation outside timing");
            fixtures::build(name, viewport)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut checksums = BTreeMap::new();
    for name in options
        .geometry_modes
        .iter()
        .filter(|name| matches!(name.as_str(), "meshes" | "meshes-expanded"))
    {
        for fixture in fixtures
            .iter()
            .filter(|f| f.name.starts_with("meshes-") || f.generates_frames())
        {
            validate_retained_motion(
                &mut renderer,
                &context.gl,
                fixture,
                viewport,
                geometry_mode(name)?,
            )?;
        }
    }
    if options.geometry_modes.iter().any(|name| {
        geometry_mode(name).is_ok_and(|mode| {
            mode.circle_submission_control()
                || mode == GeometryMode::TrianglesPrograms
                || mode.reusable_meshes()
        })
    }) {
        let fixture = fixtures
            .iter()
            .find(|f| f.name == "meshes-interleaved")
            .or_else(|| fixtures.iter().find(|f| f.name == "mixed-interleaved"))
            .or_else(|| fixtures.first())
            .ok_or("no geometry fixtures")?;
        validate_submission_resize(&mut renderer, fixture, viewport, &options.geometry_modes)?;
    }
    if fixtures.iter().any(Fixture::local_geometry)
        || options
            .geometry_modes
            .iter()
            .any(|mode| mode != "triangles")
    {
        for fixture in &fixtures {
            if !fixture.sequence.is_empty() {
                validate_motion(
                    &mut renderer,
                    fixture,
                    viewport,
                    &options.geometry_modes,
                    options.geometry_output.as_deref(),
                )?;
            }
        }
    }
    let mut raster = RasterRenderer::new();
    let mut header: Option<Vec<String>> = None;
    for repeat in 0..options.presentation_repeats {
        for (position, index) in order(fixtures.len(), repeat).into_iter().enumerate() {
            let f = &fixtures[index];
            for mode_index in order(options.geometry_modes.len(), repeat + position as u32) {
                let mode = &options.geometry_modes[mode_index];
                let geometry_mode = geometry_mode(mode)?;
                // References and prior fixtures may populate the retained cache.
                // Start every fixture/mode/repeat with its own presenter, then
                // warm it outside timing. History/invalidation are tested above.
                renderer = TrianglePresenter::new(context.gl.clone())?;
                renderer.set_geometry_mode(geometry_mode);
                let label = format!("{}-{mode}", f.name);
                eprintln!("measuring {label}, repeat {repeat}");
                // All buffers, target allocation, compilation and GPU warm-up excluded.
                block(
                    &mut renderer,
                    &context.gl,
                    timer.as_ref(),
                    f,
                    viewport,
                    WARMUP,
                    options.geometry_in_flight as usize,
                )?;
                let m = block(
                    &mut renderer,
                    &context.gl,
                    timer.as_ref(),
                    f,
                    viewport,
                    options.presentation_frames,
                    options.geometry_in_flight as usize,
                )?;
                let rss = rss_kib();
                let rgba = renderer.readback()?;
                let final_frames = f.at(options.presentation_frames as usize - 1);
                // Direct mode-to-mode comparison is outside all timing, including
                // query collection. It catches order/style errors independently of
                // the software rasterizer's different small-circle edge rounding.
                let baseline_error = if geometry_mode != GeometryMode::Triangles
                    || f.local_geometry()
                {
                    renderer.set_geometry_mode(GeometryMode::Triangles);
                    renderer.draw(
                        &f.reference_scene(options.presentation_frames as usize - 1, viewport)?,
                    )?;
                    let baseline = renderer.readback()?;
                    Some(baseline_difference(
                        if f.local_geometry() {
                            GeometryMode::Meshes
                        } else {
                            geometry_mode
                        },
                        &baseline,
                        &rgba,
                    ))
                } else {
                    None
                };
                let reference = raster
                    .image_from_frames_with_layout(
                        final_frames,
                        viewport,
                        FrameLayout::EqualHorizontal,
                        RasterOptions::default(),
                    )
                    .to_rgb8()
                    .ok_or("raster reference unavailable")?;
                let checksum = fixtures::checksum(&rgba);
                let previous = checksums.insert(label.clone(), checksum);
                // Retain diagnostic captures before rejecting incorrect output.
                if previous != Some(checksum)
                    && let Some(path) = &options.geometry_output
                {
                    let suffix = if previous.is_some() {
                        format!("-repeat-{repeat}")
                    } else {
                        String::new()
                    };
                    save_png(
                        &path.join(format!("{label}{suffix}-gpu.png")),
                        width,
                        height,
                        png::ColorType::Rgba,
                        &rgba,
                    )?;
                    save_png(
                        &path.join(format!("{label}{suffix}-raster.png")),
                        width,
                        height,
                        png::ColorType::Rgb,
                        reference.as_bytes(),
                    )?;
                    File::create_new(path.join(format!("{label}{suffix}-frames.json")))?
                        .write_all(&serde_json::to_vec(final_frames)?)?;
                }
                if previous.is_some_and(|previous| previous != checksum) {
                    return Err(format!("GPU output changed between repeats for {label}").into());
                }
                let baseline_error = baseline_error
                    .transpose()
                    .map_err(|error| format!("{label}: baseline parity failed: {error}"))?;
                let submission_error = if geometry_mode.circle_submission_control()
                    || geometry_mode.reusable_meshes()
                {
                    Some(
                        submission_difference(
                            &mut renderer,
                            &f.scene(options.presentation_frames as usize - 1, viewport)?,
                            geometry_mode,
                            &rgba,
                        )
                        .map_err(|error| format!("{label}: {error}"))?,
                    )
                } else {
                    None
                };
                let (mae, different) = super::difference(reference.as_bytes(), &rgba)?;
                let supersampled = (f.name.starts_with("pizza-") || !f.sequence.is_empty())
                    .then(|| super::reference::circles(final_frames, viewport))
                    .transpose()?;
                if previous.is_none()
                    && let (Some(path), Some(pixels)) = (&options.geometry_output, &supersampled)
                {
                    save_png(
                        &path.join(format!("{label}-raster-4x-reference.png")),
                        width,
                        height,
                        png::ColorType::Rgb,
                        pixels,
                    )?;
                }
                let (reference_mae, reference_different) = compare(
                    supersampled.as_deref().unwrap_or(reference.as_bytes()),
                    &rgba,
                )
                .map_err(|error| format!("{}: {error}", f.name))?;
                let mut fields: Vec<(String, String)> = Vec::new();
                macro_rules! field {
                    ($k:expr, $v:expr) => {
                        fields.push(($k.into(), ($v).to_string()));
                    };
                }
                field!("fixture", &f.name);
                field!("mode", mode);
                field!("producer_timed", f.generates_frames());
                field!(
                    "geometry_input",
                    if f.local_geometry() { "local" } else { "world" }
                );
                field!(
                    "submission_reference_exact",
                    if geometry_mode == GeometryMode::MeshesExpanded {
                        "false"
                    } else if geometry_mode.circle_submission_control()
                        || geometry_mode.reusable_meshes()
                    {
                        "true"
                    } else {
                        ""
                    }
                );
                field!(
                    "submission_reference_rgb_mae",
                    submission_error.map_or(String::new(), |e| format!("{:.6}", e.0))
                );
                field!(
                    "submission_reference_different_pixel_fraction",
                    submission_error.map_or(String::new(), |e| format!("{:.6}", e.1))
                );
                field!("sequence_frames", f.sequence.len().max(1));
                field!(
                    "captured_sequence_frame",
                    (options.presentation_frames as usize - 1) % f.sequence.len().max(1)
                );
                field!("width", width);
                field!("height", height);
                field!("seed", f.seed);
                field!("tick", f.tick);
                field!("objects", f.objects);
                field!(
                    "primitives",
                    f.frames
                        .iter()
                        .flat_map(|f| &f.layers)
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                );
                field!("text_omitted", f.omitted_text);
                field!("input_checksum", format!("{:016x}", f.checksum));
                field!("repeat", repeat);
                field!("order", position);
                field!("frames", options.presentation_frames);
                field!("in_flight", options.geometry_in_flight);
                field!("msaa_samples", m.samples);
                field!("vertices", m.vertices);
                field!("indices", m.indices);
                field!("circle_instances", m.instances);
                field!("mesh_instances", m.mesh_instances);
                field!("mesh_cache_hits", m.mesh_hits);
                field!("mesh_cache_misses", m.mesh_misses);
                field!("mesh_cache_evictions", m.mesh_evictions);
                field!("mesh_cache_fallbacks", m.mesh_fallbacks);
                field!("resident_mesh_entries", m.mesh_entries);
                field!("resident_mesh_bytes", m.mesh_bytes);
                field!(
                    "mesh_expansion_wall_avg_ms",
                    format!(
                        "{:.6}",
                        m.expansion_wall / f64::from(options.presentation_frames)
                    )
                );
                field!("resident_circle_template_bytes", m.template_bytes);
                field!(
                    "circle_template_vertices_per_frame",
                    format!(
                        "{:.3}",
                        m.circle_vertex_work as f64 / f64::from(options.presentation_frames)
                    )
                );
                field!(
                    "scene_draws_per_frame",
                    format!(
                        "{:.3}",
                        m.draws as f64 / f64::from(options.presentation_frames)
                    )
                );
                field!(
                    "geometry_upload_bytes",
                    format!(
                        "{:.3}",
                        m.upload as f64 / f64::from(options.presentation_frames)
                    )
                );
                field!(
                    "nominal_attachment_bytes",
                    u64::from(width) * u64::from(height) * 4 * (m.samples as u64 + 1)
                );
                field!("process_rss_kib", rss);
                m.produce.fields("produce", &mut fields);
                m.prepare.fields("prepare", &mut fields);
                m.tessellate.fields("tessellate", &mut fields);
                m.submit.fields("submit", &mut fields);
                m.frame.fields("cpu_frame", &mut fields);
                field!(
                    "queue_wait_wall_avg_ms",
                    format!(
                        "{:.6}",
                        m.wait_wall / f64::from(options.presentation_frames)
                    )
                );
                field!(
                    "queue_wait_cpu_avg_ms",
                    format!("{:.6}", m.wait_cpu / f64::from(options.presentation_frames))
                );
                field!(
                    "completed_wall_avg_ms",
                    format!("{:.6}", m.wall / f64::from(options.presentation_frames))
                );
                field!(
                    "total_thread_cpu_avg_ms",
                    format!("{:.6}", m.cpu / f64::from(options.presentation_frames))
                );
                field!(
                    "completed_frames_per_sec",
                    format!(
                        "{:.3}",
                        f64::from(options.presentation_frames) * 1000.0 / m.wall
                    )
                );
                field!("gpu_samples", m.gpu.len());
                field!("gpu_disjoint", m.disjoint);
                for (name, value) in [
                    ("avg", mean(&m.gpu)),
                    ("p50", percentile(&m.gpu, 50)),
                    ("p95", percentile(&m.gpu, 95)),
                    ("p99", percentile(&m.gpu, 99)),
                ] {
                    field!(
                        format!("gpu_{name}_ms"),
                        if m.gpu.is_empty() {
                            String::new()
                        } else {
                            format!("{value:.6}")
                        }
                    );
                }
                field!("rgb_mae", format!("{mae:.6}"));
                field!("different_pixel_fraction", format!("{different:.6}"));
                field!(
                    "reference_scale",
                    if supersampled.is_some() { 4 } else { 1 }
                );
                field!("reference_rgb_mae", format!("{reference_mae:.6}"));
                field!(
                    "reference_different_pixel_fraction",
                    format!("{reference_different:.6}")
                );
                field!("gpu_checksum", format!("{checksum:016x}"));
                field!(
                    "baseline_rgb_mae",
                    baseline_error.map_or(String::new(), |e| format!("{:.6}", e.0))
                );
                field!(
                    "baseline_different_pixel_fraction",
                    baseline_error.map_or(String::new(), |e| format!("{:.6}", e.1))
                );
                let columns = fields.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
                if let Some(expected) = &header {
                    assert_eq!(expected, &columns);
                } else {
                    println!("{}", columns.join(","));
                    header = Some(columns);
                }
                println!(
                    "{}",
                    fields
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect::<Vec<_>>()
                        .join(",")
                );
                std::io::stdout().flush()?;
            }
        }
    }
    Ok(()) // Timer, renderer then current context drop, in that order.
}

#[cfg(test)]
mod tests {
    #[test]
    fn expanded_mesh_reference_allows_tiny_edges_but_keeps_other_modes_exact() {
        use super::*;
        assert_eq!(
            geometry_mode("meshes-expanded").unwrap(),
            GeometryMode::MeshesExpanded
        );
        let reference = [30, 40, 50, 255].repeat(1024);
        let mut edge = reference.clone();
        edge[0] += 1;
        assert!(mesh_reference_difference(GeometryMode::MeshesExpanded, &reference, &edge).is_ok());
        assert!(mesh_reference_difference(GeometryMode::Meshes, &reference, &edge).is_err());
        edge[..16].fill(255);
        assert!(
            mesh_reference_difference(GeometryMode::MeshesExpanded, &reference, &edge).is_err()
        );
        assert!(mesh_reference_difference(GeometryMode::MeshesExpanded, &reference, &[]).is_err());
        let mut alpha = reference.clone();
        alpha[3] = 0;
        assert!(
            mesh_reference_difference(GeometryMode::MeshesExpanded, &reference, &alpha).is_err()
        );
    }

    #[test]
    fn draw_split_control_requires_pixel_exactness() {
        use super::*;
        let rgba = [1, 2, 3, 255];
        assert_eq!(
            baseline_difference(GeometryMode::TrianglesSplit, &rgba, &rgba).unwrap(),
            (0., 0.)
        );
        assert!(baseline_difference(GeometryMode::TrianglesSplit, &rgba, &[1, 2, 4, 255]).is_err());
        assert!(
            baseline_difference(GeometryMode::TrianglesPrograms, &rgba, &[1, 2, 4, 255]).is_err()
        );
        assert_eq!(
            geometry_mode("circles-expanded").unwrap(),
            GeometryMode::CirclesExpanded
        );
        assert!(geometry_mode("unexpected").is_err());
    }

    use super::*;
    #[test]
    fn repeat_order_brackets_every_fixture() {
        assert_eq!(order(3, 0), [0, 1, 2]);
        assert_eq!(order(3, 1), [2, 1, 0]);
        assert_eq!(order(3, 2), order(3, 0));
    }
}
