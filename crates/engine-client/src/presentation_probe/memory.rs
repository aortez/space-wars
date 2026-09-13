//! Frozen, identical rendering into ordinary RAM and an optional private DRM
//! allocation. No framebuffer, modeset, page flip or DRM master is requested.
use super::*;
use drm::{buffer::Buffer as _, control::Device as _};
use i_slint_core::software_renderer::RenderingRotation;
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::{AsFd, BorrowedFd},
        unix::fs::FileTypeExt,
    },
    path::Path,
};

struct Card(File);
impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl drm::control::Device for Card {}

struct PrivateBuffer {
    card: Card,
    buffer: drm::control::dumbbuffer::DumbBuffer,
}
impl PrivateBuffer {
    fn new(path: &Path, size: (u32, u32)) -> Result<Self, Box<dyn std::error::Error>> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if !file.metadata()?.file_type().is_char_device() {
            return Err("--presentation-drm-device must name a DRM character device".into());
        }
        let card = Card(file);
        let buffer = card.create_dumb_buffer(size, drm::buffer::DrmFourcc::Xrgb8888, 32)?;
        Ok(Self { card, buffer })
    }
}
impl Drop for PrivateBuffer {
    fn drop(&mut self) {
        // The mapping is borrowed from this allocation and drops first. Closing
        // the fd also releases this client's resources if destruction fails.
        if let Err(error) = self.card.destroy_dumb_buffer(self.buffer) {
            eprintln!("Failed to destroy private probe buffer: {error}");
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShortcutPixel(Xrgb);
impl TargetPixel for ShortcutPixel {
    #[inline]
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        match color.alpha {
            // Premultiplied zero-alpha pixels also have zero RGB components.
            0 => {}
            255 => self.0 = Xrgb::from_rgb(color.red, color.green, color.blue),
            _ => self.0.blend(color),
        }
    }
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(Xrgb::from_rgb(r, g, b))
    }
    fn background() -> Self {
        Self(Xrgb::background())
    }
}

struct MemoryBuffer<'a, P> {
    pixels: &'a mut [P],
    stride: usize,
    detail: bool,
}
impl<P: TargetPixel> TargetPixelBuffer for MemoryBuffer<'_, P> {
    type TargetPixel = P;
    fn line_slice(&mut self, y: usize) -> &mut [P] {
        &mut self.pixels[y * self.stride..(y + 1) * self.stride]
    }
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn diagnostic_observer(&self) -> Option<DrawDiagnosticObserver> {
        self.detail.then_some(observe)
    }
}

fn draw<P: TargetPixel + bytemuck::Pod>(
    window: &MinimalSoftwareWindow,
    pixels: &mut [Xrgb],
    stride: usize,
    rotation: RenderingRotation,
    detail: bool,
) -> Duration {
    MEASUREMENTS.with(|m| *m.borrow_mut() = [Measurement::default(); DRAW_DIAGNOSTIC_COUNT]);
    window.request_redraw();
    let mut elapsed = Duration::ZERO;
    assert!(window.draw_if_needed(|renderer| {
        renderer.set_rendering_rotation(rotation);
        let mut buffer = MemoryBuffer::<P> {
            pixels: bytemuck::cast_slice_mut(pixels),
            stride,
            detail,
        };
        let start = Instant::now();
        renderer.render_into_buffer(&mut buffer);
        elapsed = start.elapsed();
    }));
    elapsed
}

#[derive(Clone, Copy)]
struct Variant {
    drm: bool,
    shortcut: bool,
    staged: bool,
}

