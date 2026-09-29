//! A decorative wooden surround, with two dim lamps in its upper corners.
//! It sits over the scene's edges without changing the floor or rain canopy.

use super::*;
use crate::events::duck::DuckEvent;
use crate::presentation::{Bounds, clip_quad};
use engine_common::{RenderCircle, RenderLine};

#[cfg(test)]
mod tests;

const FRAME_LAYER: i32 = 10;
const LAMP_LAYER: i32 = 11;
const DOOR_LAYER: i32 = 12;

#[derive(Clone, Copy)]
struct DoorPanel {
    bottom: f32,
    top: f32,
    hinge: Vec2,
    angle: f32,
}

impl DoorPanel {
    fn transform(self, point: Vec2) -> RenderPoint {
        let rotated = self.hinge + (point - self.hinge).rotate_radians(self.angle);
        RenderPoint::new(rotated.x, rotated.y)
    }
}

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let width = (layout.bounds_max.x - layout.bounds_min.x).min(CAMERA_HEIGHT) * 0.022;
    let doors = door_panels(state.duck_scene());
    ring(
        frame,
        layout,
        doors,
        0.0,
        width,
        RenderColor::rgb(0.22, 0.14, 0.078),
    );
    ring(
        frame,
        layout,
        doors,
        0.0,
        width * 0.15,
        RenderColor::rgb(0.28, 0.19, 0.11),
    );
    ring(
        frame,
        layout,
        doors,
        width * 0.82,
        width,
        RenderColor::rgb(0.10, 0.063, 0.035),
    );
    ring(
        frame,
        layout,
        doors,
        width * 0.72,
        width * 0.82,
        RenderColor::rgb(0.32, 0.21, 0.12),
    );

    // Short, deterministic grain marks keep the thin rails from reading as a
    // flat brown UI border. Top and bottom grain mirror each other.
    for i in 0..16 {
        let t = i as f32 / 16.0;
        let offset = width * (0.28 + (i % 3) as f32 * 0.13);
        let x = layout.bounds_min.x
            + width * 2.0
            + t * (layout.bounds_max.x - layout.bounds_min.x - width * 4.0);
        let length = (layout.bounds_max.x - layout.bounds_min.x - width * 4.0) / 16.0
            * (0.4 + (i % 4) as f32 * 0.12);
        for y in [
            layout.bounds_min.y + offset,
            layout.bounds_max.y - offset - width * 0.06,
        ] {
            frame.push_primitive(
                FRAME_LAYER,
                rectangle(
                    RenderPoint::new(x, y),
                    RenderPoint::new(x + length, y + width * 0.06),
                    RenderColor::rgb(0.29, 0.18, 0.095),
                    None,
                ),
            );
        }
        let y = layout.bounds_min.y + width * 2.0 + t * (CAMERA_HEIGHT - width * 4.0);
        let length = (CAMERA_HEIGHT - width * 4.0) / 16.0 * (0.4 + (i % 4) as f32 * 0.12);
        for (side, x) in [
            layout.bounds_min.x + offset,
            layout.bounds_max.x - offset - width * 0.06,
        ]
        .into_iter()
        .enumerate()
        {
            rail(
                frame,
                [
                    RenderPoint::new(x, y),
                    RenderPoint::new(x + width * 0.06, y),
                    RenderPoint::new(x + width * 0.06, y + length),
                    RenderPoint::new(x, y + length),
                ],
                RenderColor::rgb(0.29, 0.18, 0.095),
                doors[side],
            );
        }
    }

    let radius = ((layout.bounds_max.x - layout.bounds_min.x) * 0.22).min(96.0);
    for x in [
        layout.bounds_min.x + width * 1.8,
        layout.bounds_max.x - width * 1.8,
    ] {
        let center = RenderPoint::new(x, layout.bounds_max.y - width * 1.8);
        for shell in (1..=12).rev() {
            frame.push_primitive(
                LAMP_LAYER,
                RenderPrimitive::Circle(RenderCircle::filled(
                    center,
                    radius * shell as f32 / 12.0,
                    RenderColor::rgba(1.0, 0.55, 0.20, 0.012),
                )),
            );
        }
        for (radius, color) in [
            (width * 0.28, RenderColor::rgb(0.30, 0.18, 0.075)),
            (width * 0.13, RenderColor::rgb(0.78, 0.52, 0.24)),
        ] {
            frame.push_primitive(
                LAMP_LAYER,
                RenderPrimitive::Circle(RenderCircle::filled(center, radius, color)),
            );
        }
    }
    for door in doors.into_iter().flatten() {
        let inward = door.angle.signum();
        let cut_edge =
            [0.0, width].map(|x| door.transform(Vec2::new(door.hinge.x + inward * x, door.bottom)));
        frame.push_primitive(
            DOOR_LAYER,
            RenderPrimitive::Line(RenderLine::new(
                cut_edge[0],
                cut_edge[1],
                Stroke::new(RenderColor::rgb(0.36, 0.24, 0.13), width * 0.08),
            )),
        );
        // A small fixed pin makes the top hinge readable while the wood moves.
        frame.push_primitive(
            DOOR_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(door.hinge.x, door.hinge.y),
                width * 0.16,
                RenderColor::rgba(0.58, 0.39, 0.17, (door.angle.abs() * 3.0).min(1.0)),
            )),
        );
    }
}

