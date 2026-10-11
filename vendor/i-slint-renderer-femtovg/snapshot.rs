//! Normalize cardinal display rotations only, and only when a snapshot is
//! requested. No frame copies, readbacks or pixel retention in normal drawing.
use i_slint_core::{
    api::PhysicalSize,
    graphics::{Rgba8Pixel, SharedPixelBuffer},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuarterTurn {
    Clockwise,
    Half,
    Counterclockwise,
}

impl QuarterTurn {
    pub(crate) fn from_transform(
        degrees: f32,
        translation: (f32, f32),
        panel: PhysicalSize,
        window: PhysicalSize,
    ) -> Option<Self> {
        // Do not invent inverse transforms for arbitrary callers of Slint's
        // transformed-render API. Match the standard LinuxKMS output transform.
        let (rotation, expected_translation, expected_size) = match degrees.rem_euclid(360.) {
            90. => (
                Self::Clockwise,
                (0., -(panel.width as f32)),
                PhysicalSize::new(panel.height, panel.width),
            ),
            180. => (Self::Half, (-(panel.width as f32), -(panel.height as f32)), panel),
            270. => (
                Self::Counterclockwise,
                (-(panel.height as f32), 0.),
                PhysicalSize::new(panel.height, panel.width),
            ),
            _ => return None,
        };
        (translation == expected_translation && window == expected_size).then_some(rotation)
    }
}

pub(crate) fn upright(
    source: SharedPixelBuffer<Rgba8Pixel>,
    rotation: Option<QuarterTurn>,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let Some(rotation) = rotation else {
        return source;
    };
    let (width, height) = (source.width(), source.height());
    let (out_width, out_height) = match rotation {
        QuarterTurn::Half => (width, height),
        _ => (height, width),
    };
    let mut result = SharedPixelBuffer::new(out_width, out_height);
    let output = result.make_mut_slice();
    for y in 0..height {
        for x in 0..width {
            let (out_x, out_y) = match rotation {
                QuarterTurn::Clockwise => (y, width - 1 - x),
                QuarterTurn::Half => (width - 1 - x, height - 1 - y),
                QuarterTurn::Counterclockwise => (height - 1 - y, x),
            };
            output[(out_y * out_width + out_x) as usize] =
                source.as_slice()[(y * width + x) as usize];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels() -> SharedPixelBuffer<Rgba8Pixel> {
        let mut p = SharedPixelBuffer::new(3, 2);
        for (i, pixel) in p.make_mut_slice().iter_mut().enumerate() {
            *pixel = Rgba8Pixel::new(i as u8, 10 + i as u8, 20 + i as u8, 30 + i as u8);
        }
        p
    }

    #[test]
    fn non_square_pixel_order_and_all_channels_survive_inverse_rotation() {
        for (rotation, size, indices) in [
            (QuarterTurn::Clockwise, (2, 3), [2, 5, 1, 4, 0, 3]),
            (QuarterTurn::Half, (3, 2), [5, 4, 3, 2, 1, 0]),
            (QuarterTurn::Counterclockwise, (2, 3), [3, 0, 4, 1, 5, 2]),
        ] {
            let source = pixels();
            let output = upright(source.clone(), Some(rotation));
            assert_eq!((output.width(), output.height()), size);
            for (pixel, index) in output.as_slice().iter().zip(indices) {
                assert_eq!(*pixel, source.as_slice()[index]);
            }
        }
    }

    #[test]
    fn unrotated_snapshots_reuse_the_original_allocation() {
        let source = pixels();
        let pointer = source.as_slice().as_ptr();
        let output = upright(source, None);
        assert_eq!(pointer, output.as_slice().as_ptr());
    }

    #[test]
    fn recognizes_only_cardinal_output_transforms_and_matching_dimensions() {
        let panel = PhysicalSize::new(3, 2);
        let transposed = PhysicalSize::new(2, 3);
        for (angle, translation, size, expected) in [
            (90., (0., -3.), transposed, QuarterTurn::Clockwise),
            (180., (-3., -2.), panel, QuarterTurn::Half),
            (270., (-2., 0.), transposed, QuarterTurn::Counterclockwise),
            (-90., (-2., 0.), transposed, QuarterTurn::Counterclockwise),
        ] {
            assert_eq!(
                QuarterTurn::from_transform(angle, translation, panel, size),
                Some(expected)
            );
            assert_eq!(QuarterTurn::from_transform(angle, (0., 0.), panel, size), None);
            assert_eq!(
                QuarterTurn::from_transform(angle, translation, panel, PhysicalSize::new(8, 5)),
                None
            );
        }
        for angle in [0., 45., 89.99, f32::NAN, f32::INFINITY] {
            assert_eq!(QuarterTurn::from_transform(angle, (0., -3.), panel, transposed), None);
        }
    }
}
