//! A supersampled software reference for dense/subpixel geometry. The native rasterizer
//! rounds scanline ends and strokes to whole pixels; comparing those edges to
//! MSAA gets less meaningful as circles shrink. Keep the native capture too.
use crate::{
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use engine_common::{RenderFrame, RenderPrimitive};

pub(super) fn circles(frames: &[RenderFrame], viewport: Viewport) -> Result<Vec<u8>, String> {
    const SCALE: u32 = 4;
    let mut frames = frames.to_vec();
    for primitive in frames
        .iter_mut()
        .flat_map(|f| &mut f.layers)
        .flat_map(|l| &mut l.primitives)
    {
        // Stroke widths are logical pixels, while positions/radii are world units.
        let stroke = match primitive {
            RenderPrimitive::Circle(circle) => circle.stroke.as_mut(),
            RenderPrimitive::Polygon(polygon) => polygon.stroke.as_mut(),
            RenderPrimitive::Line(line) => Some(&mut line.stroke),
            RenderPrimitive::Text(_) => {
                return Err("supersampled geometry reference excludes text".into());
            }
        };
        if let Some(stroke) = stroke {
            stroke.width *= SCALE as f32;
        }
    }
    let image = RasterRenderer::new()
        .image_from_frames_with_layout(
            &frames,
            Viewport::new(
                viewport.width * SCALE as f32,
                viewport.height * SCALE as f32,
            ),
            FrameLayout::EqualHorizontal,
            RasterOptions::default(),
        )
        .to_rgb8()
        .ok_or("supersampled reference unavailable")?;
    Ok(reduce(
        image.as_bytes(),
        viewport.width as usize,
        viewport.height as usize,
        SCALE as usize,
    ))
}

fn reduce(rgb: &[u8], width: usize, height: usize, scale: usize) -> Vec<u8> {
    let mut result = vec![0; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 3];
            for sy in 0..scale {
                for sx in 0..scale {
                    let start = ((y * scale + sy) * width * scale + x * scale + sx) * 3;
                    for channel in 0..3 {
                        sum[channel] += u32::from(rgb[start + channel]);
                    }
                }
            }
            for channel in 0..3 {
                result[(y * width + x) * 3 + channel] =
                    ((sum[channel] + (scale * scale / 2) as u32) / (scale * scale) as u32) as u8;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Camera2, RenderCircle, RenderColor, RenderPoint, Stroke};

    #[test]
    fn reduction_averages_each_pixel_without_reordering_rows() {
        let rgb: Vec<u8> = (0..16).flat_map(|v| [v * 10; 3]).collect();
        assert_eq!(
            reduce(&rgb, 2, 2, 2),
            [25, 25, 25, 45, 45, 45, 105, 105, 105, 125, 125, 125]
        );
    }

    #[test]
    fn supersampling_preserves_input_and_logical_stroke_width() {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 20.0));
        let mut circle = RenderCircle::filled(RenderPoint::ZERO, 6.0, RenderColor::WHITE);
        circle.stroke = Some(Stroke::new(RenderColor::rgba(1.0, 0.0, 0.0, 1.0), 2.0));
        frame.push_primitive(0, RenderPrimitive::Circle(circle));
        let original = frame.clone();
        let rgb = circles(std::slice::from_ref(&frame), Viewport::new(20.0, 20.0)).unwrap();
        assert_eq!(frame, original);
        let pixel = |x: usize, y: usize| &rgb[(y * 20 + x) * 3..(y * 20 + x) * 3 + 3];
        assert_eq!(pixel(10, 10), [255, 255, 255]);
        assert_eq!(pixel(15, 10), [255, 0, 0]);
        assert_eq!(pixel(18, 10), [5, 5, 20]);
    }
}
