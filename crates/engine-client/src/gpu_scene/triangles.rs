//! Ordered colored-triangle/optional circle batches into a multisample target, then
//! one resolve for Slint. No scene pixels cross from CPU to GPU each frame.

use super::{DrawOutput, gl_state, target_allocated, target_deleted};
use crate::gpu_scene::{
    frame_profile,
    mesh::{Batch, GeometryMode, MeshBuilder, Vertex},
    plan::Scene,
    streaming::{self, Policy as StreamPolicy},
    submit_profile::{Profile, Stage, Timer},
};
use glow::HasContext;
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

#[path = "circles_gpu.rs"]
mod circles_gpu;
#[path = "circles_layout_gpu.rs"]
mod circles_layout_gpu;
#[path = "meshes_gpu.rs"]
mod meshes_gpu;
#[path = "resident_gpu.rs"]
mod resident_gpu;

#[derive(Clone, Copy, Default)]
pub(crate) struct Stats {
    pub vertices: usize,
    pub indices: usize,
    pub draws: u64,
    pub upload_bytes: u64,
    pub samples: i32,
    pub circle_instances: usize,
    pub circle_template_bytes: usize,
    pub circle_vertex_work: usize,
    pub reusable: crate::gpu_scene::reusable::Stats,
    pub mesh_instances: usize,
    pub expansion_wall: Duration,
    pub retained: crate::gpu_scene::retained::Stats,
    pub resident: crate::gpu_scene::resident::Stats,
    pub submission: Profile,
    pub streaming: streaming::Stats,
}

pub(crate) struct Presenter {
    gl: Rc<glow::Context>,
    resources: Resources,
    viewport: glow::NativeUniformLocation,
    alternate_viewport: Option<glow::NativeUniformLocation>,
    target: Option<Target>,
    retired: Vec<Target>,
    mesh: MeshBuilder,
    limit: u32,
    prepared: Option<(u32, u32, Duration)>,
    geometry_mode: GeometryMode,
    circles: Option<circles_gpu::Resources>,
    unified: Option<circles_gpu::Resources>,
    fixed_layout: Option<circles_layout_gpu::Resources>,
    reusable: crate::gpu_scene::reusable::Builder,
    meshes: Option<meshes_gpu::Resources>,
    expansion_wall: Duration,
    retained: crate::gpu_scene::retained::Cache,
    retained_stats: crate::gpu_scene::retained::Stats,
    retention: Option<crate::gpu_scene::retained::Policy>,
    admission: crate::gpu_scene::AdmissionMode,
    resident_frame: Option<crate::gpu_scene::resident::Frame>,
    resident_gpu: Option<resident_gpu::Resources>,
    resident_active: bool,
    // Submission-lab control. Ordinary/live rendering keeps using resources.
    stream_policy: StreamPolicy,
    pass_streams: Vec<Resources>,
    stream_index: usize,
}

