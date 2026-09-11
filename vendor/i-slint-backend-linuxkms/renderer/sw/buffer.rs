// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Keep pixel rendering separate from scanout memory access. The shadow path
//! deliberately repaints and copies the whole frame; it does not assume that a
//! triple-buffered display target contains the previous frame's pixels.

use super::{DumbBufferPixelXrgb888, SoftwareRenderer};
use crate::profiling::{self, Scope, Stage};
use drm::buffer::DrmFourcc;
use i_slint_core::platform::PlatformError;
use i_slint_core::software_renderer::{
    DrawTextureArgs, PhysicalRegion, RepaintBufferType, Rgb565Pixel, TargetPixel, TargetPixelBuffer,
};
use i_slint_core::Brush;

mod texture;
pub(super) use texture::TextureMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BufferMode {
    Direct,
    Ram,
}

impl BufferMode {
    pub(super) fn parse(value: Option<&str>) -> Result<Self, PlatformError> {
        match value {
            // Picade measurements favor direct rendering once the extra copy is
            // counted. Retain RAM as an opt-in experiment, not a default cost.
            None | Some("direct") => Ok(Self::Direct),
            Some("ram") => Ok(Self::Ram),
            Some(_) => Err("SPACEWARS_KMS_BUFFER must be 'direct' or 'ram'".into()),
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Ram => "ram",
        }
    }
}

enum ShadowPixels {
    Empty,
    Xrgb(Vec<DumbBufferPixelXrgb888>),
    Rgb565(Vec<Rgb565Pixel>),
}

pub(super) struct OutputBuffer {
    mode: BufferMode,
    texture_mode: TextureMode,
    compare_next_rgb: bool,
    shadow: ShadowPixels,
}

impl OutputBuffer {
    pub(super) fn new(mode: BufferMode, texture_mode: TextureMode) -> Self {
        Self { mode, texture_mode, compare_next_rgb: false, shadow: ShadowPixels::Empty }
    }

    pub(super) fn render(
        &mut self,
        renderer: &SoftwareRenderer,
        pixels: &mut [u8],
        stride: usize,
        age: u8,
        format: DrmFourcc,
    ) -> Result<PhysicalRegion, PlatformError> {
        let texture_mode = if self.texture_mode == TextureMode::Compare {
            let selected =
                if self.compare_next_rgb { TextureMode::Rgb } else { TextureMode::Generic };
            self.compare_next_rgb = !self.compare_next_rgb;
            selected
        } else {
            self.texture_mode
        };
        let region = match format {
            DrmFourcc::Xrgb8888 | DrmFourcc::Argb8888 => {
                if self.mode == BufferMode::Ram && !matches!(self.shadow, ShadowPixels::Xrgb(_)) {
                    self.shadow = ShadowPixels::Xrgb(Vec::new());
                }
                let shadow = match &mut self.shadow {
                    ShadowPixels::Xrgb(pixels) => Some(pixels),
                    _ => None,
                };
                render_pixels(
                    renderer,
                    bytemuck::cast_slice_mut::<_, DumbBufferPixelXrgb888>(pixels),
                    stride,
                    age,
                    shadow,
                    texture_mode,
                )
            }
            DrmFourcc::Rgb565 => {
                if self.mode == BufferMode::Ram && !matches!(self.shadow, ShadowPixels::Rgb565(_)) {
                    self.shadow = ShadowPixels::Rgb565(Vec::new());
                }
                let shadow = match &mut self.shadow {
                    ShadowPixels::Rgb565(pixels) => Some(pixels),
                    _ => None,
                };
                render_pixels(
                    renderer,
                    bytemuck::cast_slice_mut::<_, Rgb565Pixel>(pixels),
                    stride,
                    age,
                    shadow,
                    texture_mode,
                )
            }
            _ => {
                return Err(format!(
                    "Unsupported frame buffer format {format} used with software renderer"
                )
                .into())
            }
        };
        let bytes = match &self.shadow {
            ShadowPixels::Empty => 0,
            ShadowPixels::Xrgb(pixels) => pixels.capacity() * size_of::<DumbBufferPixelXrgb888>(),
            ShadowPixels::Rgb565(pixels) => pixels.capacity() * size_of::<Rgb565Pixel>(),
        };
        profiling::render_target(self.mode.name(), bytes);
        Ok(region)
    }
}

