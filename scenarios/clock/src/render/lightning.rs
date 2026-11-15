//! Lightning: the bolt down from the canopy, the glow where it struck and the
//! sparks it threw, and the flash that lights the room. The flash lights, in
//! place, the opaque things the room is made of: the wall, the floor, the
//! frame, the unlit cells and the creatures, each layer by its own gain. Lit
//! cells, glows, shadows, water and text keep the light they have.

use super::*;
use crate::rain::RainEvent;
use crate::rain::lightning::Strike;
use frame::{DOOR_LAYER, FRAME_LAYER};

#[cfg(test)]
mod tests;

/// The bolt draws over the face and its water, under the creatures, the
/// frame and the lamps.
pub(crate) const BOLT_LAYER: i32 = 7;

const CORE_COLOR: RenderColor = RenderColor::rgb(0.94, 0.96, 1.0);
const GLOW_COLOR: RenderColor = RenderColor::rgb(0.55, 0.66, 1.0);
const SPOT_COLOR: RenderColor = RenderColor::rgb(0.78, 0.85, 1.0);
/// A spark leaves white-hot and cools to orange.
const SPARK_HOT: RenderColor = RenderColor::rgb(1.0, 0.97, 0.84);
const SPARK_COOL: RenderColor = RenderColor::rgb(1.0, 0.58, 0.18);
/// The channel's core, its glow and the fainter, wider glow around that:
/// half the width of each in cells, at most a fraction of the step it is
/// drawn along, and how opaque at full light. The wide glow follows the
/// channel two steps at a time, so it can be wider and still never fold
/// over itself at a bend. A fork has a core and a glow, which start
/// `FORK_WIDTH` as wide and narrow to nothing at its tip.
const CORE_HALF: f32 = 0.055;
const CORE_STEP: f32 = 0.2;
const GLOW_HALF: f32 = 0.17;
const GLOW_STEP: f32 = 0.4;
const GLOW_ALPHA: f32 = 0.32;
const HAZE_HALF: f32 = 0.42;
const HAZE_STEP: f32 = 0.4;
const HAZE_ALPHA: f32 = 0.14;
const FORK_WIDTH: f32 = 0.6;
/// The soft light where the bolt leaves the canopy and where it strikes:
/// discs laid over one another, so it fades out without an edge. Their radii
/// in cells, and how opaque each is at full light.
const SPOT_RADII: [f32; 5] = [0.12, 0.25, 0.42, 0.64, 0.92];
const SPOT_ALPHA: f32 = 0.1;
/// A spark is a short streak: how far behind it its tail trails and how far
/// ahead its head runs, in seconds of its flight, and half its width, in
/// cells.
const SPARK_TAIL: f32 = 0.045;
const SPARK_HEAD: f32 = 0.025;
const SPARK_HALF: f32 = 0.07;

/// The flash's light, and how far toward it each layer's opaque things go
/// at full flash.
const FLASH_COLOR: RenderColor = RenderColor::rgb(0.80, 0.86, 1.0);
const FLASH_LAYERS: [(i32, f32); 6] = [
    (BACKGROUND_LAYER, 0.30),
    (ARENA_LAYER, 0.26),
    (INACTIVE_CELL_LAYER, 0.30),
    (CREATURE_LAYER, 0.30),
    (FRAME_LAYER, 0.26),
    (DOOR_LAYER, 0.26),
];

/// Draws the shower's latest strike while it shows: its bolt while the
/// channel glows, the glow where it struck, and its sparks while they live.
pub(super) fn bolt(frame: &mut RenderFrame, event: &RainEvent) {
    let Some(strike) = event.storm().latest() else {
        return;
    };
    let Some(age) = event.tick.checked_sub(strike.tick) else {
        return;
    };
    let layout = event.layout;
    let light = strike.channel(event.tick);
    if light > 0.0 {
        channel(frame, strike, light, layout.pitch);
        for point in [strike.top(), strike.ground()] {
            for radius in SPOT_RADII {
                frame.push_primitive(
                    BOLT_LAYER,
                    RenderPrimitive::Circle(RenderCircle::filled(
                        RenderPoint::new(point.x, point.y),
                        radius * layout.pitch,
                        RenderColor {
                            a: SPOT_ALPHA * light,
                            ..SPOT_COLOR
                        },
                    )),
                );
            }
        }
    }
    sparks(frame, strike, age, &layout);
}

