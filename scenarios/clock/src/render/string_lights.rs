//! The Christmas theme's string of lights (#460): colored bulbs on a green
//! wire, hung in swags along the wall just under the canopy from one side
//! rail to the other. It is wall art only, sized by the frame's rails rather
//! than the face, and it never moves. Nothing here is a body, a surface or a
//! light source, so no plant grows by it and the lamps never see it.

use super::*;
use std::f32::consts::TAU;

#[cfg(test)]
mod tests;

/// On the wall, drawn just after it, so everything in the room shows in front
/// of the string: the face, the creatures, the water, the bolt and the frame.
/// The lightning's flash lights the wall's layer, and the string with it.
pub(super) const LIGHTS_LAYER: i32 = BACKGROUND_LAYER;

/// A swag's width and depth, in frame widths.
const SWAG_WIDTH: f32 = 7.0;
const SWAG_SAG: f32 = 1.4;
/// The most swags a very wide room gets, which bounds the bulbs.
pub(super) const MAX_SWAGS: usize = 16;
pub(super) const BULBS_PER_SWAG: usize = 3;
/// How far under the canopy the string is pinned to the wall, in frame
/// widths, and never less than a clip's width.
const PIN_DROP: f32 = 0.35;
/// Straight pieces in each swag's curve.
const WIRE_PIECES: usize = 8;
const WIRE_HALF: f32 = 0.07;
const WIRE_COLOR: RenderColor = RenderColor::rgb(0.10, 0.25, 0.12);
/// A bulb's socket, and the bulb hanging from it.
const SOCKET_HALF: f32 = 0.13;
const SOCKET_HEIGHT: f32 = 0.28;
const SOCKET_COLOR: RenderColor = RenderColor::rgb(0.13, 0.30, 0.15);
const BULB_HALF_WIDTH: f32 = 0.27;
const BULB_HALF_HEIGHT: f32 = 0.40;
/// The hot middle of a lit bulb: its size against the bulb's, and how far
/// its color goes toward white.
const CORE_SCALE: f32 = 0.5;
const CORE_WHITE: f32 = 0.6;
const BULB_POINTS: usize = 10;
/// The steady glow around each bulb, nested so it fades out in steps: radius
/// in frame widths, and alpha.
const HALOS: [(f32, f32); 3] = [(1.3, 0.08), (0.9, 0.08), (0.55, 0.08)];
/// The bulbs' colors, in order along the string.
pub(super) const BULB_COLORS: [RenderColor; 4] = [
    RenderColor::rgb(1.0, 0.20, 0.16),
    RenderColor::rgb(1.0, 0.72, 0.22),
    RenderColor::rgb(0.22, 0.88, 0.34),
    RenderColor::rgb(0.28, 0.50, 1.0),
];

/// Where the string hangs: its pins along the wall and each bulb's center,
/// both decided by the room's width alone.
pub(super) struct Hang {
    pub pins: Vec<Vec2>,
    pub sag: f32,
    pub bulbs: Vec<Vec2>,
}

/// The string is tied to each side rail and pinned to the wall between swags,
/// a little under the canopy. It never moves, and it never hangs from the
/// canopy, where the spider's hatch and the ceiling's openings come and go.
pub(super) fn hang(layout: Layout) -> Hang {
    let unit = layout.frame_width;
    let left = layout.bounds_min.x + unit;
    let right = layout.bounds_max.x - unit;
    let swags = ((right - left) / (unit * SWAG_WIDTH))
        .round()
        .clamp(2.0, MAX_SWAGS as f32) as usize;
    let width = (right - left) / swags as f32;
    let sag = unit * SWAG_SAG;
    let y = layout.canopy_y - (unit * PIN_DROP).max(clip_radius(layout) * 2.0);
    let pins: Vec<Vec2> = (0..=swags)
        .map(|i| Vec2::new(left + width * i as f32, y))
        .collect();
    let drop = Vec2::new(0.0, -unit * (SOCKET_HEIGHT + BULB_HALF_HEIGHT));
    let bulbs = pins
        .windows(2)
        .flat_map(|pair| {
            (0..BULBS_PER_SWAG).map(move |i| {
                let t = (i as f32 + 0.5) / BULBS_PER_SWAG as f32;
                wire_at(pair[0], pair[1], sag, t) + drop
            })
        })
        .collect();
    Hang { pins, sag, bulbs }
}

