//! A small rubber-duck sprite attached to the passive hull's actual pose.
use super::*;
use crate::rain::RainEvent;

pub(super) fn floor(frame: &mut RenderFrame, event: &RainEvent) {
    for side in 0..2 {
        let points = event.floor.shape.panel_points(side, event.floor.opening);
        let points = points.map(|p| RenderPoint::new(p.x, p.y));
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points.into(),
                RenderColor {
                    a: event.opacity(),
                    ..FLOOR_COLOR
                },
            )),
        );
        let mut edge = points;
        // A thin band along the actual panel top, matching the closed floor.
        let (_, angle) = event.floor.shape.panel_pose(side, event.floor.opening);
        let down =
            Vec2::new(0.0, -(event.layout.pitch * 0.10).clamp(1.5, 3.0)).rotate_radians(angle);
        edge[0] = RenderPoint::new(edge[3].x + down.x, edge[3].y + down.y);
        edge[1] = RenderPoint::new(edge[2].x + down.x, edge[2].y + down.y);
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                edge.into(),
                RenderColor {
                    a: event.opacity(),
                    ..FLOOR_EDGE_COLOR
                },
            )),
        );
    }
}

pub(super) fn render(frame: &mut RenderFrame, event: &RainEvent) {
    let opacity = event.opacity();
    // Digit ledges and released water share the face's layer, above dim anchors.
    // The colon/AM-PM are display-only and drawn afterwards, remaining legible.
    meltdown::render_water(frame, &event.water, ACTIVE_CELL_LAYER, opacity);
    let pitch = event.layout.pitch;
    if let Some((floor, open)) = event.door() {
        let half = pitch * 0.8;
        let left = event.entry_x - half;
        frame.push_primitive(
            ARENA_LAYER,
            rectangle(
                RenderPoint::new(left, floor),
                RenderPoint::new(event.entry_x + half, floor + pitch * 1.25),
                RenderColor::rgb(0.025, 0.045, 0.085),
                Some(Stroke::new(FLOOR_EDGE_COLOR, pitch * 0.06)),
            ),
        );
        if open < 1.0 {
            frame.push_primitive(
                ARENA_LAYER,
                rectangle(
                    RenderPoint::new(left, floor + pitch * 1.25 * open),
                    RenderPoint::new(event.entry_x + half, floor + pitch * 1.25),
                    FLOOR_COLOR,
                    None,
                ),
            );
        }
    }
    let Some((position, angle)) = event.duck_pose() else {
        return;
    };
    let pixel = pitch * 0.085;
    // The lower yellow body aligns with the simple centered box collider; the
    // beak and head are decoration, not extra colliders or scripted motion.
    for (row, cells) in [
        "       YYYY  ",
        "      YYYYY  ",
        "      YYKYYOO",
        "     YYYYYOO ",
        " YY YYYYYY   ",
        "YYYYYYYYYY   ",
        " YYYYYYYY    ",
        "  YYYYYY     ",
    ]
    .iter()
    .enumerate()
    {
        for (column, cell) in cells.bytes().enumerate() {
            let color = match cell {
                b'Y' => RenderColor::rgb(1.0, 0.88, 0.12),
                b'O' => RenderColor::rgb(1.0, 0.48, 0.08),
                b'K' => BACKGROUND_COLOR,
                _ => continue,
            };
            let local = Vec2::new(
                (column as f32 - 4.5) * pixel * event.facing,
                (5.5 - row as f32) * pixel,
            );
            let center = position + local.rotate_radians(angle);
            let points =
                [(-0.51, -0.51), (0.51, -0.51), (0.51, 0.51), (-0.51, 0.51)].map(|(x, y)| {
                    let p = center + Vec2::new(x * pixel, y * pixel).rotate_radians(angle);
                    RenderPoint::new(p.x, p.y)
                });
            frame.push_primitive(
                ARENA_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    points.to_vec(),
                    RenderColor {
                        a: opacity,
                        ..color
                    },
                )),
            );
        }
    }
}
