// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Delegate the rendering to the [`i_slint_core::software_renderer::SoftwareRenderer`]

use i_slint_core::api::PhysicalSize as PhysicalWindowSize;
use i_slint_core::platform::PlatformError;
pub use i_slint_core::software_renderer::SoftwareRenderer;
use i_slint_core::software_renderer::{PremultipliedRgbaColor, TargetPixel};
use std::cell::RefCell;
use std::sync::Arc;

use crate::display::RenderingRotation;
use crate::profiling::{self, Counter};

mod buffer;
use buffer::{BufferMode, OutputBuffer, TextureMode};

pub struct SoftwareRendererAdapter {
    renderer: SoftwareRenderer,
    display: Arc<dyn crate::display::swdisplay::SoftwareBufferDisplay>,
    presenter: Arc<dyn crate::display::Presenter>,
    size: PhysicalWindowSize,
    output_buffer: RefCell<OutputBuffer>,
}

const SOFTWARE_RENDER_SUPPORTED_DRM_FOURCC_FORMATS: &[drm::buffer::DrmFourcc] = &[
    // Preferred formats
    drm::buffer::DrmFourcc::Xrgb8888,
    drm::buffer::DrmFourcc::Argb8888,
    // drm::buffer::DrmFourcc::Bgra8888,
    // drm::buffer::DrmFourcc::Rgba8888,

    // 16-bit formats
    drm::buffer::DrmFourcc::Rgb565,
    // drm::buffer::DrmFourcc::Bgr565,

    // // 4444 formats
    // drm::buffer::DrmFourcc::Argb4444,
    // drm::buffer::DrmFourcc::Abgr4444,
    // drm::buffer::DrmFourcc::Rgba4444,
    // drm::buffer::DrmFourcc::Bgra4444,

    // // Single channel formats
    // drm::buffer::DrmFourcc::Gray8,
    // drm::buffer::DrmFourcc::C8,
    // drm::buffer::DrmFourcc::R8,
    // drm::buffer::DrmFourcc::R16,

    // // Dual channel formats
    // drm::buffer::DrmFourcc::Gr88,
    // drm::buffer::DrmFourcc::Rg88,
    // drm::buffer::DrmFourcc::Gr1616,
    // drm::buffer::DrmFourcc::Rg1616,

    // // 10-bit formats
    // drm::buffer::DrmFourcc::Xrgb2101010,
    // drm::buffer::DrmFourcc::Argb2101010,
    // drm::buffer::DrmFourcc::Abgr2101010,
    // drm::buffer::DrmFourcc::Rgba1010102,
    // drm::buffer::DrmFourcc::Bgra1010102,
    // drm::buffer::DrmFourcc::Rgbx1010102,
    // drm::buffer::DrmFourcc::Bgrx1010102,
];

#[repr(transparent)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DumbBufferPixelXrgb888(pub u32);

impl From<DumbBufferPixelXrgb888> for PremultipliedRgbaColor {
    #[inline]
    fn from(pixel: DumbBufferPixelXrgb888) -> Self {
        let v = pixel.0;
        PremultipliedRgbaColor {
            red: (v >> 16) as u8,
            green: (v >> 8) as u8,
            blue: (v >> 0) as u8,
            alpha: (v >> 24) as u8,
        }
    }
}

impl From<PremultipliedRgbaColor> for DumbBufferPixelXrgb888 {
    #[inline]
    fn from(pixel: PremultipliedRgbaColor) -> Self {
        Self(
            (pixel.alpha as u32) << 24
                | ((pixel.red as u32) << 16)
                | ((pixel.green as u32) << 8)
                | (pixel.blue as u32),
        )
    }
}

impl TargetPixel for DumbBufferPixelXrgb888 {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let mut x = PremultipliedRgbaColor::from(*self);
        x.blend(color);
        *self = x.into();
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(0xff000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    fn background() -> Self {
        Self(0)
    }
}

impl SoftwareRendererAdapter {
    pub fn new(
        device_opener: &crate::DeviceOpener,
    ) -> Result<Box<dyn crate::fullscreenwindowadapter::FullscreenRenderer>, PlatformError> {
        let display = crate::display::swdisplay::new(
            device_opener,
            SOFTWARE_RENDER_SUPPORTED_DRM_FOURCC_FORMATS,
        )?;

        let (width, height) = display.size();
        let size = i_slint_core::api::PhysicalSize::new(width, height);
        profiling::activate((width, height));
        let mode = BufferMode::parse(std::env::var("SPACEWARS_KMS_BUFFER").ok().as_deref())?;
        let texture_mode =
            TextureMode::parse(std::env::var("SPACEWARS_KMS_TEXTURE").ok().as_deref())?;
        profiling::render_target(mode.name(), 0);
        profiling::texture_mode(texture_mode.name());

        let renderer = Box::new(Self {
            renderer: SoftwareRenderer::new(),
            display: display.clone(),
            presenter: display.as_presenter(),
            size,
            output_buffer: RefCell::new(OutputBuffer::new(mode, texture_mode)),
        });

        eprintln!("Using Software renderer ({} buffer)", mode.name());

        Ok(renderer)
    }
}

impl crate::fullscreenwindowadapter::FullscreenRenderer for SoftwareRendererAdapter {
    fn as_core_renderer(&self) -> &dyn i_slint_core::renderer::Renderer {
        &self.renderer
    }

    fn render_and_present(
        &self,
        rotation: RenderingRotation,
        _draw_mouse_cursor_callback: &dyn Fn(&mut dyn i_slint_core::item_rendering::ItemRenderer),
    ) -> Result<(), PlatformError> {
        self.display.map_back_buffer(&mut |pixels, age, format| {
            self.renderer.set_rendering_rotation(match rotation {
                RenderingRotation::NoRotation => {
                    i_slint_core::software_renderer::RenderingRotation::NoRotation
                }
                RenderingRotation::Rotate90 => {
                    i_slint_core::software_renderer::RenderingRotation::Rotate90
                }
                RenderingRotation::Rotate180 => {
                    i_slint_core::software_renderer::RenderingRotation::Rotate180
                }
                RenderingRotation::Rotate270 => {
                    i_slint_core::software_renderer::RenderingRotation::Rotate270
                }
            });

            let dirty_region = self.output_buffer.borrow_mut().render(
                &self.renderer,
                pixels,
                self.size.width as usize,
                age,
                format,
            )?;
            profiling::count(Counter::Draws);
            profiling::buffer(
                age,
                pixels.len(),
                match format {
                    drm::buffer::DrmFourcc::Xrgb8888 => "XRGB8888",
                    drm::buffer::DrmFourcc::Argb8888 => "ARGB8888",
                    drm::buffer::DrmFourcc::Rgb565 => "RGB565",
                    _ => "unknown",
                },
                dirty_region
                    .iter()
                    .map(|(_, size)| u64::from(size.width) * u64::from(size.height))
                    .sum(),
            );

            Ok(())
        })?;
        self.presenter.present()?;
        Ok(())
    }

    fn size(&self) -> i_slint_core::api::PhysicalSize {
        self.size
    }
}
