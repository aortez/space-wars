// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::{num::NonZeroU32, rc::Rc};

use i_slint_core::item_rendering::ItemRenderer;
use i_slint_core::platform::PlatformError;
use i_slint_renderer_femtovg::FemtoVGRendererExt;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use glutin::{
    context::{ContextApi, ContextAttributesBuilder},
    display::GetGlDisplay,
    prelude::*,
    surface::{SurfaceAttributesBuilder, WindowSurface},
};

use crate::display::{gbmdisplay::GbmDisplay, Presenter, RenderingRotation};
use crate::drmoutput::DrmOutput;
use crate::profiling::{self, Counter, Scope, Stage};

#[path = "gl_profile.rs"]
mod gl_profile;

pub struct FemtoVGRendererAdapter {
    renderer:
        i_slint_renderer_femtovg::FemtoVGRenderer<i_slint_renderer_femtovg::opengl::OpenGLBackend>,
    gbm_display: Rc<GbmDisplay>,
}

struct GlContextWrapper {
    glutin_context: glutin::context::PossiblyCurrentContext,
    glutin_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    gbm_display: Rc<GbmDisplay>,
    frame_capture: Rc<crate::frame_capture::Session>,
}

impl GlContextWrapper {
    fn stencil_bits(&self) -> Option<u8> {
        let address = self
            .glutin_context
            .display()
            .get_proc_address(c"glGetIntegerv");
        if address.is_null() {
            return None;
        }
        // SAFETY: Called once with the window context current, before FemtoVG
        // can bind an offscreen framebuffer. GL_STENCIL_BITS describes output.
        let get_integer = unsafe {
            std::mem::transmute::<*const std::ffi::c_void, unsafe extern "system" fn(u32, *mut i32)>(
                address,
            )
        };
        let mut bits = -1;
        unsafe {
            get_integer(0x0d57, &mut bits);
        }
        bits.try_into().ok()
    }

    fn driver_info(&self) -> Option<[String; 3]> {
        // Called once, immediately after creating and making the context current.
        // Record the actual driver so a software Mesa fallback cannot be mistaken
        // for hardware acceleration just because FemtoVG was selected.
        let address = self
            .glutin_context
            .display()
            .get_proc_address(c"glGetString");
        if address.is_null() {
            return None;
        }
        // SAFETY: EGL supplies the OpenGL entry point with this signature, and
        // this thread has the live context current for all calls below.
        let get_string = unsafe {
            std::mem::transmute::<
                *const std::ffi::c_void,
                unsafe extern "system" fn(u32) -> *const u8,
            >(address)
        };
        // GL_VENDOR, GL_RENDERER, GL_VERSION; these are available in GLES 2.
        Some([0x1f00, 0x1f01, 0x1f02].map(|name| {
            // SAFETY: Valid entry point, current context and supported enum.
            let value = unsafe { get_string(name) };
            if value.is_null() {
                return "unknown".into();
            }
            // SAFETY: OpenGL owns this NUL-terminated string for the context's
            // lifetime; copy it before the context can be destroyed.
            unsafe { std::ffi::CStr::from_ptr(value.cast()) }
                .to_string_lossy()
                .replace(['\r', '\n'], " ")
        }))
    }

    fn new(gbm_display: &Rc<GbmDisplay>) -> Result<Self, PlatformError> {
        let (width, height) = gbm_display.drm_output.size();
        let width: std::num::NonZeroU32 = width.try_into().map_err(|_| {
            format!(
                "Attempting to create window surface with an invalid width: {}",
                width
            )
        })?;
        let height: std::num::NonZeroU32 = height.try_into().map_err(|_| {
            format!(
                "Attempting to create window surface with an invalid height: {}",
                height
            )
        })?;

        let display_handle = gbm_display.display_handle().unwrap();
        let window_handle = gbm_display.window_handle().unwrap();

        let gl_display = unsafe {
            glutin::display::Display::new(
                display_handle.as_raw(),
                glutin::display::DisplayApiPreference::Egl,
            )
            .map_err(|e| format!("Error creating EGL display: {e}"))?
        };

        let config_template = gbm_display.config_template_builder().build();

        let config = unsafe {
            gl_display
                .find_configs(config_template)
                .map_err(|e| format!("Error locating EGL configs: {e}"))?
                .filter(|config| gbm_display.filter_gl_config(config))
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .ok_or("Unable to find suitable GL config")?
        };

        let gles_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(glutin::context::Version {
                major: 2,
                minor: 0,
            })))
            .build(Some(window_handle.as_raw()));

        let fallback_context_attributes =
            ContextAttributesBuilder::new().build(Some(window_handle.as_raw()));

        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&config, &gles_context_attributes)
                .or_else(|_| gl_display.create_context(&config, &fallback_context_attributes))
                .map_err(|e| format!("Error creating EGL context: {e}"))?
        };

        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window_handle.as_raw(),
            width,
            height,
        );

        let surface = unsafe {
            config
                .display()
                .create_window_surface(&config, &attrs)
                .map_err(|e| format!("Error creating EGL window surface: {e}"))?
        };

        let context = not_current_gl_context.make_current(&surface)
        .map_err(|glutin_error: glutin::error::Error| -> PlatformError {
            format!("FemtoVG Renderer: Failed to make newly created OpenGL context current: {glutin_error}")
            .into()
    })?;

        Ok(Self {
            glutin_context: context,
            glutin_surface: surface,
            gbm_display: gbm_display.clone(),
            frame_capture: crate::frame_capture::Session::activate(),
        })
    }
}

