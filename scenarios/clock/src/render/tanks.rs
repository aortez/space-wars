use super::*;
use crate::{
    events::{
        EventPhase, FIXED_HZ,
        tanks::{
            BEAM_DOWN_TICKS, BEAM_UP_TICKS, TanksEvent,
            survivor::{self, Phase, Survivor},
            wreck::{BARREL_WIDTH, Blast, Puff, Shape, TURRET_RADIUS},
        },
    },
    presentation::Bounds,
};
use engine_common::ClockTanksRebuildStage as Stage;
use engine_rapier::world::{BodyMotion, PhysicsWorld};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

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
/// A spaceling's helmet and legs, and the visor it looks out through.
const SUIT: RenderColor = RenderColor::rgb(0.9, 0.93, 0.95);
const VISOR: RenderColor = RenderColor::rgb(0.09, 0.16, 0.23);
/// An escape pod's shell, and the dark inside its open hatch.
const POD_SHELL: RenderColor = RenderColor::rgb(0.74, 0.77, 0.8);
const POD_INSIDE: RenderColor = RenderColor::rgb(0.12, 0.13, 0.15);
/// A white flag, and the stick it flies from.
const FLAG: RenderColor = RenderColor::rgb(0.98, 0.98, 0.95);
const STICK: RenderColor = RenderColor::rgb(0.6, 0.5, 0.36);
/// A running spaceling's strides a second.
const STRIDE_HZ: f32 = 3.5;
/// A jetpack's flame, and its hot core.
const FLAME: RenderColor = RenderColor::rgba(1.0, 0.55, 0.15, 0.85);
const FLAME_CORE: RenderColor = RenderColor::rgb(1.0, 0.92, 0.6);
/// How long the flash of a shell's burst lasts.
const FLASH_TICKS: u64 = 22;

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
    layer: i32,
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
        layer,
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
            ARENA_LAYER,
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
    trail(frame, ARENA_LAYER, &wreck.puffs, event.tick, scale, bounds);
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
                    ARENA_LAYER,
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

/// The smoke a burning piece, a pod in flight or a jetpack left behind it,
/// on `layer`, each puff rising and spreading as it thins.
fn trail(
    frame: &mut RenderFrame,
    layer: i32,
    puffs: &[Puff],
    tick: u64,
    scale: f32,
    bounds: Bounds,
) {
    for puff in puffs {
        let age = tick - puff.tick;
        if age >= TRAIL_LIFE {
            continue;
        }
        let t = age as f32 / TRAIL_LIFE as f32;
        let seconds = age as f32 / FIXED_HZ as f32;
        disc(
            frame,
            layer,
            puff.point + Vec2::Y * (0.8 * seconds * scale),
            (0.25 + 0.6 * t) * scale,
            bounds,
            faded(SMOKE, 0.35 * (1.0 - t)),
        );
    }
}

/// The losers' spacelings that got out: the smoke behind each pod in
/// flight, the pods, the beams that come down for the spacelings, and the
/// spacelings themselves, standing, tumbling or lying where they landed.
fn survivors(
    frame: &mut RenderFrame,
    event: &TanksEvent,
    world: &PhysicsWorld,
    [bounds, sky]: [Bounds; 2],
) {
    let scale = event.duel.placement().scale;
    let survivors = &event.survivors;
    // A jetpack's smoke rises in front of the face, with the spaceling.
    trail(frame, SHOT_LAYER, &survivors.puffs, event.tick, scale, sky);
    let beaming = event.phase == EventPhase::BeamUp;
    for survivor in &survivors.out {
        let color = TEAMS[survivor.slot];
        // One climbing into its rebuilt tank sinks from view through the
        // hatch.
        let fade = if beaming {
            survivor.fade(event.phase_tick)
        } else {
            survivor.boarded(event.tick)
        };
        if let Some(pod) = survivor.pod {
            let motion = world.motion(pod).expect("a survivor's pod");
            // A pod the beam lifts out sealed fades as it goes.
            let alpha = if survivor.pod_lifted() {
                1.0 - fade
            } else {
                1.0
            };
            escape_pod(
                frame,
                motion,
                survivor.hatch(event.tick),
                [color, faded(POD_SHELL, alpha)],
                scale,
                bounds,
            );
        }
        if beaming && survivor.phase == Phase::BeamedUp {
            rescue_beam(frame, survivor, event.phase_tick, scale, bounds);
        }
        // The winner's beam comes down on a spaceling that surrendered and
        // draws it in, until the winner's own beam takes them up together.
        if !beaming && survivor.phase == Phase::Taken {
            let mut team = TEAMS[1 - survivor.slot];
            team.a = 0.16 + 0.06 * (event.tick as f32 * 0.4).sin().abs();
            narrow_beam(
                frame,
                survivor.position - Vec2::Y * (0.6 * scale),
                (event.tick - survivor.since) as f32 / survivor::TAKE_DESCENT_TICKS as f32,
                team,
                scale,
                bounds,
            );
        }
        let Some(spaceling) = &survivor.spaceling else {
            continue;
        };
        let motion = world
            .motion(spaceling.body())
            .expect("a survivor's spaceling");
        let seconds = (event.tick - survivor.since) as f32 / FIXED_HZ as f32;
        let flag = survivor.flag(event.tick);
        // A dodger runs until it stands clear, and waits there.
        let running = match survivor.phase {
            Phase::Running | Phase::Climbing | Phase::Fleeing | Phase::Returning => true,
            Phase::Dodging => survivor
                .shot
                .as_ref()
                .and_then(|shot| shot.dodge)
                .is_some_and(|dodge| !dodge.stopped),
            _ => false,
        };
        let hopping = survivor.phase == Phase::Hopping;
        let pose = Pose {
            wave: (survivor.phase == Phase::Waving
                || (survivor.phase == Phase::BeamedUp && survivor.stood_up && flag == 0.0))
                .then_some(seconds),
            flag: (flag > 0.0).then(|| (flag, event.tick as f32 / FIXED_HZ as f32)),
            stride: running.then_some(seconds),
            jet: hopping.then_some(seconds),
        };
        // Rising out of view, it passes in front of the face, as the shells
        // do.
        let (layer, bounds) = if hopping {
            (SHOT_LAYER, sky)
        } else {
            (ARENA_LAYER, bounds)
        };
        pilot(
            frame,
            layer,
            motion,
            survivor.facing,
            pose,
            faded(color, 1.0 - fade),
            scale,
            bounds,
        );
    }
}

