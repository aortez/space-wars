//! One shader and vertex-input layout for retained polygons and streamed shapes.
//! Resources belong to this context, not a global registry or individual entity.
use super::{check_error, circles_gpu::supports_bindings, compile};
use crate::gpu_scene::reusable::{Builder, Command};
use glow::HasContext;
use std::rc::Rc;

pub(super) struct Resources {
    gl: Rc<glow::Context>,
    shaders: Vec<glow::NativeShader>,
    program: Option<glow::NativeProgram>,
    viewport: Option<glow::NativeUniformLocation>,
    mesh_vao: Option<glow::NativeVertexArray>,
    stream_vao: Option<glow::NativeVertexArray>,
    vertices: Option<glow::NativeBuffer>,
    indices: Option<glow::NativeBuffer>,
    instances: Option<glow::NativeBuffer>,
    dummy: Option<glow::NativeBuffer>,
    revision: Option<u64>,
}
impl Resources {
    // Parent holds the current-context GL state guard through initialization/draw.
    pub fn new(
        gl: Rc<glow::Context>,
        stream_vertices: Option<glow::NativeBuffer>,
        stream_indices: Option<glow::NativeBuffer>,
    ) -> Result<Self, String> {
        let v = gl.version();
        if !supports_bindings(
            v.is_embedded,
            v.major,
            v.minor,
            gl.supported_extensions()
                .contains("GL_ARB_vertex_attrib_binding"),
        ) {
            return Err(
                "reusable meshes require GLES 3.1, GL 4.3 or ARB_vertex_attrib_binding".into(),
            );
        }
        let mut result = Self {
            gl: gl.clone(),
            shaders: Vec::new(),
            program: None,
            viewport: None,
            mesh_vao: None,
            stream_vao: None,
            vertices: None,
            indices: None,
            instances: None,
            dummy: None,
            revision: None,
        };
        let version = if v.is_embedded {
            "#version 300 es\n"
        } else {
            "#version 330 core\n"
        };
        let vertex = format!(
            "{version}precision highp float;
            layout(location=0) in vec2 point;
            layout(location=1) in vec4 color_or_role;
            layout(location=2) in vec4 linear;
            layout(location=3) in vec4 translation;
            layout(location=4) in vec4 fill_color;
            layout(location=5) in vec4 stroke_color;
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                vec2 position = point;
                vertex_color = color_or_role;
                if (color_or_role.a < 0.0) {{
                    position = mat2(linear.xy, linear.zw) * point + translation.xy;
                    vertex_color = color_or_role.a < -1.5 ? stroke_color : fill_color;
                }}
                gl_Position = vec4(position.x * 2.0 / viewport.x - 1.0,
                                   1.0 - position.y * 2.0 / viewport.y, 0.0, 1.0);
            }}"
        );
        let fragment = format!(
            "{version}precision highp float;
            in vec4 vertex_color; out vec4 output_color;
            void main() {{ output_color = vertex_color; }}"
        );
        unsafe {
            for (kind, source) in [
                (glow::VERTEX_SHADER, vertex),
                (glow::FRAGMENT_SHADER, fragment),
            ] {
                let shader = gl.create_shader(kind)?;
                result.shaders.push(shader);
                compile(&gl, shader, &source)?;
            }
            let program = gl.create_program()?;
            result.program = Some(program);
            for shader in &result.shaders {
                gl.attach_shader(program, *shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(format!(
                    "reusable mesh shader: {}",
                    gl.get_program_info_log(program)
                ));
            }
            result.viewport = Some(
                gl.get_uniform_location(program, "viewport")
                    .ok_or("missing reusable mesh viewport")?,
            );
            result.mesh_vao = Some(gl.create_vertex_array()?);
            result.stream_vao = Some(gl.create_vertex_array()?);
            result.vertices = Some(gl.create_buffer()?);
            result.indices = Some(gl.create_buffer()?);
            result.instances = Some(gl.create_buffer()?);
            result.dummy = Some(gl.create_buffer()?);
            gl.bind_buffer(glow::ARRAY_BUFFER, result.dummy);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&[0.0_f32; 16]),
                glow::STATIC_DRAW,
            );
            for (vao, vertices, indices, instances) in [
                (
                    result.mesh_vao,
                    result.vertices,
                    result.indices,
                    result.instances,
                ),
                (
                    result.stream_vao,
                    stream_vertices,
                    stream_indices,
                    result.dummy,
                ),
            ] {
                gl.bind_vertex_array(vao);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, indices);
                gl.bind_vertex_buffer(0, vertices, 0, 24);
                gl.bind_vertex_buffer(1, instances, 0, 64);
                for (location, size, offset) in [(0, 2, 0), (1, 4, 8)] {
                    gl.enable_vertex_attrib_array(location);
                    gl.vertex_attrib_format_f32(location, size, glow::FLOAT, false, offset);
                    gl.vertex_attrib_binding(location, 0);
                }
                for location in 2..=5 {
                    gl.enable_vertex_attrib_array(location);
                    gl.vertex_attrib_format_f32(
                        location,
                        4,
                        glow::FLOAT,
                        false,
                        (location - 2) * 16,
                    );
                    gl.vertex_attrib_binding(location, 1);
                }
                gl.vertex_binding_divisor(0, 0);
                gl.vertex_binding_divisor(1, 1);
            }
            check_error(&gl)?;
        }
        Ok(result)
    }

    pub fn draw(&mut self, mesh: &Builder, width: u32, height: u32) -> u64 {
        let g = &self.gl;
        let mut uploaded = 0;
        unsafe {
            g.use_program(self.program);
            g.uniform_2_f32(self.viewport.as_ref(), width as f32, height as f32);
            g.bind_vertex_array(self.mesh_vao);
            if self.revision != Some(mesh.revision) {
                // Replace backing storage; never mutate data still used by an
                // earlier in-flight draw. Steady state never touches these buffers.
                g.bind_buffer(glow::ARRAY_BUFFER, self.vertices);
                g.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(&mesh.vertices),
                    glow::STATIC_DRAW,
                );
                g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
                g.buffer_data_u8_slice(
                    glow::ELEMENT_ARRAY_BUFFER,
                    bytemuck::cast_slice(&mesh.indices),
                    glow::STATIC_DRAW,
                );
                uploaded += (mesh.vertices.len() * 24 + mesh.indices.len() * 4) as u64;
                self.revision = Some(mesh.revision);
            }
            if !mesh.instances.is_empty() {
                g.bind_buffer(glow::ARRAY_BUFFER, self.instances);
                g.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(&mesh.instances),
                    glow::STREAM_DRAW,
                );
                uploaded += (mesh.instances.len() * 64) as u64;
            }
            for command in &mesh.commands {
                match *command {
                    Command::Stream { start, count } => {
                        g.bind_vertex_array(self.stream_vao);
                        g.draw_elements_instanced(
                            glow::TRIANGLES,
                            count as i32,
                            glow::UNSIGNED_INT,
                            (start * 4) as i32,
                            1,
                        );
                    }
                    Command::Mesh { slot, first, count } => {
                        let (start, indices) = mesh.ranges[slot];
                        g.bind_vertex_array(self.mesh_vao);
                        g.bind_vertex_buffer(1, self.instances, (first * 64) as i32, 64);
                        g.draw_elements_instanced(
                            glow::TRIANGLES,
                            indices as i32,
                            glow::UNSIGNED_INT,
                            (start * 4) as i32,
                            count as i32,
                        );
                    }
                }
            }
        }
        uploaded
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for vao in [self.mesh_vao, self.stream_vao].into_iter().flatten() {
                self.gl.delete_vertex_array(vao);
            }
            for buffer in [self.vertices, self.indices, self.instances, self.dummy]
                .into_iter()
                .flatten()
            {
                self.gl.delete_buffer(buffer);
            }
            if let Some(program) = self.program {
                self.gl.delete_program(program);
            }
            for shader in &self.shaders {
                self.gl.delete_shader(*shader);
            }
        }
    }
}
