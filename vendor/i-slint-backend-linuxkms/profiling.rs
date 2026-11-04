// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Local, bounded LinuxKMS presentation diagnostics. These measure CPU
//! rendering and DRM calls, not GPU execution or verified physical scanouts.
//! Read on the UI thread. Set SPACEWARS_KMS_PROFILE=0 before startup to disable.

use nix::time::{clock_gettime, ClockId};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt::Write,
    time::{Duration, Instant},
};

const SAMPLE_LIMIT: usize = 120;
const STAGE_COUNT: usize = 64;
const COUNTER_COUNT: usize = 11;

pub mod application;
pub mod geometry;
#[cfg(feature = "renderer-femtovg")]
mod text;
mod trace;

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Loop,
    Timers,
    Callbacks,
    Render,
    Dispatch,
    Map,
    Draw,
    Unmap,
    FlipWait,
    Submit,
    Background,
    Copy,
    RgbBlit,
    DrawGeneric,
    DrawRgb,
    CoreRender,
    CorePrepare,
    CoreDirty,
    CoreItems,
    CoreImage,
    CoreTexture,
    CoreFallback,
    CoreRectangle,
    CoreText,
    CorePath,
    CoreBackground,
    CoreTextFont,
    CoreTextGlyphRun,
    CoreTextTexture,
    CoreTextFallback,
    EglSwap,
    GlTextureAllocate,
    GlTextureUpload,
    GlTextureDelete,
    GlBufferUpload,
    GlDraw,
    GlCompile,
    GlLink,
    FemtoBuildItems,
    FemtoAfterItems,
    FemtoTextMeasure,
    FemtoTextBreak,
    FemtoTextShape,
    FemtoTextShapeMiss,
    FemtoTextDraw,
    FemtoCpuMaskBuild,
    FemtoCpuMaskUpload,
    FemtoGlyphQuads,
    FemtoPathFill,
    FemtoPathStroke,
    AppHostTick,
    AppHostStep,
    AppHostResult,
    AppHostScene,
    AppHostPublish,
    AppHostDiagnostics,
    AppBeforeRender,
    AppAfterRender,
    AppSceneProject,
    AppSceneStage,
    AppSceneOverlay,
    AppSourceDrop,
    AppSceneDraw,
    AppPlanDrop,
}

const STAGES: [(Stage, &str); STAGE_COUNT] = [
    (Stage::Loop, "loop"),
    (Stage::Timers, "timers"),
    (Stage::Callbacks, "callbacks"),
    (Stage::Render, "render"),
    (Stage::Dispatch, "dispatch"),
    (Stage::Map, "map"),
    (Stage::Draw, "draw"),
    (Stage::Unmap, "unmap"),
    (Stage::FlipWait, "flip_wait"),
    (Stage::Submit, "submit"),
    (Stage::Background, "background"),
    (Stage::Copy, "copy"),
    (Stage::RgbBlit, "rgb_blit"),
    (Stage::DrawGeneric, "draw_generic"),
    (Stage::DrawRgb, "draw_rgb"),
    (Stage::CoreRender, "core_render"),
    (Stage::CorePrepare, "core_prepare"),
    (Stage::CoreDirty, "core_dirty"),
    (Stage::CoreItems, "core_items"),
    (Stage::CoreImage, "core_image"),
    (Stage::CoreTexture, "core_texture"),
    (Stage::CoreFallback, "core_fallback"),
    (Stage::CoreRectangle, "core_rectangle"),
    (Stage::CoreText, "core_text"),
    (Stage::CorePath, "core_path"),
    (Stage::CoreBackground, "core_background"),
    (Stage::CoreTextFont, "core_text_font"),
    (Stage::CoreTextGlyphRun, "core_text_glyph_run"),
    (Stage::CoreTextTexture, "core_text_texture"),
    (Stage::CoreTextFallback, "core_text_fallback"),
    (Stage::EglSwap, "egl_swap"),
    (Stage::GlTextureAllocate, "gl_texture_allocate"),
    (Stage::GlTextureUpload, "gl_texture_upload"),
    (Stage::GlTextureDelete, "gl_texture_delete"),
    (Stage::GlBufferUpload, "gl_buffer_upload"),
    (Stage::GlDraw, "gl_draw"),
    (Stage::GlCompile, "gl_compile"),
    (Stage::GlLink, "gl_link"),
    (Stage::FemtoBuildItems, "femto_build_items"),
    (Stage::FemtoAfterItems, "femto_after_items"),
    (Stage::FemtoTextMeasure, "femto_text_measure"),
    (Stage::FemtoTextBreak, "femto_text_break"),
    (Stage::FemtoTextShape, "femto_text_shape"),
    (Stage::FemtoTextShapeMiss, "femto_text_shape_miss"),
    (Stage::FemtoTextDraw, "femto_text_draw"),
    (Stage::FemtoCpuMaskBuild, "femto_cpu_mask_build"),
    (Stage::FemtoCpuMaskUpload, "femto_cpu_mask_upload"),
    (Stage::FemtoGlyphQuads, "femto_glyph_quads"),
    (Stage::FemtoPathFill, "femto_path_fill"),
    (Stage::FemtoPathStroke, "femto_path_stroke"),
    (Stage::AppHostTick, "app_host_tick"),
    (Stage::AppHostStep, "app_host_step"),
    (Stage::AppHostResult, "app_host_result"),
    (Stage::AppHostScene, "app_host_scene"),
    (Stage::AppHostPublish, "app_host_publish"),
    (Stage::AppHostDiagnostics, "app_host_diagnostics"),
    (Stage::AppBeforeRender, "app_before_render"),
    (Stage::AppAfterRender, "app_after_render"),
    (Stage::AppSceneProject, "app_scene_project"),
    (Stage::AppSceneStage, "app_scene_stage"),
    (Stage::AppSceneOverlay, "app_scene_overlay"),
    (Stage::AppSourceDrop, "app_source_drop"),
    (Stage::AppSceneDraw, "app_scene_draw"),
    (Stage::AppPlanDrop, "app_plan_drop"),
];

