use super::{
    AdmissionMode, JoinMode, Mode, PlanMode, StreamMode, StrokeMode, StrokeWriter,
    plan::{Scene, Shape},
    presentation::{self, Plan},
};
use crate::{
    MainWindow,
    raster::texture_upload,
    render::{FrameLayout, Viewport},
};
use engine_common::{RenderColor, RenderFrame};
use femtovg::{Canvas, ImageFlags, ImageId, Paint, Path, PixelFormat, RenderTarget};
use glow::HasContext;
use lyon_tessellation::RoundJoinStats;
use slint::{ComponentHandle, GraphicsAPI, Image};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[path = "composite_gpu.rs"]
pub(crate) mod composite;
#[path = "gl_state.rs"]
mod gl_state;
#[path = "triangles.rs"]
pub(crate) mod triangles;

#[derive(Default)]
struct State {
    mode: Mode,
    applied: Mode,
    stream: StreamMode,
    applied_stream: Option<StreamMode>,
    strokes: StrokeMode,
    applied_strokes: Option<StrokeMode>,
    joins: JoinMode,
    applied_joins: Option<JoinMode>,
    writer: StrokeWriter,
    applied_writer: Option<StrokeWriter>,
    admission: AdmissionMode,
    applied_admission: Option<AdmissionMode>,
    stream_usage: (usize, usize),
    plan_mode: PlanMode,
    scratch: presentation::scratch::Scratch,
    pending: Option<(Plan, Duration)>,
    replacement_ready: bool,
    error: Option<String>,
    frames: u64,
    allocations: u64,
    resident: usize,
    window: Measurements,
    published: Measurements,
    views: [(f32, usize, usize); 8],
    view_count: usize,
}

#[derive(Default, Clone, Copy)]
struct Measurements {
    frames: u32,
    shapes: usize,
    prepare: Duration,
    encode: Duration,
    submit: Duration,
    mesh: triangles::Stats,
    joins: RoundJoinStats,
}

#[derive(Clone, Copy, Default)]
struct GeometryPolicies {
    culling: StrokeMode,
    joins: JoinMode,
    writer: StrokeWriter,
    admission: AdmissionMode,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

impl State {
    /// Supported Pi 4/5 profile. Diagnostic defaults remain the reference so
    /// individual experiments can still be compared without hidden changes.
    fn select_default(&mut self) {
        self.select_mode(Mode::RetainedCulled);
        self.stream = StreamMode::Bucketed;
        self.plan_mode = PlanMode::Prepared;
        self.strokes = StrokeMode::Conservative;
        self.joins = JoinMode::Exact;
        self.writer = StrokeWriter::BulkStatic;
        self.admission = AdmissionMode::Repeated;
        super::source_control::select(super::SourceMode::Pooled);
    }

    fn select_mode(&mut self, mode: Mode) {
        super::source_control::reset();
        self.scratch.clear();
        self.mode = mode;
        if mode != Mode::RetainedCulled {
            self.strokes = StrokeMode::default();
            self.joins = JoinMode::default();
            self.writer = StrokeWriter::default();
            self.admission = AdmissionMode::default();
        }
        if mode == Mode::Off {
            self.plan_mode = PlanMode::default();
        }
        if !mode.supports_stream_control() {
            self.stream = StreamMode::default();
        }
        self.pending = None;
        self.error = None;
        self.window = Measurements::default();
        self.published = Measurements::default();
        self.view_count = 0;
    }