impl Presenter {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, String> {
        let version = gl.version();
        let minimum = if version.is_embedded { (3, 1) } else { (3, 3) };
        if (version.major, version.minor) < minimum {
            return Err("triangle renderer requires GLES 3.1 or OpenGL 3.3".into());
        }
        // SAFETY: The owning window's rendering callback keeps this context current.
        let _guard = unsafe { gl_state::Guard::new(gl.clone()) };
        let mut resources = Resources::new(gl.clone());
        let version = if gl.version().is_embedded {
            "#version 300 es\n"
        } else {
            "#version 330 core\n"
        };
        let vertex = format!(
            "{version}precision highp float;
            layout(location=0) in vec2 position;
            layout(location=1) in vec4 color;
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                gl_Position = vec4(position.x * 2.0 / viewport.x - 1.0,
                                   1.0 - position.y * 2.0 / viewport.y, 0.0, 1.0);
                vertex_color = color;
            }}"
        );
        let fragment = format!(
            "{version}precision highp float;
            in vec4 vertex_color;
            out vec4 output_color;
            void main() {{ output_color = vertex_color; }}"
        );
        // The partially initialized Resources owner releases handles on every error.
        unsafe {
            let vs = gl.create_shader(glow::VERTEX_SHADER)?;
            resources.shaders.push(vs);
            compile(&gl, vs, &vertex)?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER)?;
            resources.shaders.push(fs);
            compile(&gl, fs, &fragment)?;
            let program = gl.create_program()?;
            resources.program = Some(program);
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(format!(
                    "GPU triangle shader link: {}",
                    gl.get_program_info_log(program)
                ));
            }
            let viewport = gl
                .get_uniform_location(program, "viewport")
                .ok_or("GPU triangle viewport uniform missing")?;
            resources.vao = Some(gl.create_vertex_array()?);
            resources.vertices = Some(gl.create_buffer()?);
            resources.indices = Some(gl.create_buffer()?);
            gl.bind_vertex_array(resources.vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, resources.vertices);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, resources.indices);
            gl.enable_vertex_attrib_array(0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                0,
                2,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                0,
            );
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                8,
            );
            check_error(&gl)?;
            let limit = gl
                .get_parameter_i32(glow::MAX_TEXTURE_SIZE)
                .min(gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE))
                .max(0) as u32;
            if gl.get_parameter_i32(glow::MAX_SAMPLES) < 4 {
                return Err("GPU triangle prototype requires four-sample MSAA".into());
            }
            Ok(Self {
                gl,
                resources,
                viewport,
                alternate_viewport: None,
                target: None,
                retired: Vec::new(),
                mesh: MeshBuilder::default(),
                limit,
                prepared: None,
                geometry_mode: GeometryMode::Triangles,
                circles: None,
                unified: None,
                fixed_layout: None,
                reusable: Default::default(),
                meshes: None,
                expansion_wall: Duration::ZERO,
                retained: Default::default(),
                retained_stats: Default::default(),
                retention: None,
                admission: Default::default(),
                resident_frame: None,
                resident_gpu: None,
                resident_active: false,
                stream_policy: StreamPolicy::default(),
                pass_streams: Vec::new(),
                stream_index: 0,
            })
        }
    }

    pub fn retire(&mut self) {
        if let Some(target) = self.target.take() {
            self.retired.push(target);
        }
    }

    pub fn collect_retired(&mut self) {
        if self.retired.is_empty() {
            return;
        }
        // SAFETY: AfterRendering follows submission of the replacement Slint image.
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        self.retired.clear();
    }

    pub fn draw(&mut self, scene: &Scene) -> Result<DrawOutput, String> {
        self.begin_frame();
        self.prepare(scene)?;
        self.draw_prepared()
    }

    pub fn set_instanced_circles(&mut self, enabled: bool) {
        self.set_geometry_mode(if enabled {
            GeometryMode::Circles
        } else {
            GeometryMode::Triangles
        });
    }

    pub fn set_geometry_mode(&mut self, mode: GeometryMode) {
        self.geometry_mode = mode;
        self.prepared = None;
    }

    // Separate CPU tessellation from submission so the offscreen lab can place
    // GPU queries around the same draw path used by the live renderer.
    pub fn prepare(&mut self, scene: &Scene) -> Result<(), String> {
        self.prepare_inner(
            scene,
            self.retention
                .map(|retain| (scene.retained.as_slice(), retain)),
            None,
        )
    }

    pub fn set_retention(&mut self, retention: Option<crate::gpu_scene::retained::Policy>) {
        if retention != self.retention {
            self.clear_retained();
        }
        self.retention = retention;
    }

    pub fn clear_retained(&mut self) {
        self.retained = Default::default();
        self.retained.set_admission(self.admission);
        self.retained_stats = Default::default();
        self.resident_frame = None;
        self.resident_active = false;
        // CPU identities restart with the cache. Clear its GPU mirror as well.
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        self.resident_gpu = None;
    }

    pub fn begin_frame(&mut self) {
        self.stream_index = 0;
        self.retained.begin_frame();
        if let Some(resources) = &mut self.resident_gpu {
            resources.begin_frame();
        }
        if let Some(frame) = &mut self.resident_frame {
            frame.begin_frame();
        }
    }

    pub(super) fn set_stream_policy(&mut self, policy: StreamPolicy) {
        if self.stream_policy != policy && !self.pass_streams.is_empty() {
            let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
            self.pass_streams.clear();
        }
        self.stream_policy = policy;
    }

    pub(super) fn set_stroke_culling(
        &mut self,
        control: crate::gpu_scene::stroke_culling::Control,
    ) {
        self.mesh.stroke_culling = control;
    }

    pub(super) fn set_stroke_writer(
        &mut self,
        writer: crate::gpu_scene::stroke_output::StrokeWriter,
    ) {
        self.mesh.set_stroke_writer(writer);
    }

    pub(super) fn single_pass_stats(&self) -> crate::gpu_scene::stroke_single_pass::Stats {
        self.mesh.single_pass_stats()
    }

    pub(super) fn set_admission(&mut self, mode: crate::gpu_scene::AdmissionMode) {
        self.admission = mode;
        self.retained.set_admission(mode);
    }

    pub(super) fn set_small_strokes(&mut self, enabled: bool) {
        self.mesh.small_strokes = enabled;
    }

    pub(super) fn small_strokes_used(&self) -> u64 {
        self.mesh.small_strokes_used
    }

    pub(super) fn set_round_join_mode(&mut self, mode: lyon_tessellation::RoundJoinMode) {
        self.mesh.set_round_join_mode(mode);
    }

    pub(super) fn round_join_stats(&self) -> lyon_tessellation::RoundJoinStats {
        self.mesh.round_join_stats()
    }

    pub(super) fn stream_usage(&self) -> (usize, usize) {
        (
            self.pass_streams.len(),
            self.resources.capacities.iter().sum::<usize>()
                + self
                    .pass_streams
                    .iter()
                    .map(|r| r.capacities.iter().sum::<usize>())
                    .sum::<usize>(),
        )
    }

    pub fn prepare_retained(
        &mut self,
        scene: &Scene,
        groups: &[crate::gpu_scene::retained::Placement],
        retain: bool,
    ) -> Result<(), String> {
        self.set_geometry_mode(GeometryMode::Triangles);
        self.prepare_inner(
            scene,
            Some((
                groups,
                if retain {
                    crate::gpu_scene::retained::Policy::Always
                } else {
                    crate::gpu_scene::retained::Policy::Cold
                },
            )),
            None,
        )
    }

    fn prepare_inner(
        &mut self,
        scene: &Scene,
        groups: Option<(
            &[crate::gpu_scene::retained::Placement],
            crate::gpu_scene::retained::Policy,
        )>,
        clip: Option<crate::gpu_scene::presentation::Clip>,
    ) -> Result<(), String> {
        self.prepared = None;
        if scene.width == 0
            || scene.height == 0
            || scene.width > self.limit
            || scene.height > self.limit
        {
            return Err("GPU triangle target exceeds texture/renderbuffer dimensions".into());
        }
        let started = Instant::now();
        self.expansion_wall = Duration::ZERO;
        self.retained_stats = Default::default();
        self.resident_active = false;
        let capture = frame_profile::capture_mode();
        let mut profile = None;
        if let Some((groups, retain)) = groups {
            if matches!(
                retain,
                crate::gpu_scene::retained::Policy::HybridResident
                    | crate::gpu_scene::retained::Policy::HybridResidentPasses
                    | crate::gpu_scene::retained::Policy::HybridResidentBuffers
                    | crate::gpu_scene::retained::Policy::HybridResidentUniforms
            ) {
                let frame = self.resident_frame.get_or_insert_with(Default::default);
                self.retained
                    .rebuild_resident(scene, groups, &mut self.mesh, frame)?;
                self.resident_active = true;
            } else {
                if capture == Some(true) {
                    profile = Some(if let Some(clip) = clip {
                        self.retained.rebuild_profiled_clipped(
                            scene,
                            groups,
                            &mut self.mesh,
                            retain,
                            clip,
                        )?
                    } else {
                        self.retained
                            .rebuild_profiled(scene, groups, &mut self.mesh, retain)?
                    });
                } else if let Some(clip) = clip {
                    self.retained
                        .rebuild_clipped(scene, groups, &mut self.mesh, retain, clip)?;
                } else {
                    self.retained
                        .rebuild(scene, groups, &mut self.mesh, retain)?;
                }
            }
            self.retained_stats = self.retained.stats;
        } else if self.geometry_mode.reusable_meshes() {
            self.reusable.rebuild(
                scene,
                &mut self.mesh,
                self.geometry_mode != GeometryMode::MeshesStreamed,
            )?;
            if self.geometry_mode == GeometryMode::MeshesExpanded {
                let expansion = Instant::now();
                self.reusable.expand(&mut self.mesh)?;
                self.expansion_wall = expansion.elapsed();
            }
        } else if self.geometry_mode == GeometryMode::Triangles {
            self.mesh.rebuild(scene)?;
        } else {
            self.mesh.rebuild_mode(scene, self.geometry_mode)?;
        }
        let encode = started.elapsed();
        if capture.is_some() {
            frame_profile::record(&frame_profile::sample(
                scene.draws.len(),
                self.mesh.buffers.vertices.len(),
                self.mesh.buffers.indices.len(),
                &self.retained_stats,
                encode,
                profile.as_ref(),
            ));
        }
        self.prepared = Some((scene.width, scene.height, encode));
        Ok(())
    }

    pub fn geometry_mode(&self) -> GeometryMode {
        self.geometry_mode
    }

    pub fn draw_prepared(&mut self) -> Result<DrawOutput, String> {
        let (width, height, _) = self.prepared.ok_or("no prepared triangle scene")?;
        // SAFETY: All GL work and resource retirement uses the window's current context.
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.size != (width, height))
        {
            // Allocate first: an allocation failure must not retire Slint's last image.
            let replacement = Target::new(self.gl.clone(), width, height)?;
            self.retire();
            self.target = Some(replacement);
        }
        let framebuffer = self.target.as_ref().unwrap().multisample_fbo;
        let (encode, mut submit, mut mesh) =
            self.submit_prepared::<false>(framebuffer, None, true)?;
        let target = self.target.as_ref().unwrap();
        let started = Instant::now();
        target.resolve()?;
        submit += started.elapsed();
        mesh.samples = target.samples;
        Ok(DrawOutput {
            image: target.image(),
            encode,
            submit,
            mesh,
        })
    }

    /// The caller owns a current-context guard and the destination lifetime.
    /// Shared-window scenes use global pixels, so scissor changes never distort
    /// a camera or its logical stroke widths.
    pub(super) fn draw_into<const PROFILE: bool>(
        &mut self,
        scene: &Scene,
        target: &Target,
        clip: Option<crate::gpu_scene::presentation::Clip>,
        clear: bool,
    ) -> Result<(Duration, Duration, Stats), String> {
        if (scene.width, scene.height) != target.size {
            return Err("GPU pass dimensions differ from its target".into());
        }
        self.prepare_inner(
            scene,
            self.retention
                .map(|retain| (scene.retained.as_slice(), retain)),
            clip,
        )?;
        let (encode, submit, mut stats) =
            self.submit_prepared::<PROFILE>(target.multisample_fbo, clip, clear)?;
        stats.samples = target.samples;
        Ok((encode, submit, stats))
    }

    fn submit_prepared<const PROFILE: bool>(
        &mut self,
        framebuffer: Option<glow::NativeFramebuffer>,
        clip: Option<crate::gpu_scene::presentation::Clip>,
        clear: bool,
    ) -> Result<(Duration, Duration, Stats), String> {
        let (width, height, encode) = self.prepared.take().ok_or("no prepared triangle scene")?;
        let count = i32::try_from(self.mesh.buffers.indices.len())
            .map_err(|_| "GPU triangle index count overflow")?;
        if self.resident_active && self.resident_gpu.is_none() {
            self.resident_gpu = Some(resident_gpu::Resources::new(
                self.gl.clone(),
                match self.retention {
                    Some(crate::gpu_scene::retained::Policy::HybridResidentUniforms) => {
                        resident_gpu::Access::PassUniforms
                    }
                    Some(crate::gpu_scene::retained::Policy::HybridResidentBuffers) => {
                        resident_gpu::Access::PassBuffers
                    }
                    Some(crate::gpu_scene::retained::Policy::HybridResidentPasses) => {
                        resident_gpu::Access::PassTextures
                    }
                    _ => resident_gpu::Access::Texture,
                },
            )?);
        }
        if self.geometry_mode.fixed_layout() && self.fixed_layout.is_none() {
            self.fixed_layout = Some(circles_layout_gpu::Resources::new(
                self.gl.clone(),
                self.resources.vertices,
                self.resources.indices,
            )?);
        }
        if self.geometry_mode.unified_program()
            && !self.geometry_mode.fixed_layout()
            && self.unified.is_none()
        {
            self.unified = Some(circles_gpu::Resources::new_unified(
                self.gl.clone(),
                self.resources.vertices,
                self.resources.indices,
            )?);
        }
        if !self.geometry_mode.unified_program()
            && !self.mesh.circles.is_empty()
            && self.circles.is_none()
        {
            self.circles = Some(circles_gpu::Resources::new(self.gl.clone())?);
        }
        if self.geometry_mode == GeometryMode::TrianglesPrograms {
            self.ensure_alternate_program()?;
        }
        if self.geometry_mode.instanced_meshes() && self.meshes.is_none() {
            self.meshes = Some(meshes_gpu::Resources::new(
                self.gl.clone(),
                self.resources.vertices,
                self.resources.indices,
            )?);
        }
        // Probe-only policies preserve format/program/painter order. Budget
        // reserved capacity, not just used bytes; overflow uses the shared pair.
        let used = [
            self.mesh.buffers.vertices.len() * std::mem::size_of::<Vertex>(),
            self.mesh.buffers.indices.len() * std::mem::size_of::<u32>(),
        ];
        let pooled_bytes = self
            .pass_streams
            .iter()
            .map(|r| r.capacities.iter().sum::<usize>())
            .sum::<usize>();
        let old = self
            .pass_streams
            .get(self.stream_index)
            .map_or([0; 2], |r| r.capacities);
        let eligible = self.stream_policy != StreamPolicy::SharedExact
            && !self.resident_active
            && self.geometry_mode == GeometryMode::Triangles
            && count > 0;
        let capacity = self
            .stream_policy
            .capacities(old, used)
            .filter(|new| streaming::fits(self.stream_index, pooled_bytes, old, *new));
        let dedicated = eligible && capacity.is_some();
        let split_upload = dedicated && self.stream_policy.split_upload();
        let mut streaming = streaming::Stats::default();
        let (vao, vertices, indices, capacities) = if dedicated {
            if self.stream_index == self.pass_streams.len() {
                self.pass_streams.push(Resources::stream(self.gl.clone())?);
            }
            let r = &mut self.pass_streams[self.stream_index];
            let new = capacity.unwrap();
            streaming = streaming::Stats::upload(r.capacities, new, split_upload, false);
            r.capacities = new;
            (r.vao, r.vertices, r.indices, new)
        } else {
            if !self.resident_active && count > 0 {
                streaming =
                    streaming::Stats::upload(self.resources.capacities, used, false, eligible);
                self.resources.capacities = used;
            }
            (
                self.resources.vao,
                self.resources.vertices,
                self.resources.indices,
                used,
            )
        };
        // Count allocated, nonempty passes; empty/resident passes need no pair.
        self.stream_index += usize::from(dedicated);
        let g = &self.gl;
        let started = Instant::now();
        let mut profile = Profile::default();
        let mut timer = Timer::<PROFILE>::new();
        let mut mesh_upload = 0;
        unsafe {
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, framebuffer);
            if !g.version().is_embedded {
                g.enable(glow::MULTISAMPLE);
            }
            g.viewport(0, 0, width as i32, height as i32);
            for cap in [
                glow::CULL_FACE,
                glow::DEPTH_TEST,
                glow::STENCIL_TEST,
                glow::SCISSOR_TEST,
                glow::SAMPLE_ALPHA_TO_COVERAGE,
                glow::SAMPLE_COVERAGE,
                glow::SAMPLE_MASK,
                glow::RASTERIZER_DISCARD,
                glow::DITHER,
            ] {
                g.disable(cap);
            }
            g.color_mask(true, true, true, true);
            if clear {
                g.clear_color(5.0 / 255.0, 5.0 / 255.0, 20.0 / 255.0, 1.0);
                g.clear(glow::COLOR_BUFFER_BIT);
            }
            if let Some(clip) = clip {
                g.enable(glow::SCISSOR_TEST);
                g.scissor(
                    clip.x,
                    height as i32 - clip.y - clip.height,
                    clip.width,
                    clip.height,
                );
            }
            g.enable(glow::BLEND);
            g.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
            g.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            if self.resident_active {
                let frame = self.resident_frame.as_mut().unwrap();
                let resident = frame.stats();
                let upload_bytes = self
                    .resident_gpu
                    .as_mut()
                    .unwrap()
                    .draw(frame, width, height)?;
                return Ok((
                    encode,
                    started.elapsed(),
                    Stats {
                        vertices: frame.references.len(),
                        indices: frame.indices.len(),
                        draws: u64::from(!frame.indices.is_empty()),
                        upload_bytes,
                        samples: 4,
                        retained: self.retained_stats,
                        resident,
                        ..Default::default()
                    },
                ));
            }
            g.use_program(self.resources.program);
            g.uniform_2_f32(Some(&self.viewport), width as f32, height as f32);
            g.bind_vertex_array(vao);
            g.bind_buffer(glow::ARRAY_BUFFER, vertices);
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, indices);
            timer.mark(&mut profile, Stage::Setup);
            if count > 0 {
                let vertex_data = bytemuck::cast_slice(&self.mesh.buffers.vertices);
                let index_data = bytemuck::cast_slice(&self.mesh.buffers.indices);
                if split_upload {
                    // Discard on every pass even when capacity is unchanged.
                    // Never overwrite an in-flight store without driver renaming.
                    g.buffer_data_size(glow::ARRAY_BUFFER, capacities[0] as i32, glow::STREAM_DRAW);
                    timer.mark(&mut profile, Stage::VertexReserve);
                    g.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, vertex_data);
                } else {
                    g.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_data, glow::STREAM_DRAW);
                }
                timer.mark(&mut profile, Stage::VertexUpload);
                if split_upload {
                    g.buffer_data_size(
                        glow::ELEMENT_ARRAY_BUFFER,
                        capacities[1] as i32,
                        glow::STREAM_DRAW,
                    );
                    timer.mark(&mut profile, Stage::IndexReserve);
                    g.buffer_sub_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, 0, index_data);
                } else {
                    g.buffer_data_u8_slice(
                        glow::ELEMENT_ARRAY_BUFFER,
                        index_data,
                        glow::STREAM_DRAW,
                    );
                }
                timer.mark(&mut profile, Stage::IndexUpload);
            }
            let unified = self.geometry_mode.unified_program();
            if self.geometry_mode.instanced_meshes() {
                mesh_upload = self
                    .meshes
                    .as_mut()
                    .unwrap()
                    .draw(&self.reusable, width, height);
            } else if self.geometry_mode.fixed_layout() {
                let layout = self.fixed_layout.as_ref().unwrap();
                if !self.mesh.circles.is_empty() {
                    layout.upload(&self.mesh.circles);
                }
                layout.prepare_submission(self.geometry_mode, width, height);
            } else if unified || !self.mesh.circles.is_empty() {
                let circles = if unified {
                    self.unified.as_mut()
                } else {
                    self.circles.as_mut()
                }
                .unwrap();
                if !self.mesh.circles.is_empty() {
                    circles.upload(&self.mesh.circles);
                }
                circles.prepare_submission(self.geometry_mode, width, height)?;
            }
            // Never sort by primitive type: fill/stroke and transparent source
            // ordering stay identical across the selectable modes.
            for (batch_index, batch) in self.mesh.batches.iter().enumerate() {
                match *batch {
                    Batch::Triangles { start, count } => {
                        if self.geometry_mode.fixed_layout() {
                            self.fixed_layout.as_ref().unwrap().draw_triangles(
                                start,
                                count,
                                self.geometry_mode,
                            );
                            continue;
                        }
                        if unified {
                            self.unified.as_ref().unwrap().draw_triangles(
                                start,
                                count,
                                self.geometry_mode == GeometryMode::CirclesUnifiedInstanced,
                            );
                            continue;
                        }
                        let alternate = self.geometry_mode == GeometryMode::TrianglesPrograms
                            && !batch_index.is_multiple_of(2);
                        let (program, viewport) = if alternate {
                            (
                                self.resources.alternate_program,
                                self.alternate_viewport.as_ref(),
                            )
                        } else {
                            (self.resources.program, Some(&self.viewport))
                        };
                        g.use_program(program);
                        if !self.geometry_mode.frame_uniforms() {
                            g.uniform_2_f32(viewport, width as f32, height as f32);
                        }
                        g.bind_vertex_array(vao);
                        g.draw_elements(
                            glow::TRIANGLES,
                            count as i32,
                            glow::UNSIGNED_INT,
                            (start * 4) as i32,
                        );
                    }
                    Batch::Circles {
                        start,
                        count,
                        level,
                    } => {
                        if self.geometry_mode.fixed_layout() {
                            self.fixed_layout.as_ref().unwrap().draw_circles(
                                start,
                                count,
                                level,
                                self.geometry_mode,
                            );
                            continue;
                        }
                        let circles = if unified {
                            self.unified.as_ref()
                        } else {
                            self.circles.as_ref()
                        }
                        .unwrap();
                        circles.draw(start, count, level, width, height, self.geometry_mode);
                    }
                }
            }
            timer.mark(&mut profile, Stage::Batches);
            check_error(g)?;
            timer.mark(&mut profile, Stage::ErrorCheck);
        }
        let submit = started.elapsed();
        Ok((
            encode,
            submit,
            Stats {
                vertices: self.mesh.buffers.vertices.len(),
                indices: count as usize,
                draws: if self.geometry_mode.instanced_meshes() {
                    self.reusable.commands.len()
                } else {
                    self.mesh.batches.len()
                } as u64,
                upload_bytes: self.mesh.upload_bytes() as u64 + mesh_upload,
                samples: 4,
                circle_instances: self.mesh.circles.len(),
                circle_template_bytes: self.circles.as_ref().map_or(0, |c| c.template_bytes)
                    + self.unified.as_ref().map_or(0, |c| c.template_bytes)
                    + self.fixed_layout.as_ref().map_or(0, |c| c.template_bytes),
                circle_vertex_work: self
                    .mesh
                    .batches
                    .iter()
                    .map(|batch| match batch {
                        Batch::Circles { count, level, .. } => {
                            count * (1 + 3 * crate::gpu_scene::circles::LEVELS[*level])
                        }
                        _ => 0,
                    })
                    .sum(),
                reusable: if self.geometry_mode.reusable_meshes() {
                    self.reusable.stats
                } else {
                    Default::default()
                },
                mesh_instances: if self.geometry_mode.reusable_meshes() {
                    self.reusable.instances.len()
                } else {
                    0
                },
                expansion_wall: self.expansion_wall,
                retained: self.retained_stats,
                resident: Default::default(),
                submission: profile,
                streaming,
            },
        ))
    }

    // Identical shader source, geometry and VAO; only program identity alternates.
    // This is a control, not a simulation of different circle/triangle shaders.
    fn ensure_alternate_program(&mut self) -> Result<(), String> {
        if self.alternate_viewport.is_some() {
            return Ok(());
        }
        let g = &self.gl;
        unsafe {
            let program = g.create_program()?;
            self.resources.alternate_program = Some(program);
            for shader in &self.resources.shaders {
                g.attach_shader(program, *shader);
            }
            g.link_program(program);
            let result = if g.get_program_link_status(program) {
                g.get_uniform_location(program, "viewport")
                    .ok_or_else(|| "alternate triangle viewport missing".to_owned())
            } else {
                Err(format!(
                    "alternate triangle program link: {}",
                    g.get_program_info_log(program)
                ))
            };
            match result {
                Ok(viewport) => self.alternate_viewport = Some(viewport),
                Err(error) => {
                    g.delete_program(program);
                    self.resources.alternate_program = None;
                    return Err(error);
                }
            }
            check_error(g)?;
        }
        Ok(())
    }

    /// Diagnostic readback only; never called from a live/timed frame.
    /// Returns top-left-origin RGBA bytes, preserving caller pack state.
    pub fn readback(&self) -> Result<Vec<u8>, String> {
        let target = self.target.as_ref().ok_or("no triangle target")?;
        target.readback()
    }
}

