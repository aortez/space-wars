//! Camera passes share one MSAA target. Only grouped minimaps use small
//! retained intermediate targets; there is no full-window copy per player.
use super::{
    DrawOutput, gl_state,
    triangles::{self, Target},
};
use crate::gpu_scene::presentation::{Pass, Plan};
use crate::gpu_scene::submit_profile::{Stage, Timer};
use glow::HasContext;
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) struct Presenter {
    gl: Rc<glow::Context>,
    geometry: triangles::Presenter,
    quad: Quad,
    target: Option<Target>,
    minimaps: Vec<Target>,
    retired: Vec<Target>,
}

impl Presenter {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, String> {
        let _guard = unsafe { gl_state::Guard::new(gl.clone()) };
        Ok(Self {
            geometry: triangles::Presenter::new(gl.clone())?,
            quad: Quad::new(gl.clone())?,
            gl,
            target: None,
            minimaps: Vec::new(),
            retired: Vec::new(),
        })
    }

    pub fn draw(&mut self, plan: &Plan, circles: bool) -> Result<DrawOutput, String> {
        self.draw_with_mode(
            plan,
            if circles {
                crate::gpu_scene::Mode::Circles
            } else {
                crate::gpu_scene::Mode::Triangles
            },
        )
    }

    pub fn draw_with_mode(
        &mut self,
        plan: &Plan,
        mode: crate::gpu_scene::Mode,
    ) -> Result<DrawOutput, String> {
        self.draw_profiled::<false>(plan, mode)
    }

