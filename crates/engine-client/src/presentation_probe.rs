//! Fixed-image presentation lab. No real-time simulation, input, settings or
//! frame pacing. The memory probe opens a private DRM allocation; the opt-in
//! GPU upload probe opens a render-node context. Neither takes over scanout.

use engine_common::{ClockEventProfile, Scenario};
use i_slint_core::software_renderer::{
    DRAW_DIAGNOSTIC_COUNT, DrawDiagnostic, DrawDiagnosticObserver, MinimalSoftwareWindow,
    PhysicalRegion, PremultipliedRgbaColor, RepaintBufferType, SoftwareRenderer, TargetPixel,
    TargetPixelBuffer,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, Image, PhysicalSize};
use std::{
    cell::{Cell, RefCell},
    hint::black_box,
    rc::Rc,
    time::{Duration, Instant},
};

#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
mod geometry;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod gpu_text;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod gpu_ui;
#[cfg(target_os = "linux")]
mod memory;
mod raster;
mod text;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod upload;

#[derive(Debug, Default, clap::Args)]
pub struct Options {
    /// Check rounded borders, clipping and stroke overlap in a private GPU context.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_gpu_text", "presentation_geometry", "presentation_upload", "presentation_text", "presentation_memory", "presentation_raster", "presentation_republish", "presentation_detail", "presentation_frames", "presentation_repeats"])]
    presentation_gpu_ui: bool,
    #[arg(
        long,
        requires = "presentation_gpu_ui",
        default_value = "/dev/dri/renderD128"
    )]
    gpu_ui_device: std::path::PathBuf,
    #[arg(long, requires = "presentation_gpu_ui")]
    gpu_ui_output: Option<std::path::PathBuf>,
    /// Compare frozen images in a bare window and the full UI; print CSV, without a display.
    #[arg(long, conflicts_with_all = ["scenario", "rom", "benchmark", "benchmark_headless", "debug_render", "debug_triangles", "kiosk", "touch_test", "benchmark_report", "renderer", "raster_scale"])]
    pub benchmark_presentation: bool,
    /// Draws per measured block (each UI gets the same workload).
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u32).range(1..=10000))]
    presentation_frames: u32,
    /// Paired blocks, alternating which UI runs first.
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=20))]
    presentation_repeats: u32,
    /// Time nested operations; off by default so instrumentation overhead can be compared.
    #[arg(long)]
    presentation_detail: bool,
    /// Publish a fresh identical image before every draw, outside the draw timer.
    #[arg(long)]
    presentation_republish: bool,
    /// Compare replaced and retained Spacewars text models with identical pixels.
    #[arg(
        long,
        requires = "benchmark_presentation",
        conflicts_with = "presentation_republish"
    )]
    presentation_text: bool,
    /// Reproduce the former LinuxKMS per-operation CPU clock reads in the RAM text probe.
    #[arg(long, requires_all = ["presentation_text", "presentation_detail"])]
    presentation_cpu_clocks: bool,
    /// Compare frozen text drawing with reference/shortcut blending and RAM staging.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_text", "presentation_raster", "presentation_republish"])]
    presentation_memory: bool,
    /// Also draw into a private DRM dumb buffer on this device; never present it.
    #[arg(long, requires = "presentation_memory")]
    presentation_drm_device: Option<std::path::PathBuf>,
    /// Rotate the memory-probe output to exercise clipping and transposed strides.
    #[arg(long, requires = "presentation_memory", default_value = "0", value_parser = ["0", "90", "180", "270"])]
    presentation_rotation: String,
    /// Compare frozen material-match rasterization at scales 1 and 2.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_text", "presentation_republish"])]
    presentation_raster: bool,
    /// Simulation ticks at which to freeze seed 42; simulation is outside timing.
    #[arg(long, requires = "presentation_raster", value_delimiter = ',', default_value = "120,1800,7200", value_parser = clap::value_parser!(u32).range(0..=10800))]
    presentation_match_ticks: Vec<u32>,
    /// Also measure diagnostic removals of terrain fills/strokes, HUD backing and corona.
    #[arg(long, requires = "presentation_raster")]
    presentation_raster_ablation: bool,
    /// Pair chunk culling with the uncropped reference and require identical pixels.
    #[arg(
        long,
        requires = "presentation_raster",
        conflicts_with = "presentation_raster_ablation"
    )]
    presentation_terrain_culling: bool,
    /// Save frozen geometry and output PNGs, outside all measurement timers.
    #[arg(long, requires = "presentation_raster")]
    presentation_output: Option<std::path::PathBuf>,
    /// Compare RGB/RGBA texture uploads offscreen on an explicit GPU render node.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_text", "presentation_memory", "presentation_raster", "presentation_republish", "presentation_detail"])]
    presentation_upload: bool,
    /// DRM render node used by the upload probe; never a display/scanout node.
    #[arg(
        long,
        requires = "presentation_upload",
        default_value = "/dev/dri/renderD128"
    )]
    presentation_upload_device: std::path::PathBuf,
    /// Frozen production image; both formats use the same pixels at native size.
    #[arg(long, requires = "presentation_upload", default_value = "clock-1024", value_parser = ["clock-1024", "falling-256"])]
    presentation_upload_fixture: String,
    /// Hold RGBA fixed and compare fresh, single and triple-buffered textures.
    #[arg(long, requires = "presentation_upload")]
    presentation_upload_reuse: bool,
    /// Isolate cold/cached UI text and row preparation in a private offscreen surface.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_text", "presentation_memory", "presentation_raster", "presentation_upload", "presentation_geometry", "presentation_republish", "presentation_detail", "presentation_frames"])]
    presentation_gpu_text: bool,
    /// Explicit GPU render node for the text-cache probe; never owns scanout.
    #[arg(
        long,
        requires = "presentation_gpu_text",
        default_value = "/dev/dri/renderD128"
    )]
    gpu_text_device: std::path::PathBuf,
    /// New directory for text reference PNGs and exact image-validation records.
    #[arg(long, requires = "presentation_gpu_text")]
    gpu_text_output: Option<std::path::PathBuf>,
    /// Text policy in the Slint/shaping fixtures; the default stays vector.
    #[arg(long, requires = "presentation_gpu_text", conflicts_with = "gpu_text_masks", default_value = "vector", value_parser = ["vector", "cpu", "cpu-run-faces", "cpu-shape-faces"])]
    gpu_text_policy: String,
    /// Add intrusive per-run text/path clocks; omit for the timing-overhead control.
    #[arg(
        long,
        requires = "presentation_gpu_text",
        conflicts_with = "gpu_text_masks"
    )]
    gpu_text_detail: bool,
    /// Compare vector-generated and CPU-generated glyph atlases, not the live UI.
    #[arg(long, group = "gpu_text_font_probe", requires_all = ["presentation_gpu_text", "gpu_text_font_regular", "gpu_text_font_bold"])]
    gpu_text_masks: bool,
    /// CPU-only fresh-context shaping lab; no Slint window, GPU, atlas or display.
    /// --gpu-text-detail adds intrusive word-level clocks in this lab only.
    #[arg(long, group = "gpu_text_font_probe", requires_all = ["presentation_gpu_text", "gpu_text_font_regular", "gpu_text_font_bold"], conflicts_with_all = ["gpu_text_masks", "gpu_text_output", "gpu_text_device"])]
    gpu_text_shaping: bool,
    /// Explicit regular font for the mask/shaping probes; exact bytes, face 0.
    #[arg(long, requires = "gpu_text_font_probe")]
    gpu_text_font_regular: Option<std::path::PathBuf>,
    /// Explicit bold font for the mask/shaping probes; no system-font assumption.
    #[arg(long, requires = "gpu_text_font_probe")]
    gpu_text_font_bold: Option<std::path::PathBuf>,
    /// Replay scene geometry or regenerate ship artwork, without simulation or vsync.
    #[arg(long, requires = "benchmark_presentation", conflicts_with_all = ["presentation_text", "presentation_memory", "presentation_raster", "presentation_upload", "presentation_republish", "presentation_detail"])]
    presentation_geometry: bool,
    /// Explicit render node; no display ownership or implicit software fallback.
    #[arg(
        long,
        requires = "presentation_geometry",
        default_value = "/dev/dri/renderD128"
    )]
    geometry_device: std::path::PathBuf,
    /// Production snapshots or deterministic synthetic animation/overdraw.
    #[arg(long, requires = "presentation_geometry", value_delimiter = ',', default_value = "clock-idle,clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-300,pizza-1000,pizza-5000,pizza-10000,overlap-1,overlap-8,overlap-32,overlap-128", value_parser = ["clock-idle", "clock-rain", "mixed-grouped", "mixed-interleaved", "moving-circles", "meshes-grouped", "meshes-interleaved", "meshes-churn", "ships-pair", "ships-fleet", "ships-zoom", "ships-pair-local", "ships-fleet-local", "ships-zoom-local", "pizza-300", "pizza-1000", "pizza-5000", "pizza-10000", "overlap-1", "overlap-8", "overlap-32", "overlap-128"])]
    geometry_cases: Vec<String>,
    /// Compare geometry and submission controls; normal rendering is unaffected.
    #[arg(long, requires = "presentation_geometry", value_delimiter = ',', default_value = "triangles", value_parser = ["triangles", "triangles-split", "triangles-programs", "circles", "circles-expanded", "circles-uniforms", "circles-bindings", "circles-submission", "circles-unified", "circles-unified-instanced", "circles-unified-layout", "circles-unified-vao", "meshes-streamed", "meshes", "meshes-expanded"])]
    geometry_modes: Vec<String>,
    /// New directory for GPU and raster-reference captures (never overwrite).
    #[arg(long, requires = "presentation_geometry")]
    geometry_output: Option<std::path::PathBuf>,
    /// Maximum submitted frames awaiting completion; bounds queue/resource growth.
    #[arg(long, requires = "presentation_geometry", default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=8))]
    geometry_in_flight: u32,
    /// Compare instrumentation overhead with GPU queries disabled.
    #[arg(long, requires = "presentation_geometry")]
    geometry_no_gpu_timing: bool,
    /// Validate and time split views, grouped minimaps and resize/resource history.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_layouts: bool,
    /// Compare uncached, cold and retained production terrain chunks, including edits.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_terrain: bool,
    /// Replay real Spacewars matches through the live split-screen retained path.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_live_retained: bool,
    /// Limit timed live-replay modes; all modes still undergo image validation.
    #[arg(long, requires = "geometry_live_retained", value_delimiter = ',', value_parser = ["triangles", "retained-cold", "retained", "retained-adaptive", "retained-zoom", "retained-hybrid", "retained-outlines", "retained-paced", "retained-memoized", "retained-scan", "retained-ranges", "retained-culled", "retained-resident", "retained-resident-passes", "retained-resident-buffers", "retained-resident-uniforms"])]
    geometry_live_modes: Vec<String>,
    /// Replay terrain edits, visibility changes and cold admission without hiding startup frames.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_churn: bool,
    /// Profile cold template construction on production artwork, without drawing.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_build_profile: bool,
    /// Profile warm CPU mesh encoding on moving production replays, without drawing.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_encode_profile: bool,
    /// Attribute GL submission CPU wall time; paired controls compile out stage clocks.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_submit_profile: bool,
    /// Also compare bounded per-pass vertex/index buffers against the shared pair.
    #[arg(long, requires = "geometry_submit_profile")]
    geometry_submit_passes: bool,
    /// Compare per-pass exact stores against grow-only bucketed stores plus subdata.
    #[arg(
        long,
        requires = "geometry_submit_profile",
        conflicts_with = "geometry_submit_passes"
    )]
    geometry_submit_capacity: bool,
    /// Isolate the upload API: exact combined, exact split, and bucketed split stores.
    #[arg(long, requires = "geometry_submit_profile", conflicts_with_all = ["geometry_submit_passes", "geometry_submit_capacity"])]
    geometry_submit_api: bool,
    /// Time only uninstrumented controls; still validate every mode in the selected family.
    #[arg(long, requires = "geometry_submit_profile")]
    geometry_submit_plain_only: bool,
    /// Compare exact/bucketed streams with bounded completion overlap and terrain histories.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_submit_profile", "geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_no_gpu_timing"])]
    geometry_stream_history: bool,
    /// Prepared histories for the stream-overlap probe (1..128 frames, no simulation in timing).
    #[arg(long, requires = "geometry_stream_history", value_delimiter = ',', default_value = "bots-early,bots-late,close-up-zoom,terrain-reveal-zoom,terrain-edit-replace", value_parser = ["bots-early", "bots-late", "close-up-zoom", "terrain-reveal-zoom", "terrain-edit-replace"])]
    geometry_stream_cases: Vec<String>,
    /// Compare CPU plan allocation policies on identical moving production frames.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_stream_history", "geometry_submit_profile", "geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_plan_profile: bool,
    /// Compare source point allocation on matched freshly generated moving states.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_plan_profile", "geometry_stream_history", "geometry_submit_profile", "geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_source_profile: bool,
    /// Compare uncached stroke writers on identical moving scenes, including completed GPU draws.
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_source_profile", "geometry_plan_profile", "geometry_stream_history", "geometry_submit_profile", "geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing"])]
    geometry_stroke_profile: bool,
    /// Replay bounded captured outlines with separate join/output timing controls.
    #[arg(long, requires = "geometry_stroke_profile", conflicts_with_all = ["geometry_stroke_rejection", "geometry_small_strokes", "geometry_round_joins"])]
    geometry_stroke_attribution: bool,
    /// Compare only plain/prepared/checked fixed-width guards, without region clocks.
    #[arg(long, requires = "geometry_stroke_attribution")]
    geometry_stroke_invariants: bool,
    /// Compare bounded triangle-layout reuse with ordinary captured stroke tessellation.
    #[arg(
        long,
        requires = "geometry_stroke_attribution",
        conflicts_with = "geometry_stroke_invariants"
    )]
    geometry_stroke_topology: bool,
    /// Compare the actual renderer adapter with direct replay, split by stroke outcome.
    #[arg(long, requires = "geometry_stroke_topology")]
    geometry_stroke_adapter: bool,
    /// Compare guarded stroke rejection with ordinary tessellation, keeping the stock writer.
    #[arg(long, requires = "geometry_stroke_profile")]
    geometry_stroke_rejection: bool,
    /// Compare one-pixel opaque polygon bevels with the reference; report image differences.
    #[arg(
        long,
        requires = "geometry_stroke_profile",
        conflicts_with = "geometry_stroke_rejection"
    )]
    geometry_small_strokes: bool,
    /// Compare exact zero-subdivision round-join skipping with the original calculation.
    #[arg(long, requires = "geometry_stroke_profile", conflicts_with_all = ["geometry_stroke_rejection", "geometry_small_strokes"])]
    geometry_round_joins: bool,
    /// Use the bulk writer in both exact round-join controls; cached templates stay stock.
    #[arg(long, requires = "geometry_round_joins")]
    geometry_stroke_bulk: bool,
    /// Keep the bulk writer's concrete type through fixed-width polygon/line tessellation.
    #[arg(long, requires = "geometry_stroke_bulk")]
    geometry_stroke_static: bool,
    /// Compare direct polygon input with path events; both use concrete bulk output and exact joins.
    #[arg(long, requires = "geometry_stroke_static")]
    geometry_stroke_polygon: bool,
    /// Compare adjacent-edge reuse with ordinary concrete-output path-event strokes.
    #[arg(
        long,
        requires = "geometry_stroke_static",
        conflicts_with = "geometry_stroke_polygon"
    )]
    geometry_stroke_edges: bool,
    /// Compare per-stroke prepared join bounds against bulk-static with exact joins.
    #[arg(long, requires = "geometry_stroke_static", conflicts_with_all = ["geometry_stroke_edges", "geometry_stroke_polygon", "geometry_stroke_attribution", "geometry_stroke_invariants"])]
    geometry_stroke_prepared: bool,
    /// Compare bounded single-pass polygon output against ordinary bulk-static.
    #[arg(long, requires = "geometry_stroke_static", conflicts_with_all = ["geometry_stroke_edges", "geometry_stroke_polygon", "geometry_stroke_prepared", "geometry_stroke_attribution", "geometry_stroke_invariants", "geometry_stroke_topology", "geometry_small_strokes", "geometry_stroke_rejection"])]
    geometry_stroke_single_pass: bool,
    /// Single-pass experiment; prefix compares scratch reuse with the original single-pass path.
    #[arg(long, requires = "geometry_stroke_single_pass", value_parser = ["all", "layouts", "arcs", "classify", "prefix"], conflicts_with = "geometry_stroke_encoder_profile")]
    geometry_stroke_selection: Option<String>,
    /// Attribute complete single-pass encoding, with separate uninstrumented controls.
    #[arg(
        long,
        requires = "geometry_stroke_single_pass",
        conflicts_with = "geometry_stroke_cases"
    )]
    geometry_stroke_encoder_profile: bool,
    /// Use existing scene snapshots/animations instead of the Spacewars stroke replay.
    #[arg(long, requires = "geometry_round_joins", value_delimiter = ',', value_parser = ["clock-idle", "clock-rain", "pizza-300", "pizza-5000", "mixed-interleaved"])]
    geometry_stroke_cases: Vec<String>,
    /// Validate cold/moving/stopped admission histories against ordinary geometry (no timing).
    #[arg(long, requires = "presentation_geometry", conflicts_with_all = ["geometry_stroke_profile", "geometry_source_profile", "geometry_plan_profile", "geometry_stream_history", "geometry_submit_profile", "geometry_encode_profile", "geometry_build_profile", "geometry_churn", "geometry_live_retained", "geometry_terrain", "geometry_layouts", "geometry_cases", "geometry_modes", "geometry_in_flight", "geometry_no_gpu_timing", "presentation_frames", "presentation_repeats"])]
    geometry_admission_check: bool,
}

