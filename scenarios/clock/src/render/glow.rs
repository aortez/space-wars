//! Small translucent shells follow the actual rendered cell geometry. Both
//! adapters use the same draw list; no blur texture or extra physics is needed.

use super::*;
use crate::presentation::{Bounds, clip_quad};

pub(super) fn quad(
    frame: &mut RenderFrame,
    points: [RenderPoint; 4],
    color: RenderColor,
    clip: Option<Bounds>,
) {
    if color.a <= 0.0 {
        return;
    }
    let quad = points.map(|p| Vec2::new(p.x, p.y));
    let center = quad.iter().copied().fold(Vec2::ZERO, |a, b| a + b) * 0.25;
    // Outer shells are barely visible; neighboring cells softly pool their
    // light. Alpha follows fades and the current palette, including warning.
    for (scale, alpha) in [(2.4, 0.008), (1.9, 0.012), (1.5, 0.020), (1.2, 0.028)] {
        let expanded = quad.map(|point| center + (point - center) * scale);
        let points = if let Some(bounds) = clip {
            let (points, count) = clip_quad(expanded, bounds);
            if count < 3 {
                continue;
            }
            points[..count]
                .iter()
                .map(|p| RenderPoint::new(p.x, p.y))
                .collect()
        } else {
            expanded.map(|p| RenderPoint::new(p.x, p.y)).to_vec()
        };
        frame.push_primitive(
            GLOW_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points,
                RenderColor {
                    a: color.a * alpha,
                    ..color
                },
            )),
        );
    }
}