    pub fn draw_profiled<const PROFILE: bool>(
        &mut self,
        plan: &Plan,
        mode: crate::gpu_scene::Mode,
    ) -> Result<DrawOutput, String> {
        let mut stats = triangles::Stats::default();
        let mut timer = Timer::<PROFILE>::new();
        let guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        timer.mark(&mut stats.submission, Stage::Guard);
        let size = (plan.width, plan.height);
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let replacement = Target::new(self.gl.clone(), size.0, size.1)?;
            if let Some(old) = self.target.replace(replacement) {
                self.retired.push(old);
            }
        }
        self.geometry
            .set_instanced_circles(mode == crate::gpu_scene::Mode::Circles);
        self.geometry.set_retention(mode.retention());
        self.geometry.begin_frame();
        let target = self.target.as_ref().unwrap();
        let mut encode = Duration::ZERO;
        let mut submit = Duration::ZERO;
        // Clear exactly once, including empty/transparent first camera passes.
        let started = Instant::now();
        let mut timer = Timer::<PROFILE>::new();
        unsafe {
            let g = &self.gl;
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target.multisample_fbo);
            g.disable(glow::SCISSOR_TEST);
            g.color_mask(true, true, true, true);
            g.clear_color(5.0 / 255.0, 5.0 / 255.0, 20.0 / 255.0, 1.0);
            g.clear(glow::COLOR_BUFFER_BIT);
        }
        submit += started.elapsed();
        timer.mark(&mut stats.submission, Stage::Clear);
        let mut map_index = 0;
        for pass in &plan.passes {
            let (e, s, m) = match pass {
                Pass::Direct { scene, clip } => {
                    self.geometry
                        .draw_into::<PROFILE>(scene, target, Some(*clip), false)?
                }
                Pass::Minimap {
                    scene,
                    destination,
                    opacity,
                } => {
                    let map_size = (scene.width, scene.height);
                    if map_index == self.minimaps.len() {
                        self.minimaps
                            .push(Target::new(self.gl.clone(), map_size.0, map_size.1)?);
                    } else if self.minimaps[map_index].size != map_size {
                        self.minimaps[map_index] =
                            Target::new(self.gl.clone(), map_size.0, map_size.1)?;
                    }
                    // Keep each sampled image until the final composite resolves.
                    // Same-frame scratch reuse failed the V3D interior-pixel
                    // check, despite passing on desktop GL.
                    let map = &self.minimaps[map_index];
                    map_index += 1;
                    let (e, mut s, mut m) =
                        self.geometry.draw_into::<PROFILE>(scene, map, None, true)?;
                    let started = Instant::now();
                    let mut timer = Timer::<PROFILE>::new();
                    map.resolve()?;
                    timer.mark(&mut m.submission, Stage::MinimapResolve);
                    self.quad.draw(map, target, *destination, *opacity)?;
                    timer.mark(&mut m.submission, Stage::MinimapComposite);
                    s += started.elapsed();
                    m.draws += 1;
                    (e, s, m)
                }
            };
            encode += e;
            submit += s;
            stats.vertices += m.vertices;
            stats.indices += m.indices;
            stats.draws += m.draws;
            stats.upload_bytes += m.upload_bytes;
            stats.samples = m.samples;
            stats.circle_instances += m.circle_instances;
            stats.circle_template_bytes = stats.circle_template_bytes.max(m.circle_template_bytes);
            stats.retained.accumulate(m.retained);
            stats.resident.accumulate(m.resident);
            if PROFILE {
                stats.submission.accumulate(m.submission);
            }
            stats.streaming.accumulate(m.streaming);
        }
        self.minimaps.truncate(map_index);
        let started = Instant::now();
        let mut timer = Timer::<PROFILE>::new();
        target.resolve()?;
        timer.mark(&mut stats.submission, Stage::FinalResolve);
        submit += started.elapsed();
        let mut timer = Timer::<PROFILE>::new();
        drop(guard);
        timer.mark(&mut stats.submission, Stage::Guard);
        Ok(DrawOutput {
            image: target.image(),
            encode,
            submit,
            mesh: stats,
        })
    }

    pub fn retire(&mut self) {
        self.geometry.set_admission(Default::default());
        self.geometry.clear_retained();
        self.geometry.set_stream_policy(Default::default());
        self.geometry.set_stroke_culling(Default::default());
        self.geometry.set_stroke_writer(Default::default());
        self.geometry.set_small_strokes(false);
        self.geometry
            .set_round_join_mode(lyon_tessellation::RoundJoinMode::Reference);
        self.retired.extend(self.target.take());
        self.retired.append(&mut self.minimaps);
    }

    /// Isolate first-use geometry costs without reallocating GPU targets.
    pub fn clear_retained(&mut self) {
        self.geometry.clear_retained();
    }

    /// Explicit experiment selection, independent of geometry mode or pacing.
    pub fn set_stream_policy(&mut self, policy: crate::gpu_scene::streaming::Policy) {
        self.geometry.set_stream_policy(policy);
    }

    pub fn set_stroke_writer(&mut self, writer: crate::gpu_scene::stroke_output::StrokeWriter) {
        self.geometry.set_stroke_writer(writer);
    }

    pub fn set_admission(&mut self, mode: crate::gpu_scene::AdmissionMode) {
        self.geometry.set_admission(mode);
    }

    pub fn set_stroke_culling(&mut self, control: crate::gpu_scene::stroke_culling::Control) {
        self.geometry.set_stroke_culling(control);
    }

    pub fn set_small_strokes(&mut self, enabled: bool) {
        self.geometry.set_small_strokes(enabled);
    }

    pub fn small_strokes_used(&self) -> u64 {
        self.geometry.small_strokes_used()
    }

    pub fn set_round_join_mode(&mut self, mode: lyon_tessellation::RoundJoinMode) {
        self.geometry.set_round_join_mode(mode);
    }

    pub fn round_join_stats(&self) -> lyon_tessellation::RoundJoinStats {
        self.geometry.round_join_stats()
    }

    pub fn single_pass_stats(&self) -> crate::gpu_scene::stroke_single_pass::Stats {
        self.geometry.single_pass_stats()
    }

    pub fn stream_usage(&self) -> (usize, usize) {
        self.geometry.stream_usage()
    }

    pub fn collect_retired(&mut self) {
        let _guard = unsafe { gl_state::Guard::new(self.gl.clone()) };
        self.retired.clear();
    }

    pub fn readback(&self) -> Result<Vec<u8>, String> {
        self.target
            .as_ref()
            .ok_or("no composite target")?
            .readback()
    }

    /// Validation only: enqueue pixels into caller-owned, sufficiently sized
    /// pack storage. The caller must fence completion before mapping/reusing it.
    pub fn enqueue_readback(&self, buffer: glow::NativeBuffer) -> Result<(), String> {
        self.target
            .as_ref()
            .ok_or("no composite target")?
            .read_pixels(glow::PixelPackData::BufferOffset(0), Some(buffer))
    }
}