slint::slint! {
    export component ImageOnlyWindow inherits Window {
        in property <image> frame;
        in property <bool> native;
        in property <int> crop-y;
        in property <int> crop-width: frame.width;
        in property <int> crop-height: frame.height;
        background: #050514;
        Image {
            width: parent.width; height: parent.height;
            source: root.frame;
            source-clip-y: root.crop-y;
            source-clip-width: root.crop-width;
            source-clip-height: root.crop-height;
            image-fit: root.native ? contain : fill;
            image-rendering: pixelated;
        }
    }
}

struct ProbePlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for ProbePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(target_os = "linux", derive(bytemuck::Pod, bytemuck::Zeroable))]
struct Xrgb(u32);
impl TargetPixel for Xrgb {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let mut pixel = PremultipliedRgbaColor {
            red: (self.0 >> 16) as u8,
            green: (self.0 >> 8) as u8,
            blue: self.0 as u8,
            alpha: (self.0 >> 24) as u8,
        };
        pixel.blend(color);
        self.0 = u32::from(pixel.alpha) << 24
            | u32::from(pixel.red) << 16
            | u32::from(pixel.green) << 8
            | u32::from(pixel.blue);
    }
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(0xff000000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    }
    fn background() -> Self {
        Self(0)
    }
}

struct Buffer<'a> {
    pixels: &'a mut [Xrgb],
    stride: usize,
    detail: bool,
}
impl TargetPixelBuffer for Buffer<'_> {
    type TargetPixel = Xrgb;
    fn line_slice(&mut self, y: usize) -> &mut [Xrgb] {
        &mut self.pixels[y * self.stride..(y + 1) * self.stride]
    }
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn diagnostic_observer(&self) -> Option<DrawDiagnosticObserver> {
        self.detail.then_some(observe)
    }
}