/// The beam that comes down on `survivor`, `phase_tick` ticks into the
/// beam-up, and lifts it out, in its team's color.
fn rescue_beam(
    frame: &mut RenderFrame,
    survivor: &Survivor,
    phase_tick: u64,
    scale: f32,
    bounds: Bounds,
) {
    let Some((_, from)) = survivor.lifted else {
        return;
    };
    let mut beam = TEAMS[survivor.slot];
    beam.a = 0.16 + 0.06 * (phase_tick as f32 * 0.4).sin().abs();
    narrow_beam(
        frame,
        Vec2::new(survivor.position.x, from.position.y - 0.6 * scale),
        phase_tick as f32 / survivor::RESCUE_DESCENT_TICKS as f32,
        beam,
        scale,
        bounds,
    );
}

/// A beam narrower than a tank's, `descent` of the way down from the top of
/// the view to a spaceling's `feet`.
fn narrow_beam(
    frame: &mut RenderFrame,
    feet: Vec2,
    descent: f32,
    beam: RenderColor,
    scale: f32,
    bounds: Bounds,
) {
    let width = 0.75 * scale;
    let bottom = bounds.max.y - (bounds.max.y - feet.y) * descent.min(1.0);
    polygon(
        frame,
        ARENA_LAYER,
        vec![
            Vec2::new(feet.x - width, bottom),
            Vec2::new(feet.x + width, bottom),
            Vec2::new(feet.x + width, bounds.max.y),
            Vec2::new(feet.x - width, bounds.max.y),
        ],
        bounds,
        beam,
    );
}

/// An escape pod at `motion`, its hatch `open` from shut to wide open: a
/// shell with a band of its team's color and a porthole, and once open the
/// dark inside and the hatch swung back on its hinge.
fn escape_pod(
    frame: &mut RenderFrame,
    motion: BodyMotion,
    open: f32,
    [team, shell]: [RenderColor; 2],
    scale: f32,
    bounds: Bounds,
) {
    let at = |p: Vec2| motion.position + (p * scale).rotate_radians(motion.angle);
    let hinge = survivor::HINGE;
    let [body, hatch]: [Vec<Vec2>; 2] = {
        let (low, high): (Vec<Vec2>, Vec<Vec2>) =
            survivor::POD.iter().partition(|corner| corner.y <= hinge.y);
        [low, high]
    };
    // The lower shell, closed across at the hinge's height.
    polygon(
        frame,
        ARENA_LAYER,
        body.iter().map(|&p| at(p)).collect(),
        bounds,
        shell,
    );
    polygon(
        frame,
        ARENA_LAYER,
        [
            Vec2::new(hinge.x, -0.22),
            Vec2::new(-hinge.x, -0.22),
            Vec2::new(-hinge.x, -0.08),
            Vec2::new(hinge.x, -0.08),
        ]
        .map(at)
        .to_vec(),
        bounds,
        faded(team, shell.a),
    );
    // The dome, with the hinge's corners, swung back about the hinge.
    let mut dome = vec![Vec2::new(-hinge.x, hinge.y)];
    dome.extend(hatch);
    dome.push(hinge);
    if open > 0.0 {
        polygon(
            frame,
            ARENA_LAYER,
            dome.iter().map(|&p| at(p)).collect(),
            bounds,
            POD_INSIDE,
        );
    }
    let swing = open * 1.9;
    let swung = |p: Vec2| hinge + (p - hinge).rotate_radians(swing);
    polygon(
        frame,
        ARENA_LAYER,
        dome.iter().map(|&p| at(swung(p))).collect(),
        bounds,
        shell,
    );
    disc(
        frame,
        ARENA_LAYER,
        at(swung(Vec2::new(0.0, 0.24))),
        0.11 * scale,
        bounds,
        faded(VISOR, shell.a),
    );
}