pub(super) fn run(
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let rotation = match options.presentation_rotation.as_str() {
        "0" => RenderingRotation::NoRotation,
        "90" => RenderingRotation::Rotate90,
        "180" => RenderingRotation::Rotate180,
        "270" => RenderingRotation::Rotate270,
        _ => return Err("invalid presentation rotation".into()),
    };
    let size = if matches!(
        rotation,
        RenderingRotation::Rotate90 | RenderingRotation::Rotate270
    ) {
        (height, width)
    } else {
        (width, height)
    };
    let mut private = options
        .presentation_drm_device
        .as_deref()
        .map(|path| PrivateBuffer::new(path, size))
        .transpose()?;
    // Keep row padding identical across RAM and DRM, and verify it is untouched.
    let stride = private
        .as_ref()
        .map_or(size.0 as usize + 7, |p| p.buffer.pitch() as usize / 4);
    if stride < size.0 as usize || private.as_ref().is_some_and(|p| p.buffer.pitch() % 4 != 0) {
        return Err("DRM returned an invalid XRGB stride".into());
    }
    let count = stride * size.1 as usize;
    let mut mapping = private
        .as_mut()
        .map(|p| p.card.map_dumb_buffer(&mut p.buffer))
        .transpose()?;
    if let Some(mapping) = mapping.as_mut() {
        if mapping.len() < count * 4 {
            return Err("DRM mapping is smaller than the output".into());
        }
        mapping.fill(0xa5);
    }
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(windows.clone())))?;
    let ui = crate::MainWindow::new()?;
    let (image, rows) = text::fixture(width, height, 1.0)?;
    ui.set_launcher_scenario("spacewars".into());
    ui.set_raster_visible(true);
    ui.set_raster_frame(image);
    crate::host::update_raster_text_overlay(&ui, rows);
    ui.show()?;
    ui.window().set_size(PhysicalSize::new(width, height));
    let windows = windows.borrow();
    let window = &windows[0];
    let guard = Xrgb(0xa5a5a5a5);
    let mut expected = vec![guard; count];
    let mut ram = expected.clone();
    let mut shadow = expected.clone();
    draw::<Xrgb>(window, &mut expected, stride, rotation, false);
    let checksum = expected.iter().fold(0xcbf29ce484222325u64, |h, p| {
        (h ^ u64::from(p.0)).wrapping_mul(0x100000001b3)
    });
    let mut variants = vec![
        Variant {
            drm: false,
            shortcut: false,
            staged: false,
        },
        Variant {
            drm: false,
            shortcut: true,
            staged: false,
        },
    ];
    if mapping.is_some() {
        variants.extend([
            Variant {
                drm: true,
                shortcut: false,
                staged: false,
            },
            Variant {
                drm: true,
                shortcut: true,
                staged: false,
            },
        ]);
    }
    variants.extend([
        Variant {
            drm: mapping.is_some(),
            shortcut: false,
            staged: true,
        },
        Variant {
            drm: mapping.is_some(),
            shortcut: true,
            staged: true,
        },
    ]);
    println!(
        "memory,blend,staging,width,height,stride,rotation,detail,repeat,frames,draw_ms,copy_ms,total_ms,text_ms,font_ms,glyph_run_ms,text_texture_ms,text_fallback_ms,glyph_calls,checksum"
    );
    for repeat in 0..options.presentation_repeats {
        if repeat > 0 {
            variants.reverse();
        }
        for variant in &variants {
            let output = if variant.drm {
                bytemuck::try_cast_slice_mut(&mut mapping.as_mut().unwrap()[..count * 4])
                    .map_err(|e| format!("DRM mapping is not aligned for XRGB: {e}"))?
            } else {
                &mut ram
            };
            let mut draw_time = Duration::ZERO;
            let mut copy_time = Duration::ZERO;
            let mut nested = [Duration::ZERO; 5];
            let mut calls = 0;
            for frame in 0..options.presentation_frames + 10 {
                let target = if variant.staged {
                    &mut shadow[..]
                } else {
                    &mut output[..]
                };
                let elapsed = if variant.shortcut {
                    draw::<ShortcutPixel>(
                        window,
                        target,
                        stride,
                        rotation,
                        options.presentation_detail,
                    )
                } else {
                    draw::<Xrgb>(
                        window,
                        target,
                        stride,
                        rotation,
                        options.presentation_detail,
                    )
                };
                let copied = if variant.staged {
                    let start = Instant::now();
                    output.copy_from_slice(&shadow);
                    start.elapsed()
                } else {
                    Duration::ZERO
                };
                black_box(&output);
                if frame >= 10 {
                    draw_time += elapsed;
                    copy_time += copied;
                    MEASUREMENTS.with(|m| {
                        let m = m.borrow();
                        for (i, op) in [
                            DrawDiagnostic::Text,
                            DrawDiagnostic::TextFont,
                            DrawDiagnostic::TextGlyphRun,
                            DrawDiagnostic::Texture,
                            DrawDiagnostic::TextureFallback,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            nested[i] += if i < 3 {
                                m[op as usize].elapsed
                            } else {
                                m[op as usize].text_elapsed
                            };
                        }
                        calls += m[DrawDiagnostic::Texture as usize].text_calls;
                    });
                }
            }
            // Frozen output, checked after each block outside all timers.
            assert!(
                output == expected,
                "memory/shortcut/staging changed pixels in repeat {repeat}"
            );
            for row in output.chunks_exact(stride) {
                assert!(
                    row[size.0 as usize..].iter().all(|p| *p == guard),
                    "row padding changed"
                );
            }
            let ms =
                |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(options.presentation_frames);
            println!(
                "{},{},{},{width},{height},{stride},{},{},{repeat},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.3},{checksum:016x}",
                if variant.drm { "drm" } else { "ram" },
                if variant.shortcut {
                    "shortcut"
                } else {
                    "reference"
                },
                variant.staged,
                options.presentation_rotation,
                options.presentation_detail,
                options.presentation_frames,
                ms(draw_time),
                ms(copy_time),
                ms(draw_time + copy_time),
                ms(nested[0]),
                ms(nested[1]),
                ms(nested[2]),
                ms(nested[3]),
                ms(nested[4]),
                calls as f64 / f64::from(options.presentation_frames)
            );
        }
    }
    if let Some(mapping) = &mapping {
        assert!(
            mapping[count * 4..].iter().all(|byte| *byte == 0xa5),
            "mapping tail changed"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_shortcuts_preserve_reference_blending() {
        for alpha in 0..=255u8 {
            for destination in 0..=255u8 {
                for channel in [0, alpha / 2, alpha] {
                    let color = PremultipliedRgbaColor {
                        alpha,
                        red: channel,
                        green: alpha - channel,
                        blue: alpha / 3,
                    };
                    let original = Xrgb(u32::from_be_bytes([
                        destination,
                        255 - destination,
                        destination.wrapping_mul(79),
                        destination,
                    ]));
                    let mut reference = original;
                    reference.blend(color);
                    let mut candidate = ShortcutPixel(original);
                    candidate.blend(color);
                    assert_eq!(
                        candidate.0.0, reference.0,
                        "alpha={alpha}, destination={destination}"
                    );
                }
            }
        }
    }
}
