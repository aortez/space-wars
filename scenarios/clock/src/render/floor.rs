//! The same panel tops supply water geometry, lightweight impacts and artwork.
use super::*;
use crate::floor::responsive::ResponsiveFloor;

pub(super) fn slab(frame: &mut RenderFrame, min: Vec2, max: Vec2, pitch: f32, opacity: f32) {
    surface(
        frame,
        [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
        pitch,
        opacity,
    );
}

// Flat banks, duck platforms and hinged drain panels share one material. All
// detailing stays inside the supplied solid, so gaps remain actual openings.
fn surface(frame: &mut RenderFrame, points: [Vec2; 4], pitch: f32, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    let edge = points[2] - points[3];
    let down = points[0] - points[3];
    let length = edge.length();
    let depth = down.length();
    if length <= 0.0 || depth <= 0.0 {
        return;
    }
    let along = edge / length;
    let down = down / depth;
    let mut quad = |points: [Vec2; 4], color: RenderColor| {
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points.map(|p| RenderPoint::new(p.x, p.y)).to_vec(),
                RenderColor {
                    a: opacity,
                    ..color
                },
            )),
        );
    };
    quad(points, FLOOR_COLOR);
    let lip = (pitch * 0.10).clamp(1.5, 3.0).min(depth * 0.25);
    for (near, far, color) in [
        (0.0, lip * 0.4, FLOOR_EDGE_COLOR),
        (lip * 0.4, lip, RenderColor::rgb(0.10, 0.063, 0.035)),
    ] {
        quad(
            [
                points[3] + down * far,
                points[2] + down * far,
                points[2] + down * near,
                points[3] + down * near,
            ],
            color,
        );
    }
    let count = (length / 48.0).ceil().clamp(1.0, 32.0) as usize;
    let spacing = length / count as f32;
    let grain_depth = (pitch * 0.30).min(depth * 0.7);
    for i in 0..count {
        let start = points[3]
            + along * (i as f32 + 0.16) * spacing
            + down * (grain_depth * (0.55 + (i % 3) as f32 * 0.18));
        let end = start + along * spacing * (0.38 + (i % 4) as f32 * 0.10);
        let thickness = down * (pitch * 0.027).min(depth * 0.06);
        quad(
            [start, end, end + thickness, start + thickness],
            RenderColor::rgb(0.29, 0.18, 0.095),
        );
    }
}

pub(super) fn responsive(
    frame: &mut RenderFrame,
    floor: &ResponsiveFloor,
    layout: Layout,
    opacity: f32,
) {
    for side in 0..2 {
        surface(
            frame,
            floor.shape.panel_points(side, floor.opening),
            layout.pitch,
            opacity,
        );
    }
    // Explicit event recovery, not an additional collider across the opening.
    render_floor(
        frame,
        crate::floor::FloorGeometry::closed(layout),
        layout.pitch,
        1.0 - opacity,
    );
}