// The door is the rail itself, mounted at its outer top corner. Rotate the
// original bevel and grain with it so a closed flap rejoins the wall exactly.
fn door_panels(event: Option<&DuckEvent>) -> [Option<DoorPanel>; 2] {
    let mut panels = [None; 2];
    if let Some(event) = event {
        let (entrance, exit) = event.door_openness();
        for (x, open, visible) in [
            (0.0, entrance, event.entrance_visible()),
            (event.width, exit, event.exit_visible()),
        ] {
            let open = open.min(event.course_opacity()).clamp(0.0, 1.0);
            if !visible || open <= 0.0 {
                continue;
            }
            let wall_x = event.render_position(Vec2::new(x, 0.0)).x;
            let bottom = event.door_floor(x);
            let top = bottom + event.radius * 4.25;
            // Ease at both stops, with mirrored rotations into the scene.
            let angle = open * open * (3.0 - 2.0 * open) * std::f32::consts::FRAC_PI_2;
            panels[usize::from(wall_x > 0.0)] = Some(DoorPanel {
                bottom,
                top,
                hinge: Vec2::new(wall_x, top),
                angle: if wall_x < 0.0 { angle } else { -angle },
            });
        }
    }
    panels
}

fn rail(
    frame: &mut RenderFrame,
    points: [RenderPoint; 4],
    color: RenderColor,
    door: Option<DoorPanel>,
) {
    if let Some(door) = door {
        let quad = points.map(|p| Vec2::new(p.x, p.y));
        let min_x = quad.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let max_x = quad.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let min_y = quad.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_y = quad.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        for (lower, upper, moving) in [
            (min_y, door.bottom.min(max_y), false),
            (door.bottom.max(min_y), door.top.min(max_y), true),
            (door.top.max(min_y), max_y, false),
        ] {
            if lower >= upper {
                continue;
            }
            let (clipped, count) = clip_quad(
                quad,
                Bounds {
                    min: Vec2::new(min_x, lower),
                    max: Vec2::new(max_x, upper),
                },
            );
            if count >= 3 {
                frame.push_primitive(
                    if moving { DOOR_LAYER } else { FRAME_LAYER },
                    RenderPrimitive::Polygon(RenderPolygon::filled(
                        clipped[..count]
                            .iter()
                            .map(|p| {
                                if moving {
                                    door.transform(*p)
                                } else {
                                    RenderPoint::new(p.x, p.y)
                                }
                            })
                            .collect(),
                        color,
                    )),
                );
            }
        }
    } else {
        frame.push_primitive(
            FRAME_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(points.to_vec(), color)),
        );
    }
}

fn ring(
    frame: &mut RenderFrame,
    layout: Layout,
    doors: [Option<DoorPanel>; 2],
    outer: f32,
    inner: f32,
    color: RenderColor,
) {
    let corners = |inset| {
        [
            RenderPoint::new(layout.bounds_min.x + inset, layout.bounds_min.y + inset),
            RenderPoint::new(layout.bounds_max.x - inset, layout.bounds_min.y + inset),
            RenderPoint::new(layout.bounds_max.x - inset, layout.bounds_max.y - inset),
            RenderPoint::new(layout.bounds_min.x + inset, layout.bounds_max.y - inset),
        ]
    };
    let outside = corners(outer);
    let inside = corners(inner);
    for i in 0..4 {
        let next = (i + 1) % 4;
        rail(
            frame,
            [outside[i], outside[next], inside[next], inside[i]],
            color,
            match i {
                1 => doors[1],
                3 => doors[0],
                _ => None,
            },
        );
    }
}
