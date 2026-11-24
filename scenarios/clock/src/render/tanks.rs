use super::*;
use crate::{
    events::{
        EventPhase, FIXED_HZ,
        tanks::{
            BEAM_DOWN_TICKS, BEAM_UP_TICKS, TanksEvent,
            wreck::{BARREL_WIDTH, Blast, Shape, TURRET_RADIUS},
        },
    },
    presentation::Bounds,
};
use engine_rapier::world::PhysicsWorld;
use std::f32::consts::TAU;

const EARTH: RenderColor = RenderColor::rgb(0.36, 0.27, 0.16);
const GRAIN: RenderColor = RenderColor::rgb(0.69, 0.48, 0.25);
const TEAMS: [RenderColor; 2] = [
    RenderColor::rgb(0.35, 0.85, 0.68),
    RenderColor::rgb(0.98, 0.51, 0.27),
];
const TIRE: RenderColor = RenderColor::rgb(0.08, 0.11, 0.12);
/// A blown-up tank's iron as it cools: white-hot, orange, deep red, then
/// charred.
const GLOW: [RenderColor; 4] = [
    RenderColor::rgb(0.24, 0.22, 0.2),
    RenderColor::rgb(0.62, 0.16, 0.06),
    RenderColor::rgb(1.0, 0.5, 0.12),
    RenderColor::rgb(1.0, 0.9, 0.55),
];
/// Grey enough to show against the night behind the hills.
const SMOKE: RenderColor = RenderColor::rgb(0.46, 0.44, 0.42);
/// How long a burst of fire lasts over a blast.
const FIREBALL_TICKS: u64 = 45;
/// The puffs of smoke a blast sends up, one every few ticks, and how long
/// each lasts as it rises, spreads and thins.
const PLUME_PUFFS: u64 = 14;
const PLUME_EVERY: u64 = 9;
const PLUME_LIFE: u64 = 150;
/// How long the smoke a burning piece leaves behind it lasts.
const TRAIL_LIFE: u64 = 60;

fn mix(a: RenderColor, b: RenderColor, t: f32) -> RenderColor {
    RenderColor::rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// The color of iron as hot as `heat`, from none to as hot as it gets.
fn glow(heat: f32) -> RenderColor {
    let stops = [0.0, 0.4, 0.75, 1.0];
    let index = stops[1..]
        .iter()
        .position(|&stop| heat <= stop)
        .unwrap_or(2);
    let t = (heat - stops[index]) / (stops[index + 1] - stops[index]);
    mix(GLOW[index], GLOW[index + 1], t.clamp(0.0, 1.0))
}

fn faded(mut color: RenderColor, alpha: f32) -> RenderColor {
    color.a = alpha;
    color
}

/// A number in [0, 1) that `pattern` and `index` always give, so each blast
/// keeps its own fireball and smoke from frame to frame.
fn unit(pattern: u64, index: u64) -> f32 {
    let mut x = pattern ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn polygon(
    frame: &mut RenderFrame,
    layer: i32,
    mut points: Vec<Vec2>,
    bounds: Bounds,
    color: RenderColor,
) {
    for (axis, edge, greater) in [
        (0, bounds.min.x, true),
        (0, bounds.max.x, false),
        (1, bounds.min.y, true),
        (1, bounds.max.y, false),
    ] {
        if points.is_empty() {
            return;
        }
        let coordinate = |p: Vec2| if axis == 0 { p.x } else { p.y };
        let inside = |p| {
            if greater {
                coordinate(p) >= edge
            } else {
                coordinate(p) <= edge
            }
        };
        let mut out = Vec::with_capacity(points.len() + 1);
        let mut previous = points[points.len() - 1];
        for point in points {
            if inside(previous) != inside(point) {
                let t = (edge - coordinate(previous)) / (coordinate(point) - coordinate(previous));
                out.push(previous + (point - previous) * t);
            }
            if inside(point) {
                out.push(point);
            }
            previous = point;
        }
        points = out;
    }
    if points.len() >= 3 {
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points
                    .into_iter()
                    .map(|p| RenderPoint::new(p.x, p.y))
                    .collect(),
                color,
            )),
        );
    }
}

