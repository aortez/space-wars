//! Bounded conversion from platform-neutral native frames to Slint images.

use std::fmt;

use engine_common::{NativePixelFormat, NativeVideoCrop, NativeVideoFrame, NativeVideoTiming};
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};

const BUFFER_COUNT: usize = 3;

#[derive(Debug)]
pub struct NativeVideoRenderer {
    buffers: Vec<SharedPixelBuffer<Rgb8Pixel>>,
    width: u32,
    height: u32,
    active_buffer: usize,
}

#[derive(Debug, Clone)]
pub struct NativeVideoPresentation {
    pub image: Image,
    pub source_crop: NativeVideoCrop,
    pub frame_id: u64,
    pub timing: Option<NativeVideoTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeVideoError {
    InvalidLayout,
    PaletteIndexOutOfRange { index: u8, palette_len: usize },
    SlintDimensionOutOfRange { value: u32 },
}

impl fmt::Display for NativeVideoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout => formatter.write_str("native video frame has an invalid layout"),
            Self::PaletteIndexOutOfRange { index, palette_len } => write!(
                formatter,
                "native video palette index {index} exceeds palette length {palette_len}"
            ),
            Self::SlintDimensionOutOfRange { value } => {
                write!(
                    formatter,
                    "native video dimension {value} exceeds Slint's range"
                )
            }
        }
    }
}

pub fn slint_dimension(value: u32) -> Result<i32, NativeVideoError> {
    i32::try_from(value).map_err(|_| NativeVideoError::SlintDimensionOutOfRange { value })
}

impl std::error::Error for NativeVideoError {}

impl NativeVideoRenderer {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            width: 0,
            height: 0,
            active_buffer: 0,
        }
    }

    pub fn present(
        &mut self,
        frame: NativeVideoFrame<'_>,
    ) -> Result<NativeVideoPresentation, NativeVideoError> {
        if !frame.has_valid_layout() {
            return Err(NativeVideoError::InvalidLayout);
        }
        self.ensure_size(frame.width, frame.height);

        let buffer_index = self.active_buffer;
        self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
        match frame.pixel_format {
            NativePixelFormat::Indexed8Rgb565 => {
                expand_indexed_rgb565(
                    frame.pixels,
                    frame.palette_rgb565,
                    self.buffers[buffer_index].make_mut_slice(),
                )?;
            }
        }

        Ok(NativeVideoPresentation {
            image: Image::from_rgb8(self.buffers[buffer_index].clone()),
            source_crop: frame.visible_crop,
            frame_id: frame.frame_id,
            timing: frame.timing,
        })
    }

    fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height && self.buffers.len() == BUFFER_COUNT {
            return;
        }
        self.width = width;
        self.height = height;
        self.active_buffer = 0;
        self.buffers = (0..BUFFER_COUNT)
            .map(|_| SharedPixelBuffer::new(width, height))
            .collect();
    }
}

impl Default for NativeVideoRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn expand_indexed_rgb565(
    indices: &[u8],
    palette: &[u16],
    output: &mut [Rgb8Pixel],
) -> Result<(), NativeVideoError> {
    debug_assert_eq!(indices.len(), output.len());
    for (index, destination) in indices.iter().copied().zip(output) {
        let Some(color) = palette.get(usize::from(index)).copied() else {
            return Err(NativeVideoError::PaletteIndexOutOfRange {
                index,
                palette_len: palette.len(),
            });
        };
        let [r, g, b] = rgb565_to_rgb888(color);
        *destination = Rgb8Pixel { r, g, b };
    }
    Ok(())
}

fn rgb565_to_rgb888(color: u16) -> [u8; 3] {
    let red = u32::from((color >> 11) & 0x1f);
    let green = u32::from((color >> 5) & 0x3f);
    let blue = u32::from(color & 0x1f);
    [
        ((red * 255 + 15) / 31) as u8,
        ((green * 255 + 31) / 63) as u8,
        ((blue * 255 + 15) / 31) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_native_indices_and_preserves_crop_metadata() {
        let indices = [0_u8, 1, 1, 0];
        let palette = [0x0000, 0xffff];
        let mut renderer = NativeVideoRenderer::new();
        let presentation = renderer
            .present(NativeVideoFrame {
                width: 2,
                height: 2,
                visible_crop: NativeVideoCrop {
                    x: 0,
                    y: 1,
                    width: 2,
                    height: 1,
                },
                pixel_format: NativePixelFormat::Indexed8Rgb565,
                frame_id: 9,
                pixels: &indices,
                palette_rgb565: &palette,
                timing: Some(NativeVideoTiming {
                    emulated_ticks: 100,
                    input_sequence_id: 3,
                }),
            })
            .unwrap();

        let pixels = presentation.image.to_rgb8().unwrap();
        assert_eq!(pixels.as_slice()[0], Rgb8Pixel { r: 0, g: 0, b: 0 });
        assert_eq!(
            pixels.as_slice()[1],
            Rgb8Pixel {
                r: 255,
                g: 255,
                b: 255,
            }
        );
        assert_eq!(presentation.source_crop.height, 1);
        assert_eq!(presentation.frame_id, 9);
        assert_eq!(presentation.timing.unwrap().input_sequence_id, 3);
        assert_eq!(renderer.buffers.len(), BUFFER_COUNT);
    }

    #[test]
    fn rejects_out_of_range_palette_indices() {
        let mut renderer = NativeVideoRenderer::new();
        let error = renderer
            .present(NativeVideoFrame {
                width: 1,
                height: 1,
                visible_crop: NativeVideoCrop::full(1, 1),
                pixel_format: NativePixelFormat::Indexed8Rgb565,
                frame_id: 0,
                pixels: &[2],
                palette_rgb565: &[0],
                timing: None,
            })
            .unwrap_err();
        assert_eq!(
            error,
            NativeVideoError::PaletteIndexOutOfRange {
                index: 2,
                palette_len: 1,
            }
        );
    }
}
