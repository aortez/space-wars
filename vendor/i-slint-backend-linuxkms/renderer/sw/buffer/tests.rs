// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use super::*;
use i_slint_core::software_renderer::{MinimalSoftwareWindow, RenderingRotation};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};
use std::{cell::RefCell, rc::Rc};

mod texture;

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct TestPlatform;

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        WINDOW.with(|window| {
            Ok(window.borrow().as_ref().expect("test window initialized").clone()
                as Rc<dyn WindowAdapter>)
        })
    }
}

slint::slint! {
    export component BufferTestWindow inherits Window {
        in property <image> texture;
        in property <color> window-background: #123456;
        in property <int> frame;
        background: root.window-background;
        Rectangle {
            x: -4px; y: 3px; width: parent.width - 5px; height: parent.height - 7px;
            clip: true;
            background: #b46321a0;
            Image {
                x: (root.frame * 7 - 9) * 1px; y: 2px;
                width: parent.width + 7px; height: parent.height - 1px;
                source: root.texture;
                opacity: root.frame == 2 ? 0.45 : 1;
            }
        }
        Rectangle {
            x: parent.width / 3; y: parent.height / 4;
            width: parent.width / 2; height: parent.height / 2;
            background: #2040b080;
            border-color: #eeddaa;
            border-width: 2px;
            border-radius: 5px;
        }
        Text { x: 1px; y: 2px; text: "12:34"; font-size: 12px; color: #eeeeee; }
    }
}

fn texture(alpha: bool) -> slint::Image {
    if alpha {
        let mut image = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(13, 7);
        for (i, pixel) in image.make_mut_slice().iter_mut().enumerate() {
            *pixel = slint::Rgba8Pixel::new(
                (i * 19) as u8,
                (i * 47) as u8,
                (i * 71) as u8,
                (i * 29) as u8,
            );
        }
        slint::Image::from_rgba8(image)
    } else {
        let mut image = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(13, 7);
        for (i, pixel) in image.make_mut_slice().iter_mut().enumerate() {
            *pixel = slint::Rgb8Pixel::new((i * 19) as u8, (i * 47) as u8, (i * 71) as u8);
        }
        slint::Image::from_rgb8(image)
    }
}

fn check_pixels<P: TargetPixel + bytemuck::Pod>(
    renderer: &SoftwareRenderer,
    shadow: &mut OutputBuffer,
    size: (usize, usize),
    rotation: RenderingRotation,
    format: DrmFourcc,
    frame: usize,
) {
    let (width, height) = size;
    let stride = match rotation {
        RenderingRotation::Rotate90 | RenderingRotation::Rotate270 => height,
        _ => width,
    };
    renderer.set_rendering_rotation(rotation);
    let count = width * height;
    let guard = P::from_rgb(79, 127, 193);
    let mut expected = vec![guard; count + 2];
    let mut direct = expected.clone();
    let mut copied = expected.clone();

    // The oracle is Slint's original, unwrapped slice renderer, including its
    // background-fill implementation. It does not exercise our timing wrapper.
    renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
    renderer.render(&mut expected[1..count + 1], stride);
    OutputBuffer::new(BufferMode::Direct, TextureMode::Rgb)
        .render(renderer, bytemuck::cast_slice_mut(&mut direct[1..count + 1]), stride, 3, format)
        .unwrap();
    let region = shadow
        .render(
            renderer,
            bytemuck::cast_slice_mut(&mut copied[1..count + 1]),
            stride,
            (frame % 4) as u8,
            format,
        )
        .unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&expected),
        bytemuck::cast_slice::<_, u8>(&direct),
        "direct wrapper: {format:?}, {size:?}, {rotation:?}, frame {frame}"
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&expected),
        bytemuck::cast_slice::<_, u8>(&copied),
        "RAM + copy: {format:?}, {size:?}, {rotation:?}, frame {frame}"
    );
    assert_eq!(
        region.iter().map(|(_, size)| size.width as usize * size.height as usize).sum::<usize>(),
        count,
        "RAM mode deliberately repaints the full frame"
    );
}

#[test]
fn direct_and_shadow_match_original_renderer_for_formats_rotations_alpha_and_resize() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = BufferTestWindow::new().unwrap();
    ui.show().unwrap();
    let mut shadow = OutputBuffer::new(BufferMode::Ram, TextureMode::Rgb);
    for format in [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888, DrmFourcc::Rgb565] {
        for size in [(48, 32), (73, 41), (39, 27)] {
            window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
            for rotation in [
                RenderingRotation::NoRotation,
                RenderingRotation::Rotate90,
                RenderingRotation::Rotate180,
                RenderingRotation::Rotate270,
            ] {
                for frame in 0..4 {
                    ui.set_frame(frame as i32);
                    ui.set_texture(texture(frame % 2 == 1));
                    ui.set_window_background(slint::Color::from_argb_u8(
                        if frame == 3 { 120 } else { 255 },
                        12,
                        (31 * frame) as u8,
                        73,
                    ));
                    ui.window().request_redraw();
                    assert!(window.draw_if_needed(|renderer| {
                        if format != DrmFourcc::Rgb565 {
                            check_pixels::<DumbBufferPixelXrgb888>(
                                renderer,
                                &mut shadow,
                                size,
                                rotation,
                                format,
                                frame,
                            );
                        } else {
                            check_pixels::<Rgb565Pixel>(
                                renderer,
                                &mut shadow,
                                size,
                                rotation,
                                DrmFourcc::Rgb565,
                                frame,
                            );
                        }
                    }));
                }
            }
        }
    }
}

#[test]
fn ram_is_opt_in_and_invalid_modes_are_rejected() {
    assert_eq!(BufferMode::parse(None).unwrap(), BufferMode::Direct);
    assert_eq!(BufferMode::parse(Some("direct")).unwrap(), BufferMode::Direct);
    assert_eq!(BufferMode::parse(Some("ram")).unwrap(), BufferMode::Ram);
    assert!(BufferMode::parse(Some("invalid")).is_err());
}

#[test]
fn unsupported_format_does_not_change_output_or_allocate_a_shadow() {
    let renderer = SoftwareRenderer::new();
    let mut buffer = OutputBuffer::new(BufferMode::Ram, TextureMode::Rgb);
    let mut output = [42_u8; 64];
    assert!(buffer.render(&renderer, &mut output, 4, 0, DrmFourcc::C8).is_err());
    assert_eq!(output, [42_u8; 64]);
    assert!(matches!(buffer.shadow, ShadowPixels::Empty));
}

#[test]
fn same_size_frames_reuse_storage_and_copy_whole_frames_to_rotating_targets() {
    let renderer = SoftwareRenderer::new();
    let mut shadow = vec![DumbBufferPixelXrgb888(0xff123456); 64];
    let address = shadow.as_ptr();
    // With no window attached, Slint leaves the shadow untouched. This isolates
    // the copy/reuse lifecycle from scene rendering (covered above).
    let mut targets = [
        vec![DumbBufferPixelXrgb888(0); 64],
        vec![DumbBufferPixelXrgb888(1); 64],
        vec![DumbBufferPixelXrgb888(2); 64],
    ];
    for index in 0..9 {
        let target = &mut targets[index % 3];
        for pixel in &mut shadow {
            pixel.0 = 0xff123456 + index as u32;
        }
        render_pixels(&renderer, target, 8, 3, Some(&mut shadow), TextureMode::Rgb);
        assert_eq!(shadow.as_ptr(), address);
        assert!(target.iter().all(|pixel| pixel.0 == 0xff123456 + index as u32));
    }
}