fn disc(
    frame: &mut RenderFrame,
    layer: i32,
    center: Vec2,
    radius: f32,
    bounds: Bounds,
    color: RenderColor,
) {
    polygon(
        frame,
        layer,
        (0..16)
            .map(|i| center + Vec2::from_radians(i as f32 * std::f32::consts::TAU / 16.0) * radius)
            .collect(),
        bounds,
        color,
    );
}

fn strip(
    frame: &mut RenderFrame,
    a: Vec2,
    b: Vec2,
    width: f32,
    bounds: Bounds,
    color: RenderColor,
) {
    let direction = (b - a).normalized();
    let side = Vec2::new(-direction.y, direction.x) * width * 0.5;
    polygon(
        frame,
        ARENA_LAYER,
        vec![a - side, b - side, b + side, a + side],
        bounds,
        color,
    );
}

/// A wheel turned to `angle`: its tire, and its spokes and hub in `color`.
fn wheel(
    frame: &mut RenderFrame,
    center: Vec2,
    angle: f32,
    radius: f32,
    [tire, color]: [RenderColor; 2],
    bounds: Bounds,
) {
    disc(frame, ARENA_LAYER, center, radius, bounds, tire);
    for spoke in 0..3 {
        let direction = Vec2::from_radians(angle + spoke as f32 * TAU / 3.0);
        strip(
            frame,
            center,
            center + direction * 0.8 * radius,
            0.2 * radius,
            bounds,
            color,
        );
    }
    disc(frame, ARENA_LAYER, center, 0.3 * radius, bounds, color);
}

/// The losers blown apart: the smoke rising from each blast and trailing
/// each burning piece, the pieces themselves, glowing while they are hot,
/// and the fire over each blast.
fn wreck(
    frame: &mut RenderFrame,
    event: &TanksEvent,
    world: &PhysicsWorld,
    [bounds, sky]: [Bounds; 2],
) {
    let scale = event.duel.placement().scale;
    let wreck = &event.wreck;
    // The smoke first, so the pieces show through it.
    for blast in &wreck.blasts {
        plume(frame, blast, event.tick, scale, bounds);
    }
    for puff in &wreck.puffs {
        let age = event.tick - puff.tick;
        if age >= TRAIL_LIFE {
            continue;
        }
        let t = age as f32 / TRAIL_LIFE as f32;
        let seconds = age as f32 / FIXED_HZ as f32;
        disc(
            frame,
            ARENA_LAYER,
            puff.point + Vec2::Y * (0.8 * seconds * scale),
            (0.25 + 0.6 * t) * scale,
            bounds,
            faded(SMOKE, 0.35 * (1.0 - t)),
        );
    }
    for piece in &wreck.pieces {
        let motion = world.motion(piece.body).expect("a piece of a tank");
        let heat = wreck.heat(piece.slot, event.tick);
        let color = glow(heat);
        let at = |p: Vec2| motion.position + p.rotate_radians(motion.angle);
        match &piece.shape {
            Shape::Turret {
                radius,
                barrel,
                width,
            } => {
                disc(frame, ARENA_LAYER, motion.position, *radius, bounds, color);
                strip(
                    frame,
                    motion.position,
                    at(Vec2::new(*barrel, 0.0)),
                    *width,
                    bounds,
                    color,
                );
            }
            Shape::Wheel { radius } => wheel(
                frame,
                motion.position,
                motion.angle,
                *radius,
                [mix(TIRE, GLOW[1], heat), color],
                bounds,
            ),
            Shape::Hull { corners } => {
                polygon(frame, ARENA_LAYER, corners.map(at).to_vec(), bounds, color);
            }
        }
    }
    for blast in &wreck.blasts {
        fireball(frame, blast, event.tick, scale, sky);
    }
}

/// The smoke a blast sends up from the wreck, each puff rising and spreading
/// as it thins, the whole plume leaning with the wind.
fn plume(frame: &mut RenderFrame, blast: &Blast, tick: u64, scale: f32, bounds: Bounds) {
    let age = tick - blast.tick;
    let wind = (unit(blast.pattern, 0) - 0.5) * 0.8;
    for puff in 0..PLUME_PUFFS {
        let Some(life) = age.checked_sub(4 + puff * PLUME_EVERY) else {
            break;
        };
        if life >= PLUME_LIFE {
            continue;
        }
        let t = life as f32 / PLUME_LIFE as f32;
        let seconds = life as f32 / FIXED_HZ as f32;
        let rise = 1.1 + 0.6 * unit(blast.pattern, 2 * puff + 1);
        let side = (unit(blast.pattern, 2 * puff + 2) - 0.5) * 1.2;
        disc(
            frame,
            ARENA_LAYER,
            blast.point + Vec2::new(side + wind * seconds, 0.4 + rise * seconds) * scale,
            (0.55 + 1.5 * t) * scale,
            bounds,
            faded(SMOKE, 0.4 * (1.0 - t) * (life as f32 / 10.0).min(1.0)),
        );
    }
}

