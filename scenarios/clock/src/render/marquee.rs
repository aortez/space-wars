use engine_common::{Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive};

use crate::{
    events::marquee::{MarqueeEvent, viewport},
    layout::Layout,
    presentation::{clip_quad, content::Cell},
};

pub(super) fn render(frame: &mut RenderFrame, event: &MarqueeEvent, layout: Layout) {
    render_cells(frame, event, layout, |_| true);
}

/// Draws the content cells `keep` chooses.
pub(super) fn render_cells(
    frame: &mut RenderFrame,
    event: &MarqueeEvent,
    layout: Layout,
    keep: impl Fn(&Cell) -> bool,
) {
    let playback = event.playback();
    if playback.strength <= 0.0 {
        return;
    }
    let bounds = viewport(layout);
    let placement = event.placement(layout);
    for cell in event.content.cells.iter().filter(|cell| keep(cell)) {
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
        super::glow::quad(
            frame,
            quad.map(|p| RenderPoint::new(p.x, p.y)),
            RenderColor {
                a: playback.strength * brightness,
                ..palette.fill
            },
            Some(bounds),
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
