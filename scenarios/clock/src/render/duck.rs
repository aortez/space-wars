use super::*;
use crate::events::duck::DuckEvent;

pub(super) fn render(frame: &mut RenderFrame, event: &DuckEvent, debug: bool) {
    render_with_course(frame, event, debug, true);
}

pub(super) fn render_with_course(
    frame: &mut RenderFrame,
    event: &DuckEvent,
    debug: bool,
    draw_course: bool,
) {
    if draw_course {
        render_arena(frame, event, event.arena_opacity());
    }
    let opacity = event.course_opacity();
    if opacity <= 0.0 {
        return;
    }
    render_actor(frame, event, debug, opacity);
}

pub(super) fn render_arena(frame: &mut RenderFrame, event: &DuckEvent, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    if let Some(drain) = event.drain_floor {
        // Preserve the original bank geometry and edge styling on handoff.
        render_floor(frame, drain.geometry(), event.layout.pitch, opacity);
        return;
    }
    let layout = event.layout;
    if let Some(course) = &event.course {
        let spans = course.surfaces.iter().map(|surface| {
            let a = event
                .render_position(Vec2::new(surface.start.max(0.0), 0.0))
                .x;
            let b = event
                .render_position(Vec2::new(surface.end.min(event.width), 0.0))
                .x;
            (a.min(b), a.max(b), surface.height)
        });
        course_slabs(frame, layout, opacity, spans);
    } else {
        let pit = event.obstacles[1];
        for (start, end) in [(0.0, pit.start), (pit.end, event.width)] {
            slab(
                frame,
                event,
                Vec2::new(start, layout.bounds_min.y),
                Vec2::new(end, layout.floor_y),
                opacity,
            );
        }
        for obstacle in event.obstacles.iter().filter(|o| o.height > 0.0) {
            slab(
                frame,
                event,
                Vec2::new(obstacle.start, layout.floor_y),
                Vec2::new(obstacle.end, layout.floor_y + obstacle.height),
                opacity,
            );
        }
    }
}

fn render_actor(frame: &mut RenderFrame, event: &DuckEvent, debug: bool, opacity: f32) {
    let radius = event.radius;
    let orange = RenderColor::rgb(1.0, 0.48, 0.08);
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
    // The wooden surround owns the hinged entry/exit panels and their occlusion.
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
    let stride = if event.feet_moving() {
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

/// Rain retains this shared geometry even after the player leaves. It is drawn
/// before water, with the exact same slab heights, gaps and mirroring.
pub(super) fn shared_course(
    frame: &mut RenderFrame,
    geometry: &crate::events::duck::arena::CourseGeometry,
    opacity: f32,
) {
    let spans = geometry.course.surfaces.iter().map(|surface| {
        let a = geometry
            .screen_position(Vec2::new(surface.start.max(0.0), 0.0))
            .x;
        let b = geometry
            .screen_position(Vec2::new(surface.end.min(geometry.width), 0.0))
            .x;
        (a.min(b), a.max(b), surface.height)
    });
    course_slabs(frame, geometry.layout, opacity, spans);
}

fn course_slabs(
    frame: &mut RenderFrame,
    layout: Layout,
    opacity: f32,
    spans: impl Iterator<Item = (f32, f32, f32)>,
) {
    for (left, right, height) in spans {
        floor::slab(
            frame,
            Vec2::new(left, layout.bounds_min.y),
            Vec2::new(right, layout.floor_y + height),
            layout.pitch,
            opacity,
        );
    }
}

fn slab(frame: &mut RenderFrame, event: &DuckEvent, min: Vec2, max: Vec2, opacity: f32) {
    let a = event.render_position(min);
    let b = event.render_position(max);
    floor::slab(
        frame,
        Vec2::new(a.x.min(b.x), a.y),
        Vec2::new(a.x.max(b.x), b.y),
        event.layout.pitch,
        opacity,
    );
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
