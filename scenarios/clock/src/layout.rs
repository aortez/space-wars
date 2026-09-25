use engine_common::RenderPoint;
use engine_core::Vec2;

use crate::{SegmentId, digits};

pub(crate) const CAMERA_HEIGHT: f32 = 480.0;
pub(crate) const FACE_WIDTH_UNITS: f32 = 30.0;
const FACE_HEIGHT_UNITS: f32 = 9.0;
const FRAME_HEIGHT_FRACTION: f32 = 0.08;
pub(crate) const DIGIT_ORIGINS: [f32; 4] = [0.0, 7.0, 17.0, 24.0];

/// The rendered cells and their colliders use the same geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub bounds_min: RenderPoint,
    pub bounds_max: RenderPoint,
    pub pitch: f32,
    pub face_origin: RenderPoint,
    pub floor_y: f32,
    /// Underside of the decorative rain canopy; this is not a collider.
    pub canopy_y: f32,
}

impl Layout {
    pub fn new(aspect_ratio: f32) -> Self {
        let world_width = CAMERA_HEIGHT * aspect_ratio;
        let bounds_min = RenderPoint::new(-world_width * 0.5, -CAMERA_HEIGHT * 0.5);
        let bounds_max = RenderPoint::new(world_width * 0.5, CAMERA_HEIGHT * 0.5);
        let floor_y = bounds_min.y + CAMERA_HEIGHT * FRAME_HEIGHT_FRACTION;
        let canopy_y = bounds_max.y - CAMERA_HEIGHT * FRAME_HEIGHT_FRACTION;
        let horizontal_margin = (world_width * 0.06).max(16.0);
        // Symmetric inner margins retain the previous size budget (66% of
        // screen height), including space for AM/PM and character activity.
        let face_height = canopy_y - floor_y - CAMERA_HEIGHT * 0.18;
        let pitch = ((world_width - horizontal_margin * 2.0) / FACE_WIDTH_UNITS)
            .min(face_height / (FACE_HEIGHT_UNITS + 1.0))
            .max(2.0);
        let face_origin = RenderPoint::new(
            -FACE_WIDTH_UNITS * pitch * 0.5,
            -FACE_HEIGHT_UNITS * pitch * 0.5,
        );
        Self {
            bounds_min,
            bounds_max,
            pitch,
            face_origin,
            floor_y,
            canopy_y,
        }
    }

    pub fn digit_origin(self, slot: usize) -> RenderPoint {
        RenderPoint::new(
            self.face_origin.x + DIGIT_ORIGINS[slot] * self.pitch,
            self.face_origin.y,
        )
    }

    pub fn cell_center(self, id: SegmentId, cell: digits::GridCell) -> Vec2 {
        let origin = self.digit_origin(usize::from(id.digit_slot));
        Vec2::new(
            origin.x + (f32::from(cell.x) + 0.5) * self.pitch,
            origin.y + (f32::from(cell.y) + 0.5) * self.pitch,
        )
    }

    pub fn segment_center(self, id: SegmentId) -> Vec2 {
        let cells = digits::cells(id.kind);
        (self.cell_center(id, cells[0]) + self.cell_center(id, cells[cells.len() - 1])) * 0.5
    }

    pub fn drain_half_width(self) -> f32 {
        (self.pitch * 0.85).max(10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meridiem::{Glyph, PIXEL_SIZE};

    #[test]
    fn centered_face_and_equal_eight_percent_bands_fit_every_supported_aspect() {
        for aspect in [0.25, 0.6, 0.75, 1024.0 / 768.0, 800.0 / 480.0, 4.0] {
            let layout = Layout::new(aspect);
            assert_eq!(layout.floor_y, -layout.canopy_y);
            assert!((layout.floor_y - layout.bounds_min.y - CAMERA_HEIGHT * 0.08).abs() < 1e-4);
            assert_eq!(
                layout.face_origin.x + FACE_WIDTH_UNITS * layout.pitch * 0.5,
                0.0
            );
            assert_eq!(
                layout.face_origin.y + FACE_HEIGHT_UNITS * layout.pitch * 0.5,
                0.0
            );
            assert!(layout.face_origin.y > layout.floor_y);
            assert!(layout.face_origin.y + FACE_HEIGHT_UNITS * layout.pitch < layout.canopy_y);
            for glyph in [Glyph::A, Glyph::P, Glyph::M] {
                for cell in glyph.cells() {
                    let center = glyph.cell_center(layout, cell);
                    assert!(center.y - layout.pitch * PIXEL_SIZE * 0.5 > layout.floor_y);
                }
            }
        }
    }

    #[test]
    fn framing_keeps_existing_cell_sizes_and_horizontal_anchors() {
        for (aspect, pitch) in [
            (0.25, 88.0 / 30.0),
            (0.6, 8.448),
            (1024.0 / 768.0, 18.773_333),
            (800.0 / 480.0, 23.466_667),
            (4.0, 31.68),
        ] {
            let layout = Layout::new(aspect);
            assert!((layout.pitch - pitch).abs() < 1e-4, "aspect={aspect}");
            for (slot, origin) in DIGIT_ORIGINS.into_iter().enumerate() {
                assert_eq!(
                    layout.digit_origin(slot).x,
                    layout.face_origin.x + origin * layout.pitch
                );
            }
        }
    }
}
