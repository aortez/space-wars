//! Offscreen controls with identical vertex formats and divisors for streamed
//! triangles and circle templates. One program; either two VAOs or one VAO.
use super::{check_error, circles_gpu::supports_bindings, compile};
use crate::gpu_scene::{circles, mesh::GeometryMode};
use glow::HasContext;
use std::rc::Rc;

pub(super) struct Resources {
    gl: Rc<glow::Context>,
    shaders: Vec<glow::NativeShader>,
    program: Option<glow::NativeProgram>,
    viewport: Option<glow::NativeUniformLocation>,
    circle_vao: Option<glow::NativeVertexArray>,
    triangle_vao: Option<glow::NativeVertexArray>,
    vertices: Option<glow::NativeBuffer>,
    indices: Option<glow::NativeBuffer>,
    instances: Option<glow::NativeBuffer>,
    dummy_instance: Option<glow::NativeBuffer>,
    // Borrowed from the parent: never delete its streamed buffers here.
    triangle_vertices: Option<glow::NativeBuffer>,
    triangle_indices: Option<glow::NativeBuffer>,
    ranges: Vec<(i32, i32)>,
    pub template_bytes: usize,
}

impl Resources {
    // Caller holds the same-context state guard, including partial initialization.
    pub fn new(
        gl: Rc<glow::Context>,
        triangle_vertices: Option<glow::NativeBuffer>,
        triangle_indices: Option<glow::NativeBuffer>,
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
                "fixed-layout control requires GLES 3.1, GL 4.3 or ARB_vertex_attrib_binding"
                    .into(),
            );
        }
        let mut result = Self {
            gl: gl.clone(),
            shaders: Vec::new(),
            program: None,
            viewport: None,
            circle_vao: None,
            triangle_vao: None,
            vertices: None,
            indices: None,
            instances: None,
            dummy_instance: None,
            triangle_vertices,
            triangle_indices,
            ranges: Vec::new(),
            template_bytes: 0,
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
            layout(location=2) in vec4 placement;
            layout(location=3) in vec4 fill_color;
            layout(location=4) in vec4 stroke_color;
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                vec2 position;
                if (color_or_role.a >= 0.0) {{
                    position = point;
                    vertex_color = color_or_role;
                }} else {{
                    float role = -color_or_role.a - 1.0;
                    bool stroke = role > 1.5;
                    float radius = placement.z;
                    if (stroke) {{
                        radius = max(0.0, radius + (role > 2.5 ? 0.5 : -0.5) * placement.w);
                    }} else if (fill_color.a == 0.0) {{ radius = 0.0; }}
                    position = placement.xy + point * radius;
                    vertex_color = stroke ? stroke_color : fill_color;
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
                    "fixed-layout shader link: {}",
                    gl.get_program_info_log(program)
                ));
            }
            result.viewport = Some(
                gl.get_uniform_location(program, "viewport")
                    .ok_or("missing fixed-layout viewport")?,
            );
            result.circle_vao = Some(gl.create_vertex_array()?);
            result.triangle_vao = Some(gl.create_vertex_array()?);
            result.vertices = Some(gl.create_buffer()?);
            result.indices = Some(gl.create_buffer()?);
            result.instances = Some(gl.create_buffer()?);
            result.dummy_instance = Some(gl.create_buffer()?);

            let mut vertices = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            for n in circles::LEVELS {
                let (v, i) = circles::template(n);
                result
                    .ranges
                    .push(((indices.len() * 4) as i32, i.len() as i32));
                let base = vertices.len() as u32;
                indices.extend(i.into_iter().map(|i| base + i));
                vertices.extend(v.into_iter().map(circles::layout_vertex));
            }
            result.template_bytes = vertices.len() * 24 + indices.len() * 4;
            gl.bind_vertex_array(result.circle_vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, result.vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&vertices),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, result.indices);
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&indices),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, result.dummy_instance);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&[0.0_f32; 12]),
                glow::STATIC_DRAW,
            );

            // The entire format, enable mask, binding association and divisor
            // setup is identical. Only the buffers differ between the two VAOs.
            for (vao, vertices, indices, instances) in [
                (
                    result.circle_vao,
                    result.vertices,
                    result.indices,
                    result.instances,
                ),
                (
                    result.triangle_vao,
                    triangle_vertices,
                    triangle_indices,
                    result.dummy_instance,
                ),
            ] {
                gl.bind_vertex_array(vao);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, indices);
                gl.bind_vertex_buffer(0, vertices, 0, 24);
                gl.bind_vertex_buffer(1, instances, 0, 48);
                for (location, size, offset) in [(0, 2, 0), (1, 4, 8)] {
                    gl.enable_vertex_attrib_array(location);
                    gl.vertex_attrib_format_f32(location, size, glow::FLOAT, false, offset);
                    gl.vertex_attrib_binding(location, 0);
                }
                for location in 2..=4 {
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

    pub fn upload(&self, instances: &[circles::Instance]) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, self.instances);
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(instances),
                glow::STREAM_DRAW,
            );
        }
    }

    pub fn prepare_submission(&self, mode: GeometryMode, width: u32, height: u32) {
        unsafe {
            self.gl.use_program(self.program);
            self.gl
                .uniform_2_f32(self.viewport.as_ref(), width as f32, height as f32);
            // Reset after single-VAO use, including on mode changes. Do it before
            // batches so the one-VAO variant needs no per-draw VAO calls.
            self.gl.bind_vertex_array(self.circle_vao);
            self.gl.bind_vertex_buffer(0, self.vertices, 0, 24);
            self.gl
                .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
            if mode == GeometryMode::CirclesUnifiedVao {
                self.gl.bind_vertex_buffer(1, self.instances, 0, 48);
            }
        }
    }

    pub fn draw_triangles(&self, start: usize, count: usize, mode: GeometryMode) {
        unsafe {
            let g = &self.gl;
            if mode == GeometryMode::CirclesUnifiedVao {
                g.bind_vertex_buffer(0, self.triangle_vertices, 0, 24);
                g.bind_vertex_buffer(1, self.dummy_instance, 0, 48);
                g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.triangle_indices);
            } else {
                g.bind_vertex_array(self.triangle_vao);
            }
            g.draw_elements_instanced(
                glow::TRIANGLES,
                count as i32,
                glow::UNSIGNED_INT,
                (start * 4) as i32,
                1,
            );
        }
    }

    pub fn draw_circles(&self, start: usize, count: usize, level: usize, mode: GeometryMode) {
        unsafe {
            let g = &self.gl;
            if mode == GeometryMode::CirclesUnifiedVao {
                g.bind_vertex_buffer(0, self.vertices, 0, 24);
                g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
            } else {
                g.bind_vertex_array(self.circle_vao);
            }
            g.bind_vertex_buffer(1, self.instances, (start * 48) as i32, 48);
            let (offset, indices) = self.ranges[level];
            g.draw_elements_instanced(
                glow::TRIANGLES,
                indices,
                glow::UNSIGNED_INT,
                offset,
                count as i32,
            );
        }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for vao in [self.circle_vao, self.triangle_vao].into_iter().flatten() {
                self.gl.delete_vertex_array(vao);
            }
            for buffer in [
                self.vertices,
                self.indices,
                self.instances,
                self.dummy_instance,
            ]
            .into_iter()
            .flatten()
            {
                self.gl.delete_buffer(buffer);
            }
            if let Some(program) = self.program {
                self.gl.delete_program(program);
            }
            for shader in self.shaders.drain(..) {
                self.gl.delete_shader(shader);
            }
        }
    }
}
