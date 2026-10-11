//! First-use Slint/FemtoVG text cache lab. A private pbuffer, never scanout.
//! Full-output equality checks and PNG writes are outside all frame timers.
//! Row preparation is an experimental cost shift, not a production prewarm.
use super::Options;
use glow::HasContext;
use glutin::{
    api::egl::{
        context::PossiblyCurrentContext, device::Device, display::Display, surface::Surface,
    },
    config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, Version},
    prelude::*,
    surface::{PbufferSurface, SurfaceAttributesBuilder},
};
use i_slint_renderer_femtovg::{
    FemtoVGOpenGLRendererExt, FemtoVGRenderer, FemtoVGRendererExt,
    opengl::{OpenGLBackend, OpenGLInterface},
};
use nix::time::{ClockId, clock_gettime};
use slint::{
    ComponentHandle, ModelRc, PhysicalSize, VecModel, Window, WindowSize,
    platform::{Platform, PlatformError, Renderer, WindowAdapter, WindowEvent},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{CStr, c_void},
    fs::File,
    num::NonZeroU32,
    path::Path,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
mod counters;
mod detail;
mod masks;
mod shaping;
type Error = Box<dyn std::error::Error>;
type GlError = Box<dyn std::error::Error + Send + Sync>;
type UiRenderer = FemtoVGRenderer<OpenGLBackend>;

slint::slint! {
    export struct TextRow { label: string, size: float, weight: int }
    export component TextCacheWindow inherits Window {
        in property <[TextRow]> rows;
        in property <int> only-row: -1;
        in property <float> item-shift;
        in property <float> layout-width-delta;
        in property <float> size-delta;
        background: #080a12;
        for row[index] in root.rows: Text {
            visible: root.only-row < 0 || root.only-row == index;
            x: (root.width - 560px) / 2 + root.item-shift * 1px;
            y: (root.height - 420px) / 2 + index * 28px;
            width: (560 + root.layout-width-delta) * 1px;
            height: 34px;
            text: row.label;
            font-family: "Liberation Sans";
            font-size: (row.size + root.size-delta) * 1px;
            font-weight: row.weight;
            horizontal-alignment: center;
            color: index == 0 ? #ffffff : #80d8ff;
        }
    }
}

fn rows(round: u32) -> Vec<TextRow> {
    [
        ("Round Complete".to_owned(), 26., 700),
        ("Player 2 wins: opposing pilot lost".into(), 17., 400),
        (format!("Round {round}"), 14., 400),
        ("SESSION SCORE   Wins   Losses   Draws".into(), 13., 400),
        ("Player 1".into(), 18., 700),
        ("Human".into(), 13., 400),
        (format!("0     {round}     0"), 25., 400),
        ("Player 2".into(), 18., 700),
        ("Legacy bot v9".into(), 13., 400),
        (format!("{round}     0     0"), 25., 700),
        ("Planets: P1 0 | P2 2 | Neutral 10".into(), 13., 400),
        ("World seed: 0".into(), 13., 400),
        ("Rematch        New Match        Launcher".into(), 16., 400),
        (
            "A selects · Start rematches · Launcher resets scores".into(),
            12.,
            400,
        ),
    ]
    .into_iter()
    .map(|(label, size, weight)| TextRow {
        label: label.into(),
        size,
        weight,
    })
    .collect()
}

// Only one current context exists in this probe. All UI canvases are destroyed
// while it is current; the platform retains it until process teardown.
pub(super) struct Context {
    pub(super) gl: glow::Context,
    current: PossiblyCurrentContext,
    surface: Surface<PbufferSurface>,
    display: Display,
    pub(super) size: PhysicalSize,
}
impl Context {
    pub(super) fn proc_address(&self, name: &CStr) -> *const c_void {
        self.display.get_proc_address(name)
    }
    pub(super) fn new(node: &Path, size: PhysicalSize) -> Result<Rc<Self>, Error> {
        let device = Device::query_devices()?
            .find(|d| d.drm_render_device_node_path() == Some(node))
            .ok_or_else(|| format!("No EGL device for {}; refusing fallback", node.display()))?;
        // SAFETY: Explicit render node with a private pbuffer, no native window.
        let display = unsafe { Display::with_device(&device, None)? };
        let template = ConfigTemplateBuilder::new()
            .with_api(Api::GLES2)
            .with_surface_type(ConfigSurfaceTypes::PBUFFER)
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(8)
            .build();
        let config = unsafe { display.find_configs(template)? }
            .min_by_key(|c| c.num_samples())
            .ok_or("no pbuffer configuration")?;
        let attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(2, 0))))
            .build(None);
        let surface_attributes = SurfaceAttributesBuilder::<PbufferSurface>::new()
            .with_single_buffer(true)
            .build(size.width.try_into()?, size.height.try_into()?);
        let surface = unsafe { display.create_pbuffer_surface(&config, &surface_attributes)? };
        let current =
            unsafe { display.create_context(&config, &attributes)? }.make_current(&surface)?;
        let gl =
            unsafe { glow::Context::from_loader_function_cstr(|n| display.get_proc_address(n)) };
        let identity = unsafe {
            [
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
            ]
        };
        if ["llvmpipe", "softpipe", "swrast"]
            .iter()
            .any(|s| identity[1].to_lowercase().contains(s))
        {
            return Err(format!("software GL rejected: {}", identity[1]).into());
        }
        eprintln!(
            "gpu_text_probe_version=5 gl={identity:?} node={} size={}x{} scale=1 samples={} offscreen=true paced=false cpu_timing=true gpu_time=false",
            node.display(),
            size.width,
            size.height,
            config.num_samples()
        );
        Ok(Rc::new(Self {
            gl,
            current,
            surface,
            display,
            size,
        }))
    }
    pub(super) fn check(&self) -> Result<(), Error> {
        let code = unsafe { self.gl.get_error() };
        if code != glow::NO_ERROR {
            return Err(format!("probe GL error 0x{code:x}").into());
        }
        Ok(())
    }
    pub(super) fn pixels(&self) -> Result<Vec<u8>, Error> {
        let stride = self.size.width as usize * 4;
        let mut bottom_up = vec![0; stride * self.size.height as usize];
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.gl.read_pixels(
                0,
                0,
                self.size.width as i32,
                self.size.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut bottom_up)),
            );
        }
        self.check()?;
        let mut pixels = Vec::with_capacity(bottom_up.len());
        for row in bottom_up.chunks_exact(stride).rev() {
            pixels.extend_from_slice(row);
        }
        Ok(pixels)
    }
}
struct Interface(Rc<Context>);
// SAFETY: One current EGL context, retained for all renderer users and pointer calls.
unsafe impl OpenGLInterface for Interface {
    fn ensure_current(&self) -> Result<(), GlError> {
        self.0.current.make_current(&self.0.surface)?;
        unsafe {
            self.0.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Ok(())
    }
    fn swap_buffers(&self) -> Result<(), GlError> {
        // A pbuffer, never displayed. Completion is separately timed by the probe.
        unsafe {
            self.0.gl.flush();
        }
        Ok(())
    }
    fn resize(&self, w: NonZeroU32, h: NonZeroU32) -> Result<(), GlError> {
        // A rotated fixture has transposed logical window dimensions, while
        // the private pbuffer remains in its original physical orientation.
        if (w.get(), h.get()) != (self.0.size.width, self.0.size.height)
            && (w.get(), h.get()) != (self.0.size.height, self.0.size.width)
        {
            return Err("text probe does not resize its pbuffer".into());
        }
        Ok(())
    }
    fn get_proc_address(&self, name: &CStr) -> *const c_void {
        unsafe { counters::wrap(name, self.0.display.get_proc_address(name)) }
    }
}
pub(super) struct ProbeWindow {
    window: Window,
    pub(super) renderer: UiRenderer,
    size: Cell<PhysicalSize>,
}
impl WindowAdapter for ProbeWindow {
    fn window(&self) -> &Window {
        &self.window
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn set_size(&self, size: WindowSize) {
        self.size.set(size.to_physical(1.));
        self.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(1.),
        });
    }
}
pub(super) struct ProbePlatform {
    pub(super) context: Rc<Context>,
    pub(super) last: Rc<RefCell<Weak<ProbeWindow>>>,
}
impl Platform for ProbePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let renderer = UiRenderer::new_suspended();
        renderer.set_opengl_context(Interface(self.context.clone()))?;
        let window = Rc::new_cyclic(|weak: &Weak<ProbeWindow>| ProbeWindow {
            window: Window::new(weak.clone()),
            renderer,
            size: Cell::new(self.context.size),
        });
        *self.last.borrow_mut() = Rc::downgrade(&window);
        Ok(window)
    }
}

