//! A small rubber-duck sprite attached to the passive hull's actual pose.
use super::*;
use crate::rain::RainEvent;

pub(super) fn render(frame: &mut RenderFrame, event: &RainEvent) {
    let opacity = event.opacity();
    // Rain is behind the clock face, including the colon and AM/PM indicator.
    // Reuse the same volume-preserving water geometry as Meltdown.
    meltdown::render_water(frame, &event.water, ARENA_LAYER, opacity);
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