#[derive(Clone, Copy)]
pub(crate) enum Counter {
    Draws,
    FlipSubmissions,
    FlipCompletions,
    Modesets,
    FlipReadErrors,
    CopyBytes,
    RgbBlits,
    RgbPixels,
    // Keep diagnostic field indices stable in builds without the GL renderer.
    #[cfg_attr(not(feature = "renderer-femtovg"), allow(dead_code))]
    GlUploadBytes,
    #[cfg_attr(not(feature = "renderer-femtovg"), allow(dead_code))]
    GlDrawCalls,
    #[cfg_attr(not(feature = "renderer-femtovg"), allow(dead_code))]
    GlBufferBytes,
}

const COUNTERS: [&str; COUNTER_COUNT] = [
    "draws",
    "flip_submissions",
    "flip_completions",
    "modesets",
    "flip_read_errors",
    "copy_bytes",
    "rgb_blits",
    "rgb_pixels",
    "gl_upload_bytes",
    "gl_draw_calls",
    "gl_buffer_bytes",
];

#[derive(Clone, Copy)]
struct Stamp {
    wall: Instant,
    cpu: Option<Duration>,
}

impl Stamp {
    fn now() -> Self {
        let wall = Instant::now();
        let cpu = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)
            .ok()
            .and_then(|time| {
                Some(Duration::new(
                    time.tv_sec().try_into().ok()?,
                    time.tv_nsec().try_into().ok()?,
                ))
            });
        Self { wall, cpu }
    }

    fn wall_only() -> Self {
        Self {
            wall: Instant::now(),
            cpu: None,
        }
    }

    fn elapsed(self) -> Elapsed {
        let end = if self.cpu.is_some() {
            Self::now()
        } else {
            Self::wall_only()
        };
        Elapsed {
            wall: end.wall.duration_since(self.wall),
            cpu: self
                .cpu
                .zip(end.cpu)
                .and_then(|(start, end)| end.checked_sub(start)),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Elapsed {
    wall: Duration,
    cpu: Option<Duration>,
}

#[derive(Clone, Copy, Default)]
struct StageSample {
    calls: u64,
    elapsed: Elapsed,
}

impl StageSample {
    fn add(&mut self, elapsed: Elapsed) {
        self.elapsed.cpu = if self.calls == 0 {
            elapsed.cpu
        } else {
            self.elapsed.cpu.zip(elapsed.cpu).map(|(a, b)| a + b)
        };
        self.elapsed.wall += elapsed.wall;
        self.calls += 1;
    }
}

#[derive(Clone)]
struct LoopSample {
    stages: [StageSample; STAGE_COUNT],
    counters: [u64; COUNTER_COUNT],
    dirty_pixels: u64,
    trace_marker: u8,
    presentation: PresentationSample,
    geometry: geometry::Sample,
}

/// Only populated during an explicit trace. Kernel flip timestamps are kept
/// separate from our capture-relative CPU clock; never subtract their epochs.
#[derive(Clone, Copy, Default)]
struct PresentationSample {
    loop_start: Option<Duration>,
    submitted: Option<Duration>,
    flip_observed: Option<Duration>,
    flip_sequence: Option<u32>,
    flip_kernel_time: Option<Duration>,
}

impl Default for LoopSample {
    fn default() -> Self {
        Self {
            stages: [StageSample::default(); STAGE_COUNT],
            counters: [0; COUNTER_COUNT],
            dirty_pixels: 0,
            trace_marker: 0,
            presentation: PresentationSample::default(),
            geometry: geometry::Sample::default(),
        }
    }
}

struct Profile {
    active: bool,
    enabled: bool,
    renderer: &'static str,
    gl_driver: Option<[String; 3]>,
    gl_stencil_bits: Option<u8>,
    size: (u32, u32),
    format: &'static str,
    buffer_bytes: usize,
    buffer_age: u8,
    buffer_mode: &'static str,
    shadow_bytes: usize,
    texture_mode: &'static str,
    draw_detail: bool,
    current: Option<LoopSample>,
    samples: VecDeque<LoopSample>,
    loops_total: u64,
    presented_frames: u64,
    counters_total: [u64; COUNTER_COUNT],
    trace: trace::Capture,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            active: false,
            enabled: false,
            renderer: "software",
            gl_driver: None,
            gl_stencil_bits: None,
            size: (0, 0),
            format: "unknown",
            buffer_bytes: 0,
            buffer_age: 0,
            buffer_mode: "unknown",
            shadow_bytes: 0,
            texture_mode: "unknown",
            draw_detail: false,
            current: None,
            samples: VecDeque::new(),
            loops_total: 0,
            presented_frames: 0,
            counters_total: [0; COUNTER_COUNT],
            trace: trace::Capture::default(),
        }
    }
}

