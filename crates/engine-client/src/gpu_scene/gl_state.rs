//! The direct canvas shares Slint's current GLES3 context. Preserve its state,
//! including separate front/back stencil state and both framebuffer bindings.

use glow::HasContext;
use std::rc::Rc;

pub(super) struct Guard {
    gl: Rc<glow::Context>,
    program: Option<glow::NativeProgram>,
    vao: Option<glow::NativeVertexArray>,
    array: Option<glow::NativeBuffer>,
    draw: Option<glow::NativeFramebuffer>,
    read: Option<glow::NativeFramebuffer>,
    renderbuffer: Option<glow::NativeRenderbuffer>,
    textures: [Option<glow::NativeTexture>; 2],
    samplers: [Option<glow::NativeSampler>; 2],
    active: u32,
    viewport: [i32; 4],
    scissor: [i32; 4],
    clear: [f32; 4],
    color_mask: [bool; 4],
    enables: [bool; 10],
    blend: [i32; 6],
    stencil: [[i32; 7]; 2],
    cull: i32,
    front: i32,
    stencil_clear: i32,
    unpack: [i32; 4],
    unpack_buffer: Option<glow::NativeBuffer>,
    desktop_multisample: Option<bool>,
}

const CAPS: [u32; 10] = [
    glow::BLEND,
    glow::CULL_FACE,
    glow::DEPTH_TEST,
    glow::SCISSOR_TEST,
    glow::STENCIL_TEST,
    glow::SAMPLE_ALPHA_TO_COVERAGE,
    glow::SAMPLE_COVERAGE,
    glow::SAMPLE_MASK,
    glow::RASTERIZER_DISCARD,
    glow::DITHER,
];
const BLEND: [u32; 6] = [
    glow::BLEND_SRC_RGB,
    glow::BLEND_DST_RGB,
    glow::BLEND_SRC_ALPHA,
    glow::BLEND_DST_ALPHA,
    glow::BLEND_EQUATION_RGB,
    glow::BLEND_EQUATION_ALPHA,
];
const UNPACK: [u32; 4] = [
    glow::UNPACK_ALIGNMENT,
    glow::UNPACK_ROW_LENGTH,
    glow::UNPACK_SKIP_ROWS,
    glow::UNPACK_SKIP_PIXELS,
];
const FRONT: [u32; 7] = [
    glow::STENCIL_FUNC,
    glow::STENCIL_REF,
    glow::STENCIL_VALUE_MASK,
    glow::STENCIL_WRITEMASK,
    glow::STENCIL_FAIL,
    glow::STENCIL_PASS_DEPTH_FAIL,
    glow::STENCIL_PASS_DEPTH_PASS,
];
const BACK: [u32; 7] = [
    glow::STENCIL_BACK_FUNC,
    glow::STENCIL_BACK_REF,
    glow::STENCIL_BACK_VALUE_MASK,
    glow::STENCIL_BACK_WRITEMASK,
    glow::STENCIL_BACK_FAIL,
    glow::STENCIL_BACK_PASS_DEPTH_FAIL,
    glow::STENCIL_BACK_PASS_DEPTH_PASS,
];

impl Guard {
    // SAFETY: Caller keeps this GLES3 context current until the guard is dropped.
    pub(super) unsafe fn new(gl: Rc<glow::Context>) -> Self {
        unsafe {
            let active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            let textures = std::array::from_fn(|index| {
                gl.active_texture(glow::TEXTURE0 + index as u32);
                gl.get_parameter_texture(glow::TEXTURE_BINDING_2D)
            });
            let samplers = std::array::from_fn(|index| {
                gl.active_texture(glow::TEXTURE0 + index as u32);
                gl.get_parameter_sampler(glow::SAMPLER_BINDING)
            });
            // Keep our canvas allocations on unit zero; do not disturb any
            // other unit that Slint's caller may have left active.
            gl.active_texture(glow::TEXTURE0);
            let mut viewport = [0; 4];
            let mut scissor = [0; 4];
            let mut clear = [0.0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear);
            let saved = Self {
                program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                vao: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                array: gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
                draw: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                read: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                renderbuffer: gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING),
                color_mask: gl.get_parameter_bool_array(glow::COLOR_WRITEMASK),
                enables: CAPS.map(|cap| gl.is_enabled(cap)),
                blend: BLEND.map(|p| gl.get_parameter_i32(p)),
                stencil: [FRONT, BACK].map(|params| params.map(|p| gl.get_parameter_i32(p))),
                cull: gl.get_parameter_i32(glow::CULL_FACE_MODE),
                front: gl.get_parameter_i32(glow::FRONT_FACE),
                stencil_clear: gl.get_parameter_i32(glow::STENCIL_CLEAR_VALUE),
                unpack: UNPACK.map(|p| gl.get_parameter_i32(p)),
                unpack_buffer: gl.get_parameter_buffer(glow::PIXEL_UNPACK_BUFFER_BINDING),
                desktop_multisample: (!gl.version().is_embedded)
                    .then(|| gl.is_enabled(glow::MULTISAMPLE)),
                gl,
                active,
                textures,
                samplers,
                viewport,
                scissor,
                clear,
            };
            saved.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            saved
                .gl
                .blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
            saved.gl.clear_stencil(0);
            saved
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: The owning rendering callback still has the same context.
        unsafe {
            let g = &self.gl;
            g.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.draw);
            g.bind_framebuffer(glow::READ_FRAMEBUFFER, self.read);
            g.bind_renderbuffer(glow::RENDERBUFFER, self.renderbuffer);
            g.use_program(self.program);
            g.bind_vertex_array(self.vao);
            g.bind_buffer(glow::ARRAY_BUFFER, self.array);
            for (index, texture) in self.textures.iter().enumerate() {
                g.active_texture(glow::TEXTURE0 + index as u32);
                g.bind_texture(glow::TEXTURE_2D, *texture);
                g.bind_sampler(index as u32, self.samplers[index]);
            }
            g.active_texture(self.active);
            g.viewport(
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            );
            g.scissor(
                self.scissor[0],
                self.scissor[1],
                self.scissor[2],
                self.scissor[3],
            );
            g.clear_color(self.clear[0], self.clear[1], self.clear[2], self.clear[3]);
            g.color_mask(
                self.color_mask[0],
                self.color_mask[1],
                self.color_mask[2],
                self.color_mask[3],
            );
            for (cap, enabled) in CAPS.into_iter().zip(self.enables) {
                if enabled {
                    g.enable(cap);
                } else {
                    g.disable(cap);
                }
            }
            if let Some(enabled) = self.desktop_multisample {
                if enabled {
                    g.enable(glow::MULTISAMPLE);
                } else {
                    g.disable(glow::MULTISAMPLE);
                }
            }
            let b = self.blend;
            g.blend_func_separate(b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32);
            g.blend_equation_separate(b[4] as u32, b[5] as u32);
            for (face, s) in [glow::FRONT, glow::BACK].into_iter().zip(self.stencil) {
                g.stencil_func_separate(face, s[0] as u32, s[1], s[2] as u32);
                g.stencil_mask_separate(face, s[3] as u32);
                g.stencil_op_separate(face, s[4] as u32, s[5] as u32, s[6] as u32);
            }
            g.cull_face(self.cull as u32);
            g.front_face(self.front as u32);
            g.clear_stencil(self.stencil_clear);
            for (parameter, value) in UNPACK.into_iter().zip(self.unpack) {
                g.pixel_store_i32(parameter, value);
            }
            g.bind_buffer(glow::PIXEL_UNPACK_BUFFER, self.unpack_buffer);
        }
    }
}