/// The fire over a blast: a flash, lobes of burning fuel that swell and rise
/// as they burn out, and the white-hot heart of it.
fn fireball(frame: &mut RenderFrame, blast: &Blast, tick: u64, scale: f32, sky: Bounds) {
    let age = tick - blast.tick;
    if age >= FIREBALL_TICKS {
        return;
    }
    let t = age as f32 / FIREBALL_TICKS as f32;
    let seconds = age as f32 / FIXED_HZ as f32;
    if age < 8 {
        let flash = age as f32 / 8.0;
        disc(
            frame,
            SHOT_LAYER,
            blast.point,
            (2.4 + 1.2 * flash) * scale,
            sky,
            RenderColor::rgba(1.0, 0.96, 0.82, 0.8 * (1.0 - flash)),
        );
    }
    for lobe in 0..5 {
        let direction =
            Vec2::from_radians(TAU * (lobe as f32 + unit(blast.pattern, 40 + lobe)) / 5.0);
        let reach = (0.5 + 0.7 * unit(blast.pattern, 50 + lobe)) * (0.4 + t);
        let size = 0.8 + 0.4 * unit(blast.pattern, 60 + lobe);
        let color = mix(
            RenderColor::rgb(1.0, 0.82, 0.36),
            RenderColor::rgb(0.86, 0.26, 0.07),
            t,
        );
        disc(
            frame,
            SHOT_LAYER,
            blast.point + (direction * reach + Vec2::Y * (1.4 * seconds)) * scale,
            (0.6 + 1.3 * t.sqrt()) * size * scale,
            sky,
            faded(color, 0.85 * (1.0 - t).powi(2)),
        );
    }
    if age < 20 {
        let heart = age as f32 / 20.0;
        disc(
            frame,
            SHOT_LAYER,
            blast.point + Vec2::Y * (seconds * scale),
            (1.2 - 0.6 * heart) * scale,
            sky,
            RenderColor::rgba(1.0, 0.95, 0.7, 0.9 * (1.0 - heart)),
        );
    }
}

