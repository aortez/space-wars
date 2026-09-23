//! The same panel tops supply water geometry, lightweight impacts and artwork.
use super::*;
use crate::floor::responsive::ResponsiveFloor;

pub(super) fn responsive(
    frame: &mut RenderFrame,
    floor: &ResponsiveFloor,
    layout: Layout,
    opacity: f32,
) {
    for side in 0..2 {
        let points = floor.shape.panel_points(side, floor.opening);
        let points = points.map(|p| RenderPoint::new(p.x, p.y));
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points.into(),
                RenderColor {
                    a: opacity,
                    ..FLOOR_COLOR
                },
            )),
        );
        let mut edge = points;
        let (_, angle) = floor.shape.panel_pose(side, floor.opening);
        let down = Vec2::new(0.0, -(layout.pitch * 0.10).clamp(1.5, 3.0)).rotate_radians(angle);
        edge[0] = RenderPoint::new(edge[3].x + down.x, edge[3].y + down.y);
        edge[1] = RenderPoint::new(edge[2].x + down.x, edge[2].y + down.y);
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                edge.into(),
                RenderColor {
                    a: opacity,
                    ..FLOOR_EDGE_COLOR
                },
            )),
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
