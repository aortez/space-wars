//! Validation-only pixel-pack buffers. No downloads or capture commands occur
//! in timed blocks. Map only after the corresponding completion fence.
use super::Error;
use glow::HasContext;
use std::rc::Rc;

pub(super) struct Capture {
    gl: Rc<glow::Context>,
    pub buffer: glow::NativeBuffer,
    bytes: usize,
}

impl Capture {
    pub fn new(gl: Rc<glow::Context>, bytes: usize) -> Result<Self, Error> {
        let size = bytes.try_into()?;
        let buffer = unsafe { gl.create_buffer()? };
        let result = Self { gl, buffer, bytes };
        unsafe {
            let g = &result.gl;
            let old = g.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
            g.buffer_data_size(glow::PIXEL_PACK_BUFFER, size, glow::STREAM_READ);
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, old);
            if g.get_error() != glow::NO_ERROR {
                return Err("pixel-pack allocation failed".into());
            }
        }
        Ok(result)
    }

    pub fn pixels(&self, width: u32, height: u32) -> Result<Vec<u8>, Error> {
        let bytes = width as usize * height as usize * 4;
        if bytes > self.bytes {
            return Err("capture exceeds allocated pixel-pack storage".into());
        }
        let mut pixels = vec![0; bytes];
        unsafe {
            let g = &self.gl;
            let old = g.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(self.buffer));
            let ptr =
                g.map_buffer_range(glow::PIXEL_PACK_BUFFER, 0, bytes as i32, glow::MAP_READ_BIT);
            if ptr.is_null() {
                g.bind_buffer(glow::PIXEL_PACK_BUFFER, old);
                return Err("completed pixel-pack mapping failed".into());
            }
            // SAFETY: The caller has waited for the readback fence. Storage
            // and mapped length are checked above and live until unmap below.
            pixels.copy_from_slice(std::slice::from_raw_parts(ptr, bytes));
            g.unmap_buffer(glow::PIXEL_PACK_BUFFER);
            g.bind_buffer(glow::PIXEL_PACK_BUFFER, old);
            if g.get_error() != glow::NO_ERROR {
                return Err("pixel-pack readback failed".into());
            }
        }
        flip_rows(&mut pixels, width as usize * 4);
        Ok(pixels)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_buffer(self.buffer);
        }
    }
}

/// Hostile pack state verifies both synchronous references and queued captures
/// restore caller state; the deliberately tiny foreign buffer must not be used.
pub(super) struct ForeignPack {
    gl: Rc<glow::Context>,
    saved_buffer: Option<glow::NativeBuffer>,
    saved: [i32; 4],
    buffer: Capture,
}
const PACK: [u32; 4] = [
    glow::PACK_ALIGNMENT,
    glow::PACK_ROW_LENGTH,
    glow::PACK_SKIP_PIXELS,
    glow::PACK_SKIP_ROWS,
];
const HOSTILE: [i32; 4] = [8, 19, 3, 2];
impl ForeignPack {
    pub fn new(gl: Rc<glow::Context>) -> Result<Self, Error> {
        let buffer = Capture::new(gl.clone(), 64)?;
        unsafe {
            let saved_buffer = gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
            let saved = PACK.map(|p| gl.get_parameter_i32(p));
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer.buffer));
            for (p, v) in PACK.into_iter().zip(HOSTILE) {
                gl.pixel_store_i32(p, v);
            }
            Ok(Self {
                gl,
                saved_buffer,
                saved,
                buffer,
            })
        }
    }
    pub fn check(&self) -> Result<(), Error> {
        unsafe {
            if self
                .gl
                .get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING)
                != Some(self.buffer.buffer)
                || PACK.map(|p| self.gl.get_parameter_i32(p)) != HOSTILE
            {
                return Err("queued readback leaked pixel-pack state".into());
            }
        }
        Ok(())
    }
}
impl Drop for ForeignPack {
    fn drop(&mut self) {
        unsafe {
            self.gl
                .bind_buffer(glow::PIXEL_PACK_BUFFER, self.saved_buffer);
            for (p, v) in PACK.into_iter().zip(self.saved) {
                self.gl.pixel_store_i32(p, v);
            }
        }
    }
}

fn flip_rows(pixels: &mut [u8], stride: usize) {
    let rows = pixels.len() / stride;
    for y in 0..rows / 2 {
        let (top, bottom) = pixels.split_at_mut((rows - 1 - y) * stride);
        top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn readback_flip_handles_even_and_odd_rows() {
        for (mut pixels, expected) in [
            (vec![1, 2, 3, 4], vec![3, 4, 1, 2]),
            (vec![1, 2, 3, 4, 5, 6], vec![5, 6, 3, 4, 1, 2]),
        ] {
            super::flip_rows(&mut pixels, 2);
            assert_eq!(pixels, expected);
        }
    }
}
