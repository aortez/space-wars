//! The two corner lamps: a small brass head where the rails meet, a warm
//! bulb, the glow it spreads around its corner and the softer light it
//! throws down into the room. A mood dims or darkens all of these, or
//! gathers the thrown light into a spotlight's beam, which lays a pool of
//! light on the face and brightens the cells inside it, lit or not.

use super::*;
use crate::lamps::Look;
use crate::presentation::Bounds;
use std::f32::consts::{FRAC_PI_2, PI};

/// The lamp heads and their glow sit over the room and the frame.
const LAMP_LAYER: i32 = 11;
/// The spotlight's light on the cells: above the cells and the labels,
/// below visitors and the frame.
const LIT_LAYER: i32 = 7;

/// How far from straight down a steady lamp's light leans into the room.
const REST_TILT: f32 = 0.6;
/// How far a steady lamp's light falls, as a fraction of the camera's height.
const THROW_REACH: f32 = 0.30;
const THROW_DISCS: usize = 7;
/// A beam opening out of the steady light starts this wide, either side.
const OPEN_SPREAD: f32 = 0.45;
const BEAM_SHELLS: usize = 6;
const BEAM_ARC_POINTS: usize = 10;
/// The spotlight's pool, in face cells.
const POOL_RADIUS_CELLS: f32 = 3.6;
const POOL_DISCS: usize = 8;
const LIGHT_COLOR: RenderColor = RenderColor::rgb(1.0, 0.76, 0.42);

/// One lamp's light this tick, in world units.
struct Light {
    center: Vec2,
    brightness: f32,
    /// The way the head points, counterclockwise from +x.
    aim: f32,
    /// The steady light thrown down into the room: 1 is steady.
    throw: f32,
    /// The spotlight's beam: half the angle it spreads over, how far it
    /// reaches and its strength, 0 while the lamp is steady.
    spread: f32,
    reach: f32,
    beam: f32,
    /// The glow around the corner: 1 is steady.
    halo: f32,
    pool: Option<Pool>,
}

#[derive(Debug, Clone, Copy)]
struct Pool {
    center: Vec2,
    radius: f32,
    strength: f32,
}

impl Light {
    fn new(lamp: usize, look: Look, layout: Layout) -> Self {
        // Nestle the heads just inside the junction of the two rails.
        let inset = layout.frame_width * 1.3;
        let (x, inward) = if lamp == 0 {
            (layout.bounds_min.x + inset, 1.0)
        } else {
            (layout.bounds_max.x - inset, -1.0)
        };
        let center = Vec2::new(x, layout.bounds_max.y - inset);
        let narrow = look.narrow.clamp(0.0, 1.0);
        let brightness = look.brightness.max(0.0);
        let target =
            Vec2::new(layout.face_origin.x, layout.face_origin.y) + look.target * layout.pitch;
        let toward = target - center;
        let distance = toward.length().max(1.0);
        let radius = layout.pitch * POOL_RADIUS_CELLS;
        let mix = |steady: f32, spotlight: f32| steady + (spotlight - steady) * narrow;
        Self {
            center,
            brightness,
            aim: mix(-FRAC_PI_2 + inward * REST_TILT, toward.y.atan2(toward.x)),
            // The steady light gathers into the beam, which narrows until
            // it is just wide enough to hold its pool.
            throw: brightness * (1.0 - narrow),
            spread: mix(OPEN_SPREAD, (radius * 0.85 / distance).atan()),
            reach: mix(CAMERA_HEIGHT * THROW_REACH, distance + radius * 0.2),
            beam: brightness * narrow,
            // The light drawn into the beam leaves the corner dimmer.
            halo: brightness * (1.0 - 0.6 * narrow),
            pool: (narrow > 0.0).then_some(Pool {
                center: target,
                radius,
                strength: brightness.min(1.0) * narrow * narrow,
            }),
        }
    }
}

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    for lamp in 0..2 {
        let light = Light::new(lamp, state.lamps.look(lamp), layout);
        throw(frame, &light);
        beam(frame, &light, layout);
        if let Some(pool) = light.pool {
            pool_of_light(frame, pool);
        }
        head(frame, &light, layout);
    }
}

