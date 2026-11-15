//! The two corner lamps: a small brass head where the rails meet, a warm bulb,
//! the glow it spreads around its corner and the softer light it throws down
//! into the room. A lamp rests at `REST_BRIGHTNESS` of its full light, its bulb
//! an amber glow without the hot core. What the lamp does dims or darkens all
//! of these, or gathers the thrown light into a spotlight's beam. The beam
//! lays a pool of light over the wall and the floor and warms the small
//! things drawn over the pool, the cells lit or not; the mover it follows
//! shades those behind it.

use super::*;
use crate::lamps::light::FloorSample;
use crate::lamps::{Look, REST_BRIGHTNESS};
use crate::presentation::Bounds;
use std::f32::consts::{FRAC_PI_2, PI};

#[cfg(test)]
mod tests;

/// The lamp heads and their glow sit over the room and the frame.
const LAMP_LAYER: i32 = 11;
/// The layers drawn over the pool, which its light warms in place, and how
/// far toward warm white it takes what is on them at full light: the dark
/// guides of unlit cells just enough to show. The pool's own discs light
/// the wall and what is drawn under them.
const LIT_LAYERS: [(i32, f32); 4] = [
    (INACTIVE_CELL_LAYER, 0.22),
    (ACTIVE_CELL_LAYER, 0.42),
    (LABEL_LAYER, 0.42),
    (CREATURE_LAYER, 0.42),
];
const LIT_COLOR: RenderColor = RenderColor::rgb(1.0, 0.93, 0.78);
/// A pool lights a thing whole, by the light at its middle, so it lights
/// only things that reach no farther than `SMALL` of its radius from their
/// middle, and less those that reach past `SMALL_FADE`, so nothing snaps in
/// or out of the light as it grows. Larger things, like Meltdown's water,
/// stay as the scene drew them.
const SMALL: f32 = 0.5;
const SMALL_FADE: f32 = 0.35;
/// The shadow of the mover a spotlight follows, on what the pool warms
/// behind it: how far behind the mover its middle lies and how far it
/// spreads from there, in sizes of the mover, and how much of the light it
/// takes at its deepest.
const SHADOW_OFFSET: f32 = 0.6;
const SHADOW_SPREAD: f32 = 1.3;
const SHADOW_DEPTH: f32 = 0.6;

/// How far a steady lamp's light falls, as a fraction of the camera's height.
const THROW_REACH: f32 = 0.30;
const THROW_DISCS: usize = 7;
const BEAM_SHELLS: usize = 6;
const BEAM_ARC_POINTS: usize = 10;
const POOL_DISCS: usize = 8;
const LIGHT_COLOR: RenderColor = RenderColor::rgb(1.0, 0.76, 0.42);
/// The lamps' light on the floor in full sun: bright on the floor's top and
/// fading a little way down into it, in bands from the top, each down to a
/// depth under the floor line in soil cells, with how opaque it is there.
const FLOOR_BANDS: [(f32, f32); 3] = [(0.75, 0.5), (2.0, 0.2), (4.0, 0.07)];
/// The floor's light is drawn in levels of a sixteenth of full sun, so a
/// stretch in about the same light is drawn as one, and in four steps from
/// one sample to the next, so it fades in and out of a shadow over the half
/// cell between them.
const FLOOR_LEVELS: f32 = 16.0;
const FLOOR_STEPS: usize = 4;
/// How dark a plant is drawn in no light at all, as a share of how it is
/// drawn in full sun.
const PLANT_SHADE: f32 = 0.55;