impl Profile {
    fn record(&mut self, stage: Stage, elapsed: Elapsed) {
        if let Some(sample) = &mut self.current {
            sample.stages[stage as usize].add(elapsed);
        }
    }

    fn finish_loop(&mut self, elapsed: Elapsed) {
        self.record(Stage::Loop, elapsed);
        if let Some(sample) = self.current.take() {
            self.loops_total += 1;
            self.trace.record(self.loops_total, &sample);
            if self.samples.len() == SAMPLE_LIMIT {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    fn count(&mut self, counter: Counter) {
        self.add(counter, 1);
    }

    fn add(&mut self, counter: Counter, amount: u64) {
        if !self.enabled {
            return;
        }
        self.counters_total[counter as usize] += amount;
        if let Some(sample) = &mut self.current {
            sample.counters[counter as usize] += amount;
        }
    }

    fn diagnostics(&self) -> String {
        if !self.active {
            return String::new();
        }
        let mut text = format!(
            "\nkms_profile_version=13\nkms_profile_enabled={}\nkms_loop_samples={}\nkms_loops_total={}\nkms_output_width={}\nkms_output_height={}\nkms_pixel_format={}\nkms_buffer_bytes={}\nkms_buffer_age={}\nkms_buffer_mode={}\nkms_shadow_bytes={}\nkms_texture_mode={}\nkms_draw_detail={}",
            self.enabled, self.samples.len(), self.loops_total, self.size.0, self.size.1,
            self.format, self.buffer_bytes, self.buffer_age, self.buffer_mode, self.shadow_bytes, self.texture_mode, self.draw_detail,
        );
        text.push_str(&self.trace.status());
        let _ = write!(
            text,
            "\nkms_renderer={}\nkms_gpu_memory_measured=false",
            self.renderer
        );
        if let Some([vendor, renderer, version]) = &self.gl_driver {
            let _ =
                write!(text,
                "\nkms_gl_vendor={vendor}\nkms_gl_renderer={renderer}\nkms_gl_version={version}");
        }
        if let Some(bits) = self.gl_stencil_bits {
            let _ = write!(text, "\nkms_gl_stencil_bits={bits}");
        }
        let window_wall = self
            .samples
            .iter()
            .map(|s| s.stages[Stage::Loop as usize].elapsed.wall)
            .sum::<Duration>();
        let _ = write!(
            text,
            "\nkms_window_wall_ms={:.3}\nkms_dirty_pixels_window={}",
            millis(window_wall),
            self.samples.iter().map(|s| s.dirty_pixels).sum::<u64>()
        );
        for (index, name) in COUNTERS.iter().enumerate() {
            let count = self.samples.iter().map(|s| s.counters[index]).sum::<u64>();
            let _ = write!(
                text,
                "\nkms_{name}_total={}\nkms_{name}_window={count}",
                self.counters_total[index]
            );
        }
        for (stage, name) in STAGES {
            let samples: Vec<_> = self
                .samples
                .iter()
                .map(|s| s.stages[stage as usize])
                .filter(|s| s.calls > 0)
                .collect();
            let calls = samples.iter().map(|s| s.calls).sum::<u64>();
            let cpu_samples = samples.iter().filter(|s| s.elapsed.cpu.is_some()).count();
            let _ = write!(
                text,
                "\nkms_{name}_calls={calls}\nkms_{name}_cpu_samples={cpu_samples}"
            );
            if samples.is_empty() {
                continue;
            }
            let wall: Vec<_> = samples.iter().map(|s| s.elapsed.wall).collect();
            append_stats(&mut text, name, "wall", wall);
            let cpu: Vec<_> = samples.iter().filter_map(|s| s.elapsed.cpu).collect();
            if !cpu.is_empty() {
                append_stats(&mut text, name, "cpu", cpu);
            }
        }
        text
    }
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

fn append_stats(text: &mut String, stage: &str, clock: &str, mut values: Vec<Duration>) {
    values.sort_unstable();
    let total = values.iter().copied().sum::<Duration>();
    let p95 = (values.len() * 95).div_ceil(100) - 1;
    let _ = write!(text,
        "\nkms_{stage}_{clock}_total_ms={:.3}\nkms_{stage}_{clock}_avg_ms={:.3}\nkms_{stage}_{clock}_p95_ms={:.3}\nkms_{stage}_{clock}_max_ms={:.3}",
        millis(total), millis(total) / values.len() as f64, millis(values[p95]), millis(*values.last().unwrap()));
}

thread_local! { static PROFILE: RefCell<Profile> = RefCell::new(Profile::default()); }

pub(crate) fn activate(size: (u32, u32)) {
    PROFILE.with(|profile| {
        *profile.borrow_mut() = Profile {
            active: true,
            enabled: std::env::var_os("SPACEWARS_KMS_PROFILE").as_deref()
                != Some(std::ffi::OsStr::new("0")),
            draw_detail: std::env::var_os("SPACEWARS_KMS_DRAW_DETAIL").as_deref()
                != Some(std::ffi::OsStr::new("0")),
            size,
            samples: VecDeque::with_capacity(SAMPLE_LIMIT),
            ..Profile::default()
        };
    });
}

#[cfg(any(feature = "renderer-femtovg", test))]
pub(crate) fn activate_femtovg(
    size: (u32, u32),
    driver_info: Option<[String; 3]>,
    stencil_bits: Option<u8>,
) {
    activate(size);
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        profile.renderer = "femtovg";
        profile.gl_driver = driver_info;
        profile.gl_stencil_bits = stencil_bits;
        profile.buffer_mode = "gpu";
        profile.texture_mode = "gpu";
        profile.draw_detail = false;
    });
}

#[cfg(test)]
pub(crate) fn activate_for_test(size: (u32, u32)) {
    PROFILE.with(|profile| {
        *profile.borrow_mut() = Profile {
            active: true,
            enabled: true,
            draw_detail: true,
            size,
            ..Profile::default()
        };
    });
}

/// Snapshot completed loop iterations on the UI thread. Formatting/sorting is
/// only done on request, never in the rendering hot path. Empty before activation.
pub fn diagnostics() -> String {
    PROFILE.with(|profile| profile.borrow().diagnostics())
}

/// Completed DRM page flips on the UI thread. Available without profiling or
/// tracing; submission, render-loop iterations and UI timers do not advance it.
/// This is local display progress, not a claim about visible pixel correctness.
pub fn presented_frames() -> Option<u64> {
    PROFILE.with_borrow(|profile| profile.active.then_some(profile.presented_frames))
}

/// Bounded, explicitly armed CPU trace. Read/cancel never enable profiling.
/// Captures 240 complete event-loop iterations, then stops its extra clocks.
/// No file I/O, GL synchronization, shader modification or persistent settings.
pub fn render_trace(command: &str) -> Result<String, &'static str> {
    let result = PROFILE.with(|profile| {
        let mut p = profile.borrow_mut();
        match command {
            "start" | "start-text" | "start-work" | "start-geometry" => {
                if !p.enabled || p.renderer != "femtovg" {
                    return Err("render-trace requires active LinuxKMS FemtoVG profiling");
                }
                p.trace.start(command != "start", matches!(command, "start-work" | "start-geometry"))?;
                p.trace.geometry_detail = command == "start-geometry";
                Ok(p.trace.status())
            }
            "read" => Ok(p.trace.report()),
            "cancel" => {
                p.trace.cancel();
                Ok(p.trace.status())
            }
            _ => Err("render-trace requires start, start-text, start-work, start-geometry, read or cancel"),
        }
    });
    #[cfg(feature = "renderer-femtovg")]
    text::enable(PROFILE.with(|p| p.borrow().trace.text_recording()));
    result
}

/// Optional application marker on the current loop; zero means unspecified.
/// Used immediately before drawing to correlate UI state with the same sample.
pub fn render_trace_marker(marker: u8) {
    PROFILE.with(|profile| {
        let mut p = profile.borrow_mut();
        if p.trace.recording() {
            if let Some(sample) = &mut p.current {
                sample.trace_marker = marker;
            }
        }
    });
}

/// Record successful DRM submission, not GPU completion or physical latency.
/// Only populate the fixed trace fields and read a clock while recording.
pub(crate) fn page_flip_submitted() {
    PROFILE.with_borrow_mut(|p| {
        if !p.trace.recording() {
            return;
        }
        if let Some(current) = &mut p.current {
            current.presentation.submitted = p.trace.offset(Instant::now());
        }
    });
}

/// The DRM event's timestamp is the raw kernel vblank timestamp despite the
/// drm-rs field name `duration`. Sequence differences wrap at u32::MAX.
pub(crate) fn page_flip_completed(sequence: u32, kernel_time: Duration) {
    PROFILE.with_borrow_mut(|p| {
        p.presented_frames = p.presented_frames.saturating_add(1);
        if !p.trace.recording() {
            return;
        }
        if let Some(current) = &mut p.current {
            let sample = &mut current.presentation;
            sample.flip_observed = p.trace.offset(Instant::now());
            sample.flip_sequence = Some(sequence);
            sample.flip_kernel_time = Some(kernel_time);
        }
    });
}

/// Small, unsorted snapshot for an occasional on-screen overlay. Draw CPU is
/// CPU time per completed drawing loop, not GPU time or a vsync-inclusive wait.
#[derive(Debug)]
pub struct OverlaySummary {
    pub renderer: &'static str,
    pub gl_renderer: Option<String>,
    pub draw_cpu_ms: Option<f64>,
}

pub fn overlay_summary() -> Option<OverlaySummary> {
    PROFILE.with(|profile| profile.borrow().overlay_summary())
}

impl Profile {
    fn overlay_summary(&self) -> Option<OverlaySummary> {
        if !self.active {
            return None;
        }
        let mut total = Duration::ZERO;
        let mut count = 0;
        let mut complete = self.enabled;
        for sample in &self.samples {
            let draw = sample.stages[Stage::Draw as usize];
            if draw.calls > 0 {
                count += 1;
                if let Some(cpu) = draw.elapsed.cpu {
                    total += cpu;
                } else {
                    complete = false;
                }
            }
        }
        Some(OverlaySummary {
            renderer: self.renderer,
            gl_renderer: self.gl_driver.as_ref().map(|info| info[1].clone()),
            draw_cpu_ms: (complete && count > 0).then(|| millis(total) / count as f64),
        })
    }
}

pub(crate) struct Scope {
    stage: Stage,
    start: Option<Stamp>,
}

impl Scope {
    pub(crate) fn new(stage: Stage) -> Self {
        let enabled = PROFILE.with(|p| p.borrow().enabled);
        let start = enabled.then(Stamp::now);
        if matches!(stage, Stage::Loop) && enabled {
            PROFILE.with(|p| {
                let mut p = p.borrow_mut();
                let now = start.as_ref().unwrap().wall;
                p.trace.begin_loop(now);
                p.current = Some(LoopSample {
                    presentation: PresentationSample {
                        loop_start: p.trace.offset(now),
                        ..PresentationSample::default()
                    },
                    ..LoopSample::default()
                });
            });
            #[cfg(feature = "renderer-femtovg")]
            text::enable(PROFILE.with(|p| p.borrow().trace.text_recording()));
        }
        Self { stage, start }
    }

