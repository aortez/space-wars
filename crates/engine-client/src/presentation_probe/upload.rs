//! One-variable upload experiments, not an engine scene renderer. Compare
//! formats with fresh textures, or texture lifetime with RGBA fixed.
//! RGBA expansion is timed every frame; fixture creation and pixel readback are
//! outside timing. No swap, scanout, settings, IPC or per-frame glFinish.

use super::Options;
use crate::raster::publication::expand_rgba;
use femtovg::{Canvas, ImageFlags, ImageSource, Paint, Path, PixelFormat, RenderTarget};
use glow::HasContext;
use glutin::{
    api::egl::{device::Device, display::Display},
    config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, Version},
    prelude::*,
};
use nix::time::{ClockId, clock_gettime};
use slint::{Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};
use std::{
    hint::black_box,
    path::Path as FilePath,
    time::{Duration, Instant},
};

type Error = Box<dyn std::error::Error>;
type GlCanvas = Canvas<femtovg::renderer::OpenGl>;
const WARMUP_FRAMES: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Rgb,
    Rgba,
}

impl Format {
    fn label(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
        }
    }
    fn pixel_format(self) -> PixelFormat {
        match self {
            Self::Rgb => PixelFormat::Rgb8,
            Self::Rgba => PixelFormat::Rgba8,
        }
    }
    fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

fn variant_order(repeat: u32) -> [Format; 2] {
    if repeat % 2 == 0 {
        [Format::Rgb, Format::Rgba]
    } else {
        [Format::Rgba, Format::Rgb]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifetime {
    Fresh,
    Single,
    Triple,
}

impl Lifetime {
    fn label(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Single => "single",
            Self::Triple => "triple",
        }
    }
    fn slots(self) -> usize {
        match self {
            Self::Fresh => 0,
            Self::Single => 1,
            Self::Triple => 3,
        }
    }
}

fn reuse_order(repeat: u32) -> [Lifetime; 3] {
    use Lifetime::*;
    [
        [Fresh, Single, Triple],
        [Triple, Single, Fresh],
        [Single, Triple, Fresh],
        [Fresh, Triple, Single],
        [Triple, Fresh, Single],
        [Single, Fresh, Triple],
    ][repeat as usize % 6]
}

#[derive(Clone, Copy, Default)]
struct Timing {
    wall: Duration,
    cpu: Duration,
}

struct Stamp {
    wall: Instant,
    cpu: Duration,
}

impl Stamp {
    fn now() -> Result<Self, Error> {
        let wall = Instant::now();
        let cpu = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)?;
        Ok(Self {
            wall,
            cpu: Duration::new(cpu.tv_sec().try_into()?, cpu.tv_nsec().try_into()?),
        })
    }
    fn elapsed(self) -> Result<Timing, Error> {
        let end = Self::now()?;
        Ok(Timing {
            wall: end.wall.duration_since(self.wall),
            cpu: end
                .cpu
                .checked_sub(self.cpu)
                .ok_or("CPU clock moved backwards")?,
        })
    }
}

impl Timing {
    fn add(&mut self, next: Self) {
        self.wall += next.wall;
        self.cpu += next.cpu;
    }
    fn fields(self, frames: u32) -> String {
        format!(
            "{:.6},{:.6}",
            self.wall.as_secs_f64() * 1000.0 / f64::from(frames),
            self.cpu.as_secs_f64() * 1000.0 / f64::from(frames)
        )
    }
}

#[derive(Default)]
struct Measurements {
    convert: Timing,
    allocate: Timing,
    upload: Timing,
    draw: Timing,
    delete: Timing,
    drain: Timing,
    total: Timing,
}

// Canvas/GL resources are dropped before the current context, in field order.
struct Gpu {
    canvas: GlCanvas,
    gl: glow::Context,
    _context: glutin::api::egl::context::PossiblyCurrentContext,
}