/// One lamp's light this tick, in world units.
struct Light {
    center: Vec2,
    brightness: f32,
    /// The way the head points, counterclockwise from +x.
    aim: f32,
    /// The light thrown down into the room, as a fraction of full.
    throw: f32,
    /// The spotlight's beam: half the angle it spreads over, how far it
    /// reaches and its strength, 0 while the lamp is steady.
    spread: f32,
    reach: f32,
    beam: f32,
    /// The glow around the corner, as a fraction of full.
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
    /// `output` is the share of their light the lamps give at this time of
    /// day. It dims all the lamp lights, on the scale the eye reads rather
    /// than the plants' (see `lamps::light::seen`).
    fn new(lamp: usize, look: Look, layout: Layout, output: f32) -> Self {
        let beam = crate::lamps::beam(lamp, look, &layout);
        let narrow = beam.narrow;
        let brightness = look.brightness.max(0.0) * crate::lamps::light::seen(output);
        let mix = |steady: f32, spotlight: f32| steady + (spotlight - steady) * narrow;
        Self {
            center: beam.apex,
            brightness,
            aim: beam.aim,
            // The steady light gathers into the beam.
            throw: brightness * (1.0 - narrow),
            spread: beam.spread,
            reach: mix(
                CAMERA_HEIGHT * THROW_REACH,
                beam.distance + beam.pool_radius * 0.2,
            ),
            beam: brightness * narrow,
            // The beam draws on the glow around the corner, which rises
            // less than the bulb as the beam narrows.
            halo: brightness * (1.0 - 0.6 * narrow),
            pool: (narrow > 0.0).then_some(Pool {
                center: beam.target,
                radius: beam.pool_radius,
                strength: brightness.min(1.0) * narrow * narrow,
            }),
        }
    }
}

/// The lamps' light on the floor: warm on the floor's top where it lands and
/// fading a little way down into it, missing where the digits or anything
/// else solid stand between, so a plant in a shadow is seen to be in one. It
/// is as strong as the light the plants are given there (see `lamps::light`),
/// in proportion to full sun, so at night, when the lamps give 3% of their
/// day's light, it all but vanishes.
pub(super) fn floor_light(frame: &mut RenderFrame, samples: &[FloorSample]) {
    let cell = crate::grass::SOIL_CELL_SIZE;
    for stretch in samples.chunk_by(|a, b| a.stretch == b.stretch) {
        let (first, last) = (stretch[0], stretch[stretch.len() - 1]);
        let along = Vec2::new(first.angle.cos(), first.angle.sin());
        let down = Vec2::new(along.y, -along.x);
        // The floor line at the stretch's left end; the samples lie half a
        // soil cell above it.
        let start = first.position - along * (first.width * 0.5) + down * (cell * 0.5);
        let end = (last.position - start).dot(along) + last.width * 0.5;
        let (edges, levels) = floor_pieces(stretch, start, along, end);
        let mut lit: Vec<u8> = levels.iter().copied().filter(|&level| level > 0).collect();
        lit.sort_unstable();
        lit.dedup();
        // Each level is laid over the levels under it wherever the floor gets
        // at least that much light, so no two shapes of it meet edge to edge,
        // where a renderer may fill the edge twice or not at all.
        let mut under = 0;
        for level in lit {
            let mut piece = 0;
            while piece < levels.len() {
                if levels[piece] < level {
                    piece += 1;
                    continue;
                }
                let from = edges[piece];
                while piece < levels.len() && levels[piece] >= level {
                    piece += 1;
                }
                let (left, right) = (start + along * from, start + along * edges[piece]);
                for (band, &(depth, _)) in FLOOR_BANDS.iter().enumerate() {
                    let bottom = down * (cell * depth);
                    // The layers are one color, so each lets through its
                    // share, and together the layers over any point let
                    // through what `floor_clear` gives for its level and
                    // band.
                    let clear = floor_clear(level, band) * floor_clear(under, band + 1)
                        / (floor_clear(under, band) * floor_clear(level, band + 1));
                    frame.push_primitive(
                        ARENA_LAYER,
                        RenderPrimitive::Polygon(RenderPolygon::filled(
                            [left, right, right + bottom, left + bottom]
                                .map(|p| RenderPoint::new(p.x, p.y))
                                .to_vec(),
                            RenderColor {
                                a: 1.0 - clear,
                                ..LIGHT_COLOR
                            },
                        )),
                    );
                }
            }
            under = level;
        }
    }
}

/// How much of what is under it the floor's light at `level` lets through in
/// `band`, counted from the floor's top: all of it past the last band.
fn floor_clear(level: u8, band: usize) -> f32 {
    FLOOR_BANDS.get(band).map_or(1.0, |&(_, alpha)| {
        1.0 - alpha * f32::from(level) / FLOOR_LEVELS
    })
}