/// Brightens whatever cells a spotlight's pool reaches: lit cells and
/// labels toward warm white, the dark guides just enough to show.
pub(super) fn illuminate(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let pools: Vec<Pool> = (0..2)
        .filter_map(|lamp| Light::new(lamp, state.lamps.look(lamp), layout).pool)
        .filter(|pool| pool.strength > 0.0)
        .collect();
    if pools.is_empty() {
        return;
    }
    let mut lit = Vec::new();
    for layer in &frame.layers {
        let gain = match layer.z {
            ACTIVE_CELL_LAYER | LABEL_LAYER => 0.42,
            INACTIVE_CELL_LAYER => 0.22,
            _ => continue,
        };
        for primitive in &layer.primitives {
            let RenderPrimitive::Polygon(polygon) = primitive else {
                continue;
            };
            let Some(fill) = polygon.fill.as_ref().filter(|fill| fill.color.a > 0.0) else {
                continue;
            };
            if polygon.points.is_empty() {
                continue;
            }
            let centroid = polygon
                .points
                .iter()
                .fold(Vec2::ZERO, |sum, p| sum + Vec2::new(p.x, p.y))
                / polygon.points.len() as f32;
            let light: f32 = pools
                .iter()
                .map(|pool| {
                    pool.strength * falloff(centroid.distance_to(pool.center) / pool.radius)
                })
                .sum();
            if light > 0.0 {
                lit.push(RenderPolygon::filled(
                    polygon.points.clone(),
                    RenderColor::rgba(1.0, 0.93, 0.78, (gain * light * fill.color.a).min(0.85)),
                ));
            }
        }
    }
    for polygon in lit {
        frame.push_primitive(LIT_LAYER, RenderPrimitive::Polygon(polygon));
    }
}

/// Full light over the pool's inner third, fading to nothing at its rim.
fn falloff(distance: f32) -> f32 {
    let t = ((distance - 0.35) / 0.65).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The steady light falling from the lamp: overlapping discs along its
/// aim, small and close together near the bulb, wider and fainter as the
/// light spreads, so it has no edge to speak of. Behind the cells, like the
/// ceiling's sunlight.
fn throw(frame: &mut RenderFrame, light: &Light) {
    if light.throw <= 0.0 {
        return;
    }
    let reach = CAMERA_HEIGHT * THROW_REACH;
    let along = Vec2::new(light.aim.cos(), light.aim.sin());
    for disc in 0..THROW_DISCS {
        let t = (disc as f32 + 0.5) / THROW_DISCS as f32;
        let center = light.center + along * (reach * t);
        frame.push_primitive(
            GLOW_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(center.x, center.y),
                reach * (0.12 + 0.30 * t),
                RenderColor {
                    a: 0.016 * (1.0 - 0.55 * t) * light.throw,
                    ..LIGHT_COLOR
                },
            )),
        );
    }
}

/// The spotlight's beam: nested fans, each narrower and shorter than the
/// last, soften its edges and fade it with distance from the bulb.
fn beam(frame: &mut RenderFrame, light: &Light, layout: Layout) {
    let alpha = 0.042 * light.beam;
    if alpha <= 0.0 {
        return;
    }
    let bounds = Bounds {
        min: Vec2::new(layout.bounds_min.x, layout.bounds_min.y),
        max: Vec2::new(layout.bounds_max.x, layout.bounds_max.y),
    };
    for shell in 0..BEAM_SHELLS {
        let f = shell as f32 / (BEAM_SHELLS - 1) as f32;
        let spread = light.spread * (1.0 - 0.6 * f);
        let reach = light.reach * (1.0 - 0.45 * f);
        let mut fan = Vec::with_capacity(BEAM_ARC_POINTS + 2);
        fan.push(light.center);
        for k in 0..=BEAM_ARC_POINTS {
            let angle = light.aim - spread + 2.0 * spread * k as f32 / BEAM_ARC_POINTS as f32;
            fan.push(light.center + Vec2::new(angle.cos(), angle.sin()) * reach);
        }
        let fan = clip(&fan, bounds);
        if fan.len() >= 3 {
            frame.push_primitive(
                GLOW_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    fan.into_iter()
                        .map(|p| RenderPoint::new(p.x, p.y))
                        .collect(),
                    RenderColor {
                        a: alpha,
                        ..LIGHT_COLOR
                    },
                )),
            );
        }
    }
}

/// Where a spotlight lands: the wall behind the face takes a soft warm disc.
fn pool_of_light(frame: &mut RenderFrame, pool: Pool) {
    let alpha = 0.022 * pool.strength;
    if alpha <= 0.0 {
        return;
    }
    let center = RenderPoint::new(pool.center.x, pool.center.y);
    for disc in 0..POOL_DISCS {
        frame.push_primitive(
            GLOW_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                center,
                pool.radius * (1.0 - 0.8 * disc as f32 / POOL_DISCS as f32),
                RenderColor::rgba(1.0, 0.82, 0.55, alpha),
            )),
        );
    }
}