#[derive(Clone, Copy, Default)]
struct Measurement {
    start: Option<Instant>,
    depth: usize,
    calls: u64,
    elapsed: Duration,
    text_elapsed: Duration,
    text_calls: u64,
}
thread_local! {
    static PROBE_CPU_CLOCKS: Cell<bool> = const { Cell::new(false) };
    static MEASUREMENTS: RefCell<[Measurement; DRAW_DIAGNOSTIC_COUNT]> =
        RefCell::new([Measurement::default(); DRAW_DIAGNOSTIC_COUNT]);
}
fn observe(operation: DrawDiagnostic, begin: bool) {
    MEASUREMENTS.with(|measurements| {
        let mut measurements = measurements.borrow_mut();
        let inside_text = measurements[DrawDiagnostic::Text as usize].depth > 0;
        let m = &mut measurements[operation as usize];
        if begin {
            m.calls += 1;
            m.text_calls += u64::from(inside_text);
            if m.depth == 0 {
                m.start = Some(Instant::now());
                probe_cpu_clock();
            }
            m.depth += 1;
        } else {
            assert!(m.depth > 0, "unbalanced diagnostic span");
            m.depth -= 1;
            if m.depth == 0 {
                let elapsed = m.start.take().unwrap().elapsed();
                probe_cpu_clock();
                m.elapsed += elapsed;
                if inside_text {
                    m.text_elapsed += elapsed;
                }
            }
        }
    });
}