/// The light along a stretch of floor in pieces from its left end: the
/// edges between them, as distances from `start` along the stretch out to
/// `end`, and each piece's level. From the middle of one sample to the next
/// it changes in `FLOOR_STEPS` steps, and from each end of the stretch to the
/// middle of the sample there it is that sample's.
fn floor_pieces(
    stretch: &[FloorSample],
    start: Vec2,
    along: Vec2,
    end: f32,
) -> (Vec<f32>, Vec<u8>) {
    let level = |light: f32| (light / engine_core::lighting::FULL_SUN * FLOOR_LEVELS).round() as u8;
    let middle = |sample: &FloorSample| (sample.position - start).dot(along);
    let (first, last) = (&stretch[0], &stretch[stretch.len() - 1]);
    let mut edges = vec![0.0, middle(first)];
    let mut levels = vec![level(first.light)];
    for pair in stretch.windows(2) {
        let (from, to) = (middle(&pair[0]), middle(&pair[1]));
        for step in 0..FLOOR_STEPS {
            let t = (step as f32 + 0.5) / FLOOR_STEPS as f32;
            edges.push(from + (to - from) * (step + 1) as f32 / FLOOR_STEPS as f32);
            levels.push(level(pair[0].light + (pair[1].light - pair[0].light) * t));
        }
    }
    edges.push(end);
    levels.push(level(last.light));
    (edges, levels)
}

/// How a plant that gets `light`, on the plants' scale, is drawn: as `color`
/// in full sun, and darker the less light it gets, on the scale the eye reads.
pub(super) fn lit_plant(color: RenderColor, light: u16) -> RenderColor {
    let seen = crate::lamps::light::seen(f32::from(light) / engine_core::lighting::FULL_SUN);
    let level = PLANT_SHADE + (1.0 - PLANT_SHADE) * seen;
    RenderColor {
        r: color.r * level,
        g: color.g * level,
        b: color.b * level,
        a: color.a,
    }
}

/// A spotlight's pool, and the mover in it while the lamp follows one.
struct Spot {
    pool: Pool,
    caster: Option<Caster>,
}

/// A mover in a spotlight, in world units.
#[derive(Debug, Clone, Copy)]
struct Caster {
    center: Vec2,
    size: f32,
    /// The middle of the shadow it casts, behind it as the lamp sees it.
    shadow: Vec2,
}

impl Spot {
    fn new(lamp: usize, look: Look, layout: Layout, output: f32) -> Option<Self> {
        let light = Light::new(lamp, look, layout, output);
        let pool = light.pool.filter(|pool| pool.strength > 0.0)?;
        let caster = look.subject.map(|subject| {
            let center = Vec2::new(layout.face_origin.x, layout.face_origin.y)
                + subject.position * layout.pitch;
            let size = subject.size * layout.pitch;
            let away = (center - light.center).normalized();
            Caster {
                center,
                size,
                shadow: center + away * (size * SHADOW_OFFSET),
            }
        });
        Some(Self { pool, caster })
    }

    /// How much of the pool's light reaches `point`: none past its rim, and
    /// less in the shadow of the mover in it.
    fn light_at(&self, point: Vec2) -> f32 {
        let light =
            self.pool.strength * falloff(point.distance_to(self.pool.center) / self.pool.radius);
        let shade = self.caster.map_or(0.0, |caster| caster.shade(point));
        light * (1.0 - shade)
    }

    /// How much of the pool's light reaches a thing whose middle is at
    /// `point` and which reaches `extent` from there: as much as reaches
    /// its middle if it is small, and none if it is large.
    fn light_on(&self, point: Vec2, extent: f32) -> f32 {
        let large = (extent / self.pool.radius - SMALL_FADE) / (SMALL - SMALL_FADE);
        self.light_at(point) * (1.0 - smooth(large.clamp(0.0, 1.0)))
    }
}

impl Caster {
    /// How much of the light its shadow takes at `point`: the most behind
    /// its middle, and none on the mover itself.
    fn shade(&self, point: Vec2) -> f32 {
        if point.distance_to(self.center) <= self.size {
            return 0.0;
        }
        let depth = 1.0 - point.distance_to(self.shadow) / (self.size * SHADOW_SPREAD);
        SHADOW_DEPTH * smooth(depth.clamp(0.0, 1.0))
    }
}

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    for lamp in 0..2 {
        let look = state.lamps.look(lamp);
        let light = Light::new(lamp, look, layout, state.lamp_output());
        throw(frame, &light);
        beam(frame, &light, layout);
        if let Some(pool) = light.pool {
            pool_of_light(frame, pool);
        }
        glow(frame, &light, layout);
        bloom(frame, &light, layout);
        head(frame, &light, layout);
        if let Some(tick) = state.lamps.spark_tick(lamp) {
            sparks(frame, &light, layout, tick);
        }
    }
}

