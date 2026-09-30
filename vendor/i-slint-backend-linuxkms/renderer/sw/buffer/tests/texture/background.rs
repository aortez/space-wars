// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use super::*;

fn check<P: TargetPixel + bytemuck::Pod>(
    renderer: &SoftwareRenderer,
    size: (usize, usize),
    rotation: RenderingRotation,
    format: DrmFourcc,
    omit_clear: bool,
    gradient: bool,
) {
    let (width, height) = match rotation {
        RenderingRotation::Rotate90 | RenderingRotation::Rotate270 => (size.1, size.0),
        _ => size,
    };
    let stride = width + 5;
    let count = stride * height;
    let mut expected = vec![P::from_rgb(79, 127, 193); count + 2];
    let mut actual = expected.clone();
    renderer.set_rendering_rotation(rotation);
    renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
    renderer.render(&mut expected[1..count + 1], stride);
    profiling::activate_for_test((width as u32, height as u32));
    {
        let _iteration = Scope::new(Stage::Loop);
        OutputBuffer::new(BufferMode::Direct, TextureMode::Rgb)
            .render(
                renderer,
                bytemuck::cast_slice_mut(&mut actual[1..count + 1]),
                stride,
                3,
                format,
            )
            .unwrap();
    }
    assert_eq!(bytemuck::cast_slice::<_, u8>(&expected), bytemuck::cast_slice::<_, u8>(&actual));
    // Gradient backgrounds use Slint's fallback instead of our solid fill.
    let fills = usize::from(!omit_clear && !gradient);
    let status = profiling::diagnostics();
    assert!(status.contains(&format!("kms_background_calls={fills}\n")), "{status}");
}

#[test]
fn clear_is_omitted_only_for_first_covering_opaque_image() {
    let window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = TextureWindow::new().unwrap();
    ui.show().unwrap();
    for size in [(71, 53), (39, 27)] {
        window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
        for variant in 0..11 {
            let scale = if variant == 1 { 2 } else { 1 };
            ui.set_texture(if variant == 7 {
                slint::Image::default()
            } else {
                rgb_texture(size.0 as u32 * scale, size.1 as u32 * scale, variant == 4)
            });
            ui.set_image_width((size.0 - usize::from(variant == 2) * 7) as f32);
            ui.set_image_height(size.1 as f32);
            ui.set_image_opacity(if variant == 3 { 0.45 } else { 1.0 });
            ui.set_tint(if variant == 5 {
                slint::Color::from_rgb_u8(41, 211, 71)
            } else {
                slint::Color::default()
            });
            ui.set_clipped(variant == 6);
            ui.set_underlay(variant == 8);
            ui.set_window_background(slint::Color::from_argb_u8(
                if variant == 9 { 120 } else { 255 },
                21,
                43,
                64,
            ));
            ui.set_gradient_background(variant == 10);
            for rotation in [
                RenderingRotation::NoRotation,
                RenderingRotation::Rotate90,
                RenderingRotation::Rotate180,
                RenderingRotation::Rotate270,
            ] {
                ui.window().request_redraw();
                assert!(window.draw_if_needed(|renderer| {
                    let omit_clear =
                        [0, 1, 9].contains(&variant) && rotation == RenderingRotation::NoRotation;
                    check::<DumbBufferPixelXrgb888>(
                        renderer,
                        size,
                        rotation,
                        DrmFourcc::Xrgb8888,
                        omit_clear,
                        variant == 10,
                    );
                    check::<Rgb565Pixel>(
                        renderer,
                        size,
                        rotation,
                        DrmFourcc::Rgb565,
                        omit_clear,
                        variant == 10,
                    );
                }));
            }
        }
    }
}

#[test]
fn deferred_background_preserves_buffer_age_partial_damage_and_scene_transitions() {
    let original_window = window();
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let original_ui = TextureWindow::new().unwrap();
    original_ui.show().unwrap();
    let fast_window = window();
    let fast_ui = TextureWindow::new().unwrap();
    fast_ui.show().unwrap();
    for window in [&original_window, &fast_window] {
        window.set_size(PhysicalSize::new(97, 61));
    }
    for buffers in 1..=3 {
        for rotation in [
            RenderingRotation::NoRotation,
            RenderingRotation::Rotate90,
            RenderingRotation::Rotate180,
            RenderingRotation::Rotate270,
        ] {
            let (width, height) = match rotation {
                RenderingRotation::Rotate90 | RenderingRotation::Rotate270 => (61, 97),
                _ => (97, 61),
            };
            let stride = width + 5;
            let count = stride * height;
            let mut expected = vec![vec![DumbBufferPixelXrgb888(0x12345678); count + 2]; buffers];
            let mut actual = expected.clone();
            let mut output = OutputBuffer::new(BufferMode::Direct, TextureMode::Rgb);
            for frame in 0..24 {
                for ui in [&original_ui, &fast_ui] {
                    // Several overlay-only frames exercise small/disjoint dirty
                    // regions, then toggle the image like gameplay/menu changes.
                    if frame % 6 == 0 {
                        ui.set_texture(if frame % 12 == 6 {
                            slint::Image::default()
                        } else {
                            rgb_texture(97, 61, frame == 12)
                        });
                        ui.set_window_background(slint::Color::from_argb_u8(
                            120,
                            (frame * 9) as u8,
                            43,
                            64,
                        ));
                    }
                    ui.set_overlay(true);
                    ui.set_overlay_x(if frame % 2 == 0 { 4.0 } else { 74.0 });
                    ui.window().request_redraw();
                }
                let age = if frame < buffers { 0 } else { buffers as u8 };
                let repaint = match age {
                    1 => RepaintBufferType::ReusedBuffer,
                    2 => RepaintBufferType::SwappedBuffers,
                    _ => RepaintBufferType::NewBuffer,
                };
                let mut expected_region = Vec::new();
                assert!(original_window.draw_if_needed(|renderer| {
                    renderer.set_rendering_rotation(rotation);
                    renderer.set_repaint_buffer_type(repaint);
                    expected_region = renderer
                        .render(&mut expected[frame % buffers][1..count + 1], stride)
                        .iter()
                        .collect();
                }));
                assert!(fast_window.draw_if_needed(|renderer| {
                    renderer.set_rendering_rotation(rotation);
                    let region = output
                        .render(
                            renderer,
                            bytemuck::cast_slice_mut(&mut actual[frame % buffers][1..count + 1]),
                            stride,
                            age,
                            DrmFourcc::Xrgb8888,
                        )
                        .unwrap();
                    assert_eq!(region.iter().collect::<Vec<_>>(), expected_region);
                    if buffers == 1 && frame % 6 > 1 {
                        let area: u32 = region.iter().map(|(_, s)| s.width * s.height).sum();
                        assert!(area > 0 && area < 97 * 61, "expected partial damage, got {area}");
                    }
                }));
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&expected[frame % buffers]),
                    bytemuck::cast_slice::<_, u8>(&actual[frame % buffers]),
                    "buffers {buffers}, rotation {rotation:?}, frame {frame}"
                );
            }
        }
    }
}
