use engine_common::{Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive};
use engine_core::Vec2;

use crate::{
    events::marquee::MarqueeEvent,
    layout::Layout,
    presentation::{Bounds, clip_quad, effects::Placement},
};

pub(super) fn viewport(layout: Layout) -> Bounds {
    Bounds {
        min: Vec2::new(layout.bounds_min.x + 8.0, layout.floor_y + 8.0),
        max: Vec2::new(layout.bounds_max.x - 8.0, layout.bounds_max.y - 28.0),
    }
}

pub(super) fn render(frame: &mut RenderFrame, event: &MarqueeEvent, layout: Layout) {
    let playback = event.playback();
    if playback.strength <= 0.0 {
        return;
    }
    let bounds = viewport(layout);
    let placement = Placement {
        content: event.content.bounds,
        viewport: bounds,
        center: Vec2::new(0.0, layout.face_origin.y + 4.5 * layout.pitch),
        pitch: layout.pitch * if event.uses_clock { 1.0 } else { 0.7 },
    };
    for cell in &event.content.cells {
        let quad = event.recipe.quad(*cell, placement, playback);
        let (points, count) = clip_quad(quad, bounds);
        if count < 3 {
            continue;
        }
        let (palette, brightness) = event.recipe.lighting.sample(
            *cell,
            event.content.bounds,
            playback.seconds,
            playback.progress,
        );
        // Filled polygons only: clipping a stroked polygon would draw a false
        // outline along the viewport edge and let half a stroke escape the clip.
        frame.push_primitive(
            super::ACTIVE_CELL_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: points[..count]
                    .iter()
                    .map(|p| RenderPoint::new(p.x, p.y))
                    .collect(),
                fill: Some(Fill::new(RenderColor {
                    a: playback.strength * brightness,
                    ..palette.fill
                })),
                stroke: None,
            }),
        );
    }
}