/// Half the wire's width, never thinner than a pixel at the smallest scale.
pub(super) fn wire_half(layout: Layout) -> f32 {
    (layout.frame_width * WIRE_HALF).max(0.5)
}

/// A clip pinning the string between swags is twice the wire's width.
fn clip_radius(layout: Layout) -> f32 {
    wire_half(layout) * 2.0
}

/// The point `t` of the way along the swag from `from` to `to`.
fn wire_at(from: Vec2, to: Vec2, sag: f32, t: f32) -> Vec2 {
    from + (to - from) * t - Vec2::new(0.0, sag * 4.0 * t * (1.0 - t))
}

/// Hangs the string at `opacity`, from 0 for none to 1 for all of it, so a
/// change of theme fades it in or out (#457). Its shape is the same at every
/// opacity.
pub(super) fn render(frame: &mut RenderFrame, layout: Layout, opacity: f32) {
    let unit = layout.frame_width;
    let hang = hang(layout);
    let faded = |color: RenderColor| RenderColor {
        a: color.a * opacity,
        ..color
    };
    for pair in hang.pins.windows(2) {
        let path: Vec<Vec2> = (0..=WIRE_PIECES)
            .map(|i| wire_at(pair[0], pair[1], hang.sag, i as f32 / WIRE_PIECES as f32))
            .collect();
        frame.push_primitive(
            LIGHTS_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                strip(&path, wire_half(layout)),
                faded(WIRE_COLOR),
            )),
        );
    }
    // A clip pins it between swags. Its ends are tied to the rails.
    for &pin in &hang.pins[1..hang.pins.len() - 1] {
        frame.push_primitive(
            LIGHTS_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                point(pin),
                clip_radius(layout),
                faded(SOCKET_COLOR),
            )),
        );
    }
    for (index, &center) in hang.bulbs.iter().enumerate() {
        let color = BULB_COLORS[index % BULB_COLORS.len()];
        for (radius, alpha) in HALOS {
            frame.push_primitive(
                LIGHTS_LAYER,
                RenderPrimitive::Circle(RenderCircle::filled(
                    point(center),
                    unit * radius,
                    faded(RenderColor { a: alpha, ..color }),
                )),
            );
        }
        let top = center.y + unit * BULB_HALF_HEIGHT;
        let half = unit * SOCKET_HALF;
        frame.push_primitive(
            LIGHTS_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(center.x - half, top),
                    RenderPoint::new(center.x + half, top),
                    RenderPoint::new(center.x + half, top + unit * SOCKET_HEIGHT),
                    RenderPoint::new(center.x - half, top + unit * SOCKET_HEIGHT),
                ],
                faded(SOCKET_COLOR),
            )),
        );
        let white = |c: f32| c + (1.0 - c) * CORE_WHITE;
        for (scale, color) in [
            (1.0, color),
            (
                CORE_SCALE,
                RenderColor::rgb(white(color.r), white(color.g), white(color.b)),
            ),
        ] {
            frame.push_primitive(
                LIGHTS_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    (0..BULB_POINTS)
                        .map(|i| {
                            let angle = TAU * i as f32 / BULB_POINTS as f32;
                            point(
                                center
                                    + Vec2::new(
                                        angle.cos() * unit * BULB_HALF_WIDTH * scale,
                                        angle.sin() * unit * BULB_HALF_HEIGHT * scale,
                                    ),
                            )
                        })
                        .collect(),
                    faded(color),
                )),
            );
        }
    }
}

/// A band `half` either side of `path`, which bends gently enough that its
/// two edges never cross.
fn strip(path: &[Vec2], half: f32) -> Vec<RenderPoint> {
    let normal = |i: usize| {
        let along = path[(i + 1).min(path.len() - 1)] - path[i.saturating_sub(1)];
        Vec2::new(-along.y, along.x) / along.length()
    };
    let upper = (0..path.len()).map(|i| point(path[i] + normal(i) * half));
    let lower = (0..path.len())
        .rev()
        .map(|i| point(path[i] - normal(i) * half));
    upper.chain(lower).collect()
}

fn point(p: Vec2) -> RenderPoint {
    RenderPoint::new(p.x, p.y)
}
