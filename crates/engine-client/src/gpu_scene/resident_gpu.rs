//! Vertex pulling preserves one indexed draw per ordered scene pass. Geometry
//! lives in RGBA32F texels or read-only storage buffers; only references,
//! placements and guarded pieces stream. Both paths use the same record layout.
use crate::gpu_scene::{
    resident::{
        ATLAS_VERTICES, Frame, ROW_VERTICES, TRANSFORM_ADDRESS, TRANSFORM_BITS, UNIFORM_PLACEMENTS,
    },
    resident_storage as storage,
};
use glow::HasContext;
use std::rc::Rc;

use storage::Bindings;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Access {
    Texture,
    PassTextures,
    PassBuffers,
    PassUniforms,
}

impl Access {
    fn binding_target(self) -> Option<u32> {
        match self {
            Self::PassBuffers => Some(glow::SHADER_STORAGE_BUFFER),
            Self::PassUniforms => Some(glow::UNIFORM_BUFFER),
            _ => None,
        }
    }
}

pub(super) struct Resources {
    gl: Rc<glow::Context>,
    program: Option<glow::NativeProgram>,
    shaders: Vec<glow::NativeShader>,
    vao: Option<glow::NativeVertexArray>,
    references: Option<glow::NativeBuffer>,
    indices: Option<glow::NativeBuffer>,
    atlas: Option<glow::NativeTexture>,
    data: Vec<glow::NativeTexture>,
    storage: Vec<glow::NativeBuffer>,
    uniforms: Vec<glow::NativeBuffer>,
    access: Access,
    next_pass: usize,
    viewport: Option<glow::NativeUniformLocation>,
}

impl Resources {
    pub fn new(gl: Rc<glow::Context>, access: Access) -> Result<Self, String> {
        let _bindings = if let Some(target) = access.binding_target() {
            // Check limits before querying/changing any SSBO state. GL3.3 and
            // ES3.0 remain usable through the existing non-storage paths.
            unsafe {
                if access == Access::PassBuffers {
                    storage::validate(&gl)?;
                } else {
                    storage::validate_uniforms(&gl)?;
                }
            }
            Some(unsafe { Bindings::save(gl.clone(), target)? })
        } else {
            None
        };
        let mut r = Self {
            gl,
            program: None,
            shaders: Vec::new(),
            vao: None,
            references: None,
            indices: None,
            atlas: None,
            data: Vec::new(),
            storage: Vec::new(),
            uniforms: Vec::new(),
            access,
            next_pass: 0,
            viewport: None,
        };
        // SAFETY: Owner keeps the render context current, including error cleanup.
        unsafe {
            r.initialize(if access == Access::Texture { 1 } else { 8 })?;
        }
        Ok(r)
    }

    pub fn begin_frame(&mut self) {
        self.next_pass = 0;
    }