    /// Expensive per-GL-call clocks are active only during an armed capture.
    #[cfg(any(feature = "renderer-femtovg", test))]
    pub(crate) fn trace(stage: Stage) -> Self {
        let enabled = PROFILE.with(|p| p.borrow().trace.recording());
        Self {
            stage,
            start: enabled.then(Stamp::now),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let elapsed = start.elapsed();
            PROFILE.with(|p| {
                let mut profile = p.borrow_mut();
                if matches!(self.stage, Stage::Loop) {
                    profile.finish_loop(elapsed);
                } else {
                    profile.record(self.stage, elapsed);
                }
            });
            #[cfg(feature = "renderer-femtovg")]
            if matches!(self.stage, Stage::Loop) {
                text::enable(PROFILE.with(|p| p.borrow().trace.text_recording()));
            }
        }
    }
}

pub(crate) fn count(counter: Counter) {
    PROFILE.with(|p| p.borrow_mut().count(counter));
}

pub(crate) fn copied(bytes: usize) {
    PROFILE.with(|p| p.borrow_mut().add(Counter::CopyBytes, bytes as u64));
}

#[cfg(feature = "renderer-femtovg")]
pub(crate) fn gl_uploaded(bytes: u64) {
    PROFILE.with(|p| p.borrow_mut().add(Counter::GlUploadBytes, bytes));
}

#[cfg(feature = "renderer-femtovg")]
pub(crate) fn gl_buffer_uploaded(bytes: u64) {
    PROFILE.with(|p| p.borrow_mut().add(Counter::GlBufferBytes, bytes));
}

#[cfg(feature = "renderer-femtovg")]
pub(crate) fn enabled() -> bool {
    PROFILE.with(|p| p.borrow().enabled)
}

pub(crate) fn rgb_blit(pixels: usize) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.count(Counter::RgbBlits);
        p.add(Counter::RgbPixels, pixels as u64);
    });
}

