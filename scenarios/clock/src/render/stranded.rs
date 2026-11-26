use super::{
    tanks::{Pose, STICK, TEAMS, faded, narrow_beam, pilot, polygon, strip, trail},
    *,
};
use crate::{
    events::{FIXED_HZ, tanks::survivor::PILOT_SCALE},
    presentation::Bounds,
    stranded::{Phase, Scene},
};
use std::f32::consts::TAU;

/// How tall a planted flag's stick stands, how far down it its cloth
/// reaches, and how far out the cloth flies, in spaceling sizes.
const STICK_HEIGHT: f32 = 3.0;
const CLOTH_DROP: f32 = 1.1;
const CLOTH_REACH: f32 = 1.7;

/// The share of its suit's color a dead spaceling keeps, and the earthy
/// color the rest goes to as it turns to dirt.
const DEAD_TINT: f32 = 0.55;
const EARTH: RenderColor = RenderColor::rgb(0.36, 0.25, 0.15);

/// `color` gone `share` of the way to `to`.
fn toward(color: RenderColor, to: RenderColor, share: f32) -> RenderColor {
    let mix = |a: f32, b: f32| a + (b - a) * share;
    RenderColor {
        r: mix(color.r, to.r),
        g: mix(color.g, to.g),
        b: mix(color.b, to.b),
        a: color.a,
    }
}

/// A flag on a digit block, its stick's foot at `foot`, `raised` of the way
/// up from the block to standing: the stick, and a cloth of its side's
/// `color` out the way it `facing`, fluttering `seconds` into the run. Its
/// spaceling was `size` tall to the shoulder.
fn planted_flag(
    frame: &mut RenderFrame,
    (foot, facing, raised): (Vec2, f32, f32),
    seconds: f32,
    size: f32,
    color: RenderColor,
    bounds: Bounds,
) {
    let top = foot + Vec2::Y * (STICK_HEIGHT * size * raised);
    strip(frame, CREATURE_LAYER, foot, top, 0.16 * size, bounds, STICK);
    let out = Vec2::X * (facing * CLOTH_REACH * size * raised);
    let down = Vec2::Y * (-CLOTH_DROP * size * raised);
    let flutter = Vec2::Y * (0.1 * size * (seconds * TAU * 2.5).sin());
    polygon(
        frame,
        CREATURE_LAYER,
        vec![top, top + out + down * 0.5 + flutter, top + down],
        bounds,
        color,
    );
}

/// The spaceling a Tanks fight's hills left behind, and the flag one
/// planted: the flag on its block, the one the spaceling is planting, the
/// smoke its jetpack left, the beam that comes for it, and the spaceling
/// itself, standing, walking, running, planting, riding its jetpack, fading
/// as its beam lifts it out, or dead, going the color of the earth as it
/// turns to dirt. They are drawn in front of the face, at `tick` of the run.
pub(super) fn render(frame: &mut RenderFrame, scene: &Scene, tick: u64, layout: Layout) {
    let bounds = Bounds {
        min: Vec2::new(layout.bounds_min.x + layout.frame_width, layout.floor_y),
        max: Vec2::new(layout.bounds_max.x - layout.frame_width, layout.canopy_y),
    };
    let seconds = tick as f32 / FIXED_HZ as f32;
    if let Some(flag) = scene.flag {
        planted_flag(
            frame,
            (flag.foot, flag.facing, 1.0),
            seconds,
            PILOT_SCALE * flag.scale,
            TEAMS[flag.slot],
            bounds,
        );
    }
    let Some(spaceling) = &scene.spaceling else {
        return;
    };
    let scale = spaceling.scale;
    let color = TEAMS[spaceling.slot];
    // Dead, its suit dulls, and goes the color of the earth as it turns to
    // dirt.
    let suit = if spaceling.phase == Phase::Dead {
        toward(color, EARTH, 1.0 - DEAD_TINT * (1.0 - spaceling.fade()))
    } else {
        color
    };
    if let Some((foot, raised)) = spaceling.planting() {
        planted_flag(
            frame,
            (foot, spaceling.facing, raised),
            seconds,
            PILOT_SCALE * scale,
            color,
            bounds,
        );
    }
    trail(
        frame,
        CREATURE_LAYER,
        &spaceling.puffs,
        spaceling.age,
        scale,
        bounds,
    );
    if let Some((feet, descent)) = spaceling.beam() {
        let mut beam = color;
        beam.a = 0.16 + 0.06 * (spaceling.age as f32 * 0.4).sin().abs();
        narrow_beam(frame, feet, descent, beam, scale, bounds);
    }
    let pose = Pose {
        stride: spaceling.stride(),
        jet: spaceling.jet(),
        plant: spaceling.planting().map(|(_, raised)| raised),
        ..Pose::default()
    };
    pilot(
        frame,
        CREATURE_LAYER,
        spaceling.motion(),
        spaceling.facing,
        pose,
        faded(suit, 1.0 - spaceling.fade()),
        scale,
        bounds,
    );
}
