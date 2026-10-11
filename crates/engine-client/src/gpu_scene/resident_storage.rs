//! Only the opt-in buffer experiment touches storage bindings. In particular,
//! preserve foreign indexed ranges, not just buffer names, on this shared GL.
use glow::HasContext;
use std::{num::NonZeroU32, rc::Rc};

fn version_supported(embedded: bool, major: u32, minor: u32) -> bool {
    (major, minor) >= if embedded { (3, 1) } else { (4, 3) }
}

fn supported(g: &glow::Context) -> bool {
    let v = g.version();
    version_supported(v.is_embedded, v.major, v.minor)
}

fn limits_sufficient(vertex: i32, combined: i32, bindings: i32, bytes: i64) -> bool {
    // Same 32-byte record and padded maximum as the texture experiment.
    use crate::gpu_scene::{
        mesh::MAX_VERTICES,
        resident::{MAX_PLACEMENTS, ROW_VERTICES},
    };
    let frame_bytes = (MAX_VERTICES + MAX_PLACEMENTS).div_ceil(ROW_VERTICES) * ROW_VERTICES * 32;
    vertex >= 2 && combined >= 2 && bindings >= 2 && bytes >= frame_bytes as i64
}

pub(crate) unsafe fn validate(g: &glow::Context) -> Result<(), String> {
    if !supported(g) {
        return Err("resident buffers require GLES3.1 or desktop GL4.3".into());
    }
    unsafe {
        let vertex = g.get_parameter_i32(glow::MAX_VERTEX_SHADER_STORAGE_BLOCKS);
        let combined = g.get_parameter_i32(glow::MAX_COMBINED_SHADER_STORAGE_BLOCKS);
        let bindings = g.get_parameter_i32(glow::MAX_SHADER_STORAGE_BUFFER_BINDINGS);
        let bytes = g.get_parameter_i64(glow::MAX_SHADER_STORAGE_BLOCK_SIZE);
        if !limits_sufficient(vertex, combined, bindings, bytes) {
            return Err(format!(
                "resident SSBO limits insufficient: vertex={vertex}, combined={combined}, bindings={bindings}, block_bytes={bytes}"
            ));
        }
    }
    Ok(())
}

pub(crate) unsafe fn validate_uniforms(g: &glow::Context) -> Result<(), String> {
    unsafe {
        if g.get_parameter_i32(glow::MAX_VERTEX_UNIFORM_BLOCKS) < 1
            || g.get_parameter_i32(glow::MAX_UNIFORM_BUFFER_BINDINGS) < 1
            || g.get_parameter_i32(glow::MAX_UNIFORM_BLOCK_SIZE)
                < (super::resident::UNIFORM_PLACEMENTS * 32) as i32
        {
            return Err("resident transforms need one 16 KiB vertex uniform block".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Range {
    buffer: Option<glow::NativeBuffer>,
    start: i32,
    size: i32,
}

pub(crate) struct Bindings {
    gl: Rc<glow::Context>,
    generic: Option<glow::NativeBuffer>,
    indexed: [Range; 2],
    target: u32,
    count: usize,
}

impl Bindings {
    // SAFETY: Caller checked support and keeps the same context current.
    pub(crate) unsafe fn save(gl: Rc<glow::Context>, target: u32) -> Result<Self, String> {
        unsafe {
            let (binding, start, size, count) = match target {
                glow::SHADER_STORAGE_BUFFER => (
                    glow::SHADER_STORAGE_BUFFER_BINDING,
                    glow::SHADER_STORAGE_BUFFER_START,
                    glow::SHADER_STORAGE_BUFFER_SIZE,
                    2,
                ),
                glow::UNIFORM_BUFFER => (
                    glow::UNIFORM_BUFFER_BINDING,
                    glow::UNIFORM_BUFFER_START,
                    glow::UNIFORM_BUFFER_SIZE,
                    1,
                ),
                _ => return Err("unsupported resident binding target".into()),
            };
            let read = |i| -> Result<Range, String> {
                Ok(Range {
                    buffer: NonZeroU32::new(gl.get_parameter_indexed_i32(binding, i) as u32)
                        .map(glow::NativeBuffer),
                    // glow's binding API uses i32. Reject an unrepresentable
                    // foreign range before changing state rather than truncate.
                    start: gl
                        .get_parameter_indexed_i64(start, i)
                        .try_into()
                        .map_err(|_| "foreign storage offset exceeds binding API")?,
                    size: gl
                        .get_parameter_indexed_i64(size, i)
                        .try_into()
                        .map_err(|_| "foreign storage size exceeds binding API")?,
                })
            };
            Ok(Self {
                generic: gl.get_parameter_buffer(binding),
                indexed: [
                    read(0)?,
                    if count == 2 {
                        read(1)?
                    } else {
                        Range::default()
                    },
                ],
                gl,
                target,
                count,
            })
        }
    }
}
impl Drop for Bindings {
    fn drop(&mut self) {
        // SAFETY: Same current context; saved ranges were checked above.
        unsafe {
            for (i, r) in self.indexed[..self.count].iter().enumerate() {
                if r.buffer.is_some() && r.size > 0 {
                    self.gl
                        .bind_buffer_range(self.target, i as u32, r.buffer, r.start, r.size);
                } else {
                    self.gl.bind_buffer_base(self.target, i as u32, r.buffer);
                }
            }
            self.gl.bind_buffer(self.target, self.generic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_is_opt_in_and_limit_gated() {
        assert!(!version_supported(true, 3, 0));
        assert!(version_supported(true, 3, 1));
        assert!(!version_supported(false, 3, 3));
        assert!(!version_supported(false, 4, 2));
        assert!(version_supported(false, 4, 3));
        assert!(limits_sufficient(2, 2, 2, 128 * 1024 * 1024));
        for limits in [
            (0, 2, 2, i64::MAX),
            (2, 1, 2, i64::MAX),
            (2, 2, 1, i64::MAX),
            (2, 2, 2, 16 * 1024 * 1024),
        ] {
            assert!(!limits_sufficient(limits.0, limits.1, limits.2, limits.3));
        }
    }
}
