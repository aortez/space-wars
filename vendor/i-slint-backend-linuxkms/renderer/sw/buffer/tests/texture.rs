use super::*;

slint::slint! {
    export component TextureWindow inherits Window {
        in property <image> texture;
        in property <length> image-width: self.width;
        in property <length> image-height: self.height;
        in property <length> image-x;
        in property <length> image-y;
        in property <float> image-opacity: 1;
        in property <color> tint: transparent;
        in property <bool> clipped;
        in property <int> crop-x;
        in property <int> crop-y;
        in property <int> crop-width: root.texture.width;
        in property <int> crop-height: root.texture.height;
        in property <bool> tiled;
        in property <bool> overlay;
        in property <length> overlay-x: 8px;
        background: #152b40;
        Rectangle {
            width: root.clipped ? parent.width - 7px : parent.width;
            height: root.clipped ? parent.height - 5px : parent.height;
            clip: root.clipped;
            Image {
                x: root.image-x; y: root.image-y;
                width: root.image-width; height: root.image-height;
                source: root.texture;
                image-fit: fill;
                horizontal-tiling: root.tiled ? repeat : none;
                vertical-tiling: root.tiled ? repeat : none;
                source-clip-x: root.crop-x;
                source-clip-y: root.crop-y;
                source-clip-width: root.crop-width;
                source-clip-height: root.crop-height;
                opacity: root.image-opacity;
                colorize: root.tint;
            }
        }
        if root.overlay : Rectangle {
            x: root.overlay-x; y: 4px; width: 13px; height: 9px;
            background: #ee771188; border-color: #ddaaff; border-width: 1px;
        }
    }
}

fn window() -> Rc<MinimalSoftwareWindow> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window
}

fn rgb_texture(width: u32, height: u32, alpha: bool) -> slint::Image {
    if alpha {
        let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
        for (i, p) in buffer.make_mut_slice().iter_mut().enumerate() {
            *p = slint::Rgba8Pixel::new(
                (i * 19) as u8,
                (i * 47) as u8,
                (i * 71) as u8,
                (i * 29) as u8,
            );
        }
        slint::Image::from_rgba8(buffer)
    } else {
        let mut buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(width, height);
        for (i, p) in buffer.make_mut_slice().iter_mut().enumerate() {
            *p = slint::Rgb8Pixel::new((i * 19) as u8, (i * 47) as u8, (i * 71) as u8);
        }
        slint::Image::from_rgb8(buffer)
    }
}

struct CheckedBuffer<'a, P> {
    pixels: &'a mut [P],
    stride: usize,
    hits: usize,
}

impl<P: TargetPixel + bytemuck::Pod> TargetPixelBuffer for CheckedBuffer<'_, P> {
    type TargetPixel = P;
    fn line_slice(&mut self, y: usize) -> &mut [P] {
        &mut self.pixels[y * self.stride..(y + 1) * self.stride]
    }
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        let before = bytemuck::cast_slice::<_, u8>(self.pixels).to_vec();
        let handled = super::super::texture::draw_rgb(args, clip, self.pixels, self.stride);
        if handled {
            self.hits += 1;
        } else {
            assert_eq!(
                before,
                bytemuck::cast_slice::<_, u8>(self.pixels),
                "fallback must not write"
            );
        }
        handled
    }
}

fn compare<P: TargetPixel + bytemuck::Pod>(
    renderer: &SoftwareRenderer,
    size: (usize, usize),
    rotation: RenderingRotation,
) -> usize {
    renderer.set_rendering_rotation(rotation);
    let (width, height) = match rotation {
        RenderingRotation::Rotate90 | RenderingRotation::Rotate270 => (size.1, size.0),
        _ => size,
    };
    // Row padding and end guards must be identical, including for cropped sources
    // whose byte stride exceeds their visible width.
    let stride = width + 5;
    let count = stride * height;
    let mut original = vec![P::from_rgb(93, 137, 191); count + 2];
    let mut fast = original.clone();
    renderer.render(&mut original[1..count + 1], stride);
    let hits = {
        let mut buffer = CheckedBuffer { pixels: &mut fast[1..count + 1], stride, hits: 0 };
        renderer.render_into_buffer(&mut buffer);
        buffer.hits
    };
    assert_eq!(bytemuck::cast_slice::<_, u8>(&original), bytemuck::cast_slice::<_, u8>(&fast));
    hits
}