impl Target {
    /// Diagnostic readback only; no live frame downloads pixels.
    pub(super) fn readback(&self) -> Result<Vec<u8>, String> {
        let (w, h) = self.size;
        let mut pixels = vec![0; (w * h * 4) as usize];
        self.read_pixels(glow::PixelPackData::Slice(Some(&mut pixels)), None)?;
        let stride = w as usize * 4;
        for y in 0..h as usize / 2 {
            let (top, bottom) = pixels.split_at_mut((h as usize - 1 - y) * stride);
            top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
        }
        Ok(pixels)
    }

    /// Diagnostic transfers only. Preserve pack and framebuffer state for both
    /// synchronous slices and validation's fenced pixel-pack buffers.
    pub(super) fn read_pixels(
        &self,
        pixels: glow::PixelPackData<'_>,
        buffer: Option<glow::NativeBuffer>,
    ) -> Result<(), String> {
        let (w, h) = self.size;
        let g = &self.gl;
        let _guard = unsafe { gl_state::Guard::new(g.clone()) };
        unsafe {
            let parameters = [
                glow::PACK_ALIGNMENT,
                glow::PACK_ROW_LENGTH,
                glow::PACK_SKIP_PIXELS,
                glow::PACK_SKIP_ROWS,
            ];
            let saved = parameters.map(|p| g.get_parameter_i32(p));
            let pack = g.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, buffer);
            for (p, v) in parameters.into_iter().zip([1, 0, 0, 0]) {
                g.pixel_store_i32(p, v);
            }
            g.bind_framebuffer(glow::READ_FRAMEBUFFER, self.resolve_fbo);
            g.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                pixels,
            );
            for (p, v) in parameters.into_iter().zip(saved) {
                g.pixel_store_i32(p, v);
            }
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, pack);
            check_error(g)?;
        }
        Ok(())
    }
}

