//! Real floor geometry expressed as transforms from retained material coordinates.
use super::*;
use crate::{
    events::duck::{ArenaTerrain, arena::CourseGeometry},
    floor::{FloorGeometry, responsive::ResponsiveFloor},
    layout::Layout,
};

pub const MAX_SOIL_SURFACES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Surface {
    pub left: f32,
    pub right: f32,
    pub reference: Vec2,
    pub position: Vec2,
    pub angle: f32,
}
impl Surface {
    pub fn map(self, point: Vec2) -> Vec2 {
        self.position + (point - self.reference).rotate_radians(self.angle)
    }
    pub fn covers(self, left: f32, right: f32) -> bool {
        self.left <= left && right <= self.right
    }
}

pub(crate) fn ordinary(floor: FloorGeometry) -> Vec<Surface> {
    floor
        .slabs()
        .map(|(a, b)| Surface {
            left: a.x,
            right: b.x,
            reference: Vec2::ZERO,
            position: Vec2::ZERO,
            angle: 0.0,
        })
        .collect()
}
pub(crate) fn responsive(layout: Layout, floor: &ResponsiveFloor) -> Vec<Surface> {
    (0..2)
        .map(|side| {
            let (mut reference, _) = floor.shape.panel_pose(side, 0.0);
            reference.y += layout.floor_y - floor.shape.floor_y as f32;
            let (position, angle) = floor.shape.panel_pose(side, floor.opening);
            Surface {
                left: if side == 0 {
                    -floor.shape.half_width as f32
                } else {
                    0.0
                },
                right: if side == 0 {
                    0.0
                } else {
                    floor.shape.half_width as f32
                },
                reference,
                position,
                angle,
            }
        })
        .collect()
}
pub(crate) fn course(course: &CourseGeometry) -> Vec<Surface> {
    course
        .course
        .surfaces
        .iter()
        .map(|s| {
            let a = course.screen_position(Vec2::new(s.start.max(0.0), 0.0)).x;
            let b = course
                .screen_position(Vec2::new(s.end.min(course.width), 0.0))
                .x;
            Surface {
                left: a.min(b),
                right: a.max(b),
                reference: Vec2::ZERO,
                position: Vec2::new(0.0, s.height),
                angle: 0.0,
            }
        })
        .collect()
}
pub(crate) fn terrain(terrain: &ArenaTerrain) -> Vec<Surface> {
    if let Some(floor) = terrain.responsive_floor() {
        responsive(terrain.layout, floor)
    } else if let Some(floor) = terrain.drain_floor {
        ordinary(floor.geometry())
    } else if terrain.course.is_some() {
        course(&CourseGeometry::from_terrain(terrain))
    } else {
        Vec::new()
    }
}