fn sparks(frame: &mut RenderFrame, light: &Light, layout: Layout, tick: u32) {
    let width = layout.frame_width;
    let origin = bulb(light, layout);
    let inward = -light.center.x.signum();
    let time = tick as f32 / crate::FIXED_HZ as f32;
    if tick < 6 {
        frame.push_primitive(
            LAMP_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(origin.x, origin.y),
                width * (0.4 + time * 5.0),
                RenderColor::rgba(1.0, 0.87, 0.48, 0.6 * (1.0 - tick as f32 / 6.0)),
            )),
        );
    }
    // A few hot flecks fall inward from the bulb, with short fading trails.
    for (index, (vx, vy)) in [
        (1.6, -0.5),
        (3.0, -1.7),
        (4.7, -0.9),
        (2.0, -3.4),
        (3.8, 0.1),
    ]
    .into_iter()
    .enumerate()
    {
        let life = crate::lamps::SPARK_TICKS as f32 / crate::FIXED_HZ as f32 - index as f32 * 0.025;
        if time >= life {
            continue;
        }
        let position = |t: f32| {
            let p = origin + Vec2::new(inward * vx * t, vy * t - 12.0 * t * t) * width;
            RenderPoint::new(p.x, p.y)
        };
        let alpha = (1.0 - time / life).sqrt();
        for (thickness, color) in [
            (0.25, RenderColor::rgba(1.0, 0.42, 0.08, alpha * 0.25)),
            (0.09, RenderColor::rgba(1.0, 0.91, 0.56, alpha)),
        ] {
            frame.push_primitive(
                LAMP_LAYER,
                RenderPrimitive::Line(engine_common::RenderLine::new(
                    position((time - 0.045).max(0.0)),
                    position(time),
                    Stroke::new(color, (width * thickness).max(0.8)),
                )),
            );
        }
    }
}

/// Warms every small thing drawn over a spotlight's pool, whichever way the
/// scene drew it, as far as the pool's light reaches it. A thing takes the
/// light in place, toward warm white, so it keeps its place in the order the
/// scene drew it and the frame gains nothing.
pub(super) fn illuminate(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let spots: Vec<Spot> = (0..2)
        .filter_map(|lamp| Spot::new(lamp, state.lamps.look(lamp), layout, state.lamp_output()))
        .collect();
    light_scene(frame, &spots);
}

/// Warms what `spots` reach, as `illuminate` does.
fn light_scene(frame: &mut RenderFrame, spots: &[Spot]) {
    let Some(largest) = spots
        .iter()
        .map(|spot| spot.pool.radius * SMALL)
        .reduce(f32::max)
    else {
        return;
    };
    for layer in &mut frame.layers {
        let Some(&(_, gain)) = LIT_LAYERS.iter().find(|(z, _)| *z == layer.z) else {
            continue;
        };
        for primitive in &mut layer.primitives {
            let Some((center, extent)) = footprint(primitive) else {
                continue;
            };
            if extent >= largest {
                continue;
            }
            let light: f32 = spots.iter().map(|spot| spot.light_on(center, extent)).sum();
            if light > 0.0 {
                warm(primitive, (gain * light).min(0.85));
            }
        }
    }
}

/// Where a primitive sits and how far it reaches from there; None for text.
fn footprint(primitive: &RenderPrimitive) -> Option<(Vec2, f32)> {
    let point = |p: &RenderPoint| Vec2::new(p.x, p.y);
    match primitive {
        RenderPrimitive::Circle(circle) => Some((point(&circle.center), circle.radius)),
        RenderPrimitive::Line(line) => {
            let (start, end) = (point(&line.start), point(&line.end));
            Some(((start + end) * 0.5, start.distance_to(end) * 0.5))
        }
        RenderPrimitive::Polygon(polygon) if !polygon.points.is_empty() => {
            let center = polygon
                .points
                .iter()
                .fold(Vec2::ZERO, |sum, p| sum + point(p))
                / polygon.points.len() as f32;
            let extent = polygon
                .points
                .iter()
                .map(|p| center.distance_to(point(p)))
                .fold(0.0, f32::max);
            Some((center, extent))
        }
        RenderPrimitive::Polygon(_) | RenderPrimitive::Text(_) => None,
    }
}