    fn select_stream(&mut self, stream: StreamMode) -> Result<(), String> {
        if !self.mode.supports_stream_control() {
            return Err("gpu-stream requires an active CPU-expanded GPU scene (triangles or non-resident retained modes)".into());
        }
        if self.stream != stream {
            self.stream = stream;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        Ok(())
    }

    fn select_strokes(&mut self, mode: StrokeMode) -> Result<(), String> {
        if mode != StrokeMode::Off && self.mode != Mode::RetainedCulled {
            return Err("gpu-strokes requires gpu-scene retained-culled".into());
        }
        if self.strokes != mode {
            self.strokes = mode;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        // Keep the pending plan and warm cache: neither depends on rejection.
        // Applied mode changes only after a successful drawing callback.
        Ok(())
    }

    fn select_plan(&mut self, mode: PlanMode) -> Result<(), String> {
        if self.mode == Mode::Off {
            return Err("gpu-plan requires an active GPU scene".into());
        }
        if self.plan_mode != mode {
            self.plan_mode = mode;
            self.scratch.clear();
            self.pending = None;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        Ok(())
    }

    fn select_joins(&mut self, mode: JoinMode) -> Result<(), String> {
        if mode != JoinMode::Reference && self.mode != Mode::RetainedCulled {
            return Err("gpu-joins requires gpu-scene retained-culled".into());
        }
        if self.joins != mode {
            self.joins = mode;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        // Exact output: preserve the pending plan and warm cache. Only a
        // successful render may change the reported applied mode.
        Ok(())
    }

    fn select_writer(&mut self, writer: StrokeWriter) -> Result<(), String> {
        if writer != StrokeWriter::Stock && self.mode != Mode::RetainedCulled {
            return Err("gpu-stroke-writer requires gpu-scene retained-culled".into());
        }
        if self.writer != writer {
            self.writer = writer;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        // Identical geometry: preserve the pending plan and warm cache, and
        // publish the applied writer only after a successful draw.
        Ok(())
    }

    fn select_admission(&mut self, mode: AdmissionMode) -> Result<(), String> {
        if mode != AdmissionMode::Repeated && self.mode != Mode::RetainedCulled {
            return Err("gpu-admission requires gpu-scene retained-culled".into());
        }
        if self.admission != mode {
            self.admission = mode;
            self.window = Measurements::default();
            self.published = Measurements::default();
        }
        Ok(())
    }

    fn take_plan_storage(&mut self) -> Option<Plan> {
        if !self.plan_mode.reuses_storage() {
            return None;
        }
        // A second host tick may replace a plan not yet drawn. It is no longer
        // needed, and recycling it avoids keeping a second idle allocation set.
        if let Some((old, _)) = self.pending.take() {
            self.scratch.recycle(old);
        }
        self.scratch.take()
    }
}

pub(super) fn requested() -> bool {
    STATE.with_borrow(|s| s.mode != Mode::Off)
}
pub(super) fn applied() -> bool {
    STATE.with_borrow(|s| s.applied != Mode::Off)
}

pub(super) fn request(window: &MainWindow, mode: Mode) -> Result<(), String> {
    if mode != Mode::Off {
        if texture_upload::requested() != texture_upload::Mode::Slint {
            return Err("select host raster-textures slint before enabling gpu-scene".into());
        }
        // One shared current-context callback, never competing rendering notifiers.
        texture_upload::request(window, texture_upload::Mode::Slint)?;
    }
    STATE.with_borrow_mut(|s| s.select_mode(mode));
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_default(window: &MainWindow) -> Result<(), String> {
    // Restore standard image publication if a previous session used a texture
    // lifetime experiment, then install the shared render callback.
    texture_upload::request(window, texture_upload::Mode::Slint)?;
    STATE.with_borrow_mut(State::select_default);
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_stream(window: &MainWindow, stream: StreamMode) -> Result<(), String> {
    // No GL mutation outside the rendering callback, no scene/cache reset,
    // no persistent settings and no change to application pacing.
    STATE.with_borrow_mut(|s| s.select_stream(stream))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_plan(window: &MainWindow, mode: PlanMode) -> Result<(), String> {
    STATE.with_borrow_mut(|s| s.select_plan(mode))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_strokes(window: &MainWindow, mode: StrokeMode) -> Result<(), String> {
    STATE.with_borrow_mut(|s| s.select_strokes(mode))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_joins(window: &MainWindow, mode: JoinMode) -> Result<(), String> {
    STATE.with_borrow_mut(|s| s.select_joins(mode))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_writer(window: &MainWindow, mode: StrokeWriter) -> Result<(), String> {
    STATE.with_borrow_mut(|s| s.select_writer(mode))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn request_admission(window: &MainWindow, mode: AdmissionMode) -> Result<(), String> {
    STATE.with_borrow_mut(|s| s.select_admission(mode))?;
    window.window().request_redraw();
    Ok(())
}

pub(super) fn end_session() {
    super::source_control::reset();
    STATE.with_borrow_mut(|s| {
        s.mode = Mode::Off;
        s.plan_mode = PlanMode::default();
        s.scratch.clear();
        s.pending = None;
        s.applied = Mode::Off;
        s.stream = StreamMode::default();
        s.applied_stream = None;
        s.strokes = StrokeMode::default();
        s.applied_strokes = None;
        s.joins = JoinMode::default();
        s.applied_joins = None;
        s.writer = StrokeWriter::default();
        s.applied_writer = None;
        s.admission = AdmissionMode::default();
        s.applied_admission = None;
        s.window = Measurements::default();
        s.published = Measurements::default();
        s.view_count = 0;
    });
}

fn fail(error: String) {
    super::source_control::reset();
    tracing::error!(%error, "direct GPU scene stopped; returning to raster on the next host frame");
    STATE.with_borrow_mut(|s| {
        s.mode = Mode::Off;
        s.plan_mode = PlanMode::default();
        s.scratch.clear();
        s.stream = StreamMode::default();
        s.pending = None;
        s.strokes = StrokeMode::default();
        s.window = Measurements::default();
        s.joins = JoinMode::default();
        s.published = Measurements::default();
        s.writer = StrokeWriter::default();
        s.error = Some(error.replace(['\r', '\n'], " "));
        s.admission = AdmissionMode::default();
    });
}

pub(super) fn stage(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
) -> bool {
    if !requested() {
        return false;
    }
    let started = Instant::now();
    let (retain, mode) = STATE.with_borrow(|s| (s.mode.retention().is_some(), s.plan_mode));
    let project_trace = crate::frame_trace::Scope::new(crate::frame_trace::Stage::SceneProject);
    let storage = STATE.with_borrow_mut(State::take_plan_storage);
    let projected =
        presentation::build_reusing(frames, viewport, layout, scale, retain, mode, storage);
    drop(project_trace);
    match projected {
        Ok(scene) => {
            let _stage_trace =
                crate::frame_trace::Scope::new(crate::frame_trace::Stage::SceneStage);
            STATE.with_borrow_mut(|s| {
                s.view_count = frames.len();
                for (i, frame) in frames.iter().enumerate() {
                    s.views[i] = (
                        frame.camera.height,
                        frame.geometry_hints.len(),
                        scene.passes.get(i).map_or(0, |p| p.scene().retained.len()),
                    );
                }
                s.pending = Some((scene, started.elapsed()));
            });
            true
        }
        Err(error) => {
            fail(error);
            false
        }
    }
}

pub(super) fn diagnostics() -> String {
    STATE.with_borrow(|s| {
        let p = s.published;
        let ms = |d: Duration| if p.frames == 0 { "unavailable".into() } else { format!("{:.3}", d.as_secs_f64() * 1000.0 / f64::from(p.frames)) };
        let mesh_avg = |v: u64| if p.mesh.samples == 0 || p.frames == 0 { "unavailable".into() } else { format!("{:.3}", v as f64 / f64::from(p.frames)) };
        let mut text = format!("\ngpu_scene_requested={}\ngpu_scene_applied={}\ngpu_scene_mode={}\ngpu_scene_applied_mode={}\ngpu_scene_error={}\ngpu_scene_frames_total={}\ngpu_scene_allocations_total={}\ngpu_scene_resident={}\ngpu_scene_window_frames={}\ngpu_scene_shapes={}\ngpu_scene_prepare_avg_ms={}\ngpu_scene_encode_avg_ms={}\ngpu_scene_submit_avg_ms={}\ngpu_scene_mesh_vertices={}\ngpu_scene_mesh_indices={}\ngpu_scene_msaa_samples={}\ngpu_scene_mesh_draws_avg={}\ngpu_scene_mesh_upload_bytes_avg={}\ngpu_scene_circle_instances={}\ngpu_scene_circle_template_bytes={}", s.mode != Mode::Off, s.applied != Mode::Off, s.mode.name(), s.applied.name(), s.error.as_deref().unwrap_or("none"), s.frames, s.allocations, s.resident, p.frames, p.shapes, ms(p.prepare), ms(p.encode), ms(p.submit), p.mesh.vertices, p.mesh.indices, p.mesh.samples, mesh_avg(p.mesh.draws), mesh_avg(p.mesh.upload_bytes), p.mesh.circle_instances, p.mesh.circle_template_bytes);
        text.push_str(&format!("\ngpu_scene_plan_policy={}", s.plan_mode.name()));
        text.push_str(&format!("\ngpu_scene_scratch_limit_bytes={}\ngpu_scene_scratch_idle_bytes={}\ngpu_scene_scratch_high_water_bytes={}\ngpu_scene_scratch_reuses={}\ngpu_scene_scratch_discards={}",
            presentation::scratch::LIMIT_BYTES, s.scratch.bytes, s.scratch.high_water_bytes, s.scratch.reuses, s.scratch.discards));
        let c = p.mesh.retained;
        text.push_str(&format!("\ngpu_scene_stroke_policy={}\ngpu_scene_stroke_applied={}\ngpu_scene_stroke_bound_tests_avg={}\ngpu_scene_stroke_bound_unknown_avg={}\ngpu_scene_stroke_bound_rejections_avg={}",
            s.strokes.name(), s.applied_strokes.map_or("inactive", StrokeMode::name),
            mesh_avg(c.stroke_bound_tests), mesh_avg(c.stroke_bound_unknown), mesh_avg(c.stroke_bound_rejections)));
        text.push_str(&format!("\ngpu_scene_join_policy={}\ngpu_scene_join_applied={}\ngpu_scene_join_shortcuts_avg={}\ngpu_scene_join_fallbacks_avg={}",
            s.joins.name(), s.applied_joins.map_or("inactive", JoinMode::name),
            mesh_avg(p.joins.shortcuts), mesh_avg(p.joins.fallbacks)));
        text.push_str(&format!("\ngpu_scene_stroke_writer_policy={}\ngpu_scene_stroke_writer_applied={}",
            s.writer.name(), s.applied_writer.map_or("inactive", StrokeWriter::name)));
        text.push_str(&format!("\ngpu_scene_admission_policy={}\ngpu_scene_admission_applied={}",
            s.admission.name(), s.applied_admission.map_or("inactive", AdmissionMode::name)));
        text.push_str(&format!("\ngpu_scene_stream_policy={}\ngpu_scene_stream_applied={}\ngpu_scene_stream_slots={}\ngpu_scene_stream_capacity_bytes={}", s.stream.name(), s.applied_stream.map_or("inactive", StreamMode::name), s.stream_usage.0, s.stream_usage.1));
        for (name, count) in super::streaming::NAMES.into_iter().zip(p.mesh.streaming.0) {
            let name = name.strip_prefix("stream_").unwrap_or(name);
            text.push_str(&format!("\ngpu_scene_stream_{name}_avg={}", mesh_avg(count)));
        }
        let r = p.mesh.resident;
        text.push_str(&format!("\ngpu_scene_atlas_entries={}\ngpu_scene_atlas_payload_bytes={}\ngpu_scene_atlas_upload_bytes_avg={}\ngpu_scene_cached_vertices_avg={}\ngpu_scene_streamed_vertices_avg={}\ngpu_scene_placements_avg={}\ngpu_scene_resident_fallback_groups={}", r.atlas_entries, r.atlas_payload_bytes, mesh_avg(r.atlas_upload_bytes as u64), mesh_avg(r.cached_vertices as u64), mesh_avg(r.streamed_vertices as u64), mesh_avg(r.placements as u64), r.fallback_groups));
        text.push_str(&format!("\ngpu_scene_cache_scale_misses={}", c.scale_misses));
        text.push_str(&format!("\ngpu_scene_cache_build_reuses={}", c.build_reuses));
        text.push_str(&format!("\ngpu_scene_culled_pieces={}\ngpu_scene_culled_indices={}", c.culled_pieces, c.culled_indices));
        text.push_str(&format!("\ngpu_scene_cache_admissions={}\ngpu_scene_cache_bypasses={}\ngpu_scene_cache_probation_entries={}", c.admissions, c.bypasses, c.probation_entries));
        text.push_str(&format!("\ngpu_scene_cache_build_avg_ms={}\ngpu_scene_cache_build_max_ms={:.3}\ngpu_scene_cache_build_work={}\ngpu_scene_cache_deferred={}\ngpu_scene_cache_oversized={}", ms(Duration::from_nanos(c.build_ns)), c.max_build_ns as f64 / 1e6, c.build_work, c.deferred, c.oversized));
        text.push_str(&format!("\ngpu_scene_zoom_reused_polygons={}\ngpu_scene_zoom_fallback_polygons={}", c.zoom_reused_polygons, c.zoom_fallback_polygons));
        text.push_str(&format!("\ngpu_scene_zoom_reused_fills={}\ngpu_scene_zoom_reused_strokes={}", c.zoom_reused_fills, c.zoom_reused_strokes));
        text.push_str(&format!("\ngpu_scene_hybrid_exact_groups={}\ngpu_scene_hybrid_zoom_groups={}\ngpu_scene_hybrid_fallback_groups={}\ngpu_scene_cache_promotions={}\ngpu_scene_cache_exact_entries={}\ngpu_scene_cache_zoom_entries={}", c.hybrid_exact_groups, c.hybrid_zoom_groups, c.hybrid_fallback_groups, c.promotions, c.exact_entries, c.zoom_entries));
        for (i, &(height, source, accepted)) in s.views[..s.view_count].iter().enumerate() {
            text.push_str(&format!("\ngpu_scene_view_{i}_height={height:.8}\ngpu_scene_view_{i}_source_groups={source}\ngpu_scene_view_{i}_accepted_groups={accepted}"));
        }
        text.push_str(&format!("\ngpu_scene_cache_hits={}\ngpu_scene_cache_misses={}\ngpu_scene_cache_evictions={}\ngpu_scene_cache_fallbacks={}\ngpu_scene_cache_entries={}\ngpu_scene_cache_bytes={}", c.hits, c.misses, c.evictions, c.fallbacks, c.entries, c.bytes));
        text
    })
}

pub(crate) fn before_rendering(
    window: &MainWindow,
    api: &GraphicsAPI<'_>,
    presenter: &mut Option<Presenter>,
) -> bool {
    STATE.with_borrow_mut(|s| s.replacement_ready = false);
    let Some((scene, prepare)) = STATE.with_borrow_mut(|s| s.pending.take()) else {
        return false;
    };
    let (mode, stream, strokes) = STATE.with_borrow(|s| {
        (
            s.mode,
            s.stream,
            GeometryPolicies {
                culling: s.strokes,
                joins: s.joins,
                writer: s.writer,
                admission: s.admission,
            },
        )
    });
    let draw_trace = crate::frame_trace::Scope::new(crate::frame_trace::Stage::SceneDraw);
    let result = (|| {
        if presenter.is_none() {
            *presenter = Some(Presenter::new(api)?);
        }
        presenter
            .as_mut()
            .unwrap()
            .draw(&scene, mode, stream, strokes, api)
    })();
    drop(draw_trace);
    match result {
        Ok((output, counts)) => {
            window.set_raster_frame(output.image);
            STATE.with_borrow_mut(|s| {
                s.applied = mode;
                s.applied_stream = mode.supports_stream_control().then_some(stream);
                s.applied_strokes = (mode == Mode::RetainedCulled).then_some(strokes.culling);
                s.applied_joins = (mode == Mode::RetainedCulled).then_some(strokes.joins);
                s.applied_writer = (mode == Mode::RetainedCulled).then_some(strokes.writer);
                s.applied_admission = (mode == Mode::RetainedCulled).then_some(strokes.admission);
                s.replacement_ready = true;
                s.frames += 1;
                s.window.frames += 1;
                s.window.shapes = scene.shapes();
                s.window.prepare += prepare;
                s.window.encode += output.encode;
                s.window.submit += output.submit;
                s.window.mesh.vertices = output.mesh.vertices;
                s.window.mesh.indices = output.mesh.indices;
                s.window.mesh.samples = output.mesh.samples;
                s.window.mesh.circle_instances = output.mesh.circle_instances;
                s.window.mesh.circle_template_bytes = output.mesh.circle_template_bytes;
                s.window.mesh.draws += output.mesh.draws;
                s.window.mesh.upload_bytes += output.mesh.upload_bytes;
                s.window.mesh.retained.accumulate(output.mesh.retained);
                s.window.mesh.resident.accumulate(output.mesh.resident);
                s.window.mesh.streaming.accumulate(output.mesh.streaming);
                s.window.joins.shortcuts =
                    s.window.joins.shortcuts.saturating_add(counts.shortcuts);
                s.window.joins.fallbacks =
                    s.window.joins.fallbacks.saturating_add(counts.fallbacks);
                if s.window.frames == 120 {
                    s.published = std::mem::take(&mut s.window);
                }
            });
        }
        Err(error) => fail(error), // Keep last image alive until CPU replacement arrives.
    }
    let _plan_drop = crate::frame_trace::Scope::new(crate::frame_trace::Stage::PlanDrop);
    STATE.with_borrow_mut(|s| {
        if s.plan_mode.reuses_storage() {
            s.scratch.recycle(scene);
        } else {
            drop(scene);
        }
    });
    true
}

pub(crate) fn after_rendering(presenter: &mut Option<Presenter>, raster_replaced: bool) {
    let geometry_replaced = STATE.with_borrow_mut(|s| std::mem::take(&mut s.replacement_ready));
    if let Some(p) = presenter.as_mut() {
        if raster_replaced {
            p.retire();
            STATE.with_borrow_mut(|s| {
                s.applied = Mode::Off;
                s.applied_stream = None;
                s.applied_strokes = None;
                s.applied_joins = None;
                s.applied_writer = None;
            });
        }
        // Failed resize/draw must retain the texture still borrowed by Slint.
        if raster_replaced || geometry_replaced {
            p.collect_retired();
        }
        STATE.with_borrow_mut(|s| s.stream_usage = p.stream_usage());
    }
}

pub(crate) fn teardown(presenter: &mut Option<Presenter>) {
    super::source_control::reset();
    if let Some(p) = presenter.take() {
        // SAFETY: RenderingTeardown runs with the owning window context current.
        let _guard = unsafe { gl_state::Guard::new(p.gl.clone()) };
        drop(p);
    }
    STATE.with_borrow_mut(|s| {
        s.plan_mode = PlanMode::default();
        s.scratch.clear();
        s.pending = None;
        s.applied = Mode::Off;
        s.applied_stream = None;
        s.strokes = StrokeMode::default();
        s.applied_strokes = None;
        s.joins = JoinMode::default();
        s.applied_joins = None;
        s.window = Measurements::default();
        s.writer = StrokeWriter::default();
        s.applied_writer = None;
        s.published = Measurements::default();
        s.admission = AdmissionMode::default();
        s.applied_admission = None;
        s.stream = StreamMode::default();
        s.stream_usage = (0, 0);
    });
}

pub(crate) struct Presenter {
    gl: Rc<glow::Context>,
    paths: Option<PathPresenter>,
    triangles: Option<triangles::Presenter>,
    composite: Option<composite::Presenter>,
}

pub(crate) struct DrawOutput {
    pub image: Image,
    pub encode: Duration,
    pub submit: Duration,
    pub mesh: triangles::Stats,
}

fn target_allocated() {
    STATE.with_borrow_mut(|s| {
        s.allocations += 1;
        s.resident += 1;
    });
}
fn target_deleted() {
    STATE.with_borrow_mut(|s| s.resident -= 1);
}

pub(crate) fn target_counts() -> (u64, usize) {
    STATE.with_borrow(|s| (s.allocations, s.resident))
}

/// Offscreen integration diagnostic using the actual post-render retirement
/// callback. In particular, a failed resized draw is NOT a replacement image.
pub(crate) fn validate_retirement(api: &GraphicsAPI<'_>, plan: &Plan) -> Result<(), String> {
    use crate::gpu_scene::presentation::{Clip, Pass};
    let baseline = target_counts().1;
    let mut owner = Some(Presenter::new(api)?);
    let gl = owner.as_ref().unwrap().gl.clone();
    let _guard = unsafe { gl_state::Guard::new(gl) };
    owner.as_mut().unwrap().draw(
        plan,
        Mode::Triangles,
        StreamMode::Bucketed,
        GeometryPolicies::default(),
        api,
    )?;
    if owner.as_ref().unwrap().stream_usage().0 == 0 {
        return Err("live bucketed policy did not allocate a stream".into());
    }
    STATE.with_borrow_mut(|s| s.replacement_ready = true);
    after_rendering(&mut owner, false);
    let resident = target_counts().1;
    let make_pass = || Pass::Direct {
        scene: Scene {
            width: 1,
            height: 1,
            draws: Vec::new(),
            reuse: Vec::new(),
            retained: Vec::new(),
        },
        clip: Clip {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
    };
    // Valid new target, deliberately mismatched pass sizes: fail AFTER allocation.
    let bad = Plan {
        width: plan.width + 1,
        height: plan.height,
        passes: vec![make_pass(), make_pass()],
    };
    if owner
        .as_mut()
        .unwrap()
        .draw(
            &bad,
            Mode::Triangles,
            StreamMode::Exact,
            GeometryPolicies::default(),
            api,
        )
        .is_ok()
    {
        return Err("invalid resized presentation unexpectedly succeeded".into());
    }
    after_rendering(&mut owner, false);
    if target_counts().1 != resident + 1 {
        return Err("failed resized draw released the still-borrowed target".into());
    }
    let single = Plan {
        width: 1,
        height: 1,
        passes: vec![make_pass()],
    };
    owner.as_mut().unwrap().draw(
        &single,
        Mode::Triangles,
        StreamMode::Bucketed,
        GeometryPolicies::default(),
        api,
    )?;
    STATE.with_borrow_mut(|s| s.replacement_ready = true);
    after_rendering(&mut owner, false);
    if target_counts().1 != baseline + 1 {
        return Err("successful replacement failed to retire previous layout".into());
    }
    after_rendering(&mut owner, true); // Ordinary CPU image replaces GPU image.
    if owner.as_ref().unwrap().stream_usage().0 != 0 {
        return Err("CPU fallback retained live stream pools".into());
    }
    if target_counts().1 != baseline {
        return Err("CPU fallback retained GPU targets".into());
    }
    owner.as_mut().unwrap().draw(
        plan,
        Mode::Circles,
        StreamMode::Shared,
        GeometryPolicies::default(),
        api,
    )?;
    teardown(&mut owner);
    if target_counts().1 != baseline {
        return Err("context teardown retained targets".into());
    }
    Ok(())
}

impl Presenter {
    fn new(api: &GraphicsAPI<'_>) -> Result<Self, String> {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return Err("direct scene requires native OpenGL".into());
        };
        // SAFETY: Rendering notifier has made this window's context current.
        let gl = Rc::new(unsafe { glow::Context::from_loader_function_cstr(get_proc_address) });
        let version = gl.version();
        let minimum = if version.is_embedded { (3, 1) } else { (3, 3) };
        if (version.major, version.minor) < minimum {
            return Err("direct scene prototype requires GLES 3.1 or desktop OpenGL 3.3".into());
        }
        Ok(Self {
            gl,
            paths: None,
            triangles: None,
            composite: None,
        })
    }

    fn draw(
        &mut self,
        plan: &Plan,
        mode: Mode,
        stream: StreamMode,
        strokes: GeometryPolicies,
        api: &GraphicsAPI<'_>,
    ) -> Result<(DrawOutput, RoundJoinStats), String> {
        if stream != StreamMode::Shared && !mode.supports_stream_control() {
            return Err("selected geometry mode does not support gpu-stream".into());
        }
        if strokes.culling != StrokeMode::Off && mode != Mode::RetainedCulled {
            return Err("selected geometry mode does not support gpu-strokes".into());
        }
        let joins = strokes.joins;
        if joins != JoinMode::Reference && mode != Mode::RetainedCulled {
            return Err("selected geometry mode does not support gpu-joins".into());
        }
        if strokes.writer != StrokeWriter::Stock && mode != Mode::RetainedCulled {
            return Err("selected geometry mode does not support gpu-stroke-writer".into());
        }
        if strokes.admission != AdmissionMode::Repeated && mode != Mode::RetainedCulled {
            return Err("selected geometry mode does not support gpu-admission".into());
        }
        let mut counts = RoundJoinStats::default();
        let output = match mode {
            Mode::Paths => {
                let scene = plan.single_scene().ok_or(
                    "paths reference supports one full-window view; use triangles for compositing",
                )?;
                if self.paths.is_none() {
                    self.paths = Some(PathPresenter::new(api)?);
                }
                let (image, encode, submit) = self.paths.as_mut().unwrap().draw(scene)?;
                DrawOutput {
                    image,
                    encode,
                    submit,
                    mesh: triangles::Stats::default(),
                }
            }
            Mode::Triangles
            | Mode::Circles
            | Mode::Retained
            | Mode::RetainedCold
            | Mode::RetainedAdaptive
            | Mode::RetainedZoom
            | Mode::RetainedHybrid
            | Mode::RetainedOutlines
            | Mode::RetainedPaced
            | Mode::RetainedMemoized
            | Mode::RetainedScan
            | Mode::RetainedRanges
            | Mode::RetainedCulled
            | Mode::RetainedResident
            | Mode::RetainedResidentPasses
            | Mode::RetainedResidentBuffers
            | Mode::RetainedResidentUniforms => {
                if let Some(scene) = plan.single_scene() {
                    if self.triangles.is_none() {
                        self.triangles = Some(triangles::Presenter::new(self.gl.clone())?);
                    }
                    let renderer = self.triangles.as_mut().unwrap();
                    renderer.set_instanced_circles(mode == Mode::Circles);
                    renderer.set_retention(mode.retention());
                    renderer.set_stream_policy(stream.policy());
                    renderer.set_stroke_culling(strokes.culling);
                    renderer.set_stroke_writer(strokes.writer);
                    renderer.set_admission(strokes.admission);
                    renderer.set_round_join_mode(joins.policy());
                    let before =
                        (joins != JoinMode::Reference).then(|| renderer.round_join_stats());
                    let output = renderer.draw(scene)?;
                    if let Some(before) = before {
                        counts = super::join_control::delta(renderer.round_join_stats(), before);
                    }
                    output
                } else {
                    if self.composite.is_none() {
                        self.composite = Some(composite::Presenter::new(self.gl.clone())?);
                    }
                    let renderer = self.composite.as_mut().unwrap();
                    renderer.set_stream_policy(stream.policy());
                    renderer.set_stroke_culling(strokes.culling);
                    renderer.set_stroke_writer(strokes.writer);
                    renderer.set_admission(strokes.admission);
                    renderer.set_round_join_mode(joins.policy());
                    let before =
                        (joins != JoinMode::Reference).then(|| renderer.round_join_stats());
                    let output = renderer.draw_with_mode(plan, mode)?;
                    if let Some(before) = before {
                        counts = super::join_control::delta(renderer.round_join_stats(), before);
                    }
                    output
                }
            }
            Mode::Off => return Err("direct scene disabled before drawing".into()),
        };
        // Only retire the other path after the replacement succeeded; deletion
        // waits until Slint submits that replacement in AfterRendering.
        if mode != Mode::Paths
            && let Some(p) = self.paths.as_mut()
        {
            p.retire();
        }
        if (mode == Mode::Paths || plan.single_scene().is_none())
            && let Some(p) = self.triangles.as_mut()
        {
            p.set_stream_policy(Default::default());
            p.set_stroke_culling(StrokeMode::Off);
            p.set_round_join_mode(JoinMode::Reference.policy());
            p.set_stroke_writer(StrokeWriter::Stock);
            p.retire();
            p.set_admission(AdmissionMode::Repeated);
        }
        if (mode == Mode::Paths || plan.single_scene().is_some())
            && let Some(p) = self.composite.as_mut()
        {
            p.retire();
        }
        Ok((output, counts))
    }

    fn retire(&mut self) {
        if let Some(p) = self.paths.as_mut() {
            p.retire();
        }
        if let Some(p) = self.triangles.as_mut() {
            p.set_admission(AdmissionMode::Repeated);
            p.clear_retained();
            p.set_stream_policy(Default::default());
            p.set_stroke_culling(StrokeMode::Off);
            p.set_round_join_mode(JoinMode::Reference.policy());
            p.set_stroke_writer(StrokeWriter::Stock);
            p.retire();
        }
        if let Some(p) = self.composite.as_mut() {
            p.retire();
        }
    }

    fn stream_usage(&self) -> (usize, usize) {
        let a = self.triangles.as_ref().map_or((0, 0), |p| p.stream_usage());
        let b = self.composite.as_ref().map_or((0, 0), |p| p.stream_usage());
        (a.0 + b.0, a.1 + b.1)
    }
    fn collect_retired(&mut self) {
        if let Some(p) = self.paths.as_mut() {
            p.collect_retired();
        }
        if let Some(p) = self.triangles.as_mut() {
            p.collect_retired();
        }
        if let Some(p) = self.composite.as_mut() {
            p.collect_retired();
        }
    }
}

struct PathPresenter {
    gl: Rc<glow::Context>,
    canvas: Canvas<femtovg::renderer::OpenGl>,
    target: Option<ImageId>,
    size: (u32, u32),
    retired: Vec<ImageId>,
}

impl PathPresenter {
    fn new(api: &GraphicsAPI<'_>) -> Result<Self, String> {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return Err("direct scene requires native OpenGL".into());
        };
        // SAFETY: The same current-context callback used for borrowed textures.
        let gl = Rc::new(unsafe { glow::Context::from_loader_function_cstr(get_proc_address) });
        if gl.version().major < 3 {
            return Err("direct scene prototype requires OpenGL/GLES 3".into());
        }
        let _guard = unsafe { gl_state::Guard::new(gl.clone()) };
        let renderer =
            unsafe { femtovg::renderer::OpenGl::new_from_function_cstr(get_proc_address) }
                .map_err(|e| e.to_string())?;
        let canvas = Canvas::new(renderer).map_err(|e| e.to_string())?;
        Ok(Self {
            gl,
            canvas,
            target: None,
            size: (0, 0),
            retired: Vec::new(),
        })
    }

    fn retire(&mut self) {
        if let Some(target) = self.target.take() {
            self.retired.push(target);
        }
    }

    fn collect_retired(&mut self) {
        if self.retired.is_empty() {
            return;
        }
        // SAFETY: Called after Slint submitted the replacement image in this context.
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        for target in self.retired.drain(..) {
            self.canvas.delete_image(target);
            target_deleted();
        }
    }

    fn draw(&mut self, scene: &Scene) -> Result<(Image, Duration, Duration), String> {
        // The guard restores state on every return, including GL/allocation errors.
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        let limit = unsafe { self.gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) };
        if scene.width > limit as u32 || scene.height > limit as u32 {
            return Err("GPU scene exceeds texture dimensions".into());
        }
        if self.target.is_none() || self.size != (scene.width, scene.height) {
            let replacement = self
                .canvas
                .create_image_empty(
                    scene.width as usize,
                    scene.height as usize,
                    PixelFormat::Rgba8,
                    ImageFlags::empty(),
                )
                .map_err(|e| e.to_string())?;
            self.retire();
            self.size = (scene.width, scene.height);
            self.target = Some(replacement);
            target_allocated();
        }
        let target = self.target.unwrap();
        let started = Instant::now();
        self.canvas.set_size(scene.width, scene.height, 1.0);
        self.canvas.reset();
        // Force an FBO bind each frame: Slint used a different FBO in between.
        self.canvas.set_render_target(RenderTarget::Screen);
        self.canvas.set_render_target(RenderTarget::Image(target));
        self.canvas.clear_rect(
            0,
            0,
            scene.width,
            scene.height,
            femtovg::Color::rgb(5, 5, 20),
        );
        self.canvas
            .scissor(0.0, 0.0, scene.width as f32, scene.height as f32);
        for draw in &scene.draws {
            let mut path = Path::new();
            match &draw.shape {
                Shape::Circle { center, radius } => path.circle(center.x, center.y, *radius),
                Shape::Line { start, end } => {
                    path.move_to(start.x, start.y);
                    path.line_to(end.x, end.y);
                }
                Shape::Polygon(points) => {
                    path.move_to(points[0].x, points[0].y);
                    for p in &points[1..] {
                        path.line_to(p.x, p.y);
                    }
                    path.close();
                }
                Shape::LocalPolygon(polygon) => {
                    for (i, p) in polygon.definition.points().iter().enumerate() {
                        let p = polygon.project(*p);
                        if i == 0 {
                            path.move_to(p.x, p.y);
                        } else {
                            path.line_to(p.x, p.y);
                        }
                    }
                    path.close();
                }
            }
            if let Some(fill) = draw.fill {
                self.canvas.fill_path(&path, &Paint::color(color(fill)));
            }
            if let Some(stroke) = draw.stroke {
                self.canvas.stroke_path(
                    &path,
                    &Paint::color(color(stroke.color))
                        .with_line_width(stroke.width)
                        .with_line_cap(femtovg::LineCap::Round)
                        .with_line_join(femtovg::LineJoin::Round),
                );
            }
        }
        let encode = started.elapsed();
        let started = Instant::now();
        // One vertex/command batch. No scene pixels uploaded or read back.
        self.canvas.flush();
        let submit = started.elapsed();
        // SAFETY: Context remains current. FBO has a private stencil attachment.
        unsafe {
            let error = self.gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("GPU scene GL error 0x{error:04x}"));
            }
            if self
                .gl
                .get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING)
                .is_none()
                || self.gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER)
                    != glow::FRAMEBUFFER_COMPLETE
            {
                return Err("GPU scene render target is incomplete".into());
            }
        }
        let texture = self
            .canvas
            .get_native_texture(target)
            .map_err(|e| e.to_string())?;
        // SAFETY: Same window context; keep target alive until Slint has submitted
        // a replacement image. FBO rendering has bottom-left texture origin.
        let image = unsafe {
            slint::BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(
                texture.0,
                (scene.width, scene.height).into(),
            )
        }
        .origin(slint::BorrowedOpenGLTextureOrigin::BottomLeft)
        .build();
        Ok((image, encode, submit))
    }
}

fn color(c: RenderColor) -> femtovg::Color {
    femtovg::Color::rgbaf(c.r, c.g, c.b, c.a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Camera2, RenderPoint};

    #[test]
    fn supported_profile_is_restored_only_at_a_new_session_boundary() {
        std::thread::spawn(|| {
            for _ in 0..3 {
                STATE.with_borrow_mut(State::select_default);
                STATE.with_borrow(|s| {
                    assert_eq!(s.mode, Mode::RetainedCulled);
                    assert_eq!(s.stream, StreamMode::Bucketed);
                    assert_eq!(s.plan_mode, PlanMode::Prepared);
                    assert_eq!(s.strokes, StrokeMode::Conservative);
                    assert_eq!(s.joins, JoinMode::Exact);
                    assert_eq!(s.writer, StrokeWriter::BulkStatic);
                    assert_eq!(s.admission, AdmissionMode::Repeated);
                    assert!(s.error.is_none());
                });
                assert!(
                    super::super::source_control::diagnostics()
                        .contains("gpu_source_policy=pooled")
                );
                // Diagnostic opt-out and rendering failures must not cause a
                // per-frame attempt to re-enable the default renderer.
                STATE.with_borrow_mut(|s| s.select_mode(Mode::Off));
                assert!(!requested());
                STATE.with_borrow_mut(State::select_default);
                fail("injected draw failure".into());
                assert!(!requested());
                assert!(diagnostics().contains("gpu_scene_error=injected draw failure"));
                end_session();
                assert!(!requested());
                assert!(STATE.with_borrow(|s| s.pending.is_none()));
                assert!(
                    super::super::source_control::diagnostics()
                        .contains("gpu_source_policy=reference")
                );
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn admission_selection_is_idempotent_and_resets_at_session_boundaries() {
        std::thread::spawn(|| {
            for boundary in 0..4 {
                STATE.with_borrow_mut(|s| {
                    *s = State::default();
                    assert!(s.select_admission(AdmissionMode::Settled).is_err());
                    s.select_mode(Mode::RetainedCulled);
                    s.select_stream(StreamMode::Bucketed).unwrap();
                    s.select_plan(PlanMode::Prepared).unwrap();
                    s.select_writer(StrokeWriter::BulkStatic).unwrap();
                    s.applied_admission = Some(AdmissionMode::Repeated);
                    s.published.frames = 120;
                    s.select_admission(AdmissionMode::Settled).unwrap();
                    assert_eq!(s.published.frames, 0);
                    assert_eq!(s.applied_admission, Some(AdmissionMode::Repeated));
                    s.window.frames = 7;
                    s.select_admission(AdmissionMode::Settled).unwrap();
                    assert_eq!(s.window.frames, 7);
                    assert_eq!(s.stream, StreamMode::Bucketed);
                    assert_eq!(s.plan_mode, PlanMode::Prepared);
                    assert_eq!(s.writer, StrokeWriter::BulkStatic);
                });
                let frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
                assert!(stage(
                    &[frame],
                    Viewport::new(200., 100.),
                    FrameLayout::EqualHorizontal,
                    1.
                ));
                STATE.with_borrow_mut(|s| s.select_admission(AdmissionMode::Repeated).unwrap());
                assert!(STATE.with_borrow(|s| s.pending.is_some()));
                STATE.with_borrow_mut(|s| {
                    s.select_admission(AdmissionMode::Settled).unwrap();
                    s.applied_admission = Some(AdmissionMode::Settled);
                });
                assert!(diagnostics().contains("gpu_scene_admission_policy=settled"));
                assert!(diagnostics().contains("gpu_scene_admission_applied=settled"));
                match boundary {
                    0 => end_session(),
                    1 => teardown(&mut None),
                    2 => {
                        fail("test admission failure".into());
                        assert_eq!(STATE.with_borrow(|s| s.admission), AdmissionMode::Repeated);
                        assert_eq!(
                            STATE.with_borrow(|s| s.applied_admission),
                            Some(AdmissionMode::Settled)
                        );
                        end_session();
                    }
                    _ => {
                        STATE.with_borrow_mut(|s| s.select_mode(Mode::Triangles));
                        assert_eq!(STATE.with_borrow(|s| s.admission), AdmissionMode::Repeated);
                        end_session();
                    }
                }
                assert!(STATE.with_borrow(|s| s.pending.is_none()));
                assert_eq!(STATE.with_borrow(|s| s.admission), AdmissionMode::Repeated);
                assert_eq!(STATE.with_borrow(|s| s.applied_admission), None);
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn writer_requests_preserve_other_policies_and_reset_only_changed_windows() {
        for writer in [
            StrokeWriter::Bulk,
            StrokeWriter::BulkStatic,
            StrokeWriter::BulkPolygon,
            StrokeWriter::BulkEdges,
            StrokeWriter::BulkEdgesChecked,
            StrokeWriter::BulkPrepared,
            StrokeWriter::BulkPreparedChecked,
            StrokeWriter::BulkSinglePass,
            StrokeWriter::BulkSinglePassChecked,
        ] {
            let mut s = State::default();
            assert_eq!(s.writer, StrokeWriter::Stock);
            assert!(s.select_writer(writer).is_err());
            s.select_mode(Mode::RetainedCulled);
            s.select_stream(StreamMode::Bucketed).unwrap();
            s.select_plan(PlanMode::Prepared).unwrap();
            s.select_strokes(StrokeMode::Conservative).unwrap();
            s.select_joins(JoinMode::Exact).unwrap();
            s.applied_writer = Some(StrokeWriter::Stock);
            s.window.frames = 9;
            s.published.frames = 120;
            s.select_writer(writer).unwrap();
            assert_eq!(s.applied_writer, Some(StrokeWriter::Stock));
            assert_eq!((s.window.frames, s.published.frames), (0, 0));
            assert_eq!(s.stream, StreamMode::Bucketed);
            assert_eq!(s.plan_mode, PlanMode::Prepared);
            assert_eq!(s.strokes, StrokeMode::Conservative);
            assert_eq!(s.joins, JoinMode::Exact);
            s.window.frames = 7;
            s.select_writer(writer).unwrap();
            assert_eq!(s.window.frames, 7);
            s.select_writer(StrokeWriter::Stock).unwrap();
            assert_eq!(s.window.frames, 0);
            for mode in [
                Mode::Off,
                Mode::Paths,
                Mode::Triangles,
                Mode::Circles,
                Mode::RetainedRanges,
                Mode::RetainedResident,
            ] {
                s.select_mode(Mode::RetainedCulled);
                s.select_writer(writer).unwrap();
                s.select_mode(mode);
                assert_eq!(s.writer, StrokeWriter::Stock);
                assert!(s.select_writer(writer).is_err());
                s.select_writer(StrokeWriter::Stock).unwrap();
            }
        }
    }

    #[test]
    fn writer_switch_keeps_pending_plan_but_session_and_failure_reset_it() {
        std::thread::spawn(|| {
            let frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
            for writer in [
                StrokeWriter::Bulk,
                StrokeWriter::BulkStatic,
                StrokeWriter::BulkPolygon,
                StrokeWriter::BulkEdges,
                StrokeWriter::BulkEdgesChecked,
                StrokeWriter::BulkPrepared,
                StrokeWriter::BulkPreparedChecked,
                StrokeWriter::BulkSinglePass,
                StrokeWriter::BulkSinglePassChecked,
            ] {
                for boundary in 0..3 {
                    STATE.with_borrow_mut(|s| {
                        s.select_mode(Mode::RetainedCulled);
                        s.select_writer(writer).unwrap();
                        s.applied_writer = Some(writer);
                        s.published.frames = 120;
                    });
                    let report = diagnostics();
                    assert!(
                        report
                            .contains(&format!("gpu_scene_stroke_writer_policy={}", writer.name()))
                    );
                    assert!(report.contains(&format!(
                        "gpu_scene_stroke_writer_applied={}",
                        writer.name()
                    )));
                    assert!(stage(
                        std::slice::from_ref(&frame),
                        Viewport::new(200., 100.),
                        FrameLayout::EqualHorizontal,
                        1.
                    ));
                    STATE.with_borrow_mut(|s| s.select_writer(StrokeWriter::Stock).unwrap());
                    assert!(STATE.with_borrow(|s| s.pending.is_some()));
                    assert!(diagnostics().contains("gpu_scene_window_frames=0"));
                    STATE.with_borrow_mut(|s| s.select_writer(writer).unwrap());
                    match boundary {
                        0 => end_session(),
                        1 => teardown(&mut None),
                        _ => {
                            fail("test writer failure".into());
                            assert_eq!(STATE.with_borrow(|s| s.writer), StrokeWriter::Stock);
                            // Preserve the last successful image's applied policy until replacement.
                            assert_eq!(STATE.with_borrow(|s| s.applied_writer), Some(writer));
                            end_session();
                        }
                    }
                    assert!(STATE.with_borrow(|s| s.pending.is_none()));
                    assert_eq!(STATE.with_borrow(|s| s.writer), StrokeWriter::Stock);
                    assert_eq!(STATE.with_borrow(|s| s.applied_writer), None);
                }
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn join_requests_are_explicit_idempotent_and_preserve_other_policies() {
        let mut s = State::default();
        assert_eq!(s.joins, JoinMode::Reference);
        assert!(s.select_joins(JoinMode::Exact).is_err());
        s.select_mode(Mode::RetainedCulled);
        s.select_stream(StreamMode::Bucketed).unwrap();
        s.select_plan(PlanMode::Prepared).unwrap();
        s.select_strokes(StrokeMode::Conservative).unwrap();
        s.applied_joins = Some(JoinMode::Reference);
        s.window.frames = 9;
        s.published.frames = 120;
        s.published.joins.shortcuts = 60;
        s.select_joins(JoinMode::Exact).unwrap();
        assert_eq!(s.applied_joins, Some(JoinMode::Reference));
        assert_eq!(s.window.frames, 0);
        assert_eq!(s.published.frames, 0);
        assert_eq!(s.published.joins, RoundJoinStats::default());
        assert_eq!(s.stream, StreamMode::Bucketed);
        assert_eq!(s.plan_mode, PlanMode::Prepared);
        assert_eq!(s.strokes, StrokeMode::Conservative);
        s.window.frames = 7;
        s.select_joins(JoinMode::Exact).unwrap();
        assert_eq!(s.window.frames, 7);
        s.select_joins(JoinMode::Checked).unwrap();
        assert_eq!(s.window.frames, 0);
        for mode in [
            Mode::Off,
            Mode::Paths,
            Mode::Triangles,
            Mode::Circles,
            Mode::RetainedRanges,
            Mode::RetainedResident,
        ] {
            s.select_mode(Mode::RetainedCulled);
            s.select_joins(JoinMode::Checked).unwrap();
            s.select_mode(mode);
            assert_eq!(s.joins, JoinMode::Reference);
            assert!(s.select_joins(JoinMode::Exact).is_err());
            s.select_joins(JoinMode::Reference).unwrap();
        }
    }

    #[test]
    fn join_diagnostics_and_pending_plan_survive_switch_but_not_session_exit() {
        std::thread::spawn(|| {
            let frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
            for boundary in 0..3 {
                STATE.with_borrow_mut(|s| {
                    s.select_mode(Mode::RetainedCulled);
                    s.select_joins(JoinMode::Checked).unwrap();
                    s.applied_joins = Some(JoinMode::Checked);
                    s.published.frames = 120;
                    s.published.mesh.samples = 4;
                    s.published.joins = RoundJoinStats {
                        shortcuts: 480,
                        fallbacks: 120,
                    };
                });
                let report = diagnostics();
                for expected in [
                    "gpu_scene_join_policy=checked",
                    "gpu_scene_join_applied=checked",
                    "gpu_scene_join_shortcuts_avg=4.000",
                    "gpu_scene_join_fallbacks_avg=1.000",
                ] {
                    assert!(report.contains(expected), "{report}");
                }
                assert!(stage(
                    std::slice::from_ref(&frame),
                    Viewport::new(200., 100.),
                    FrameLayout::EqualHorizontal,
                    1.
                ));
                STATE.with_borrow_mut(|s| s.select_joins(JoinMode::Exact).unwrap());
                assert!(STATE.with_borrow(|s| s.pending.is_some()));
                assert!(diagnostics().contains("gpu_scene_join_shortcuts_avg=unavailable"));
                match boundary {
                    0 => end_session(),
                    1 => teardown(&mut None),
                    _ => {
                        fail("test join failure".into());
                        assert_eq!(STATE.with_borrow(|s| s.joins), JoinMode::Reference);
                        // Last successful image remains borrowed until replacement.
                        assert_eq!(
                            STATE.with_borrow(|s| s.applied_joins),
                            Some(JoinMode::Checked)
                        );
                        end_session();
                    }
                }
                assert!(STATE.with_borrow(|s| s.pending.is_none()));
                assert_eq!(STATE.with_borrow(|s| s.joins), JoinMode::Reference);
                assert_eq!(STATE.with_borrow(|s| s.applied_joins), None);
                let report = diagnostics();
                assert!(report.contains("gpu_scene_join_applied=inactive"));
                assert!(report.contains("gpu_scene_join_shortcuts_avg=unavailable"));
                assert!(report.contains("gpu_scene_join_fallbacks_avg=unavailable"));
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn stroke_requests_preserve_other_policies_and_only_apply_after_drawing() {
        let mut s = State::default();
        assert!(s.select_strokes(StrokeMode::Conservative).is_err());
        assert_eq!(s.strokes, StrokeMode::Off);
        s.select_mode(Mode::RetainedCulled);
        s.select_stream(StreamMode::Bucketed).unwrap();
        s.select_plan(PlanMode::Prepared).unwrap();
        s.applied_strokes = Some(StrokeMode::Off);
        s.window.frames = 9;
        s.published.frames = 120;
        s.select_strokes(StrokeMode::Conservative).unwrap();
        assert_eq!(s.window.frames, 0);
        assert_eq!(s.published.frames, 0);
        assert_eq!(s.applied_strokes, Some(StrokeMode::Off));
        assert_eq!(s.mode, Mode::RetainedCulled);
        assert_eq!(s.stream, StreamMode::Bucketed);
        assert_eq!(s.plan_mode, PlanMode::Prepared);
        s.window.frames = 7;
        s.select_strokes(StrokeMode::Conservative).unwrap();
        assert_eq!(s.window.frames, 7);
        s.select_strokes(StrokeMode::Checked).unwrap();
        assert_eq!(s.window.frames, 0);
        for mode in [
            Mode::Off,
            Mode::Paths,
            Mode::Circles,
            Mode::Triangles,
            Mode::RetainedRanges,
            Mode::RetainedResident,
        ] {
            s.select_mode(Mode::RetainedCulled);
            s.select_strokes(StrokeMode::Checked).unwrap();
            s.select_mode(mode);
            assert_eq!(s.strokes, StrokeMode::Off);
            assert!(s.select_strokes(StrokeMode::Conservative).is_err());
            s.select_strokes(StrokeMode::Off).unwrap();
        }
    }

    #[test]
    fn stroke_diagnostics_and_lifecycle_do_not_leak_between_sessions() {
        std::thread::spawn(|| {
            let frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
            for boundary in 0..3 {
                STATE.with_borrow_mut(|s| {
                    s.select_mode(Mode::RetainedCulled);
                    s.select_strokes(StrokeMode::Checked).unwrap();
                    s.applied_strokes = Some(StrokeMode::Checked);
                    s.published.frames = 120;
                    s.published.mesh.samples = 4;
                    s.published.mesh.retained.stroke_bound_tests = 480;
                    s.published.mesh.retained.stroke_bound_unknown = 120;
                    s.published.mesh.retained.stroke_bound_rejections = 240;
                });
                let report = diagnostics();
                assert!(report.contains("gpu_scene_stroke_applied=checked"));
                assert!(report.contains("gpu_scene_stroke_bound_tests_avg=4.000"));
                assert!(report.contains("gpu_scene_stroke_bound_unknown_avg=1.000"));
                assert!(report.contains("gpu_scene_stroke_bound_rejections_avg=2.000"));
                assert!(stage(
                    std::slice::from_ref(&frame),
                    Viewport::new(200., 100.),
                    FrameLayout::EqualHorizontal,
                    1.
                ));
                STATE.with_borrow_mut(|s| s.select_strokes(StrokeMode::Conservative).unwrap());
                assert!(STATE.with_borrow(|s| s.pending.is_some()));
                match boundary {
                    0 => end_session(),
                    1 => teardown(&mut None),
                    _ => {
                        fail("test failure".into());
                        // Until raster replacement, the last published image is
                        // still the checked image; the request is already off.
                        assert_eq!(
                            STATE.with_borrow(|s| s.applied_strokes),
                            Some(StrokeMode::Checked)
                        );
                        end_session();
                    }
                }
                assert!(STATE.with_borrow(|s| s.pending.is_none()));
                assert_eq!(STATE.with_borrow(|s| s.strokes), StrokeMode::Off);
                assert_eq!(STATE.with_borrow(|s| s.applied_strokes), None);
                let report = diagnostics();
                assert!(report.contains("gpu_scene_stroke_applied=inactive"));
                assert!(report.contains("gpu_scene_stroke_bound_tests_avg=unavailable"));
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn plan_requests_are_idempotent_separate_and_session_only() {
        let mut s = State::default();
        assert_eq!(s.plan_mode, PlanMode::Reference);
        assert!(s.select_plan(PlanMode::Reserved).is_err());
        s.select_mode(Mode::RetainedCulled);
        s.select_stream(StreamMode::Bucketed).unwrap();
        s.window.frames = 9;
        s.select_plan(PlanMode::Reserved).unwrap();
        assert_eq!(s.window.frames, 0);
        assert_eq!(s.stream, StreamMode::Bucketed);
        assert_eq!(s.mode, Mode::RetainedCulled);
        s.window.frames = 7;
        s.select_plan(PlanMode::Reserved).unwrap();
        assert_eq!(s.window.frames, 7);
        s.select_mode(Mode::Triangles);
        assert_eq!(s.plan_mode, PlanMode::Reserved);
        s.select_mode(Mode::Off);
        assert_eq!(s.plan_mode, PlanMode::Reference);
    }

    #[test]
    fn scratch_storage_covers_replaced_pending_plans_and_failure_cleanup() {
        std::thread::spawn(|| {
            for mode in [PlanMode::Scratch, PlanMode::Prepared] {
                let viewport = Viewport::new(640., 480.);
                let f = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
                STATE.with_borrow_mut(|s| {
                    s.select_mode(Mode::RetainedCulled);
                    s.select_plan(mode).unwrap();
                });
                for _ in 0..4 {
                    assert!(stage(
                        std::slice::from_ref(&f),
                        viewport,
                        FrameLayout::EqualHorizontal,
                        1.
                    ));
                }
                assert_eq!(STATE.with_borrow(|s| s.scratch.reuses), 3);
                assert_eq!(STATE.with_borrow(|s| s.scratch.bytes), 0);
                // An invalid replacement must never reuse or render an old plan.
                let mut bad = f.clone();
                bad.camera.height = f32::NAN;
                assert!(!stage(&[bad], viewport, FrameLayout::EqualHorizontal, 1.));
                assert!(STATE.with_borrow(|s| s.pending.is_none()));
                assert_eq!(STATE.with_borrow(|s| s.scratch.high_water_bytes), 0);
                assert_eq!(STATE.with_borrow(|s| s.plan_mode), PlanMode::Reference);
                end_session();
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn stream_requests_preserve_geometry_and_do_not_cross_incompatible_modes() {
        let mut s = State::default();
        assert!(s.select_stream(StreamMode::Bucketed).is_err());
        assert_eq!(s.stream, StreamMode::Shared);
        s.select_mode(Mode::RetainedCulled);
        s.window.frames = 10;
        s.published.frames = 120;
        s.applied_stream = Some(StreamMode::Shared);
        s.select_stream(StreamMode::Bucketed).unwrap();
        assert_eq!(s.mode, Mode::RetainedCulled);
        assert_eq!(s.applied_stream, Some(StreamMode::Shared));
        assert_eq!(s.window.frames, 0);
        assert_eq!(s.published.frames, 0);
        s.window.frames = 4;
        s.select_stream(StreamMode::Bucketed).unwrap();
        assert_eq!(s.window.frames, 4); // Idempotent, no perpetual warmup reset.
        s.select_mode(Mode::RetainedRanges);
        assert_eq!(s.stream, StreamMode::Bucketed);
        for mode in [
            Mode::Off,
            Mode::Paths,
            Mode::Circles,
            Mode::RetainedResident,
        ] {
            s.select_mode(Mode::RetainedCulled);
            s.select_stream(StreamMode::Exact).unwrap();
            s.select_mode(mode);
            assert_eq!(s.stream, StreamMode::Shared);
            assert!(s.select_stream(StreamMode::Bucketed).is_err());
        }
    }

    #[test]
    fn pending_scenes_are_bounded_and_requests_do_not_leak_between_sessions() {
        std::thread::spawn(|| {
            let f = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
            assert!(!stage(
                &[f.clone()],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                1.0
            ));
            STATE.with_borrow_mut(|s| s.mode = Mode::Triangles);
            assert!(stage(
                &[f.clone()],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                1.0
            ));
            assert!(stage(
                &[f],
                Viewport::new(300.0, 200.0),
                FrameLayout::EqualHorizontal,
                1.0
            ));
            assert_eq!(
                STATE.with_borrow(|s| s.pending.as_ref().unwrap().0.width),
                300
            );
            STATE.with_borrow_mut(|s| s.select_stream(StreamMode::Bucketed).unwrap());
            assert_eq!(
                STATE.with_borrow(|s| s.pending.as_ref().unwrap().0.width),
                300
            );
            STATE.with_borrow_mut(|s| s.select_plan(PlanMode::Reserved).unwrap());
            assert!(STATE.with_borrow(|s| s.pending.is_none()));
            end_session();
            assert!(!requested());
            assert!(STATE.with_borrow(|s| s.pending.is_none()));
            assert_eq!(STATE.with_borrow(|s| s.stream), StreamMode::Shared);
            assert_eq!(STATE.with_borrow(|s| s.applied_stream), None);
            assert_eq!(STATE.with_borrow(|s| s.plan_mode), PlanMode::Reference);
            assert!(diagnostics().contains("gpu_scene_stream_applied=inactive"));
        })
        .join()
        .unwrap();
    }
}