pub(crate) fn texture_mode(mode: &'static str) {
    PROFILE.with(|p| p.borrow_mut().texture_mode = mode);
}

#[cfg(feature = "renderer-software")]
pub(crate) fn draw_observer() -> Option<i_slint_core::software_renderer::DrawDiagnosticObserver> {
    PROFILE.with(|p| (p.borrow().enabled && p.borrow().draw_detail).then_some(draw_event as _))
}

#[cfg(feature = "renderer-software")]
fn draw_event(operation: i_slint_core::software_renderer::DrawDiagnostic, begin: bool) {
    use i_slint_core::software_renderer::{DrawDiagnostic as D, DRAW_DIAGNOSTIC_COUNT};
    // Fixed storage. Nested instances of the same operation are timed as one
    // outer span, avoiding double-counting recursive rendering.
    #[derive(Clone, Copy, Default)]
    struct Active {
        start: Option<Stamp>,
        depth: usize,
    }
    thread_local! {
        static ACTIVE: RefCell<[Active; DRAW_DIAGNOSTIC_COUNT]> =
            RefCell::new([Active::default(); DRAW_DIAGNOSTIC_COUNT]);
    }
    let stage = match operation {
        D::Render => Stage::CoreRender,
        D::Prepare => Stage::CorePrepare,
        D::DirtyRegion => Stage::CoreDirty,
        D::Items => Stage::CoreItems,
        D::Image => Stage::CoreImage,
        D::Texture => Stage::CoreTexture,
        D::TextureFallback => Stage::CoreFallback,
        D::Rectangle => Stage::CoreRectangle,
        D::Text => Stage::CoreText,
        D::Path => Stage::CorePath,
        D::Background => Stage::CoreBackground,
        D::TextFont => Stage::CoreTextFont,
        D::TextGlyphRun => Stage::CoreTextGlyphRun,
    };
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let inside_text = active[D::Text as usize].depth > 0;
        let active = &mut active[operation as usize];
        if begin {
            if active.depth == 0 {
                // Glyph painting has nested texture and fallback spans. CPU
                // clock reads distort the measurement; keep CPU clocks on the
                // coarser Scope stages and use monotonic time for core hooks.
                active.start = Some(Stamp::wall_only());
            }
            active.depth += 1;
        } else if active.depth > 0 {
            active.depth -= 1;
            if active.depth == 0 {
                if let Some(start) = active.start.take() {
                    let elapsed = start.elapsed();
                    PROFILE.with(|p| {
                        let mut p = p.borrow_mut();
                        p.record(stage, elapsed);
                        // Reuse the same elapsed span, without another clock
                        // pair per glyph, to separate text from image textures.
                        if inside_text {
                            match operation {
                                D::Texture => p.record(Stage::CoreTextTexture, elapsed),
                                D::TextureFallback => p.record(Stage::CoreTextFallback, elapsed),
                                _ => {}
                            }
                        }
                    });
                }
            }
        }
    });
}