/// Takes a primitive's fill and outline `amount` of the way to warm white,
/// as warm light laid over it would, and leaves how opaque they are.
fn warm(primitive: &mut RenderPrimitive, amount: f32) {
    let toward = |color: &mut RenderColor| {
        color.r += (LIT_COLOR.r - color.r) * amount;
        color.g += (LIT_COLOR.g - color.g) * amount;
        color.b += (LIT_COLOR.b - color.b) * amount;
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

/// Full light over the pool's inner third, fading to nothing at its rim.
fn falloff(distance: f32) -> f32 {
    1.0 - smooth(((distance - 0.35) / 0.65).clamp(0.0, 1.0))
}

/// Eases from 0 to 1 as `t` does, without a corner at either end.
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
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

/// Where the bulb sits in the head, a little forward along its aim.
fn bulb(light: &Light, layout: Layout) -> Vec2 {
    light.center + Vec2::new(light.aim.cos(), light.aim.sin()) * (layout.frame_width * 0.16)
}

/// The glow the lamp spreads around its corner.
fn glow(frame: &mut RenderFrame, light: &Light, layout: Layout) {
    if light.halo <= 0.0 {
        return;
    }
    let center = RenderPoint::new(light.center.x, light.center.y);
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

/// A soft bloom, so the bulb reads as lit at the device's scale.
fn bloom(frame: &mut RenderFrame, light: &Light, layout: Layout) {
    if light.brightness <= 0.0 {
        return;
    }
    let bulb = bulb(light, layout);
    for (radius, alpha) in [(2.6, 0.04), (1.6, 0.075), (0.9, 0.15)] {
        frame.push_primitive(
            LAMP_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(bulb.x, bulb.y),
                layout.frame_width * radius,
                RenderColor::rgba(1.0, 0.72, 0.38, alpha * light.brightness),
            )),
        );
    }
}

/// The head itself: the mount, the hood and the bulb's glass and core.
fn head(frame: &mut RenderFrame, light: &Light, layout: Layout) {
    let width = layout.frame_width;
    let center = RenderPoint::new(light.center.x, light.center.y);
    let bulb = bulb(light, layout);
    let bulb = RenderPoint::new(bulb.x, bulb.y);
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
    frame.push_primitive(
        LAMP_LAYER,
        RenderPrimitive::Circle(RenderCircle::filled(
            bulb,
            width * 0.34,
            glass(light.brightness),
        )),
    );
    // The hot core shows only above rest, and fully at full light.
    let core = ((light.brightness - REST_BRIGHTNESS) / (1.0 - REST_BRIGHTNESS)).min(1.0);
    if core > 0.0 {
        frame.push_primitive(
            LAMP_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                bulb,
                width * 0.16,
                RenderColor::rgba(1.0, 0.96, 0.84, core),
            )),
        );
    }
}

/// The bulb's glass: a dark filament when out, an amber glow at rest and
/// warm light at full.
fn glass(brightness: f32) -> RenderColor {
    const DARK: [f32; 3] = [0.24, 0.11, 0.05];
    const AMBER: [f32; 3] = [0.86, 0.50, 0.20];
    const WARM: [f32; 3] = [1.0, 0.80, 0.46];
    let brightness = brightness.clamp(0.0, 1.0);
    let (from, to, t) = if brightness < REST_BRIGHTNESS {
        (DARK, AMBER, brightness / REST_BRIGHTNESS)
    } else {
        (
            AMBER,
            WARM,
            (brightness - REST_BRIGHTNESS) / (1.0 - REST_BRIGHTNESS),
        )
    };
    let channel = |i: usize| from[i] * (1.0 - t) + to[i] * t;
    RenderColor::rgb(channel(0), channel(1), channel(2))
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