impl Gpu {
    fn new(render_node: &FilePath) -> Result<Self, Error> {
        let device = Device::query_devices()?
            .find(|device| device.drm_render_device_node_path() == Some(render_node))
            .ok_or_else(|| {
                format!(
                    "EGL did not enumerate render node {}; refusing implicit/software fallback",
                    render_node.display()
                )
            })?;
        // SAFETY: Device comes from EGL enumeration; no raw native display or
        // DRM-master descriptor is supplied. This never owns a display output.
        let display = unsafe { Display::with_device(&device, None)? };
        let template = ConfigTemplateBuilder::new()
            .with_api(Api::GLES2)
            .with_surface_type(ConfigSurfaceTypes::PBUFFER)
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(8)
            .build();
        // SAFETY: No native window handles in the config or context attributes.
        let config = unsafe { display.find_configs(template)? }
            .min_by_key(|config| config.num_samples())
            .ok_or("No offscreen GLES configuration")?;
        let attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(2, 0))))
            .build(None);
        let context =
            unsafe { display.create_context(&config, &attributes)? }.make_current_surfaceless()?;
        // SAFETY: The current EGL context remains alive throughout this Gpu.
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name))
        };
        let identity = unsafe {
            [
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
            ]
        };
        if identity[1].to_lowercase().contains("llvmpipe")
            || identity[1].to_lowercase().contains("softpipe")
            || identity[1].to_lowercase().contains("swrast")
        {
            return Err(format!("Software GL renderer rejected: {}", identity[1]).into());
        }
        eprintln!(
            "upload_probe_gl={identity:?}; render_node={}; offscreen=true; paced=false; GPU_execution_time_measured=false",
            render_node.display()
        );
        let renderer = unsafe {
            femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                display.get_proc_address(name)
            })?
        };
        Ok(Self {
            canvas: Canvas::new(renderer)?,
            gl,
            _context: context,
        })
    }

    fn finish(&self) {
        // SAFETY: This context is current on this thread. Only drain at block
        // boundaries, never force a CPU/GPU round trip inside each frame.
        unsafe {
            self.gl.finish();
        }
    }

    fn check_error(&self) -> Result<(), Error> {
        // Outside measurement; failed GL work must not appear as a fast result.
        let code = unsafe { self.gl.get_error() };
        if code == glow::NO_ERROR {
            Ok(())
        } else {
            Err(format!("OpenGL error: 0x{code:04x}").into())
        }
    }

    fn block(
        &mut self,
        rgb: &SharedPixelBuffer<Rgb8Pixel>,
        format: Format,
        lifetime: Lifetime,
        changing_marker: bool,
        frames: u32,
    ) -> Result<(Measurements, Vec<Rgba8Pixel>), Error> {
        let width = rgb.width() as usize;
        let height = rgb.height() as usize;
        self.canvas.set_size(rgb.width(), rgb.height(), 1.0);
        let target = self.canvas.create_image_empty(
            width,
            height,
            PixelFormat::Rgba8,
            ImageFlags::NEAREST,
        )?;
        self.canvas.set_render_target(RenderTarget::Image(target));
        self.canvas.clear_rect(
            0,
            0,
            rgb.width(),
            rgb.height(),
            femtovg::Color::rgb(0, 0, 0),
        );
        self.canvas.flush();
        self.finish();
        self.check_error()?;
        let mut rgba = vec![
            Rgba8Pixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            };
            width * height
        ];
        let mut path = Path::new();
        path.rect(0.0, 0.0, width as f32, height as f32);
        // Pool setup is outside steady-state timing, equally for every block.
        let textures = (0..lifetime.slots())
            .map(|_| {
                self.canvas.create_image_empty(
                    width,
                    height,
                    format.pixel_format(),
                    ImageFlags::NEAREST,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut measurements = Measurements::default();
        let mut input = rgb.clone();
        if changing_marker {
            input.make_mut_slice();
        }
        let mut total = Stamp::now()?;
        for frame in 0..WARMUP_FRAMES + frames {
            if frame == WARMUP_FRAMES {
                self.finish();
                measurements = Measurements::default();
                total = Stamp::now()?;
            }
            if changing_marker {
                mark_frame(input.make_mut_slice(), frame);
            }
            if format == Format::Rgba {
                let started = Stamp::now()?;
                expand_rgba(black_box(input.as_slice()), black_box(&mut rgba));
                measurements.convert.add(started.elapsed()?);
            }
            let started = Stamp::now()?;
            let image = if textures.is_empty() {
                self.canvas.create_image_empty(
                    width,
                    height,
                    format.pixel_format(),
                    ImageFlags::NEAREST,
                )?
            } else {
                textures[frame as usize % textures.len()]
            };
            measurements.allocate.add(started.elapsed()?);
            let source = match format {
                Format::Rgb => ImageSource::Rgb(femtovg::imgref::ImgRef::new(
                    input.as_slice(),
                    width,
                    height,
                )),
                Format::Rgba => {
                    ImageSource::Rgba(femtovg::imgref::ImgRef::new(&rgba, width, height))
                }
            };
            let started = Stamp::now()?;
            self.canvas.update_image(image, source, 0, 0)?;
            measurements.upload.add(started.elapsed()?);
            let started = Stamp::now()?;
            self.canvas.fill_path(
                &path,
                &Paint::image(image, 0.0, 0.0, width as f32, height as f32, 0.0, 1.0)
                    .with_anti_alias(false),
            );
            self.canvas.flush();
            measurements.draw.add(started.elapsed()?);
            let started = Stamp::now()?;
            if textures.is_empty() {
                self.canvas.delete_image(image);
            }
            measurements.delete.add(started.elapsed()?);
        }
        let started = Stamp::now()?;
        self.finish();
        measurements.drain = started.elapsed()?;
        measurements.total = total.elapsed()?;
        self.check_error()?;
        // Exclude readback and comparison from timing. FemtoVG's screenshot
        // normalizes the GL framebuffer's vertical orientation.
        let pixels = self.canvas.screenshot()?.buf().to_vec();
        self.check_error()?;
        verify_pixels(input.as_slice(), &pixels)?;
        for image in textures {
            self.canvas.delete_image(image);
        }
        self.canvas.delete_image(target);
        Ok((measurements, pixels))
    }
}

// Changing pixels catch a stale reused texture without replacing the production
// fixture. This tiny, identical mutation is included in each reuse variant's total.
fn mark_frame(pixels: &mut [Rgb8Pixel], frame: u32) {
    for (index, pixel) in pixels.iter_mut().take(16).enumerate() {
        *pixel = Rgb8Pixel {
            r: frame as u8,
            g: (frame >> 8) as u8,
            b: index as u8,
        };
    }
}

fn verify_pixels(rgb: &[Rgb8Pixel], actual: &[Rgba8Pixel]) -> Result<(), Error> {
    if rgb.len() != actual.len() {
        return Err("Readback dimensions differ from input".into());
    }
    let errors = rgb
        .iter()
        .zip(actual)
        .filter(|(src, dst)| (src.r, src.g, src.b, 255) != (dst.r, dst.g, dst.b, dst.a))
        .count();
    if errors > 0 {
        return Err(format!("GPU readback differs at {errors} of {} pixels", rgb.len()).into());
    }
    Ok(())
}

pub(super) fn run(options: &Options) -> Result<(), Error> {
    let fixture = super::fixtures()
        .into_iter()
        .find(|fixture| fixture.name == options.presentation_upload_fixture)
        .ok_or("Unknown upload fixture")?;
    let rgb = fixture
        .image
        .to_rgb8()
        .ok_or("Upload fixture must be RGB")?;
    let checksum: u64 = rgb
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    let mut gpu = Gpu::new(&options.presentation_upload_device)?;
    eprintln!(
        "upload_probe_reuse={}; warmup_frames_per_block={WARMUP_FRAMES}; changing_marker={}; pool_setup_excluded=true",
        options.presentation_upload_reuse, options.presentation_upload_reuse
    );
    println!(
        "fixture,width,height,format,lifetime,repeat,order,frames,payload_bytes,convert_wall_avg_ms,convert_cpu_avg_ms,allocate_wall_avg_ms,allocate_cpu_avg_ms,upload_wall_avg_ms,upload_cpu_avg_ms,draw_wall_avg_ms,draw_cpu_avg_ms,delete_wall_avg_ms,delete_cpu_avg_ms,drain_wall_avg_ms,drain_cpu_avg_ms,total_wall_avg_ms,total_cpu_avg_ms,input_rgb_checksum,pixels_exact"
    );
    for repeat in 0..options.presentation_repeats {
        let variants: Vec<_> = if options.presentation_upload_reuse {
            reuse_order(repeat)
                .into_iter()
                .map(|lifetime| (Format::Rgba, lifetime))
                .collect()
        } else {
            variant_order(repeat)
                .into_iter()
                .map(|format| (format, Lifetime::Fresh))
                .collect()
        };
        for (order, (format, lifetime)) in variants.into_iter().enumerate() {
            let (m, _) = gpu.block(
                &rgb,
                format,
                lifetime,
                options.presentation_upload_reuse,
                options.presentation_frames,
            )?;
            let frames = options.presentation_frames;
            println!(
                "{},{},{},{},{},{repeat},{order},{frames},{},{},{},{},{},{},{},{},{checksum:016x},true",
                fixture.name,
                rgb.width(),
                rgb.height(),
                format.label(),
                lifetime.label(),
                rgb.as_slice().len() * format.channels(),
                m.convert.fields(frames),
                m.allocate.fields(frames),
                m.upload.fields(frames),
                m.draw.fields(frames),
                m.delete.fields(frames),
                m.drain.fields(frames),
                m.total.fields(frames)
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgba_expansion_is_opaque_and_preserves_every_channel() {
        let rgb: Vec<_> = (0..=255)
            .map(|r| Rgb8Pixel {
                r,
                g: 255 - r,
                b: r / 2,
            })
            .collect();
        let mut rgba = vec![Rgba8Pixel::default(); rgb.len()];
        expand_rgba(&rgb, &mut rgba);
        verify_pixels(&rgb, &rgba).unwrap();
        rgba[4].a = 0;
        assert!(verify_pixels(&rgb, &rgba).is_err());
        assert!(verify_pixels(&rgb, &rgba[..5]).is_err());
    }
    #[test]
    fn every_pair_has_both_formats_and_alternates_order() {
        assert_eq!(variant_order(0), [Format::Rgb, Format::Rgba]);
        assert_eq!(variant_order(1), [Format::Rgba, Format::Rgb]);
        assert_eq!(variant_order(2), variant_order(0));
    }
    #[test]
    fn texture_orders_balance_every_position_and_bound_storage() {
        for variant in [Lifetime::Fresh, Lifetime::Single, Lifetime::Triple] {
            for position in 0..3 {
                assert_eq!(
                    (0..6)
                        .filter(|repeat| reuse_order(*repeat)[position] == variant)
                        .count(),
                    2
                );
            }
        }
        assert_eq!(Lifetime::Fresh.slots(), 0);
        assert_eq!(Lifetime::Single.slots(), 1);
        assert_eq!(Lifetime::Triple.slots(), 3);
    }

    #[test]
    fn changing_marker_detects_stale_frame_and_preserves_rest_of_fixture() {
        let mut pixels = vec![Rgb8Pixel { r: 1, g: 2, b: 3 }; 32];
        mark_frame(&mut pixels, 513);
        assert_eq!((pixels[15].r, pixels[15].g, pixels[15].b), (1, 2, 15));
        assert!(pixels[16..].iter().all(|p| (p.r, p.g, p.b) == (1, 2, 3)));
        let mut output = vec![Rgba8Pixel::default(); pixels.len()];
        expand_rgba(&pixels, &mut output);
        verify_pixels(&pixels, &output).unwrap();
        mark_frame(&mut pixels, 514);
        assert!(verify_pixels(&pixels, &output).is_err());
    }
}