    unsafe fn initialize(&mut self, slots: usize) -> Result<(), String> {
        let g = &self.gl;
        let buffers = self.access == Access::PassBuffers;
        // At most one million streamed vertices plus 4096 transforms, two
        // texels each. GLES3's guaranteed 2048 texture height is sufficient.
        if !buffers && unsafe { g.get_parameter_i32(glow::MAX_TEXTURE_SIZE) } < 2048 {
            return Err("resident geometry needs 2048 texture dimensions".into());
        }
        let version = match (g.version().is_embedded, buffers) {
            (true, true) => "#version 310 es\n",
            (false, true) => "#version 430 core\n",
            (true, false) => "#version 300 es\n",
            (false, false) => "#version 330 core\n",
        };
        let access = if buffers {
            "layout(std430, binding=0) readonly buffer Atlas { vec4 atlas_data[]; };
             layout(std430, binding=1) readonly buffer FrameData { vec4 frame_data[]; };
             vec4 geometry(uint record, uint part) { return atlas_data[record * 2u + part]; }
             vec4 dynamic_data(uint record, uint part) { return frame_data[record * 2u + part]; }"
        } else {
            "uniform highp sampler2D atlas;
             uniform highp sampler2D frame_data;
             ivec2 texel(uint record, uint component) {
                 uint address = record * 2u + component;
                 return ivec2(int(address % 1024u), int(address / 1024u));
             }
             vec4 geometry(uint record, uint part) { return texelFetch(atlas, texel(record, part), 0); }
             vec4 dynamic_data(uint record, uint part) { return texelFetch(frame_data, texel(record, part), 0); }"
        };
        let placement_access = if self.access == Access::PassUniforms {
            format!(
                "layout(std140) uniform Placements {{ vec4 placements[{}]; }};
                vec4 placement(uint reference, uint part) {{
                    uint compact = reference >> {TRANSFORM_BITS}u;
                    if (compact < {UNIFORM_PLACEMENTS}u) return placements[compact * 2u + part];
                    return dynamic_data(reference & {TRANSFORM_ADDRESS}u, part);
                }}",
                UNIFORM_PLACEMENTS * 2
            )
        } else {
            format!(
                "vec4 placement(uint reference, uint part) {{ return dynamic_data(reference & {TRANSFORM_ADDRESS}u, part); }}"
            )
        };
        let vertex = format!(
            "{version}precision highp float;
            precision highp int;
            layout(location=0) in uvec2 reference;
            {access}
            {placement_access}
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                vec2 position;
                if ((reference.x & 0x80000000u) != 0u) {{
                    uint address = reference.x & 0x7fffffffu;
                    position = dynamic_data(address, 0u).xy;
                    vertex_color = dynamic_data(address, 1u);
                }} else {{
                    vec4 vertex = geometry(reference.x, 0u);
                    vertex_color = geometry(reference.x, 1u);
                    vec4 transform = placement(reference.y, 0u);
                    vec2 translation = placement(reference.y, 1u).xy;
                    vec2 local = vertex.xy * transform.x + vertex.zw;
                    float s = transform.y;
                    float c = transform.z;
                    position = vec2(c * local.x + s * local.y + translation.x,
                                    -s * local.x + c * local.y + translation.y);
                }}
                gl_Position = vec4(position.x * 2.0 / viewport.x - 1.0,
                                   1.0 - position.y * 2.0 / viewport.y, 0.0, 1.0);
            }}"
        );
        let fragment = format!(
            "{version}precision highp float;
            in vec4 vertex_color;
            out vec4 output_color;
            void main() {{ output_color = vertex_color; }}"
        );
        unsafe {
            for (kind, source) in [
                (glow::VERTEX_SHADER, vertex),
                (glow::FRAGMENT_SHADER, fragment),
            ] {
                let shader = g.create_shader(kind)?;
                self.shaders.push(shader);
                super::compile(g, shader, &source)?;
            }
            let program = g.create_program()?;
            self.program = Some(program);
            for shader in &self.shaders {
                g.attach_shader(program, *shader);
            }
            g.link_program(program);
            if !g.get_program_link_status(program) {
                return Err(format!(
                    "resident shader link: {}",
                    g.get_program_info_log(program)
                ));
            }
            self.viewport = Some(
                g.get_uniform_location(program, "viewport")
                    .ok_or("resident viewport missing")?,
            );
            g.use_program(Some(program));
            if self.access == Access::PassUniforms {
                let block = g
                    .get_uniform_block_index(program, "Placements")
                    .ok_or("resident uniform block missing")?;
                g.uniform_block_binding(program, block, 0);
                for _ in 0..slots {
                    self.uniforms.push(g.create_buffer()?);
                }
            }
            if !buffers {
                for (name, unit) in [("atlas", 0), ("frame_data", 1)] {
                    let uniform = g
                        .get_uniform_location(program, name)
                        .ok_or("resident sampler missing")?;
                    g.uniform_1_i32(Some(&uniform), unit);
                }
            }
            self.vao = Some(g.create_vertex_array()?);
            self.references = Some(g.create_buffer()?);
            self.indices = Some(g.create_buffer()?);
            g.bind_vertex_array(self.vao);
            g.bind_buffer(glow::ARRAY_BUFFER, self.references);
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
            g.enable_vertex_attrib_array(0);
            g.vertex_attrib_pointer_i32(0, 2, glow::UNSIGNED_INT, 8, 0);
            if buffers {
                for _ in 0..=slots {
                    self.storage.push(g.create_buffer()?);
                }
                g.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(self.storage[0]));
                g.buffer_data_size(
                    glow::SHADER_STORAGE_BUFFER,
                    (ATLAS_VERTICES * 32) as i32,
                    glow::DYNAMIC_DRAW,
                );
                return super::check_error(g);
            }
            self.atlas = Some(g.create_texture()?);
            for _ in 0..slots {
                self.data.push(g.create_texture()?);
            }
            for texture in self.atlas.iter().chain(&self.data) {
                g.bind_texture(glow::TEXTURE_2D, Some(*texture));
                for (parameter, value) in [
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    g.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
                }
            }
            g.bind_texture(glow::TEXTURE_2D, self.atlas);
            // tex_image rather than immutable storage also supports desktop GL3.3.
            g.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA32F as i32,
                (ROW_VERTICES * 2) as i32,
                (ATLAS_VERTICES / ROW_VERTICES) as i32,
                0,
                glow::RGBA,
                glow::FLOAT,
                glow::PixelUnpackData::Slice(None),
            );
            super::check_error(g)?;
        }
        Ok(())
    }

    /// Returns actual uploaded bytes including row padding and new atlas rows.
    /// Ordinary ordered GL writes protect older in-flight draws; no unsynchronized
    /// buffer mappings, fences polled by the CPU, or per-group draw commands.
    pub unsafe fn draw(
        &mut self,
        frame: &mut Frame,
        width: u32,
        height: u32,
    ) -> Result<u64, String> {
        let g = &self.gl;
        let buffers = self.access == Access::PassBuffers;
        let _bindings = if let Some(target) = self.access.binding_target() {
            Some(unsafe { Bindings::save(g.clone(), target)? })
        } else {
            None
        };
        let mut uploaded = 0;
        unsafe {
            if buffers {
                g.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(self.storage[0]));
            } else {
                g.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                g.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                for p in [
                    glow::UNPACK_ROW_LENGTH,
                    glow::UNPACK_SKIP_ROWS,
                    glow::UNPACK_SKIP_PIXELS,
                ] {
                    g.pixel_store_i32(p, 0);
                }
                g.active_texture(glow::TEXTURE0);
                g.bind_sampler(0, None);
                g.bind_texture(glow::TEXTURE_2D, self.atlas);
            }
            for rows in frame.atlas.dirty_rows() {
                let data = bytemuck::cast_slice(
                    &frame.atlas.vertices[rows.start * ROW_VERTICES..rows.end * ROW_VERTICES],
                );
                if buffers {
                    g.buffer_sub_data_u8_slice(
                        glow::SHADER_STORAGE_BUFFER,
                        (rows.start * ROW_VERTICES * 32) as i32,
                        data,
                    );
                } else {
                    g.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        rows.start as i32,
                        (ROW_VERTICES * 2) as i32,
                        rows.len() as i32,
                        glow::RGBA,
                        glow::FLOAT,
                        glow::PixelUnpackData::Slice(Some(data)),
                    );
                }
                uploaded += data.len() as u64;
            }
            if !buffers {
                g.active_texture(glow::TEXTURE1);
                g.bind_sampler(1, None);
                // Separate camera-pass storage tests read/write dependencies without
                // changing the shader or upload volume. Bounded wrap remains safe
                // through ordered GL writes; this is not a fence-free frame ring.
                g.bind_texture(
                    glow::TEXTURE_2D,
                    Some(self.data[self.next_pass % self.data.len()]),
                );
            }
            let rows = frame.data.len().div_ceil(ROW_VERTICES).max(1);
            frame.data.resize(rows * ROW_VERTICES, [0.; 8]);
            let data = bytemuck::cast_slice(&frame.data);
            if buffers {
                // Match the texture control's per-camera lifetime and padding.
                // No persistent/unsynchronized mappings or per-group draws.
                let buffer = self.storage[1 + self.next_pass % (self.storage.len() - 1)];
                g.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(buffer));
                g.buffer_data_u8_slice(glow::SHADER_STORAGE_BUFFER, data, glow::STREAM_DRAW);
                g.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 0, Some(self.storage[0]));
                g.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 1, Some(buffer));
            } else {
                g.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA32F as i32,
                    (ROW_VERTICES * 2) as i32,
                    rows as i32,
                    0,
                    glow::RGBA,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(data)),
                );
            }
            uploaded += data.len() as u64;
            if self.access == Access::PassUniforms {
                let buffer = self.uniforms[self.next_pass % self.uniforms.len()];
                let data = bytemuck::cast_slice(
                    &frame.transforms[..frame.transforms.len().min(UNIFORM_PLACEMENTS)],
                );
                g.bind_buffer(glow::UNIFORM_BUFFER, Some(buffer));
                g.buffer_data_size(
                    glow::UNIFORM_BUFFER,
                    (UNIFORM_PLACEMENTS * 32) as i32,
                    glow::STREAM_DRAW,
                );
                if !data.is_empty() {
                    g.buffer_sub_data_u8_slice(glow::UNIFORM_BUFFER, 0, data);
                }
                g.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(buffer));
                uploaded += data.len() as u64;
            }
            self.next_pass += 1;
            g.use_program(self.program);
            g.uniform_2_f32(self.viewport.as_ref(), width as f32, height as f32);
            g.bind_vertex_array(self.vao);
            g.bind_buffer(glow::ARRAY_BUFFER, self.references);
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
            let references = bytemuck::cast_slice(&frame.references);
            let indices = bytemuck::cast_slice(&frame.indices);
            g.buffer_data_u8_slice(glow::ARRAY_BUFFER, references, glow::STREAM_DRAW);
            g.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices, glow::STREAM_DRAW);
            uploaded += (references.len() + indices.len()) as u64;
            if !frame.indices.is_empty() {
                g.draw_elements(
                    glow::TRIANGLES,
                    frame.indices.len() as i32,
                    glow::UNSIGNED_INT,
                    0,
                );
            }
            super::check_error(g)?;
            frame.atlas.dirty.clear();
        }
        Ok(uploaded)
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        // SAFETY: Presenter owns these resources only while its context is current.
        unsafe {
            let g = &self.gl;
            if let Some(p) = self.program.take() {
                g.delete_program(p);
            }
            for s in self.shaders.drain(..) {
                g.delete_shader(s);
            }
            if let Some(v) = self.vao.take() {
                g.delete_vertex_array(v);
            }
            for b in [self.references.take(), self.indices.take()]
                .into_iter()
                .flatten()
            {
                g.delete_buffer(b);
            }
            for t in self.atlas.take().into_iter().chain(self.data.drain(..)) {
                g.delete_texture(t);
            }
            for b in self.storage.drain(..).chain(self.uniforms.drain(..)) {
                g.delete_buffer(b);
            }
        }
    }
}
