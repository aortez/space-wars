//! Immutable course geometry shared by character collision, rain supports and
//! rendering. Copies happen only at event/visit boundaries, never per tick.
use super::{DuckEvent, planner::Course};
use crate::layout::Layout;
use engine_core::Vec2;
use engine_water::{Boundary, PoolSpec};

#[derive(Clone)]
pub(crate) struct CourseGeometry {
    pub layout: Layout,
    pub width: f32,
    pub radius: f32,
    pub direction: f32,
    pub course: Course,
}

impl CourseGeometry {
    pub fn from_duck(duck: &DuckEvent) -> Self {
        Self {
            layout: duck.layout,
            width: duck.width,
            radius: duck.radius,
            direction: duck.direction,
            course: duck.course.clone().expect("player course"),
        }
    }

    pub fn screen_position(&self, p: Vec2) -> Vec2 {
        Vec2::new((p.x - self.width * 0.5) * self.direction, p.y)
    }

    /// Exactly the visible/collidable top of each slab, with gaps left open.
    /// Higher adjacent slabs are spill lips, not permeable vertical walls.
    pub fn water_pools(&self) -> Vec<PoolSpec> {
        let mut spans: Vec<_> = self
            .course
            .surfaces
            .iter()
            .map(|s| {
                let a = self.screen_position(Vec2::new(s.start.max(0.0), 0.0)).x;
                let b = self
                    .screen_position(Vec2::new(s.end.min(self.width), 0.0))
                    .x;
                (
                    f64::from(a.min(b)),
                    f64::from(a.max(b)),
                    f64::from(self.layout.floor_y + s.height),
                )
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        spans
            .iter()
            .enumerate()
            .map(|(i, &(left, right, top))| {
                // Across all <= 7 slabs this uses at most 128 + 7 columns.
                let columns = ((right - left) / f64::from(self.width) * 128.0)
                    .ceil()
                    .max(1.0) as usize;
                let boundary = |edge| {
                    let (x, neighbor) = if edge == 0 {
                        (left, i.checked_sub(1).and_then(|n| spans.get(n)))
                    } else {
                        (right, spans.get(i + 1))
                    };
                    if (x.abs() - f64::from(self.width * 0.5)).abs() < 1e-4 {
                        Boundary::Closed
                    } else {
                        let lip = neighbor
                            .filter(|s| ((if edge == 0 { s.1 } else { s.0 }) - x).abs() < 1e-4)
                            .map_or(top, |s| top.max(s.2));
                        Boundary::Spill { lip }
                    }
                };
                PoolSpec {
                    left,
                    column_width: (right - left) / columns as f64,
                    bed: vec![top; columns],
                    boundaries: [boundary(0), boundary(1)],
                }
            })
            .collect()
    }
}