pub(super) fn render(
    frame: &mut RenderFrame,
    event: &TanksEvent,
    world: &PhysicsWorld,
    layout: Layout,
) {
    // The battlefield stays below the face, and transport uses a clear lane
    // through it. Shells and their bursts fly in front of it, up to the
    // canopy.
    let bounds = Bounds {
        min: Vec2::new(layout.bounds_min.x + layout.frame_width, layout.floor_y),
        max: Vec2::new(
            layout.bounds_max.x - layout.frame_width,
            layout.face_origin.y - 4.0,
        ),
    };
    let sky = Bounds {
        max: Vec2::new(bounds.max.x, layout.canopy_y),
        ..bounds
    };
    let scale = event.duel.placement().scale;
    for field in event.duel.terrain() {
        let motion = world.motion(field.assembly.body()).expect("owned terrain");
        let transform = |p: Vec2| motion.position + p.rotate_radians(motion.angle);
        for chunk in field.geometry.chunks() {
            for rect in &chunk.rectangles {
                let center = rect.local_center(&field.terrain);
                let half = rect.half_extents(&field.terrain);
                polygon(
                    frame,
                    ARENA_LAYER,
                    [
                        Vec2::new(-half.x, -half.y),
                        Vec2::new(half.x, -half.y),
                        half,
                        Vec2::new(-half.x, half.y),
                    ]
                    .map(|p| transform(center + p))
                    .to_vec(),
                    bounds,
                    EARTH,
                );
            }
            for patch in &chunk.polygons {
                polygon(
                    frame,
                    ARENA_LAYER,
                    patch.vertices.iter().map(|p| transform(*p)).collect(),
                    bounds,
                    EARTH,
                );
            }
        }
    }
    for grain in event.duel.grains().chain(event.converted.iter()) {
        let motion = world.motion(grain.body()).expect("owned grain");
        if let Some(vertices) = grain.shape().vertices(grain.radius()) {
            polygon(
                frame,
                ARENA_LAYER,
                vertices
                    .into_iter()
                    .map(|p| motion.position + p.rotate_radians(motion.angle))
                    .collect(),
                bounds,
                GRAIN,
            );
        } else {
            disc(
                frame,
                ARENA_LAYER,
                motion.position,
                grain.radius(),
                bounds,
                GRAIN,
            );
        }
    }
    if event.phase != EventPhase::Raising {
        let tank_bounds = if event.phase == EventPhase::BeamDown {
            Bounds {
                max: Vec2::new(bounds.max.x, layout.bounds_max.y - layout.frame_width),
                ..bounds
            }
        } else {
            bounds
        };
        for (slot, tank) in event
            .duel
            .tanks(world)
            .into_iter()
            .enumerate()
            .filter(|(_, tank)| tank.present)
        {
            let mut color = if tank.health > 0.0 {
                TEAMS[slot]
            } else {
                RenderColor::rgb(0.28, 0.28, 0.25)
            };
            color.a = match event.phase {
                EventPhase::BeamDown => (event.phase_tick as f32 / BEAM_DOWN_TICKS as f32).max(0.1),
                EventPhase::BeamUp => 1.0 - event.phase_tick as f32 / BEAM_UP_TICKS as f32,
                _ => 1.0,
            };
            if matches!(event.phase, EventPhase::BeamDown | EventPhase::BeamUp) {
                let mut beam = TEAMS[slot];
                beam.a = 0.14 + 0.06 * (event.phase_tick as f32 * 0.4).sin().abs();
                let width = 2.1 * scale;
                let x = tank.motion.position.x;
                let bottom = if event.phase == EventPhase::BeamDown {
                    event.landing(slot).y
                } else {
                    tank.motion.position.y
                };
                polygon(
                    frame,
                    ARENA_LAYER,
                    vec![
                        Vec2::new(x - width, bottom),
                        Vec2::new(x + width, bottom),
                        Vec2::new(x + width, tank_bounds.max.y),
                        Vec2::new(x - width, tank_bounds.max.y),
                    ],
                    tank_bounds,
                    beam,
                );
            }
            let local =
                |p: Vec2| tank.motion.position + (p * scale).rotate_radians(tank.motion.angle);
            // Clock's tanks run on sprung wheels: the hull's box, and each
            // wheel where its spring holds it, its spokes turning as it rolls.
            let gear = event
                .duel
                .running_gear(world, slot)
                .expect("a present tank's running gear");
            let half = gear.hull_half_extents;
            polygon(
                frame,
                ARENA_LAYER,
                [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(half.x, -half.y),
                    half,
                    Vec2::new(-half.x, half.y),
                ]
                .map(|p| tank.motion.position + p.rotate_radians(tank.motion.angle))
                .to_vec(),
                tank_bounds,
                color,
            );
            let tire = faded(TIRE, color.a);
            for motion in gear.wheels {
                wheel(
                    frame,
                    motion.position,
                    motion.angle,
                    gear.wheel_radius,
                    [tire, color],
                    tank_bounds,
                );
            }
            let barrel = if event.phase == EventPhase::BeamDown {
                [local(Vec2::Y), local(Vec2::Y * 3.5)]
            } else {
                tank.barrel
            };
            disc(
                frame,
                ARENA_LAYER,
                barrel[0],
                TURRET_RADIUS * scale,
                tank_bounds,
                color,
            );
            strip(
                frame,
                barrel[0],
                barrel[1],
                BARREL_WIDTH * scale,
                tank_bounds,
                color,
            );
        }
    }
    wreck(frame, event, world, [bounds, sky]);
    for shell in event.duel.shells() {
        disc(
            frame,
            SHOT_LAYER,
            shell.position,
            0.24 * scale,
            sky,
            RenderColor::rgb(1.0, 0.91, 0.62),
        );
    }
    for (point, age) in event.duel.flashes().filter(|_| event.result.is_none()) {
        let t = age as f32 / 22.0;
        disc(
            frame,
            SHOT_LAYER,
            point,
            (0.5 + 2.0 * t) * scale,
            sky,
            RenderColor::rgba(1.0, 0.65, 0.18, 0.6 * (1.0 - t)),
        );
    }
}
