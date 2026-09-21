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
    let opacity = event.course_opacity();
    if opacity <= 0.0 {
        return;
    }
    let layout = event.layout;
    let radius = event.radius;
    let orange = RenderColor::rgb(1.0, 0.48, 0.08);
    if draw_course {
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
            course_slabs(frame, layout, radius, opacity, spans);
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
    for (x, open, visible) in [
        (radius * 2.0, entrance, event.entrance_visible()),
        (event.width - radius * 2.0, exit, event.exit_visible()),
    ] {
        if !visible {
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
    course_slabs(frame, geometry.layout, geometry.radius, opacity, spans);
}

fn course_slabs(
    frame: &mut RenderFrame,
    layout: Layout,
    radius: f32,
    opacity: f32,
    spans: impl Iterator<Item = (f32, f32, f32)>,
) {
    for (left, right, height) in spans {
        let top = layout.floor_y + height;
        for (bottom, top, color, layer) in [
            (
                layout.bounds_min.y,
                layout.floor_y,
                FLOOR_COLOR,
                ARENA_LAYER,
            ),
            (
                layout.floor_y,
                top,
                RenderColor::rgb(1.0, 0.48, 0.08),
                ARENA_LAYER,
            ),
            (
                top - radius * 0.25,
                top,
                if height > 0.0 {
                    RenderColor::rgb(1.0, 0.85, 0.45)
                } else {
                    FLOOR_EDGE_COLOR
                },
                ACTIVE_CELL_LAYER,
            ),
        ] {
            if top <= bottom {
                continue;
            }
            frame.push_primitive(
                layer,
                rectangle(
                    RenderPoint::new(left, bottom),
                    RenderPoint::new(right, top),
                    RenderColor {
                        a: opacity,
                        ..color
                    },
                    None,
                ),
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventPhase, duck::DUCK_TICKS};

    fn door_counts(event: &DuckEvent) -> [usize; 2] {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        render(&mut frame, event, false);
        let mut counts = [0; 2];
        for layer in &frame.layers {
            if layer.z != ARENA_LAYER && layer.z != LABEL_LAYER {
                continue;
            }
            for primitive in &layer.primitives {
                let RenderPrimitive::Polygon(polygon) = primitive else {
                    continue;
                };
                if !polygon.points.iter().any(|p| p.y > event.layout.floor_y) {
                    continue;
                }
                for (side, count) in counts.iter_mut().enumerate() {
                    if polygon.points.iter().all(|p| {
                        let x = p.x * event.direction + event.width * 0.5;
                        if side == 0 {
                            x < event.radius * 4.0
                        } else {
                            x > event.width - event.radius * 4.0
                        }
                    }) {
                        *count += 1;
                    }
                }
            }
        }
        counts
    }

    #[test]
    fn entrance_door_disappears_after_closing_and_stays_gone() {
        for aspect in [1024.0 / 768.0, 800.0 / 480.0] {
            for direction in [-1.0, 1.0] {
                let mut event = DuckEvent::new_platforms(Layout::new(aspect), 42);
                event.direction = direction;
                event.step();
                assert_eq!(door_counts(&event), [3, 0], "entrance opens alone");
                while event.phase == EventPhase::Opening {
                    assert!(!event.step(), "opening must finish within the event");
                }
                assert_eq!(door_counts(&event), [2, 0], "entrance is fully open");
                while event.door_openness().0 == 1.0 {
                    assert!(
                        !event.step(),
                        "entrance must begin closing within the event"
                    );
                }
                assert_eq!(door_counts(&event), [3, 0], "keep the closing panel");
                while event.door_openness().0 > 0.0 {
                    assert!(
                        !event.step(),
                        "entrance must finish closing within the event"
                    );
                }
                assert!(event.position().is_some(), "remove the door, not the duck");
                assert_eq!(door_counts(&event), [0, 0], "closed entrance disappears");

                let mut saw_exit = false;
                while event.tick < DUCK_TICKS {
                    let exit_count = if event.exit_visible() && event.course_opacity() > 0.0 {
                        saw_exit = true;
                        if event.door_openness().1 < 1.0 { 3 } else { 2 }
                    } else {
                        0
                    };
                    assert_eq!(
                        door_counts(&event),
                        [0, exit_count],
                        "aspect={aspect} direction={direction} tick={}",
                        event.tick
                    );
                    event.step();
                }
                assert!(saw_exit, "the later exit door must still appear");
                assert_eq!(
                    event.diagnostics().outcome,
                    Some(engine_common::ClockDuckOutcome::Exited)
                );
                assert_eq!(door_counts(&event), [0, 0]);
            }
        }
    }
}
