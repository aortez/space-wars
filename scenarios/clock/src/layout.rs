use engine_common::RenderPoint;
use engine_core::Vec2;

use crate::{SegmentId, digits};

pub(crate) const CAMERA_HEIGHT: f32 = 480.0;
pub(crate) const FACE_WIDTH_UNITS: f32 = 30.0;
const FACE_HEIGHT_UNITS: f32 = 9.0;
const DIGIT_ORIGINS: [f32; 4] = [0.0, 7.0, 17.0, 24.0];

/// The rendered cells and their colliders use the same geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub bounds_min: RenderPoint,
    pub bounds_max: RenderPoint,
    pub pitch: f32,
    pub face_origin: RenderPoint,
    pub floor_y: f32,
}

impl Layout {
    pub fn new(aspect_ratio: f32) -> Self {
        let world_width = CAMERA_HEIGHT * aspect_ratio;
        let bounds_min = RenderPoint::new(-world_width * 0.5, -CAMERA_HEIGHT * 0.5);
        let bounds_max = RenderPoint::new(world_width * 0.5, CAMERA_HEIGHT * 0.5);
        let floor_y = bounds_min.y + CAMERA_HEIGHT * 0.16;
        let horizontal_margin = (world_width * 0.06).max(16.0);
        let face_bottom = floor_y + CAMERA_HEIGHT * 0.08;
        let face_top = bounds_max.y - CAMERA_HEIGHT * 0.10;
        let pitch = ((world_width - horizontal_margin * 2.0) / FACE_WIDTH_UNITS)
            .min((face_top - face_bottom) / (FACE_HEIGHT_UNITS + 1.0))
            .max(2.0);
        let face_origin = RenderPoint::new(
            -FACE_WIDTH_UNITS * pitch * 0.5,
            (face_bottom + face_top - FACE_HEIGHT_UNITS * pitch) * 0.5,
        );
        Self {
            bounds_min,
            bounds_max,
            pitch,
            face_origin,
            floor_y,
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