/// What a spaceling does with its arms and legs: waves, `wave` seconds into
/// a wave; holds up a white flag, from down to all the way up, `flag`
/// seconds into the event; or runs, `stride` seconds into its run.
#[derive(Clone, Copy, Default)]
struct Pose {
    wave: Option<f32>,
    flag: Option<(f32, f32)>,
    stride: Option<f32>,
    /// Seconds into a jetpack hop.
    jet: Option<f32>,
}

/// A spaceling at `motion`, facing `facing`, in a suit of `color`, posed
/// as `pose` says: its legs, its arms, its body, its helmet and its visor,
/// the flag it may hold up, and the jetpack it may rise on. It is drawn in
/// the physics crate's spaceling's proportions, shrunk to Clock's, on
/// `layer`.
#[allow(clippy::too_many_arguments)]
fn pilot(
    frame: &mut RenderFrame,
    layer: i32,
    motion: BodyMotion,
    facing: f32,
    pose: Pose,
    color: RenderColor,
    scale: f32,
    bounds: Bounds,
) {
    let size = survivor::PILOT_SCALE * scale;
    let at =
        |x: f32, y: f32| motion.position + (Vec2::new(x, y) * size).rotate_radians(motion.angle);
    let legs = faded(SUIT, color.a);
    for side in [-1.0, 1.0] {
        // Running, each leg swings forward and back, half a stride behind
        // the other, and lifts its foot as it comes forward.
        let foot = match pose.stride {
            Some(seconds) => {
                let swing = (seconds * TAU * STRIDE_HZ + (side + 1.0) * FRAC_PI_2).sin();
                at(
                    side * 0.06 + facing * 0.3 * swing,
                    -0.8 + 0.15 * swing.max(0.0),
                )
            }
            None => at(side * 0.12, -0.8),
        };
        strip(
            frame,
            layer,
            at(side * 0.1, -0.15),
            foot,
            0.14 * size,
            bounds,
            legs,
        );
        let shoulder = at(side * 0.2, 0.3);
        // An arm's angle from the body's side, out across the way it faces
        // on this side and up from straight down.
        let arm = |lift: f32| {
            let angle = if side > 0.0 { lift } else { PI - lift };
            shoulder + Vec2::from_radians(angle + motion.angle) * (0.5 * size)
        };
        let hand = match (pose.flag, pose.wave, pose.stride) {
            // The arm on the side it faces holds the flag, rising from its
            // side to high over its head.
            (Some((raised, _)), _, _) if side == facing => arm(-1.2 + 2.5 * raised),
            // The arm on the side it faces, raised and waving.
            (_, Some(seconds), _) if side == facing => {
                let swing = 0.35 * (seconds * TAU * 2.0).sin();
                arm(FRAC_PI_2 - 0.45 - swing)
            }
            // Running, the arms swing against the legs.
            (_, _, Some(seconds)) => {
                let swing = (seconds * TAU * STRIDE_HZ + (side + 1.0) * FRAC_PI_2).sin();
                arm(-FRAC_PI_2 - side * facing * 0.6 * swing)
            }
            _ => at(side * 0.36, -0.12),
        };
        strip(frame, layer, shoulder, hand, 0.13 * size, bounds, color);
        if let Some((_, seconds)) = pose.flag
            && side == facing
        {
            white_flag(
                frame,
                layer,
                hand,
                motion.angle,
                facing,
                seconds,
                size,
                bounds,
                color.a,
            );
        }
    }
    // A jetpack on its back, and the flame it rises on, flickering.
    if let Some(seconds) = pose.jet {
        let back = -facing;
        polygon(
            frame,
            layer,
            [(0.24, -0.12), (0.42, -0.12), (0.42, 0.34), (0.24, 0.34)]
                .map(|(x, y)| at(back * x, y))
                .to_vec(),
            bounds,
            faded(POD_SHELL, color.a),
        );
        let flicker = 0.8 + 0.2 * (seconds * TAU * 9.0).sin();
        for (half, length, flame) in [(0.1, 1.4, FLAME), (0.05, 0.7, FLAME_CORE)] {
            polygon(
                frame,
                layer,
                vec![
                    at(back * (0.33 - half), -0.14),
                    at(back * (0.33 + half), -0.14),
                    at(back * 0.33, -0.14 - length * flicker),
                ],
                bounds,
                faded(flame, color.a),
            );
        }
    }
    polygon(
        frame,
        layer,
        [(-0.23, -0.2), (0.23, -0.2), (0.26, 0.4), (-0.26, 0.4)]
            .map(|(x, y)| at(x, y))
            .to_vec(),
        bounds,
        color,
    );
    disc(frame, layer, at(0.0, 0.63), 0.25 * size, bounds, legs);
    disc(
        frame,
        layer,
        at(facing * 0.1, 0.65),
        0.14 * size,
        bounds,
        faded(VISOR, color.a),
    );
}