fn render_pixels<P: TargetPixel>(
    renderer: &SoftwareRenderer,
    output: &mut [P],
    stride: usize,
    age: u8,
    shadow: Option<&mut Vec<P>>,
    texture_mode: TextureMode,
) -> PhysicalRegion {
    if let Some(shadow) = shadow {
        // A persistent RAM buffer is not the display buffer described by `age`.
        // Force a complete repaint for now: the experiment changes memory access,
        // not dirty-region policy. Resize/format changes cannot retain stale pixels.
        shadow.resize_with(output.len(), P::background);
        renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        let region = draw(renderer, shadow, stride, texture_mode);
        {
            let _copy = Scope::new(Stage::Copy);
            output.copy_from_slice(shadow);
        }
        profiling::copied(std::mem::size_of_val(output));
        region
    } else {
        renderer.set_repaint_buffer_type(match age {
            1 => RepaintBufferType::ReusedBuffer,
            2 => RepaintBufferType::SwappedBuffers,
            _ => RepaintBufferType::NewBuffer,
        });
        draw(renderer, output, stride, texture_mode)
    }
}

fn draw<P: TargetPixel>(
    renderer: &SoftwareRenderer,
    pixels: &mut [P],
    stride: usize,
    texture_mode: TextureMode,
) -> PhysicalRegion {
    let _draw = Scope::new(Stage::Draw);
    let _implementation = Scope::new(match texture_mode {
        TextureMode::Generic => Stage::DrawGeneric,
        TextureMode::Rgb => Stage::DrawRgb,
        TextureMode::Compare => unreachable!("comparison mode is resolved before drawing"),
    });
    renderer.render_into_buffer(&mut TimedPixelBuffer { pixels, stride, texture_mode })
}

/// Keep unsupported scene operations on Slint's fallback. Background fill and
/// eligible RGB image blits are timed within the outer Draw measurement.
struct TimedPixelBuffer<'a, P> {
    pixels: &'a mut [P],
    stride: usize,
    texture_mode: TextureMode,
}

impl<P: TargetPixel> TargetPixelBuffer for TimedPixelBuffer<'_, P> {
    type TargetPixel = P;

    fn line_slice(&mut self, line: usize) -> &mut [P] {
        &mut self.pixels[line * self.stride..(line + 1) * self.stride]
    }

    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }

    fn diagnostic_observer(
        &self,
    ) -> Option<i_slint_core::software_renderer::DrawDiagnosticObserver> {
        profiling::draw_observer()
    }

    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        if self.texture_mode == TextureMode::Generic {
            return false;
        }
        texture::draw_rgb(args, clip, self.pixels, self.stride)
    }

    fn fill_background(&mut self, brush: &Brush, region: &PhysicalRegion) -> bool {
        if !matches!(brush, Brush::SolidColor(_)) {
            return false;
        }
        let _background = Scope::new(Stage::Background);
        let mut color = P::background();
        color.blend(brush.color().into());
        for (position, size) in region.iter() {
            let left = (i64::from(position.x)).clamp(0, self.stride as i64) as usize;
            let right = (i64::from(position.x) + i64::from(size.width)).clamp(0, self.stride as i64)
                as usize;
            let top = (i64::from(position.y)).clamp(0, self.num_lines() as i64) as usize;
            let bottom = (i64::from(position.y) + i64::from(size.height))
                .clamp(0, self.num_lines() as i64) as usize;
            for line in top..bottom {
                self.line_slice(line)[left..right].fill(color);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