struct Quad {
    gl: Rc<glow::Context>,
    program: Option<glow::NativeProgram>,
    shaders: Vec<glow::NativeShader>,
    vao: Option<glow::NativeVertexArray>,
    viewport: Option<glow::NativeUniformLocation>,
    destination: Option<glow::NativeUniformLocation>,
    opacity: Option<glow::NativeUniformLocation>,
}

impl Quad {
    fn new(gl: Rc<glow::Context>) -> Result<Self, String> {
        let mut q = Self {
            gl,
            program: None,
            shaders: Vec::new(),
            vao: None,
            viewport: None,
            destination: None,
            opacity: None,
        };
        let g = &q.gl;
        let version = if g.version().is_embedded {
            "#version 300 es\n"
        } else {
            "#version 330 core\n"
        };
        let vertex = format!("{version}precision highp float;
            uniform vec2 viewport;
            uniform vec4 destination;
            out vec2 uv;
            void main() {{
                uv = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
                vec2 p = destination.xy + uv * destination.zw;
                gl_Position = vec4(p.x * 2.0 / viewport.x - 1.0, 1.0 - p.y * 2.0 / viewport.y, 0.0, 1.0);
            }}");
        let fragment = format!(
            "{version}precision highp float;
            uniform sampler2D source;
            uniform vec4 destination;
            uniform float opacity;
            in vec2 uv;
            out vec4 output_color;
            void main() {{
                vec2 p = (uv - vec2(0.5)) * destination.zw;
                float radius = min(destination.z, destination.w) * 0.5;
                float coverage = 1.0 - smoothstep(radius - 0.5, radius + 0.5, length(p));
                output_color = texture(source, vec2(uv.x, 1.0 - uv.y)) * (opacity * coverage);
            }}"
        );
        unsafe {
            for (kind, source) in [
                (glow::VERTEX_SHADER, vertex),
                (glow::FRAGMENT_SHADER, fragment),
            ] {
                let s = g.create_shader(kind)?;
                q.shaders.push(s);
                triangles::compile(g, s, &source)?;
            }
            let p = g.create_program()?;
            q.program = Some(p);
            for s in &q.shaders {
                g.attach_shader(p, *s);
            }
            g.link_program(p);
            if !g.get_program_link_status(p) {
                return Err(format!("GPU minimap shader: {}", g.get_program_info_log(p)));
            }
            q.viewport = Some(
                g.get_uniform_location(p, "viewport")
                    .ok_or("missing viewport")?,
            );
            q.destination = Some(
                g.get_uniform_location(p, "destination")
                    .ok_or("missing destination")?,
            );
            q.opacity = Some(
                g.get_uniform_location(p, "opacity")
                    .ok_or("missing opacity")?,
            );
            g.use_program(q.program);
            g.uniform_1_i32(g.get_uniform_location(p, "source").as_ref(), 0);
            q.vao = Some(g.create_vertex_array()?);
        }
        Ok(q)
    }

    fn draw(
        &self,
        source: &Target,
        target: &Target,
        v: crate::render::Viewport,
        opacity: f32,
    ) -> Result<(), String> {
        let g = &self.gl;
        unsafe {
            // draw_into already established premultiplied source-over blending
            // and disabled depth/stencil/culling for this ordered pass.
            g.disable(glow::SCISSOR_TEST);
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target.multisample_fbo);
            g.viewport(0, 0, target.size.0 as i32, target.size.1 as i32);
            g.use_program(self.program);
            g.bind_vertex_array(self.vao);
            g.active_texture(glow::TEXTURE0);
            g.bind_texture(glow::TEXTURE_2D, source.texture);
            g.uniform_2_f32(
                self.viewport.as_ref(),
                target.size.0 as f32,
                target.size.1 as f32,
            );
            g.uniform_4_f32(self.destination.as_ref(), v.x, v.y, v.width, v.height);
            g.uniform_1_f32(self.opacity.as_ref(), opacity);
            g.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            let error = g.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("GPU minimap composite GL error 0x{error:04x}"));
            }
        }
        Ok(())
    }
}

impl Drop for Quad {
    fn drop(&mut self) {
        unsafe {
            if let Some(p) = self.program {
                self.gl.delete_program(p);
            }
            if let Some(v) = self.vao {
                self.gl.delete_vertex_array(v);
            }
            for s in self.shaders.drain(..) {
                self.gl.delete_shader(s);
            }
        }
    }
}