#[test]
fn rgb_blits_match_original_for_scaling_cropping_clipping_formats_and_fallbacks() {
    let window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = TextureWindow::new().unwrap();
    ui.show().unwrap();
    for size in [(71, 53), (53, 31)] {
        window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
        for (width, height, sx, sy) in
            [(41, 29, 1, 1), (41, 29, 2, 2), (41, 29, 2, 1), (41, 29, 1, 2), (4200, 29, 2, 1)]
        {
            for variant in 0..8 {
                // Native/scaled, cropped/padded, opacity, RGBA, tint, tiling,
                // fractional sampling, and magnification.
                let crop = variant == 1;
                let source_w = width * sx;
                let source_h = height * sy;
                ui.set_texture(rgb_texture(
                    source_w + u32::from(crop) * 8,
                    source_h + u32::from(crop) * 6,
                    variant == 3,
                ));
                ui.set_crop_x(if crop { 3 } else { 0 });
                ui.set_crop_y(if crop { 2 } else { 0 });
                ui.set_crop_width(source_w as i32);
                ui.set_crop_height(source_h as i32);
                ui.set_image_width(if variant == 6 {
                    width as f32 + 3.0
                } else if variant == 7 {
                    source_w as f32 * 2.0
                } else {
                    width as f32
                });
                ui.set_image_height(height as f32);
                ui.set_image_x(if crop && width > 4096 {
                    -3000.0
                } else if crop {
                    -9.0
                } else {
                    size.0 as f32 - 30.0
                });
                ui.set_image_y(if crop { -5.0 } else { 3.0 });
                ui.set_clipped(true);
                ui.set_overlay(true);
                ui.set_image_opacity(if variant == 2 { 0.45 } else { 1.0 });
                ui.set_tint(if variant == 4 {
                    slint::Color::from_rgb_u8(41, 211, 71)
                } else {
                    slint::Color::default()
                });
                ui.set_tiled(variant == 5);
                for rotation in [
                    RenderingRotation::NoRotation,
                    RenderingRotation::Rotate90,
                    RenderingRotation::Rotate180,
                    RenderingRotation::Rotate270,
                ] {
                    ui.window().request_redraw();
                    assert!(window.draw_if_needed(|renderer| {
                        let xrgb_hits = compare::<DumbBufferPixelXrgb888>(renderer, size, rotation);
                        let rgb565_hits = compare::<Rgb565Pixel>(renderer, size, rotation);
                        let expected = usize::from(
                            variant <= 1
                                && source_w <= 4096
                                && rotation == RenderingRotation::NoRotation,
                        );
                        assert_eq!(xrgb_hits, expected, "variant {variant}, rotation {rotation:?}");
                        assert_eq!(rgb565_hits, expected);
                    }));
                }
            }
        }
    }
}

#[test]
fn rgb_blits_preserve_partial_damage_and_pixels_outside_dirty_regions() {
    let original_window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let original_ui = TextureWindow::new().unwrap();
    original_ui.show().unwrap();
    let fast_window = window();
    let fast_ui = TextureWindow::new().unwrap();
    fast_ui.show().unwrap();
    for (window, ui) in [(&original_window, &original_ui), (&fast_window, &fast_ui)] {
        window.set_size(PhysicalSize::new(97, 61));
        ui.set_texture(rgb_texture(194, 122, false));
        ui.set_overlay(true);
    }
    let mut original = vec![DumbBufferPixelXrgb888(0x12345678); 97 * 61];
    let mut fast = original.clone();
    for (frame, x) in [4.0, 11.0, 72.0, 8.0, 70.0, 12.0, 74.0, 6.0].into_iter().enumerate() {
        for ui in [&original_ui, &fast_ui] {
            ui.set_overlay_x(x);
            ui.window().request_redraw();
        }
        assert!(original_window.draw_if_needed(|renderer| {
            renderer.render(&mut original, 97);
            renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
        }));
        assert!(fast_window.draw_if_needed(|renderer| {
            let mut buffer = CheckedBuffer { pixels: &mut fast, stride: 97, hits: 0 };
            let region = renderer.render_into_buffer(&mut buffer);
            assert!(buffer.hits > 0);
            if frame > 1 {
                let area: u32 = region.iter().map(|(_, s)| s.width * s.height).sum();
                assert!(area > 0 && area < 97 * 61, "expected partial damage, got {area}");
            }
            renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
        }));
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&original),
            bytemuck::cast_slice::<_, u8>(&fast),
            "frame {frame}"
        );
    }
}

#[test]
fn texture_mode_selection_is_explicit() {
    assert_eq!(TextureMode::parse(None).unwrap(), TextureMode::Rgb);
    assert_eq!(TextureMode::parse(Some("rgb")).unwrap(), TextureMode::Rgb);
    assert_eq!(TextureMode::parse(Some("generic")).unwrap(), TextureMode::Generic);
    assert_eq!(TextureMode::parse(Some("compare")).unwrap(), TextureMode::Compare);
    assert!(TextureMode::parse(Some("other")).is_err());
}