/// The channel and its forks: the wide glow, then every glow, then every
/// core, so no glow dims a core.
fn channel(frame: &mut RenderFrame, strike: &Strike, light: f32, pitch: f32) {
    let step = strike.bolt.step;
    let path = &strike.bolt.channel;
    let mut haze: Vec<Vec2> = path.iter().step_by(2).copied().collect();
    if haze.last() != path.last() {
        // Close on the end over three steps rather than one.
        haze.pop();
        haze.extend(path.last());
    }
    let half = (HAZE_HALF * pitch).min(HAZE_STEP * 2.0 * step);
    frame.push_primitive(
        BOLT_LAYER,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            ribbon(&haze, |_| half),
            RenderColor {
                a: HAZE_ALPHA * light,
                ..GLOW_COLOR
            },
        )),
    );
    let glow = (GLOW_HALF * pitch).min(GLOW_STEP * step);
    let core = (CORE_HALF * pitch).min(CORE_STEP * step);
    for (half, color) in [
        (
            glow,
            RenderColor {
                a: GLOW_ALPHA * light,
                ..GLOW_COLOR
            },
        ),
        (
            core,
            RenderColor {
                a: light.min(1.0),
                ..CORE_COLOR
            },
        ),
    ] {
        frame.push_primitive(
            BOLT_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                ribbon(&strike.bolt.channel, |_| half),
                color,
            )),
        );
        for fork in &strike.bolt.forks {
            let last = (fork.len() - 1) as f32;
            frame.push_primitive(
                BOLT_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    ribbon(fork, |point| {
                        half * FORK_WIDTH * (1.0 - point as f32 / last)
                    }),
                    color,
                )),
            );
        }
    }
}

/// The sparks still alive `age` ticks after the strike, and not yet fallen
/// below the floor, each a short streak along its flight that fades out.
fn sparks(frame: &mut RenderFrame, strike: &Strike, age: u64, layout: &Layout) {
    let ground = strike.ground();
    let seconds = age as f32 / crate::FIXED_HZ as f32;
    for spark in &strike.sparks {
        if age >= u64::from(spark.life) {
            continue;
        }
        let head = ground + spark.offset(seconds + SPARK_HEAD) * layout.pitch;
        let tail = ground + spark.offset((seconds - SPARK_TAIL).max(0.0)) * layout.pitch;
        if head.y < layout.floor_y {
            continue;
        }
        // Bright for most of its life, then gone quickly.
        let lived = age as f32 / spark.life as f32;
        let cool = |hot: f32, cool: f32| hot + (cool - hot) * lived;
        frame.push_primitive(
            BOLT_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                ribbon(&[tail, head], |_| SPARK_HALF * layout.pitch),
                RenderColor::rgba(
                    cool(SPARK_HOT.r, SPARK_COOL.r),
                    cool(SPARK_HOT.g, SPARK_COOL.g),
                    cool(SPARK_HOT.b, SPARK_COOL.b),
                    1.0 - lived * lived,
                ),
            )),
        );
    }
}

/// A filled strip along `path`, `half(i)` either side of its `i`th point,
/// mitred at each bend along the bisector, but never more than twice as
/// far. A strip along a path that drops at every step, as a bolt's paths
/// do, and narrower than half a step never folds over itself.
pub(crate) fn ribbon(path: &[Vec2], half: impl Fn(usize) -> f32) -> Vec<RenderPoint> {
    let normal = |from: Vec2, to: Vec2| {
        let along = (to - from).normalized();
        Vec2::new(-along.y, along.x)
    };
    let last = path.len() - 1;
    let mut left = Vec::with_capacity(path.len());
    let mut right = Vec::with_capacity(path.len());
    for (index, &point) in path.iter().enumerate() {
        let before = (index > 0).then(|| normal(path[index - 1], point));
        let after = (index < last).then(|| normal(point, path[index + 1]));
        let offset = match (before, after) {
            (Some(before), Some(after)) => {
                let bisector = (before + after).normalized();
                bisector * (1.0 / bisector.dot(before).max(0.5))
            }
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => Vec2::ZERO,
        };
        let half = half(index);
        left.push(point + offset * half);
        right.push(point - offset * half);
    }
    left.into_iter()
        .chain(right.into_iter().rev())
        .map(|point| RenderPoint::new(point.x, point.y))
        .collect()
}

/// Lights the room by the flash, `level` 1 at full: the opaque things on
/// each of `FLASH_LAYERS` go that layer's gain of the way toward the flash's
/// light.
pub(super) fn flash(frame: &mut RenderFrame, level: f32) {
    if level <= 0.0 {
        return;
    }
    for layer in &mut frame.layers {
        let Some(&(_, gain)) = FLASH_LAYERS.iter().find(|(z, _)| *z == layer.z) else {
            continue;
        };
        for primitive in &mut layer.primitives {
            light(primitive, gain * level);
        }
    }
}

/// Takes the opaque fill and stroke of `primitive` `amount` of the way to
/// the flash's light. Translucent ones are glows and shadows, which a flash
/// would wash out rather than light.
fn light(primitive: &mut RenderPrimitive, amount: f32) {
    let toward = |color: &mut RenderColor| {
        if color.a < 1.0 {
            return;
        }
        color.r += (FLASH_COLOR.r - color.r) * amount;
        color.g += (FLASH_COLOR.g - color.g) * amount;
        color.b += (FLASH_COLOR.b - color.b) * amount;
    };
    let (fill, stroke) = match primitive {
        RenderPrimitive::Circle(circle) => (circle.fill.as_mut(), circle.stroke.as_mut()),
        RenderPrimitive::Polygon(polygon) => (polygon.fill.as_mut(), polygon.stroke.as_mut()),
        RenderPrimitive::Line(line) => (None, Some(&mut line.stroke)),
        RenderPrimitive::Text(_) => (None, None),
    };
    if let Some(fill) = fill {
        toward(&mut fill.color);
    }
    if let Some(stroke) = stroke {
        toward(&mut stroke.color);
    }
}
