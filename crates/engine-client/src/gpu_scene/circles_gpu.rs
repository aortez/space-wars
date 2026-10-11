//! GPU-resident templates plus a streamed instance buffer. Owned by one GL
//! context; no global registry, per-entity cache, or scenario-facing handles.
use super::{check_error, compile};
use crate::gpu_scene::{
    circles::{self, Instance, Vertex},
    mesh::GeometryMode,
};
use glow::HasContext;
use std::rc::Rc;

pub(super) struct Resources {
    gl: Rc<glow::Context>,
    shaders: Vec<glow::NativeShader>,
    program: Option<glow::NativeProgram>,
    viewport: Option<glow::NativeUniformLocation>,
    vao: Option<glow::NativeVertexArray>,
    binding_vao: Option<glow::NativeVertexArray>,
    triangle_vao: Option<glow::NativeVertexArray>,
    vertices: Option<glow::NativeBuffer>,
    indices: Option<glow::NativeBuffer>,
    instances: Option<glow::NativeBuffer>,
    ranges: Vec<(i32, i32)>,
    pub template_bytes: usize,
}

impl Resources {
    // Caller holds the shared-context state guard during construction/use/drop.
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, String> {
        Self::new_inner(gl, false)
    }

    fn new_inner(gl: Rc<glow::Context>, unified: bool) -> Result<Self, String> {
        let mut result = Self {
            gl: gl.clone(),
            shaders: Vec::new(),
            program: None,
            viewport: None,
            vao: None,
            binding_vao: None,
            triangle_vao: None,
            vertices: None,
            indices: None,
            instances: None,
            ranges: Vec::new(),
            template_bytes: 0,
        };
        let version = if gl.version().is_embedded {
            "#version 300 es\n"
        } else {
            "#version 330 core\n"
        };
        let vertex = if unified {
            format!(
                "{version}precision highp float;
            layout(location=0) in vec3 unit_vertex;
            layout(location=1) in vec4 placement;
            layout(location=2) in vec4 fill_color;
            layout(location=3) in vec4 stroke_color;
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                vec2 position;
                if (unit_vertex.z == 0.0) {{
                    position = unit_vertex.xy;
                    vertex_color = fill_color;
                }} else {{
                    float role = unit_vertex.z - 1.0;
                    bool stroke = role > 1.5;
                    float radius = placement.z;
                    if (stroke) {{
                        radius = max(0.0, radius + (role > 2.5 ? 0.5 : -0.5) * placement.w);
                    }} else if (fill_color.a == 0.0) {{ radius = 0.0; }}
                    position = placement.xy + unit_vertex.xy * radius;
                    vertex_color = stroke ? stroke_color : fill_color;
                }}
                gl_Position = vec4(position.x * 2.0 / viewport.x - 1.0,
                                   1.0 - position.y * 2.0 / viewport.y, 0.0, 1.0);
            }}"
            )
        } else {
            format!(
                "{version}precision highp float;
            layout(location=0) in vec3 unit_vertex;
            layout(location=1) in vec4 placement;
            layout(location=2) in vec4 fill_color;
            layout(location=3) in vec4 stroke_color;
            uniform vec2 viewport;
            out vec4 vertex_color;
            void main() {{
                bool stroke = unit_vertex.z > 1.5;
                float radius = placement.z;
                if (stroke) {{
                    radius = max(0.0, radius + (unit_vertex.z > 2.5 ? 0.5 : -0.5) * placement.w);
                }} else if (fill_color.a == 0.0) {{ radius = 0.0; }}
                vec2 position = placement.xy + unit_vertex.xy * radius;
                gl_Position = vec4(position.x * 2.0 / viewport.x - 1.0,
                                   1.0 - position.y * 2.0 / viewport.y, 0.0, 1.0);
                vertex_color = stroke ? stroke_color : fill_color;
            }}"
            )
        };
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
                    "GPU circle shader link: {}",
                    gl.get_program_info_log(program)
                ));
            }
            result.viewport = Some(
                gl.get_uniform_location(program, "viewport")
                    .ok_or("missing circle viewport")?,
            );
            result.vao = Some(gl.create_vertex_array()?);
            result.vertices = Some(gl.create_buffer()?);
            result.indices = Some(gl.create_buffer()?);
            result.instances = Some(gl.create_buffer()?);
            let mut vertices: Vec<Vertex> = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            for n in circles::LEVELS {
                let (mut v, i) = circles::template(n);
                if unified {
                    // Zero is the streamed-triangle tag (a vec2 attribute's
                    // implicit z). Shift circle roles without changing geometry.
                    for vertex in &mut v {
                        vertex[2] += 1.0;
                    }
                }
                result
                    .ranges
                    .push(((indices.len() * 4) as i32, i.len() as i32));
                let base = vertices.len() as u32;
                indices.extend(i.into_iter().map(|i| i + base));
                vertices.extend(v);
            }
            result.template_bytes =
                vertices.len() * std::mem::size_of::<Vertex>() + indices.len() * 4;
            gl.bind_vertex_array(result.vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, result.vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&vertices),
                glow::STATIC_DRAW,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, result.indices);
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&indices),
                glow::STATIC_DRAW,
            );
            for location in 1..=3 {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_divisor(location, 1);
            }
            check_error(&gl)?;
        }
        Ok(result)
    }

    pub fn new_unified(
        gl: Rc<glow::Context>,
        vertices: Option<glow::NativeBuffer>,
        indices: Option<glow::NativeBuffer>,
    ) -> Result<Self, String> {
        let mut result = Self::new_inner(gl.clone(), true)?;
        unsafe {
            result.triangle_vao = Some(gl.create_vertex_array()?);
            gl.bind_vertex_array(result.triangle_vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, vertices);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, indices);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 24, 0);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 24, 8);
            check_error(&gl)?;
        }
        Ok(result)
    }

    pub fn upload(&self, instances: &[Instance]) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, self.instances);
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(instances),
                glow::STREAM_DRAW,
            );
        }
    }

    // Separate attribute format from buffer offset. One additional owned VAO,
    // not a per-object cache; do not mutate the baseline VAO or Slint's arrays.
    fn ensure_bindings(&mut self) -> Result<(), String> {
        if self.binding_vao.is_some() {
            return Ok(());
        }
        let g = &self.gl;
        let v = g.version();
        if !supports_bindings(
            v.is_embedded,
            v.major,
            v.minor,
            g.supported_extensions()
                .contains("GL_ARB_vertex_attrib_binding"),
        ) {
            return Err(
                "separate vertex bindings require GLES 3.1, GL 4.3 or ARB_vertex_attrib_binding"
                    .into(),
            );
        }
        unsafe {
            let vao = g.create_vertex_array()?;
            // Own the handle before any later fallible operation.
            self.binding_vao = Some(vao);
            g.bind_vertex_array(Some(vao));
            g.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, self.indices);
            g.bind_vertex_buffer(0, self.vertices, 0, 12);
            g.enable_vertex_attrib_array(0);
            g.vertex_attrib_format_f32(0, 3, glow::FLOAT, false, 0);
            g.vertex_attrib_binding(0, 0);
            g.bind_vertex_buffer(1, self.instances, 0, 48);
            for location in 1..=3 {
                g.enable_vertex_attrib_array(location);
                g.vertex_attrib_format_f32(location, 4, glow::FLOAT, false, (location - 1) * 16);
                g.vertex_attrib_binding(location, 1);
            }
            g.vertex_binding_divisor(1, 1);
            if let Err(error) = check_error(g) {
                g.delete_vertex_array(vao);
                self.binding_vao = None;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn prepare_submission(
        &mut self,
        mode: GeometryMode,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        if mode.buffer_bindings() {
            self.ensure_bindings()?;
        }
        if mode.frame_uniforms() {
            unsafe {
                self.gl.use_program(self.program);
                self.gl
                    .uniform_2_f32(self.viewport.as_ref(), width as f32, height as f32);
            }
        }
        Ok(())
    }

    pub fn draw(
        &self,
        start: usize,
        count: usize,
        level: usize,
        width: u32,
        height: u32,
        mode: GeometryMode,
    ) {
        unsafe {
            let g = &self.gl;
            if !mode.unified_program() {
                g.use_program(self.program);
            }
            if !mode.frame_uniforms() {
                g.uniform_2_f32(self.viewport.as_ref(), width as f32, height as f32);
            }
            if mode.buffer_bindings() {
                g.bind_vertex_array(self.binding_vao);
                g.bind_vertex_buffer(1, self.instances, (start * 48) as i32, 48);
            } else {
                g.bind_vertex_array(self.vao);
                g.bind_buffer(glow::ARRAY_BUFFER, self.instances);
                for location in 1..=3 {
                    g.vertex_attrib_pointer_f32(
                        location,
                        4,
                        glow::FLOAT,
                        false,
                        48,
                        (start * 48 + (location as usize - 1) * 16) as i32,
                    );
                }
            }
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

    // The shared program and viewport were selected once in prepare_submission.
    // Only VAOs change between triangle and circle batches, never source order.
    pub fn draw_triangles(&self, start: usize, count: usize, instanced: bool) {
        unsafe {
            self.gl.bind_vertex_array(self.triangle_vao);
            if instanced {
                self.gl.draw_elements_instanced(
                    glow::TRIANGLES,
                    count as i32,
                    glow::UNSIGNED_INT,
                    (start * 4) as i32,
                    1,
                );
            } else {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    count as i32,
                    glow::UNSIGNED_INT,
                    (start * 4) as i32,
                );
            }
        }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for vao in [self.vao, self.binding_vao, self.triangle_vao]
                .into_iter()
                .flatten()
            {
                self.gl.delete_vertex_array(vao);
            }
            for buffer in [self.vertices, self.indices, self.instances]
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

pub(super) fn supports_bindings(embedded: bool, major: u32, minor: u32, extension: bool) -> bool {
    if embedded {
        (major, minor) >= (3, 1)
    } else {
        (major, minor) >= (4, 3) || extension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_binding_api_is_capability_gated() {
        assert!(supports_bindings(true, 3, 1, false));
        assert!(!supports_bindings(true, 3, 0, false));
        assert!(supports_bindings(false, 4, 3, false));
        assert!(!supports_bindings(false, 3, 3, false));
        assert!(supports_bindings(false, 3, 3, true));
    }
}