unsafe impl i_slint_renderer_femtovg::opengl::OpenGLInterface for GlContextWrapper {
    fn ensure_current(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.glutin_context.is_current() {
            self.glutin_context
                .make_current(&self.glutin_surface)
                .map_err(|glutin_error| -> PlatformError {
                    format!("FemtoVG: Error making context current: {glutin_error}").into()
                })?;
        }
        Ok(())
    }

    fn swap_buffers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Slint has flushed the scene and run AfterRendering. Read this frame
        // now; after EGL swaps, glReadPixels can return an older back buffer.
        self.frame_capture.before_swap();
        // Make sure the in-flight font-buffer from the previous swap_buffers call has been
        // posted to the screen.
        {
            let _wait = Scope::new(Stage::FlipWait);
            self.gbm_display.drm_output.wait_for_page_flip();
        }
        let _swap = Scope::new(Stage::EglSwap);
        self.glutin_surface
            .swap_buffers(&self.glutin_context)
            .map_err(|glutin_error| -> PlatformError {
                format!("FemtoVG: Error swapping buffers: {glutin_error}").into()
            })?;
        Ok(())
    }

    fn resize(
        &self,
        _width: NonZeroU32,
        _height: NonZeroU32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Ignore resize requests
        Ok(())
    }

    fn get_proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
        let address = self.glutin_context.display().get_proc_address(name);
        // SAFETY: Same name and live context as the GL loader's request.
        unsafe { gl_profile::wrap(name, address) }
    }
}

impl FemtoVGRendererAdapter {
    pub fn new(
        device_opener: &crate::DeviceOpener,
    ) -> Result<Box<dyn crate::fullscreenwindowadapter::FullscreenRenderer>, PlatformError> {
        let drm_output = DrmOutput::new(device_opener)?;
        let egl_display = Rc::new(crate::display::gbmdisplay::GbmDisplay::new(drm_output)?);
        let context = GlContextWrapper::new(&egl_display)?;
        let driver_info = context.driver_info();
        let stencil_bits = context.stencil_bits();
        // Activate before FemtoVG loads GL entry points so profiling-disabled
        // builds keep the original pointers with no per-call wrapper overhead.
        profiling::activate_femtovg(
            egl_display.drm_output.size(),
            driver_info.clone(),
            stencil_bits,
        );

        let renderer = Box::new(Self {
            renderer: i_slint_renderer_femtovg::FemtoVGRenderer::new(context)?,
            gbm_display: egl_display,
        });

        eprintln!("Using FemtoVG OpenGL renderer: {driver_info:?}; stencil_bits={stencil_bits:?}");

        Ok(renderer)
    }
}

impl crate::fullscreenwindowadapter::FullscreenRenderer for FemtoVGRendererAdapter {
    fn as_core_renderer(&self) -> &dyn i_slint_core::renderer::Renderer {
        &self.renderer
    }

    fn render_and_present(
        &self,
        rotation: RenderingRotation,
        draw_mouse_cursor_callback: &dyn Fn(&mut dyn ItemRenderer),
    ) -> Result<(), PlatformError> {
        let size = self.size();
        // CPU command generation and any driver waits, not GPU execution time.
        let draw = Scope::new(Stage::Draw);
        // Build includes layout and the application's BeforeRendering callback.
        // AfterItems includes command flush, cleanup and swap. These are nested
        // inside Draw, and are timed only in an explicitly armed short trace.
        let build_items = std::cell::RefCell::new(Some(Scope::trace(Stage::FemtoBuildItems)));
        let after_items = std::cell::RefCell::new(None);
        self.renderer.render_transformed_with_post_callback(
            rotation.degrees(),
            rotation.translation_after_rotation(size),
            size,
            Some(&|item_renderer| {
                draw_mouse_cursor_callback(item_renderer);
                drop(build_items.borrow_mut().take());
                *after_items.borrow_mut() = Some(Scope::trace(Stage::FemtoAfterItems));
            }),
        )?;
        drop(build_items.into_inner());
        drop(after_items.into_inner());
        drop(draw);
        let _submit = Scope::new(Stage::Submit);
        self.gbm_display.present()?;
        profiling::count(Counter::Draws);
        Ok(())
    }
    fn size(&self) -> i_slint_core::api::PhysicalSize {
        let (width, height) = self.gbm_display.drm_output.size();
        i_slint_core::api::PhysicalSize::new(width, height)
    }
}