fn cpu() -> Result<Duration, Error> {
    let t = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)?;
    Ok(Duration::new(
        t.tv_sec().try_into()?,
        t.tv_nsec().try_into()?,
    ))
}
#[derive(Clone, Copy, Default)]
struct Time {
    wall: f64,
    cpu: f64,
}
struct Stamp {
    wall: Instant,
    cpu: Duration,
}
impl Stamp {
    fn now() -> Result<Self, Error> {
        Ok(Self {
            wall: Instant::now(),
            cpu: cpu()?,
        })
    }
    fn elapsed(&self) -> Result<Time, Error> {
        Ok(Time {
            wall: self.wall.elapsed().as_secs_f64() * 1000.,
            cpu: cpu()?
                .checked_sub(self.cpu)
                .ok_or("CPU clock reset")?
                .as_secs_f64()
                * 1000.,
        })
    }
}
#[derive(Debug)]
struct CacheWork {
    masks: u64,
    face_parses: u64,
    uploads: u64,
    upload_bytes: u64,
}
type Measurement = (
    Time,
    Time,
    Time,
    counters::Counts,
    CacheWork,
    detail::Totals,
);
fn measure(window: &ProbeWindow, context: &Context) -> Result<Measurement, Error> {
    counters::reset();
    let detail = detail::Capture::begin();
    let before = femtovg::glyph_cache_stats();
    let start = Stamp::now()?;
    let build = Cell::new(None);
    window.renderer.render_transformed_with_post_callback(
        0.,
        (0., 0.),
        context.size,
        Some(&|_| {
            build.set(Some(start.elapsed()));
        }),
    )?;
    let draw = start.elapsed()?;
    let build = build.take().ok_or("post-item callback not invoked")??;
    let drain = Stamp::now()?;
    // Deliberate per-frame completion in this offscreen latency experiment.
    // This is not added to live rendering or a queued-throughput benchmark.
    unsafe {
        context.gl.finish();
    }
    let drain = drain.elapsed()?;
    let counts = counters::counts();
    context.check()?;
    let after = femtovg::glyph_cache_stats();
    Ok((
        draw,
        build,
        drain,
        counts,
        CacheWork {
            masks: after.rasterized - before.rasterized,
            face_parses: after.face_parses - before.face_parses,
            uploads: after.uploads - before.uploads,
            upload_bytes: after.uploaded_bytes - before.uploaded_bytes,
        },
        detail.finish(),
    ))
}
fn report(repeat: u32, mode: &str, stage: &str, m: &Measurement) {
    let (draw, build, drain, c, cache, detail) = m;
    print!(
        "{repeat},{mode},{stage},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{},{},{},{},{}",
        draw.wall,
        draw.cpu,
        build.wall,
        build.cpu,
        drain.wall,
        drain.cpu,
        draw.wall + drain.wall,
        draw.cpu + drain.cpu,
        c.screen_draws,
        c.atlas_draws,
        c.buffer_bytes,
        c.texture_store_bytes,
        c.texture_upload_bytes,
        cache.masks,
        cache.face_parses,
        cache.uploads,
        cache.upload_bytes
    );
    detail.report();
}
fn new_ui(
    context: &Context,
    last: &RefCell<Weak<ProbeWindow>>,
) -> Result<(TextCacheWindow, Rc<ProbeWindow>), Error> {
    let ui = TextCacheWindow::new()?;
    ui.window().set_size(context.size);
    ui.show()?;
    let window = last.borrow().upgrade().ok_or("missing probe window")?;
    // Initialize the surface/empty scene, not fonts or visible text.
    measure(&window, context)?;
    ui.set_rows(ModelRc::new(VecModel::from(rows(1))));
    Ok((ui, window))
}
fn order(repeat: u32) -> [&'static str; 3] {
    [
        ["cold", "row-prepared", "other-canvas"],
        ["other-canvas", "row-prepared", "cold"],
        ["row-prepared", "cold", "other-canvas"],
        ["other-canvas", "cold", "row-prepared"],
        ["cold", "other-canvas", "row-prepared"],
        ["row-prepared", "other-canvas", "cold"],
    ][repeat as usize % 6]
}
const STAGES: [&str; 10] = [
    "first",
    "repeat",
    "item-half-pixel",
    "item-repeat",
    "layout-half-pixel",
    "layout-repeat",
    "score-change",
    "score-repeat",
    "size-change",
    "restore",
];
fn set_stage(ui: &TextCacheWindow, stage: &str) {
    ui.set_only_row(-1);
    ui.set_item_shift(if matches!(stage, "item-half-pixel" | "item-repeat") {
        0.5
    } else {
        0.
    });
    ui.set_layout_width_delta(if matches!(stage, "layout-half-pixel" | "layout-repeat") {
        1.
    } else {
        0.
    });
    ui.set_size_delta(if stage == "size-change" { 1. } else { 0. });
    ui.set_rows(ModelRc::new(VecModel::from(rows(
        if matches!(stage, "score-change" | "score-repeat") {
            2
        } else {
            1
        },
    ))));
}
fn save(path: &Path, size: PhysicalSize, pixels: &[u8]) -> Result<(), Error> {
    let mut encoder = png::Encoder::new(File::create_new(path)?, size.width, size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}
pub(super) fn run(options: &Options, width: u32, height: u32) -> Result<(), Error> {
    if options.gpu_text_shaping {
        return shaping::run(options);
    }
    if width < 640 || height < 480 {
        return Err("text fixture requires at least 640×480".into());
    }
    if let Some(path) = &options.gpu_text_output {
        std::fs::create_dir(path)?;
    }
    let context = Context::new(&options.gpu_text_device, PhysicalSize::new(width, height))?;
    if options.gpu_text_masks {
        return masks::run(options, context);
    }
    let policy = crate::gpu_text::Mode::parse(&options.gpu_text_policy)?;
    detail::enable(options.gpu_text_detail);
    let cpu_masks = policy != crate::gpu_text::Mode::Vector;
    femtovg::request_glyph_cache(policy.into());
    eprintln!(
        "slint_text_policy={} font=Liberation_Sans",
        options.gpu_text_policy
    );
    let last = Rc::new(RefCell::new(Weak::new()));
    slint::platform::set_platform(Box::new(ProbePlatform {
        context: context.clone(),
        last: last.clone(),
    }))?;
    let mut reference = BTreeMap::<&str, Vec<u8>>::new();
    let mut checked = 0;
    print!(
        "repeat,mode,stage,draw_wall_ms,draw_cpu_ms,build_wall_ms,build_cpu_ms,drain_wall_ms,drain_cpu_ms,complete_wall_ms,complete_cpu_ms,screen_draws,atlas_draws,buffer_bytes,texture_store_bytes,texture_upload_bytes,mask_misses,face_parses,mask_uploads,mask_upload_bytes"
    );
    detail::header();
    for repeat in 0..options.presentation_repeats {
        for mode in order(repeat) {
            let (mut ui, mut window) = new_ui(&context, &last)?;
            if mode != "cold" {
                for row in 0..rows(1).len() {
                    ui.set_only_row(row as i32);
                    let result = measure(&window, &context)?;
                    report(repeat, mode, &format!("prepare-{row}"), &result);
                }
                if mode == "other-canvas" {
                    ui.hide()?;
                    drop(ui);
                    drop(window);
                    (ui, window) = new_ui(&context, &last)?;
                }
            }
            for stage in STAGES {
                set_stage(&ui, stage);
                let before = femtovg::glyph_cache_stats();
                let result = measure(&window, &context)?;
                let after = femtovg::glyph_cache_stats();
                report(repeat, mode, stage, &result);
                if cpu_masks {
                    eprintln!(
                        "slint_cpu_masks repeat={repeat} mode={mode} stage={stage} masks={} uploads={} face_parses={} pages={} entries={} errors={} fallback_runs={}",
                        after.rasterized - before.rasterized,
                        after.uploads - before.uploads,
                        after.face_parses - before.face_parses,
                        after.pages,
                        after.entries,
                        after.errors,
                        after.fallback_runs
                    );
                    if after.errors != 0 || after.fallback_runs != 0 || result.3.atlas_draws != 0 {
                        return Err(
                            "unexpected CPU text error or fallback in supported fixture".into()
                        );
                    }
                    if (policy == crate::gpu_text::Mode::Cpu
                        && result.4.face_parses != result.4.masks)
                        || (matches!(
                            policy,
                            crate::gpu_text::Mode::CpuRunFaces
                                | crate::gpu_text::Mode::CpuShapeFaces
                        ) && (result.4.face_parses > result.4.masks
                            || result.4.face_parses > 14))
                    {
                        return Err(format!("unexpected font-parse work: {:?}", result.4).into());
                    }
                }
                let cached = matches!(
                    stage,
                    "repeat" | "item-repeat" | "layout-repeat" | "score-repeat" | "restore"
                ) || (mode == "row-prepared" && stage == "first");
                if cached
                    && (result.3.atlas_draws != 0
                        || after.rasterized != before.rasterized
                        || after.face_parses != before.face_parses
                        || after.uploads != before.uploads)
                {
                    return Err(
                        format!("unexpected cache misses: {mode}/{stage}: {:?}", result.3).into(),
                    );
                }
                if stage == "first"
                    && mode != "row-prepared"
                    && (if cpu_masks {
                        after.rasterized == before.rasterized
                    } else {
                        result.3.atlas_draws == 0
                    })
                {
                    return Err("fresh canvas did not generate any atlas geometry".into());
                }
                let pixels = context.pixels()?;
                if let Some(expected) = reference.get(stage) {
                    if expected != &pixels {
                        return Err(
                            format!("output changed: repeat={repeat} {mode}/{stage}").into()
                        );
                    }
                    checked += 1;
                } else {
                    reference.insert(stage, pixels.clone());
                }
                if let Some(path) = options.gpu_text_output.as_ref().filter(|_| repeat == 0) {
                    save(
                        &path.join(format!("{mode}-{stage}.png")),
                        context.size,
                        &pixels,
                    )?;
                }
            }
            ui.hide()?;
            drop(ui);
            drop(window);
            if last.borrow().upgrade().is_some() {
                return Err("probe retained a UI renderer".into());
            }
            if femtovg::glyph_cache_stats().cpu_canvases != 0 {
                return Err("probe retained a CPU glyph cache".into());
            }
            context.check()?;
        }
    }
    if reference["first"] != reference["repeat"] || reference["first"] != reference["restore"] {
        return Err("cold/warm/restored reference differs".into());
    }
    // Guard against a fast but blank output: count colors beyond the opaque background.
    let lit = reference["first"]
        .chunks_exact(4)
        .filter(|p| p[0] > 32 || p[1] > 32 || p[2] > 32)
        .count();
    if lit < 1000 {
        return Err(format!("fixture unexpectedly empty: {lit} lit pixels").into());
    }
    eprintln!(
        "gpu_text_validation comparisons={checked} stages={} lit_pixels={lit} exact_rgba=true all_renderers_released=true",
        STAGES.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_preserves_other_text_when_scores_change() {
        let a = rows(1);
        let b = rows(2);
        assert_eq!(a.len(), 14);
        assert_eq!(
            a.iter().zip(&b).filter(|(a, b)| a.label != b.label).count(),
            3
        );
        for (a, b) in a.iter().zip(b) {
            assert_eq!((a.size, a.weight), (b.size, b.weight));
        }
    }
    #[test]
    fn ordering_counterbalances_each_mode_at_each_position() {
        for position in 0..3 {
            for mode in order(0) {
                assert_eq!((0..6).filter(|r| order(*r)[position] == mode).count(), 2);
            }
        }
    }
}