fn head(frame: &mut RenderFrame, light: &Light, layout: Layout) {
    let width = layout.frame_width;
    let center = RenderPoint::new(light.center.x, light.center.y);
    let lit = light.brightness.min(1.0);
    // The glow spread around the corner.
    if light.halo > 0.0 {
        let radius = ((layout.bounds_max.x - layout.bounds_min.x) * 0.22).min(96.0);
        for shell in (1..=12).rev() {
            frame.push_primitive(
                LAMP_LAYER,
                RenderPrimitive::Circle(RenderCircle::filled(
                    center,
                    radius * shell as f32 / 12.0,
                    RenderColor::rgba(1.0, 0.55, 0.20, 0.012 * light.halo),
                )),
            );
        }
    }
    let bulb = light.center + Vec2::new(light.aim.cos(), light.aim.sin()) * (width * 0.16);
    let bulb = RenderPoint::new(bulb.x, bulb.y);
    // A soft bloom, so the bulb reads as lit at the device's scale.
    if light.brightness > 0.0 {
        for (radius, alpha) in [(2.6, 0.04), (1.6, 0.075), (0.9, 0.15)] {
            frame.push_primitive(
                LAMP_LAYER,
                RenderPrimitive::Circle(RenderCircle::filled(
                    bulb,
                    width * radius,
                    RenderColor::rgba(1.0, 0.72, 0.38, alpha * light.brightness),
                )),
            );
        }
    }
    // The mount where the rails meet, then the hood: the back half of the
    // head, turned the way the light points.
    frame.push_primitive(
        LAMP_LAYER,
        RenderPrimitive::Circle(RenderCircle::filled(
            center,
            width * 0.55,
            RenderColor::rgb(0.20, 0.13, 0.06),
        )),
    );
    let hood = (0..=8)
        .map(|k| {
            let angle = light.aim + FRAC_PI_2 + PI * k as f32 / 8.0;
            let point = light.center + Vec2::new(angle.cos(), angle.sin()) * (width * 0.62);
            RenderPoint::new(point.x, point.y)
        })
        .collect();
    frame.push_primitive(
        LAMP_LAYER,
        RenderPrimitive::Polygon(RenderPolygon {
            points: hood,
            fill: Some(Fill::new(RenderColor::rgb(0.42, 0.29, 0.13))),
            stroke: Some(Stroke::new(
                RenderColor::rgb(0.66, 0.47, 0.22),
                (width * 0.06).max(0.6),
            )),
        }),
    );
    // The glass, from a dark filament up to warm light, and its hot core.
    let [dark, warm] = [[0.24, 0.11, 0.05], [1.0, 0.80, 0.46]];
    let glass = |channel: usize| dark[channel] + (warm[channel] - dark[channel]) * lit;
    frame.push_primitive(
        LAMP_LAYER,
        RenderPrimitive::Circle(RenderCircle::filled(
            bulb,
            width * 0.34,
            RenderColor::rgb(glass(0), glass(1), glass(2)),
        )),
    );
    if lit > 0.0 {
        frame.push_primitive(
            LAMP_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                bulb,
                width * 0.16,
                RenderColor::rgba(1.0, 0.96, 0.84, lit),
            )),
        );
    }
}

/// A convex polygon clipped to the view, so no point of it leaves the
/// camera.
fn clip(points: &[Vec2], bounds: Bounds) -> Vec<Vec2> {
    let mut output = points.to_vec();
    for (axis, edge, keep_greater) in [
        (0, bounds.min.x, true),
        (0, bounds.max.x, false),
        (1, bounds.min.y, true),
        (1, bounds.max.y, false),
    ] {
        let input = std::mem::take(&mut output);
        let Some(&last) = input.last() else {
            break;
        };
        let coord = |p: Vec2| if axis == 0 { p.x } else { p.y };
        let inside = |p: Vec2| {
            if keep_greater {
                coord(p) >= edge
            } else {
                coord(p) <= edge
            }
        };
        let mut previous = last;
        for current in input {
            if inside(current) != inside(previous) {
                let t = (edge - coord(previous)) / (coord(current) - coord(previous));
                let mut crossing = previous + (current - previous) * t;
                if axis == 0 {
                    crossing.x = edge;
                } else {
                    crossing.y = edge;
                }
                output.push(crossing);
            }
            if inside(current) {
                output.push(current);
            }
            previous = current;
        }
    }
    output
}