// Owners below are only created/dropped inside a context-guarded rendering
// callback. Optional handles make failure partway through allocation safe.
struct Resources {
    gl: Rc<glow::Context>,
    shaders: Vec<glow::NativeShader>,
    program: Option<glow::NativeProgram>,
    alternate_program: Option<glow::NativeProgram>,
    vao: Option<glow::NativeVertexArray>,
    vertices: Option<glow::NativeBuffer>,
    indices: Option<glow::NativeBuffer>,
    capacities: [usize; 2],
}
impl Resources {
    fn new(gl: Rc<glow::Context>) -> Self {
        Self {
            gl,
            shaders: Vec::new(),
            program: None,
            alternate_program: None,
            vao: None,
            vertices: None,
            indices: None,
            capacities: [0; 2],
        }
    }

    fn stream(gl: Rc<glow::Context>) -> Result<Self, String> {
        let mut r = Self::new(gl);
        let g = &r.gl;
        unsafe {
            r.vao = Some(g.create_vertex_array()?);
            r.vertices = Some(g.create_buffer()?);
            r.indices = Some(g.create_buffer()?);
            g.bind_vertex_array(r.vao);
            g.bind_buffer(glow::ARRAY_BUFFER, r.vertices);
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, r.indices);
            for (attribute, size, offset) in [(0, 2, 0), (1, 4, 8)] {
                g.enable_vertex_attrib_array(attribute);
                g.vertex_attrib_pointer_f32(
                    attribute,
                    size,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    offset,
                );
            }
            check_error(g)?;
        }
        Ok(r)
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if let Some(v) = self.vao {
                self.gl.delete_vertex_array(v);
            }
            for b in [self.vertices, self.indices].into_iter().flatten() {
                self.gl.delete_buffer(b);
            }
            for p in [self.program, self.alternate_program].into_iter().flatten() {
                self.gl.delete_program(p);
            }
            for s in self.shaders.drain(..) {
                self.gl.delete_shader(s);
            }
        }
    }
}