fn probe_cpu_clock() {
    // Match the backend Stamp order: monotonic first, CPU second. The outer
    // operation includes its children's clock-read cost, as the former backend did.
    #[cfg(target_os = "linux")]
    if PROBE_CPU_CLOCKS.get() {
        black_box(nix::time::clock_gettime(nix::time::ClockId::CLOCK_THREAD_CPUTIME_ID).ok());
    }
}

struct Fixture {
    name: &'static str,
    image: Image,
    native: bool,
    crop_y: i32,
    crop_width: i32,
    crop_height: i32,
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = vec![Fixture {
        name: "background",
        image: Image::default(),
        native: false,
        crop_y: 0,
        crop_width: 0,
        crop_height: 0,
    }];
    for (name, width, height) in [("clock-1024", 1024, 768), ("clock-2048", 2048, 1536)] {
        let mut state = scenario_clock::ClockScenario::init(
            scenario_clock::ClockConfig {
                event_profile: ClockEventProfile::Off,
                aspect_ratio: 4.0 / 3.0,
                ..Default::default()
            },
            0,
        );
        scenario_clock::ClockScenario::step(
            &mut state,
            &[scenario_clock::ClockAction::set_reading(
                scenario_clock::ClockReading::new(8, 8, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
            &[scenario_clock::ClockScenario::render_frame(&state)],
            crate::render::Viewport::new(width as f32, height as f32),
            crate::render::FrameLayout::EqualHorizontal,
            Default::default(),
        );
        fixtures.push(Fixture {
            name,
            image,
            native: false,
            crop_y: 0,
            crop_width: width,
            crop_height: height,
        });
    }
    // Boot the bundled, redistributable cartridge once, then freeze its title frame.
    let mut falling = scenario_falling::FallingScenario::init(Default::default(), 0);
    for _ in 0..180 {
        scenario_falling::FallingScenario::step(
            &mut falling,
            &[],
            Duration::from_secs_f64(1.0 / 60.0),
        );
    }
    let video = falling.native_video_frame();
    let crop = video.visible_crop;
    let image = crate::native_video::NativeVideoRenderer::new()
        .present(video)
        .unwrap()
        .image;
    fixtures.push(Fixture {
        name: "falling-256",
        image,
        native: true,
        crop_y: crop.y as i32,
        crop_width: crop.width as i32,
        crop_height: crop.height as i32,
    });
    fixtures
}

fn fresh_image(image: &Image) -> Image {
    image.to_rgb8().map_or_else(Image::default, |pixels| {
        Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
        ))
    })
}

fn publish(
    bare: &ImageOnlyWindow,
    full: &crate::MainWindow,
    fixture: &Fixture,
    image: Image,
    is_bare: bool,
) {
    if is_bare {
        bare.set_frame(image);
    } else if fixture.native {
        full.set_native_video_frame(image);
    } else {
        full.set_raster_frame(image);
    }
}

fn render(
    window: &MinimalSoftwareWindow,
    pixels: &mut [Xrgb],
    width: usize,
    detail: bool,
) -> Duration {
    let mut elapsed = Duration::ZERO;
    assert!(window.draw_if_needed(|renderer: &SoftwareRenderer| {
        let start = Instant::now();
        let _: PhysicalRegion = renderer.render_into_buffer(&mut Buffer {
            pixels,
            stride: width,
            detail,
        });
        elapsed = start.elapsed();
    }));
    elapsed
}

pub fn run(options: &Options, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
    if width > 2048 || height > 2048 {
        return Err("presentation viewport must be at most 2048×2048".into());
    }
    if options.presentation_gpu_ui {
        #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
        return gpu_ui::run(options);
        #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
        return Err("--presentation-gpu-ui requires Linux and the pi-kiosk-gpu feature".into());
    }
    if options.presentation_gpu_text {
        #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
        return gpu_text::run(options, width, height);
        #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
        return Err("--presentation-gpu-text requires Linux and the pi-kiosk-gpu feature".into());
    }
    if options.presentation_geometry {
        #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
        return geometry::run(options, width, height);
        #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
        return Err("--presentation-geometry requires Linux and the pi-kiosk-gpu feature".into());
    }
    if options.presentation_upload {
        #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
        return upload::run(options);
        #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
        return Err("--presentation-upload requires Linux and the pi-kiosk-gpu feature".into());
    }
    if options.presentation_memory {
        #[cfg(target_os = "linux")]
        return memory::run(options, width, height);
        #[cfg(not(target_os = "linux"))]
        return Err("--presentation-memory requires Linux".into());
    }
    if options.presentation_text {
        return text::run(options, width, height);
    }
    if options.presentation_raster {
        return raster::run(options, width, height);
    }
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(windows.clone())))?;
    let bare = ImageOnlyWindow::new()?;
    let full = crate::MainWindow::new()?;
    bare.show()?;
    full.show()?;
    let windows = windows.borrow();
    for window in windows.iter() {
        window.set_size(PhysicalSize::new(width, height));
    }
    let mut pixels = [
        vec![Xrgb::default(); (width * height) as usize],
        vec![Xrgb::default(); (width * height) as usize],
    ];
    print!(
        "fixture,ui,width,height,source_width,source_height,publication,detail,repeat,frames,draw_wall_ms,checksum"
    );
    for name in [
        "render",
        "prepare",
        "dirty",
        "items",
        "image",
        "texture",
        "fallback",
        "rectangle",
        "text",
        "path",
        "background",
        "text_font",
        "text_glyph_run",
    ] {
        print!(",{name}_calls_per_frame,{name}_wall_ms");
    }
    println!();
    for fixture in fixtures() {
        bare.set_native(fixture.native);
        bare.set_crop_y(fixture.crop_y);
        bare.set_crop_width(fixture.crop_width);
        bare.set_crop_height(fixture.crop_height);
        // Falling has no gameplay HUD/button overlay. Keep that same shell for
        // every source so both windows produce exactly the same visible pixels.
        full.set_launcher_scenario("falling".into());
        full.set_native_video_visible(fixture.native);
        full.set_raster_visible(!fixture.native);
        full.set_native_video_crop_y(fixture.crop_y);
        full.set_native_video_crop_width(fixture.crop_width);
        full.set_native_video_crop_height(fixture.crop_height);
        for is_bare in [true, false] {
            publish(&bare, &full, &fixture, fixture.image.clone(), is_bare);
        }
        for repeat in 0..options.presentation_repeats {
            for index in if repeat % 2 == 0 { [0, 1] } else { [1, 0] } {
                let window = &windows[index];
                let mut elapsed = Duration::ZERO;
                for frame in 0..options.presentation_frames + 10 {
                    if frame == 10 {
                        MEASUREMENTS.with(|m| {
                            *m.borrow_mut() = [Measurement::default(); DRAW_DIAGNOSTIC_COUNT]
                        });
                    }
                    if options.presentation_republish {
                        publish(
                            &bare,
                            &full,
                            &fixture,
                            fresh_image(&fixture.image),
                            index == 0,
                        );
                    }
                    window.request_redraw();
                    let sample = render(
                        window,
                        &mut pixels[index],
                        width as usize,
                        options.presentation_detail,
                    );
                    if frame >= 10 {
                        elapsed += sample;
                    }
                    black_box(&pixels[index]);
                }
                let checksum = pixels[index].iter().fold(0xcbf29ce484222325u64, |h, p| {
                    (h ^ u64::from(p.0)).wrapping_mul(0x100000001b3)
                });
                let frames = f64::from(options.presentation_frames);
                print!(
                    "{},{},{width},{height},{},{},{},{},{repeat},{},{:.6},{checksum:016x}",
                    fixture.name,
                    if index == 0 { "bare" } else { "full" },
                    fixture.image.size().width,
                    fixture.image.size().height,
                    if options.presentation_republish {
                        "replace"
                    } else {
                        "retained"
                    },
                    options.presentation_detail,
                    options.presentation_frames,
                    elapsed.as_secs_f64() * 1000.0 / frames
                );
                MEASUREMENTS.with(|m| {
                    for measurement in m.borrow().iter() {
                        assert_eq!(measurement.depth, 0);
                        print!(
                            ",{:.3},{:.6}",
                            measurement.calls as f64 / frames,
                            measurement.elapsed.as_secs_f64() * 1000.0 / frames
                        );
                    }
                });
                println!();
            }
            if pixels[0] != pixels[1] {
                return Err(format!("{}: bare/full pixels differ", fixture.name).into());
            }
        }
    }
    Ok(())
}
