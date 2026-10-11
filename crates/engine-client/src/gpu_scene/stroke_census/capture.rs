//! Hard-bounded, diagnostic-only copy of the outlines actually tessellated.
use super::{Draw, Shape, Vertex};
use engine_common::Stroke;
use lyon_tessellation::math::{Point, point};

const MAX_DRAWS: usize = 8192;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct Outline {
    pub points: Vec<Point>,
    pub stroke: Stroke,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Default)]
pub(crate) struct Capture {
    pub outlines: Vec<Outline>,
    pub bytes: usize,
    pub error: Option<&'static str>,
}

impl Capture {
    pub fn record(&mut self, draw: &Draw, vertices: &[Vertex], indices: &[u32], base: usize) {
        if self.error.is_some() {
            return;
        }
        let (Shape::Polygon(points), Some(stroke)) = (&draw.shape, draw.stroke) else {
            self.error = Some("stroke capture requires a projected polygon outline");
            return;
        };
        let bytes = points
            .len()
            .saturating_mul(size_of::<Point>())
            .saturating_add(std::mem::size_of_val(vertices))
            .saturating_add(std::mem::size_of_val(indices))
            .saturating_add(size_of::<Outline>());
        if self.outlines.len() >= MAX_DRAWS || bytes > MAX_BYTES.saturating_sub(self.bytes) {
            self.error = Some("stroke capture exceeded its per-frame budget; no truncated replay");
            return;
        }
        if indices
            .iter()
            .any(|&i| (i as usize) < base || (i as usize) - base >= vertices.len())
        {
            self.error = Some("captured stroke indices escape its vertex range");
            return;
        }
        self.bytes += bytes;
        self.outlines.push(Outline {
            points: points.iter().map(|p| point(p.x, p.y)).collect(),
            stroke,
            vertices: vertices.to_vec(),
            indices: indices.iter().map(|&i| i - base as u32).collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{RenderColor, RenderPoint};

    fn draw() -> Draw {
        Draw {
            shape: Shape::Polygon(vec![RenderPoint::new(0., 0.); 3]),
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, 1.)),
        }
    }

    #[test]
    fn capture_rebases_indices_and_fails_closed_on_budget_or_bad_range() {
        let vertices = [[0.; 6]; 3];
        let mut c = Capture::default();
        c.record(&draw(), &vertices, &[9, 10, 11], 9);
        assert_eq!(c.outlines[0].indices, [0, 1, 2]);
        c.record(&draw(), &vertices, &[8, 10, 11], 9);
        assert!(c.error.is_some());
        c.record(&draw(), &vertices, &[9, 10, 11], 9);
        assert_eq!(c.outlines.len(), 1);
        let mut c = Capture {
            bytes: MAX_BYTES,
            ..Default::default()
        };
        c.record(&draw(), &vertices, &[0, 1, 2], 0);
        assert!(c.error.is_some());
        assert!(c.outlines.is_empty());
    }
}
