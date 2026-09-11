use super::*;
use crate::events::duck::DuckEvent;

pub(super) fn render(frame: &mut RenderFrame, event: &DuckEvent, debug: bool) {
    let opacity = event.course_opacity();
    if opacity <= 0.0 {
        return;
    }
    let layout = event.layout;
    let radius = event.radius;
    // Replace the ordinary center drain with this event's physical floor/pit.
    rect(
        frame,
        event,
        Vec2::new(0.0, layout.bounds_min.y),
        Vec2::new(event.width, layout.floor_y),
        BACKGROUND_COLOR,
        ARENA_LAYER,
        opacity,
    );
    let orange = RenderColor::rgb(1.0, 0.48, 0.08);
    if let Some(course) = &event.course {
        for surface in &course.surfaces {
            let left = surface.start.max(0.0);
            let right = surface.end.min(event.width);
            let top = layout.floor_y + surface.height;
            rect(
                frame,
                event,
                Vec2::new(left, layout.bounds_min.y),
                Vec2::new(right, layout.floor_y),
                FLOOR_COLOR,
                ARENA_LAYER,
                opacity,
            );
            if surface.height > 0.0 {
                rect(
                    frame,
                    event,
                    Vec2::new(left, layout.floor_y),
                    Vec2::new(right, top),
                    orange,
                    ARENA_LAYER,
                    opacity,
                );
            }
            rect(
                frame,
                event,
                Vec2::new(left, top - event.radius * 0.25),
                Vec2::new(right, top),
                if surface.height > 0.0 {
                    RenderColor::rgb(1.0, 0.85, 0.45)
                } else {
                    FLOOR_EDGE_COLOR
                },
                ACTIVE_CELL_LAYER,
                opacity,
            );
        }
    } else {
        let pit = event.obstacles[1];
        for (start, end) in [(0.0, pit.start), (pit.end, event.width)] {
            rect(
                frame,
                event,
                Vec2::new(start, layout.bounds_min.y),
                Vec2::new(end, layout.floor_y),
                FLOOR_COLOR,
                ARENA_LAYER,
                opacity,
            );
            rect(
                frame,
                event,
                Vec2::new(start, layout.floor_y - 2.0),
                Vec2::new(end, layout.floor_y),
                FLOOR_EDGE_COLOR,
                ARENA_LAYER,
                opacity,
            );
        }
        for obstacle in event.obstacles.iter().filter(|o| o.height > 0.0) {
            rect(
                frame,
                event,
                Vec2::new(obstacle.start, layout.floor_y),
                Vec2::new(obstacle.end, layout.floor_y + obstacle.height),
                orange,
                ACTIVE_CELL_LAYER,
                opacity,
            );
            rect(
                frame,
                event,
                Vec2::new(
                    obstacle.start,
                    layout.floor_y + obstacle.height - radius * 0.3,
                ),
                Vec2::new(obstacle.end, layout.floor_y + obstacle.height),
                RenderColor::rgb(1.0, 0.85, 0.45),
                ACTIVE_CELL_LAYER,
                opacity,
            );
        }
    }
    if debug && let Some(arc) = event.debug_arc() {
        for (index, point) in arc.into_iter().enumerate() {
            let size = radius * if index == 0 || index == 24 { 0.5 } else { 0.15 };
            let color = if index == 24 {
                RenderColor::rgb(0.15, 1.0, 0.4)
            } else if index == 0 {
                RenderColor::rgb(0.15, 0.85, 1.0)
            } else {
                RenderColor::rgb(0.85, 0.4, 1.0)
            };
            rect(
                frame,
                event,
                point - Vec2::splat(size),
                point + Vec2::splat(size),
                color,
                LABEL_LAYER,
                opacity,
            );
        }
    }
    let (entrance, exit) = event.door_openness();
    for (x, open) in [(radius * 2.0, entrance), (event.width - radius * 2.0, exit)] {
        if x > event.width * 0.5 && !event.exit_visible() {
            continue;
        }
        let bottom = layout.floor_y;
        let top = bottom + radius * 4.0;
        let half = radius * 1.5;
        rect(
            frame,
            event,
            Vec2::new(x - half - radius * 0.25, bottom),
            Vec2::new(x + half + radius * 0.25, top + radius * 0.25),
            FLOOR_EDGE_COLOR,
            ARENA_LAYER,
            opacity,
        );
        rect(
            frame,
            event,
            Vec2::new(x - half, bottom),
            Vec2::new(x + half, top),
            BACKGROUND_COLOR,
            ARENA_LAYER,
            opacity,
        );
        // Logical backstage doors. The rising panel occludes the duck until it
        // is open; no moving-door collider can accidentally trap the runner.
        if open < 1.0 {
            rect(
                frame,
                event,
                Vec2::new(x - half, bottom + radius * 4.0 * open),
                Vec2::new(x + half, top),
                FLOOR_EDGE_COLOR,
                LABEL_LAYER,
                opacity,
            );
        }
    }
    let Some(position) = event.position() else {
        return;
    };
    let pixel = radius * 0.33;
    let yellow = RenderColor::rgb(1.0, 0.88, 0.12);
    // Code-native pixel art renders identically through both presentation paths.
    let rows = [
        "       YYYY  ",
        "      YYYYY  ",
        "      YYKYYOO",
        "     YYYYYOO ",
        " YY YYYYYY   ",
        "YYYYYYYYYY   ",
        " YYYYYYYY    ",
        "  YYYYYY     ",
    ];
    for (row, cells) in rows.into_iter().enumerate() {
        for (column, cell) in cells.bytes().enumerate() {
            let color = match cell {
                b'Y' => yellow,
                b'O' => orange,
                b'K' => BACKGROUND_COLOR,
                _ => continue,
            };
            let offset_x = (column as f32 - 6.0) * pixel;
            let min = position
                + Vec2::new(
                    if event.facing() > 0.0 {
                        offset_x
                    } else {
                        -offset_x - pixel
                    },
                    -radius + (8 - row) as f32 * pixel,
                );
            rect(
                frame,
                event,
                min,
                min + Vec2::new(pixel, pixel),
                color,
                ACTIVE_CELL_LAYER,
                1.0,
            );
        }
    }
    let stride = if event.grounded() {
        ((event.tick / 6) % 2) as f32 * pixel
    } else {
        0.0
    };
    for (x, offset) in [(-2.0, stride), (1.0, -stride)] {
        let offset_x = x * pixel + offset;
        let min = position
            + Vec2::new(
                if event.facing() > 0.0 {
                    offset_x
                } else {
                    -offset_x - pixel * 2.0
                },
                -radius,
            );
        rect(
            frame,
            event,
            min,
            min + Vec2::new(pixel * 2.0, pixel),
            orange,
            ACTIVE_CELL_LAYER,
            1.0,
        );
    }
}

fn rect(
    frame: &mut RenderFrame,
    event: &DuckEvent,
    min: Vec2,
    max: Vec2,
    color: RenderColor,
    layer: i32,
    opacity: f32,
) {
    let a = event.render_position(min);
    let b = event.render_position(max);
    frame.push_primitive(
        layer,
        rectangle(
            RenderPoint::new(a.x.min(b.x), a.y),
            RenderPoint::new(a.x.max(b.x), b.y),
            RenderColor {
                a: color.a * opacity,
                ..color
            },
            None,
        ),
    );
}