pub(crate) fn render_target(mode: &'static str, shadow_bytes: usize) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.buffer_mode = mode;
        p.shadow_bytes = shadow_bytes;
    });
}

pub(crate) fn buffer(age: u8, bytes: usize, format: &'static str, dirty_pixels: u64) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        if !p.enabled {
            return;
        }
        p.buffer_age = age;
        p.buffer_bytes = bytes;
        p.format = format;
        if let Some(sample) = &mut p.current {
            sample.dirty_pixels += dirty_pixels;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_profile_names_the_path_without_claiming_gpu_time_or_memory() {
        activate_femtovg(
            (480, 800),
            Some(["vendor", "driver", "version"].map(str::to_owned)),
            Some(8),
        );
        let text = diagnostics();
        assert!(text.contains("kms_renderer=femtovg\n"));
        assert!(text.contains("kms_gl_renderer=driver\n"));
        assert!(text.contains("kms_gl_stencil_bits=8\n"));
        assert!(text.contains("kms_gpu_memory_measured=false"));
        assert!(text.contains("kms_buffer_mode=gpu\n"));
        assert!(text.contains("kms_draw_detail=false\n"));
        assert!(!text.contains("kms_egl_swap_cpu_avg_ms"));
        activate_for_test((800, 480));
        assert!(diagnostics().contains("kms_renderer=software\n"));
        assert!(!diagnostics().contains("kms_gl_renderer="));
        assert!(!diagnostics().contains("kms_gl_stencil_bits="));
    }

    fn elapsed(wall: u64, cpu: Option<u64>) -> Elapsed {
        Elapsed {
            wall: Duration::from_millis(wall),
            cpu: cpu.map(Duration::from_millis),
        }
    }

    #[test]
    fn overlay_uses_completed_draw_cpu_not_waits_or_idle_loops() {
        let mut p = Profile {
            active: true,
            enabled: true,
            ..Profile::default()
        };
        assert!(p.overlay_summary().unwrap().draw_cpu_ms.is_none());
        for cpu in [2, 4] {
            p.current = Some(LoopSample::default());
            p.record(Stage::Draw, elapsed(15, Some(cpu)));
            p.finish_loop(elapsed(20, Some(cpu + 1)));
        }
        p.current = Some(LoopSample::default());
        p.finish_loop(elapsed(100, Some(0)));
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(999, Some(999)));
        assert_eq!(p.overlay_summary().unwrap().draw_cpu_ms, Some(3.0));
        p.finish_loop(elapsed(999, Some(999)));
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(1, None));
        p.finish_loop(elapsed(1, None));
        assert!(p.overlay_summary().unwrap().draw_cpu_ms.is_none());
        p.enabled = false;
        assert!(p.overlay_summary().unwrap().draw_cpu_ms.is_none());
        p.active = false;
        assert!(p.overlay_summary().is_none());
    }

    #[test]
    fn completed_windows_are_bounded_and_do_not_mix_inflight_work() {
        let mut p = Profile {
            active: true,
            enabled: true,
            ..Profile::default()
        };
        for i in 0..150 {
            p.current = Some(LoopSample::default());
            p.record(Stage::Draw, elapsed(i, Some(2)));
            p.count(Counter::Draws);
            p.finish_loop(elapsed(i + 5, Some(3)));
        }
        assert_eq!(p.samples.len(), SAMPLE_LIMIT);
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(9999, None));
        let text = p.diagnostics();
        assert!(text.contains("kms_draw_wall_avg_ms=89.500"));
        assert!(text.contains("kms_draw_wall_p95_ms=143.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=2.000"));
        assert!(text.contains("kms_draws_total=150\nkms_draws_window=120"));
        assert!(text.contains("kms_loops_total=150"));
    }

    #[test]
    fn idle_loops_waits_and_missing_cpu_clocks_are_explicit() {
        let mut p = Profile {
            active: true,
            enabled: true,
            ..Profile::default()
        };
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(4, None));
        p.count(Counter::Draws);
        p.finish_loop(elapsed(10, None));
        p.current = Some(LoopSample::default());
        p.record(Stage::Dispatch, elapsed(50, Some(0)));
        p.finish_loop(elapsed(50, Some(0)));
        let text = p.diagnostics();
        assert!(text.contains("kms_window_wall_ms=60.000"));
        assert!(text.contains("kms_draw_calls=1\nkms_draw_cpu_samples=0"));
        assert!(!text.contains("kms_draw_cpu_avg_ms"));
        assert!(text.contains("kms_dispatch_wall_avg_ms=50.000"));
        assert!(text.contains("kms_dispatch_cpu_avg_ms=0.000"));
    }

    #[test]
    fn counters_distinguish_initial_modesets_submissions_and_completions() {
        let mut p = Profile {
            active: true,
            enabled: true,
            ..Profile::default()
        };
        p.current = Some(LoopSample::default());
        p.count(Counter::Modesets);
        p.count(Counter::FlipSubmissions);
        p.count(Counter::FlipSubmissions);
        p.count(Counter::FlipCompletions);
        p.count(Counter::FlipReadErrors);
        p.finish_loop(elapsed(1, Some(1)));
        let text = p.diagnostics();
        assert!(text.contains("kms_modesets_window=1"));
        assert!(text.contains("kms_flip_submissions_window=2"));
        assert!(text.contains("kms_flip_completions_window=1"));
        assert!(text.contains("kms_flip_read_errors_window=1"));
    }

    #[test]
    fn copy_accounting_records_bytes_and_keeps_background_nested_in_draw() {
        let mut p = Profile {
            active: true,
            enabled: true,
            ..Profile::default()
        };
        p.current = Some(LoopSample::default());
        p.record(Stage::Background, elapsed(2, Some(2)));
        p.record(Stage::Draw, elapsed(5, Some(5)));
        p.record(Stage::Copy, elapsed(1, Some(1)));
        p.add(Counter::CopyBytes, 3 * 1024 * 1024);
        p.finish_loop(elapsed(7, Some(6)));
        let text = p.diagnostics();
        assert!(text.contains("kms_background_cpu_avg_ms=2.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=5.000"));
        assert!(text.contains("kms_copy_cpu_avg_ms=1.000"));
        assert!(text.contains("kms_copy_bytes_window=3145728"));
    }

    #[test]
    fn rgb_blit_accounting_is_nested_in_draw_and_counts_only_handled_pixels() {
        let mut p = Profile {
            active: true,
            enabled: true,
            texture_mode: "rgb",
            ..Profile::default()
        };
        p.current = Some(LoopSample::default());
        p.record(Stage::RgbBlit, elapsed(1, Some(1)));
        p.record(Stage::Draw, elapsed(4, Some(4)));
        p.count(Counter::RgbBlits);
        p.add(Counter::RgbPixels, 1024 * 768);
        p.finish_loop(elapsed(6, Some(6)));
        let text = p.diagnostics();
        assert!(text.contains("kms_profile_version=13"));
        assert!(text.contains("kms_texture_mode=rgb"));
        assert!(text.contains("kms_rgb_blit_cpu_avg_ms=1.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=4.000"));
        assert!(text.contains("kms_rgb_blits_window=1"));
        assert!(text.contains("kms_rgb_pixels_window=786432"));
        assert!(text.contains("kms_copy_bytes_window=0"));
    }

    #[cfg(feature = "renderer-software")]
    #[test]
    fn text_substages_exclude_image_textures_and_reuse_outer_spans() {
        use i_slint_core::software_renderer::DrawDiagnostic as D;
        activate_for_test((64, 64));
        {
            let _iteration = Scope::new(Stage::Loop);
            for (op, begin) in [
                (D::Image, true),
                (D::Texture, true),
                (D::Texture, false),
                (D::Image, false),
                (D::Text, true),
                (D::TextFont, true),
                (D::TextFont, false),
                (D::TextGlyphRun, true),
                (D::Texture, true),
                (D::Texture, true),
                (D::Texture, false),
                (D::TextureFallback, true),
                (D::TextureFallback, false),
                (D::Texture, false),
                (D::TextGlyphRun, false),
                (D::Text, false),
            ] {
                draw_event(op, begin);
            }
        }
        PROFILE.with(|p| {
            let p = p.borrow();
            let s = &p.samples[0].stages;
            assert_eq!(s[Stage::CoreTexture as usize].calls, 2);
            for stage in [
                Stage::CoreText,
                Stage::CoreTextFont,
                Stage::CoreTextGlyphRun,
                Stage::CoreTextTexture,
                Stage::CoreTextFallback,
            ] {
                assert_eq!(s[stage as usize].calls, 1);
                assert!(s[stage as usize].elapsed.cpu.is_none());
            }
            assert!(
                s[Stage::CoreText as usize].elapsed.wall
                    >= s[Stage::CoreTextFont as usize].elapsed.wall
                        + s[Stage::CoreTextGlyphRun as usize].elapsed.wall
            );
            assert!(
                s[Stage::CoreTextGlyphRun as usize].elapsed.wall
                    >= s[Stage::CoreTextTexture as usize].elapsed.wall
            );
            assert_eq!(
                s[Stage::CoreTextFallback as usize].elapsed.wall,
                s[Stage::CoreFallback as usize].elapsed.wall
            );
        });
        PROFILE.with(|p| *p.borrow_mut() = Profile::default());
    }

    #[test]
    fn disabled_and_uninitialized_profiles_do_not_report_measurements() {
        let mut p = Profile::default();
        assert!(p.diagnostics().is_empty());
        p.active = true;
        p.count(Counter::Draws);
        let text = p.diagnostics();
        assert!(text.contains("kms_profile_enabled=false"));
        assert!(text.contains("kms_draws_total=0"));
        assert!(!text.contains("kms_draw_wall_avg_ms"));
    }

    #[test]
    fn health_counts_completed_page_flips_without_optional_profiling() {
        PROFILE.with_borrow_mut(|p| *p = Profile::default());
        assert_eq!(presented_frames(), None);
        PROFILE.with_borrow_mut(|p| p.active = true);
        assert_eq!(presented_frames(), Some(0));
        page_flip_submitted();
        {
            let _iteration = Scope::new(Stage::Loop);
            count(Counter::Draws);
        }
        assert_eq!(presented_frames(), Some(0));
        page_flip_completed(u32::MAX, Duration::from_secs(10));
        page_flip_completed(0, Duration::from_secs(11));
        assert_eq!(presented_frames(), Some(2));
        PROFILE.with_borrow(|p| {
            assert!(!p.enabled);
            assert!(!p.trace.recording());
            assert!(p.samples.is_empty());
        });
        PROFILE.with_borrow_mut(|p| p.presented_frames = u64::MAX);
        page_flip_completed(1, Duration::from_secs(12));
        assert_eq!(presented_frames(), Some(u64::MAX));
        PROFILE.with_borrow_mut(|p| *p = Profile::default());
    }

    #[test]
    fn scopes_commit_completed_work_on_early_return() {
        PROFILE.with(|p| {
            *p.borrow_mut() = Profile {
                active: true,
                enabled: true,
                ..Profile::default()
            };
        });
        let render = || -> Result<(), ()> {
            let _iteration = Scope::new(Stage::Loop);
            let _render = Scope::new(Stage::Render);
            {
                let _draw = Scope::new(Stage::Draw);
                count(Counter::Draws);
                buffer(3, 4096, "XRGB8888", 1024);
            }
            assert!(diagnostics().contains("kms_loop_samples=0"));
            Err(())
        };
        assert_eq!(render(), Err(()));
        PROFILE.with(|p| {
            let p = p.borrow();
            assert!(p.current.is_none());
            assert_eq!(p.samples.len(), 1);
            let sample = &p.samples[0];
            for stage in [Stage::Loop, Stage::Render, Stage::Draw] {
                assert_eq!(sample.stages[stage as usize].calls, 1);
            }
            assert_eq!(sample.dirty_pixels, 1024);
            assert_eq!(p.buffer_age, 3);
            assert_eq!(p.buffer_bytes, 4096);
            assert_eq!(p.format, "XRGB8888");
        });
        PROFILE.with(|p| *p.borrow_mut() = Profile::default());
        {
            let _iteration = Scope::new(Stage::Loop);
            let _draw = Scope::new(Stage::Draw);
            count(Counter::Draws);
        }
        assert!(diagnostics().is_empty());
    }

    #[test]
    fn repeated_stages_sum_within_a_loop_and_preserve_unknown_cpu() {
        let mut sample = StageSample::default();
        sample.add(elapsed(3, Some(2)));
        sample.add(elapsed(5, Some(4)));
        assert_eq!(sample.calls, 2);
        assert_eq!(sample.elapsed.wall, Duration::from_millis(8));
        assert_eq!(sample.elapsed.cpu, Some(Duration::from_millis(6)));
        sample.add(elapsed(1, None));
        sample.add(elapsed(1, Some(1)));
        assert_eq!(sample.calls, 4);
        assert_eq!(sample.elapsed.wall, Duration::from_millis(10));
        assert_eq!(sample.elapsed.cpu, None);
    }
}
