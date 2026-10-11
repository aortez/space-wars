//! Own textures only while their creating context is current. Retire replaced
//! textures after Slint has submitted the new image's draw. Never mutate or
//! delete Slint-owned textures, and never force glFinish or readback per frame.

use super::Mode;
use crate::{MainWindow, raster::publication};
use glow::HasContext;
use slint::{ComponentHandle, GraphicsAPI, Image, RenderingState, Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;

#[derive(Default)]
struct State {
    requested: Mode,
    applied: Mode,
    installed: bool,
    pending: Option<Image>,
    uploads: u64,
    allocations: u64,
    deletions: u64,
    resident: usize,
    error: Option<String>,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub(super) fn requested() -> Mode {
    STATE.with_borrow(|state| state.requested)
}

pub(super) fn stage(image: &Image) -> bool {
    STATE.with_borrow_mut(|state| {
        if !state.installed {
            return false;
        }
        // Latest frame only: no unbounded queue if presentation falls behind.
        state.pending = Some(image.clone());
        true
    })
}

pub(super) fn diagnostics() -> String {
    STATE.with_borrow(|s| format!(
        "\nraster_texture_mode={}\nraster_texture_applied={}\nraster_texture_notifier={}\nraster_texture_uploads_total={}\nraster_texture_allocations_total={}\nraster_texture_deletions_total={}\nraster_texture_resident={}\nraster_texture_error={}",
        s.requested.name(), s.applied.name(), s.installed, s.uploads, s.allocations,
        s.deletions, s.resident, s.error.as_deref().unwrap_or("none"),
    ))
}

pub(super) fn request(window: &MainWindow, mode: Mode) -> Result<(), String> {
    if mode != Mode::Slint && crate::gpu_scene::requested() {
        return Err("disable host gpu-scene before selecting texture upload experiments".into());
    }
    let gpu = i_slint_backend_linuxkms::profiling::overlay_summary()
        .is_some_and(|summary| summary.renderer == "femtovg");
    if !gpu {
        return Err("raster-textures requires active linuxkms-femtovg".into());
    }
    if mode != Mode::Slint && publication::requested_format() != publication::UploadFormat::Rgba {
        return Err("select host raster-upload rgba before changing texture lifetime".into());
    }
    if !STATE.with_borrow(|s| s.installed) {
        let weak = window.as_weak();
        let mut textures: Option<Textures> = None;
        let mut geometry: Option<crate::gpu_scene::Presenter> = None;
        let mut raster_replaced = false;
        window
            .window()
            .set_rendering_notifier(move |event, api| {
                match event {
                    RenderingState::BeforeRendering => {
                        let _trace =
                            crate::frame_trace::Scope::new(crate::frame_trace::Stage::BeforeRender);
                        raster_replaced = false;
                        let Some(window) = weak.upgrade() else {
                            return;
                        };
                        i_slint_backend_linuxkms::profiling::render_trace_marker(
                            if window.get_launcher_visible() {
                                4
                            } else if window.get_game_over_visible() {
                                3
                            } else if window.get_ingame_menu_visible() {
                                2
                            } else {
                                1
                            },
                        );
                        let pending = STATE.with_borrow_mut(|s| {
                            s.pending.take().map(|image| (image, s.requested))
                        });
                        if !window.get_raster_visible() {
                            // Leaving gameplay (or switching to vector/native
                            // video) must release the image, not just hide its UI
                            // item while retaining its borrowed GPU texture.
                            crate::gpu_scene::end_session();
                            window.set_raster_frame(Image::default());
                            if let Some(textures) = textures.as_mut() {
                                textures.retire();
                            }
                            raster_replaced = true;
                            return;
                        }
                        if crate::gpu_scene::before_rendering(&window, api, &mut geometry) {
                            if crate::gpu_scene::applied() {
                                if let Some(textures) = textures.as_mut() {
                                    textures.retire();
                                }
                            }
                            return;
                        }
                        let Some((image, mode)) = pending else {
                            return;
                        };
                        raster_replaced = true;
                        if mode == Mode::Slint {
                            window.set_raster_frame(image);
                            if let Some(textures) = textures.as_mut() {
                                textures.retire();
                            }
                            STATE.with_borrow_mut(|s| s.applied = Mode::Slint);
                            return;
                        }
                        let result = (|| {
                            if textures.is_none() {
                                textures = Some(Textures::new(api)?);
                            }
                            if image.to_rgb8().is_some() {
                                return Err("texture experiment requires RGBA publication".into());
                            }
                            let pixels = image.to_rgba8().ok_or("missing RGBA pixels")?;
                            textures.as_mut().unwrap().upload(&pixels, mode)
                        })();
                        match result {
                            Ok(borrowed) => {
                                window.set_raster_frame(borrowed);
                                STATE.with_borrow_mut(|s| s.applied = mode);
                            }
                            Err(error) => {
                                // Explicit, observable fallback; benchmark collectors reject it.
                                window.set_raster_frame(image);
                                if let Some(textures) = textures.as_mut() {
                                    textures.retire();
                                }
                                tracing::error!(%error, "texture lifetime experiment stopped");
                                STATE.with_borrow_mut(|s| {
                                    s.requested = Mode::Slint;
                                    s.applied = Mode::Slint;
                                    s.error = Some(error.replace(['\r', '\n'], " "));
                                });
                            }
                        }
                    }
                    RenderingState::AfterRendering => {
                        let _trace =
                            crate::frame_trace::Scope::new(crate::frame_trace::Stage::AfterRender);
                        crate::gpu_scene::after_rendering(&mut geometry, raster_replaced);
                        if let Some(textures) = textures.as_mut() {
                            textures.collect_retired();
                        }
                    }
                    RenderingState::RenderingTeardown => {
                        if let Some(window) = weak.upgrade() {
                            window.set_raster_frame(Image::default());
                        }
                        crate::gpu_scene::teardown(&mut geometry);
                        if let Some(mut textures) = textures.take() {
                            textures.retire();
                            textures.collect_retired();
                        }
                    }
                    _ => {}
                }
            })
            .map_err(|error| error.to_string())?;
        STATE.with_borrow_mut(|s| s.installed = true);
    }
    STATE.with_borrow_mut(|s| {
        s.requested = mode;
        s.error = None;
    });
    window.window().request_redraw();
    Ok(())
}

struct Textures {
    gl: glow::Context,
    limit: u32,
    mode: Mode,
    size: (u32, u32),
    frame: u64,
    active: Vec<glow::NativeTexture>,
    retired: Vec<glow::NativeTexture>,
}

impl Textures {
    fn new(api: &GraphicsAPI<'_>) -> Result<Self, String> {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return Err("texture experiment requires native OpenGL".into());
        };
        // SAFETY: Slint's rendering callback supplies its current context and
        // loader. Every subsequent GL call is in callbacks for this same window.
        let gl = unsafe { glow::Context::from_loader_function_cstr(get_proc_address) };
        let limit = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) }.max(0) as u32;
        Ok(Self {
            gl,
            limit,
            mode: Mode::Slint,
            size: (0, 0),
            frame: 0,
            active: Vec::with_capacity(3),
            retired: Vec::with_capacity(4),
        })
    }

    fn retire(&mut self) {
        self.retired.append(&mut self.active);
        self.frame = 0;
    }

    fn collect_retired(&mut self) {
        for texture in self.retired.drain(..) {
            // SAFETY: New image is already drawn/submitted; no UI source uses
            // this retired texture. GL defers destruction of pending GPU work.
            unsafe {
                self.gl.delete_texture(texture);
            }
            STATE.with_borrow_mut(|s| {
                s.deletions += 1;
                s.resident -= 1;
            });
        }
    }

    fn upload(
        &mut self,
        pixels: &SharedPixelBuffer<Rgba8Pixel>,
        mode: Mode,
    ) -> Result<Image, String> {
        let size = (pixels.width(), pixels.height());
        if size.0 == 0 || size.1 == 0 || size.0 > self.limit || size.1 > self.limit {
            return Err("raster dimensions exceed GL texture limits".into());
        }
        if self.mode != mode || self.size != size || mode == Mode::Fresh {
            self.retire();
            self.mode = mode;
            self.size = size;
        }
        // Restore all pixel-unpack/texture state we touch even on allocation or
        // GL error; another renderer owns the surrounding context.
        let saved = unsafe { Bindings::save(&self.gl) };
        let result = self.upload_inner(pixels);
        unsafe {
            saved.restore(&self.gl);
        }
        result
    }

    fn upload_inner(&mut self, pixels: &SharedPixelBuffer<Rgba8Pixel>) -> Result<Image, String> {
        let slot = self.mode.slot(self.frame).unwrap_or(0);
        let texture = if let Some(texture) = self.active.get(slot) {
            *texture
        } else {
            // SAFETY: Current context; validated dimensions; no unpack buffer
            // bound, so None means allocation with no client pixel payload.
            let texture = unsafe { self.gl.create_texture()? };
            self.active.push(texture);
            STATE.with_borrow_mut(|s| {
                s.allocations += 1;
                s.resident += 1;
            });
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                for (parameter, value) in [
                    // main.slint's raster Image uses default smooth sampling.
                    // Match it even at 1×: fractional UV rounding can otherwise
                    // change a few edge pixels and invalidate the A/B check.
                    (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                    (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    self.gl
                        .tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
                }
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    self.size.0 as i32,
                    self.size.1 as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            texture
        };
        // SAFETY: Buffer has exactly width*height RGBA8 pixels. The upload copies
        // from this live slice before returning; no pixel-unpack buffer is bound.
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                self.size.0 as i32,
                self.size.1 as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(pixels.as_bytes())),
            );
            let error = self.gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("GL upload error 0x{error:04x}"));
            }
        }
        self.frame = self.frame.wrapping_add(1);
        STATE.with_borrow_mut(|s| s.uploads += 1);
        // SAFETY: Created in this window's context, alive in active until the
        // source is replaced and Slint submits that replacement's drawing.
        Ok(unsafe {
            slint::BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(texture.0, self.size.into())
                .build()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_is_opt_in_bounded_and_retains_exact_pixels() {
        std::thread::spawn(|| {
            let first = Image::from_rgba8(SharedPixelBuffer::new(2, 3));
            assert!(!stage(&first));
            STATE.with_borrow_mut(|s| s.installed = true);
            assert!(stage(&first));
            let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(3, 2);
            pixels.make_mut_slice().fill(Rgba8Pixel {
                r: 17,
                g: 23,
                b: 42,
                a: 255,
            });
            let next = Image::from_rgba8(pixels.clone());
            assert!(stage(&next));
            let pending = STATE.with_borrow_mut(|s| s.pending.take().unwrap());
            assert_eq!(pending.to_rgba8().unwrap().as_slice(), pixels.as_slice());
            assert_eq!(pending.size(), next.size());
            assert!(STATE.with_borrow(|s| s.pending.is_none()));
            assert!(diagnostics().contains("raster_texture_allocations_total=0"));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn texture_reuse_prevents_switching_to_rgb_behind_its_back() {
        std::thread::spawn(|| {
            STATE.with_borrow_mut(|s| s.requested = Mode::Single);
            assert_eq!(
                publication::request_format(publication::UploadFormat::Rgb),
                Err("select host raster-textures slint before returning to RGB publication")
            );
        })
        .join()
        .unwrap();
    }
}

struct Bindings {
    texture: Option<glow::NativeTexture>,
    alignment: i32,
    extended: Option<(i32, i32, i32, Option<glow::NativeBuffer>)>,
}

impl Bindings {
    // SAFETY: Caller keeps the owning context current throughout save/upload/restore.
    unsafe fn save(gl: &glow::Context) -> Self {
        unsafe {
            let texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let alignment = gl.get_parameter_i32(glow::UNPACK_ALIGNMENT);
            let extended = if !gl.version().is_embedded || gl.version().major >= 3 {
                let saved = (
                    gl.get_parameter_i32(glow::UNPACK_ROW_LENGTH),
                    gl.get_parameter_i32(glow::UNPACK_SKIP_ROWS),
                    gl.get_parameter_i32(glow::UNPACK_SKIP_PIXELS),
                    gl.get_parameter_buffer(glow::PIXEL_UNPACK_BUFFER_BINDING),
                );
                gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
                Some(saved)
            } else {
                None
            };
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            Self {
                texture,
                alignment,
                extended,
            }
        }
    }

    unsafe fn restore(self, gl: &glow::Context) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, self.texture);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, self.alignment);
            if let Some((row, skip_rows, skip_pixels, buffer)) = self.extended {
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, row);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, skip_rows);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, skip_pixels);
                gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, buffer);
            }
        }
    }
}