#[test]
fn detailed_draw_spans_balance_and_distinguish_fallback_from_rgb() {
    use crate::profiling::{self, Scope, Stage};
    let window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = TextureWindow::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(64, 32));
    ui.set_texture(rgb_texture(64, 32, false));
    ui.set_overlay(true);
    let mut previous = None;
    for mode in [TextureMode::Generic, TextureMode::Rgb] {
        profiling::activate_for_test((64, 32));
        let mut output = OutputBuffer::new(BufferMode::Direct, mode);
        let mut pixels = vec![DumbBufferPixelXrgb888(0); 64 * 32];
        ui.window().request_redraw();
        {
            let _iteration = Scope::new(Stage::Loop);
            assert!(window.draw_if_needed(|renderer| {
                output
                    .render(
                        renderer,
                        bytemuck::cast_slice_mut(&mut pixels),
                        64,
                        3,
                        DrmFourcc::Xrgb8888,
                    )
                    .unwrap();
            }));
        }
        let status = profiling::diagnostics();
        for phase in ["render", "prepare", "dirty", "items", "image", "texture"] {
            assert!(status.contains(&format!("kms_core_{phase}_calls=1\n")), "{status}");
        }
        let fallbacks = usize::from(mode == TextureMode::Generic);
        assert!(status.contains(&format!("kms_core_fallback_calls={fallbacks}\n")));
        let bytes = bytemuck::cast_slice::<_, u8>(&pixels).to_vec();
        if let Some(previous) = previous {
            assert_eq!(bytes, previous);
        }
        previous = Some(bytes);
    }
}

#[test]
fn comparison_alternates_once_per_frame_with_separate_draw_accounting() {
    use crate::profiling::{self, Scope, Stage};
    profiling::activate_for_test((8, 8));
    profiling::texture_mode("compare");
    let renderer = SoftwareRenderer::new();
    let mut output = OutputBuffer::new(BufferMode::Direct, TextureMode::Compare);
    let mut pixels = vec![DumbBufferPixelXrgb888(0); 64];
    for frame in 0..6 {
        let _loop = Scope::new(Stage::Loop);
        output
            .render(&renderer, bytemuck::cast_slice_mut(&mut pixels), 8, 3, DrmFourcc::Xrgb8888)
            .unwrap();
        assert_eq!(output.compare_next_rgb, frame % 2 == 0);
    }
    let status = profiling::diagnostics();
    assert!(status.contains("kms_texture_mode=compare"));
    assert!(status.contains("kms_draw_calls=6\n"));
    assert!(status.contains("kms_draw_generic_calls=3\n"));
    assert!(status.contains("kms_draw_rgb_calls=3\n"));
}

/// Fixed-content RAM benchmark for Slint drawing, not DRM presentation or the
/// host's scene rasterizer. No pass/fail timing thresholds. Run explicitly with
/// --release --ignored --nocapture; regular CI still runs pixel equivalence.
#[test]
#[ignore = "explicit presentation microbenchmark; no timing assertions"]
fn benchmark_opaque_rgb_presentation() {
    use std::{hint::black_box, time::Instant};
    let window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = TextureWindow::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 768));
    let mut pixels = vec![DumbBufferPixelXrgb888(0); 1024 * 768];
    println!("fixture,mode,repeat,frames,wall_ms_per_frame");
    for (fixture, scale) in [("background", 0), ("rgb-native", 1), ("rgb-downscale2", 2)] {
        ui.set_texture(if scale == 0 {
            slint::Image::default()
        } else {
            rgb_texture(1024 * scale, 768 * scale, false)
        });
        ui.window().request_redraw();
        assert!(window.draw_if_needed(|renderer| {
            for repeat in 0..4 {
                let modes = if repeat % 2 == 0 {
                    [TextureMode::Generic, TextureMode::Rgb]
                } else {
                    [TextureMode::Rgb, TextureMode::Generic]
                };
                for mode in modes {
                    let mut output = OutputBuffer::new(BufferMode::Direct, mode);
                    let frames = 100;
                    for _ in 0..10 {
                        output
                            .render(
                                renderer,
                                bytemuck::cast_slice_mut(&mut pixels),
                                1024,
                                3,
                                DrmFourcc::Xrgb8888,
                            )
                            .unwrap();
                    }
                    let start = Instant::now();
                    for _ in 0..frames {
                        output
                            .render(
                                renderer,
                                bytemuck::cast_slice_mut(&mut pixels),
                                1024,
                                3,
                                DrmFourcc::Xrgb8888,
                            )
                            .unwrap();
                        black_box(&pixels);
                    }
                    println!(
                        "{fixture},{},{repeat},{frames},{:.6}",
                        mode.name(),
                        start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
                    );
                }
            }
        }));
    }
}