pub(super) struct Target {
    gl: Rc<glow::Context>,
    pub(super) size: (u32, u32),
    pub(super) texture: Option<glow::NativeTexture>,
    resolve_fbo: Option<glow::NativeFramebuffer>,
    pub(super) multisample_fbo: Option<glow::NativeFramebuffer>,
    color: Option<glow::NativeRenderbuffer>,
    samples: i32,
    counted: bool,
}
impl Target {
    pub(super) fn new(gl: Rc<glow::Context>, width: u32, height: u32) -> Result<Self, String> {
        let limit = unsafe {
            gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE)
                .min(gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE))
                .max(0) as u32
        };
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err("GPU target exceeds texture/renderbuffer dimensions".into());
        }
        let mut target = Self {
            gl,
            size: (width, height),
            texture: None,
            resolve_fbo: None,
            multisample_fbo: None,
            color: None,
            samples: 4,
            counted: false,
        };
        let g = &target.gl;
        unsafe {
            target.texture = Some(g.create_texture()?);
            g.active_texture(glow::TEXTURE0);
            g.bind_texture(glow::TEXTURE_2D, target.texture);
            for parameter in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                g.tex_parameter_i32(glow::TEXTURE_2D, parameter, glow::LINEAR as i32);
            }
            for parameter in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
                g.tex_parameter_i32(glow::TEXTURE_2D, parameter, glow::CLAMP_TO_EDGE as i32);
            }
            g.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            target.resolve_fbo = Some(g.create_framebuffer()?);
            g.bind_framebuffer(glow::FRAMEBUFFER, target.resolve_fbo);
            g.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                target.texture,
                0,
            );
            complete(g)?;
            target.color = Some(g.create_renderbuffer()?);
            g.bind_renderbuffer(glow::RENDERBUFFER, target.color);
            g.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                target.samples,
                glow::RGBA8,
                width as i32,
                height as i32,
            );
            target.samples =
                g.get_renderbuffer_parameter_i32(glow::RENDERBUFFER, glow::RENDERBUFFER_SAMPLES);
            target.multisample_fbo = Some(g.create_framebuffer()?);
            g.bind_framebuffer(glow::FRAMEBUFFER, target.multisample_fbo);
            g.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                target.color,
            );
            complete(g)?;
            check_error(g)?;
        }
        target_allocated();
        target.counted = true;
        Ok(target)
    }

    pub(super) fn resolve(&self) -> Result<(), String> {
        let g = &self.gl;
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        unsafe {
            g.disable(glow::SCISSOR_TEST);
            g.bind_framebuffer(glow::READ_FRAMEBUFFER, self.multisample_fbo);
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.resolve_fbo);
            g.blit_framebuffer(
                0,
                0,
                w,
                h,
                0,
                0,
                w,
                h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            check_error(g)
        }
    }

    pub(super) fn image(&self) -> slint::Image {
        // SAFETY: Caller retains this texture until Slint submits a replacement,
        // or clears the image during current-context teardown.
        unsafe {
            BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(
                self.texture.unwrap().0,
                self.size.into(),
            )
        }
        .origin(BorrowedOpenGLTextureOrigin::BottomLeft)
        .build()
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        unsafe {
            for f in [self.resolve_fbo, self.multisample_fbo]
                .into_iter()
                .flatten()
            {
                self.gl.delete_framebuffer(f);
            }
            if let Some(c) = self.color {
                self.gl.delete_renderbuffer(c);
            }
            if let Some(t) = self.texture {
                self.gl.delete_texture(t);
            }
        }
        if self.counted {
            target_deleted();
        }
    }
}

pub(super) unsafe fn compile(
    g: &glow::Context,
    shader: glow::NativeShader,
    source: &str,
) -> Result<(), String> {
    unsafe {
        g.shader_source(shader, source);
        g.compile_shader(shader);
        if !g.get_shader_compile_status(shader) {
            return Err(format!(
                "GPU triangle shader: {}",
                g.get_shader_info_log(shader)
            ));
        }
    }
    Ok(())
}
unsafe fn complete(g: &glow::Context) -> Result<(), String> {
    let status = unsafe { g.check_framebuffer_status(glow::FRAMEBUFFER) };
    if status != glow::FRAMEBUFFER_COMPLETE {
        return Err(format!(
            "GPU triangle framebuffer incomplete 0x{status:04x}"
        ));
    }
    Ok(())
}
unsafe fn check_error(g: &glow::Context) -> Result<(), String> {
    let error = unsafe { g.get_error() };
    if error != glow::NO_ERROR {
        return Err(format!("GPU triangle GL error 0x{error:04x}"));
    }
    Ok(())
}