/// A white flag held up in a spaceling's `hand`, its stick upright with the
/// spaceling's body at `angle`, and its cloth out the way the spaceling
/// faces, fluttering `seconds` into the event.
#[allow(clippy::too_many_arguments)]
fn white_flag(
    frame: &mut RenderFrame,
    layer: i32,
    hand: Vec2,
    angle: f32,
    facing: f32,
    seconds: f32,
    size: f32,
    bounds: Bounds,
    alpha: f32,
) {
    let up = Vec2::Y.rotate_radians(angle) * size;
    let out = Vec2::X.rotate_radians(angle) * (facing * size);
    let top = hand + up * 1.35;
    strip(
        frame,
        layer,
        hand - up * 0.25,
        top,
        0.09 * size,
        bounds,
        faded(STICK, alpha),
    );
    let flutter = |phase: f32| up * (0.08 * (seconds * TAU * 2.5 + phase).sin());
    polygon(
        frame,
        layer,
        vec![
            top,
            top + out * 0.8 + flutter(0.0),
            top - up * 0.5 + out * 0.8 + flutter(0.9),
            top - up * 0.5,
        ],
        bounds,
        faded(FLAG, alpha),
    );
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
                // The winner's beam widens to take up a spaceling it has
                // drawn to its side.
                let (mut left, mut right) = (x - width, x + width);
                if event.phase == EventPhase::BeamUp
                    && let Some(taken) = event.survivors.taken()
                    && taken.slot != slot
                {
                    let reach = (taken.position.x - x).abs() + 0.75 * scale;
                    if taken.position.x < x {
                        left = left.min(x - reach);
                    } else {
                        right = right.max(x + reach);
                    }
                }
                polygon(
                    frame,
                    ARENA_LAYER,
                    vec![
                        Vec2::new(left, bottom),
                        Vec2::new(right, bottom),
                        Vec2::new(right, tank_bounds.max.y),
                        Vec2::new(left, tank_bounds.max.y),
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
            // A tank rebuilt from its dirt shows the dirt in patches across
            // its hull, where the cells went back in.
            if event
                .rebuild
                .as_ref()
                .is_some_and(|rebuild| rebuild.slot == slot && rebuild.stage == Stage::Rebuilt)
            {
                let patch = faded(mix(color, GRAIN, 0.6), color.a);
                for index in 0..12 {
                    if unit(event.seed, index) >= 0.45 {
                        continue;
                    }
                    let middle = Vec2::new(
                        -1.25 + 0.5 * (index % 6) as f32,
                        if index < 6 { -0.25 } else { 0.25 },
                    );
                    polygon(
                        frame,
                        ARENA_LAYER,
                        [
                            Vec2::new(-0.22, -0.2),
                            Vec2::new(0.22, -0.2),
                            Vec2::new(0.22, 0.2),
                            Vec2::new(-0.22, 0.2),
                        ]
                        .map(|corner| local(middle + corner))
                        .to_vec(),
                        tank_bounds,
                        patch,
                    );
                }
            }
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
                ARENA_LAYER,
                barrel[0],
                barrel[1],
                BARREL_WIDTH * scale,
                tank_bounds,
                color,
            );
        }
    }
    wreck(frame, event, world, [bounds, sky]);
    survivors(frame, event, world, [bounds, sky]);
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
    // The duel's flashes stop with it once the fight is over, and Clock
    // times a parting shell's.
    let parting = event.survivors.out.iter().filter_map(|survivor| {
        let (point, tick) = survivor.shot.as_ref()?.landed?;
        Some((point?, event.tick - tick))
    });
    for (point, age) in event
        .duel
        .flashes()
        .filter(|_| event.result.is_none())
        .chain(parting)
        .filter(|(_, age)| *age < FLASH_TICKS)
    {
        let t = age as f32 / FLASH_TICKS as f32;
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
