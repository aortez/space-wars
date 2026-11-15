//! The light the lamps give the plants, and when. A lamp's light enters the
//! room just below the corner it hangs in. Its steady light goes every way,
//! and as it narrows into a spotlight it sends its share into a cone along
//! its beam (see `lamps::beam`), so the plants in the pool get more and the
//! rest less. The plants' light model shades both from the digits and from
//! anything else solid between a plant and the lamp.
//!
//! The lamps follow the reading's time of day. They give their whole light
//! from 07:00 to 22:00 and a dim glow at night, too dim for grass, with half
//! an hour of dawn and of dusk between. The room's own dim light stays on
//! under theirs, so moss still holds at night.

use engine_core::{
    Color, Vec2,
    lighting::{Cone, EmissionPoint, FULL_SUN, LightId, LocalSource},
};

use super::{Look, REST_BRIGHTNESS, SPOT_PEAK, beam_from, head};
use crate::{ClockReading, layout::Layout};

#[cfg(test)]
mod tests;

/// A resting lamp's light where it enters the room, by day, on the plants'
/// scale, where full sun is 1000.
pub(crate) const REST_STRENGTH: f32 = FULL_SUN;
/// How far a lamp's light reaches. Its falloff is gentle, so that by day a
/// resting lamp with nothing in its way gives the grass in the corridor on
/// its side more than the 500 a full patch needs to pay its way, in every
/// layout.
pub(crate) const RANGE: f32 = 3000.0;
/// The share of their light the lamps give at night. At their brightest the
/// two give a plant less than 175 then, so with the room's 50 no grass shoot
/// gets the 250 it needs.
pub(crate) const NIGHT_OUTPUT: f32 = 0.03;
/// The lamps give all their light from `DAY.0` to `DAY.1`, in seconds since
/// midnight, and ramp to and from the night's over `TWILIGHT` either side.
const DAY: (u32, u32) = (7 * 3600, 22 * 3600);
const TWILIGHT: u32 = 30 * 60;
/// The lamps' light identities, apart from the sunlight's and the physical
/// ones.
pub(crate) const LAMP_LIGHT_OWNER: u64 = 0x4c41_4d50_4c49_4748;
/// The share of a beam's half angle that it lights fully. The rest of it
/// fades, as the drawn beam's nested fans do.
const FULL_SHARE: f32 = 0.4;
/// How often the lamps' light along the floor is sampled for drawing, in
/// ticks, how far apart the samples lie, in face cells, and at most how many
/// a floor takes.
pub(crate) const FLOOR_TICKS: u64 = 6;
pub(crate) const FLOOR_SPACING_CELLS: f32 = 0.5;
pub(crate) const MAX_FLOOR_SAMPLES: usize = 160;

/// The lamps' light at one place along the floor, for drawing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloorSample {
    /// Which stretch of floor it lies on, counted from the left.
    pub stretch: usize,
    /// Where on the floor, half a soil cell above its line, where a moss mat
    /// reads its light.
    pub position: Vec2,
    /// The way the floor runs there, counterclockwise from +x.
    pub angle: f32,
    /// How much floor the sample stands for.
    pub width: f32,
    /// The light the lamps give it, on the plants' scale.
    pub light: f32,
}

/// How much of the day it is at `reading`: 1 by day, 0 at night, rising
/// through dawn and falling through dusk. A clock that does not know the
/// time keeps the day.
pub(crate) fn daylight(reading: Option<ClockReading>) -> f32 {
    let Some(reading) = reading else {
        return 1.0;
    };
    let now = u32::from(reading.hour()) * 3600
        + u32::from(reading.minute()) * 60
        + u32::from(reading.second());
    let ramp = |from: u32| {
        let t = ((now - from) as f32 / TWILIGHT as f32).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if now < DAY.0 - TWILIGHT || now >= DAY.1 + TWILIGHT {
        0.0
    } else if now < DAY.0 {
        ramp(DAY.0 - TWILIGHT)
    } else if now <= DAY.1 {
        1.0
    } else {
        1.0 - ramp(DAY.1)
    }
}

/// The share of their light the lamps give when it is `daylight`.
pub(crate) fn output(daylight: f32) -> f32 {
    NIGHT_OUTPUT + (1.0 - NIGHT_OUTPUT) * daylight.clamp(0.0, 1.0)
}

/// How bright a lamp giving `output` of its light looks. The eye reads light
/// about by its cube root, so the night's glow still shows.
pub(crate) fn seen(output: f32) -> f32 {
    output.clamp(0.0, 1.0).cbrt()
}

/// Where `lamp`'s light enters the room: below its head, under the frame's
/// top band, so that the ceiling the spider brings across that band does not
/// hide it.
pub(crate) fn entry(lamp: usize, layout: &Layout) -> Vec2 {
    let head = head(lamp, layout);
    Vec2::new(
        head.x,
        head.y.min(layout.canopy_y - layout.frame_width * 0.5),
    )
}

/// What `lamp` gives the plants as it looks like `look`, giving `output` of
/// its light: its steady light every way and its spotlight's beam, each left
/// out while it gives nothing.
pub(crate) fn sources(lamp: usize, look: Look, layout: &Layout, output: f32) -> Vec<LocalSource> {
    let entry = entry(lamp, layout);
    let beam = beam_from(lamp, entry, look, layout);
    // A spotlight gives up to its peak. A flare past it is for show.
    let power = REST_STRENGTH * look.brightness.clamp(0.0, SPOT_PEAK) / REST_BRIGHTNESS * output;
    let from = EmissionPoint {
        position: entry,
        surface: None,
    };
    let light = |part: u64, strength: f32, cone: Option<Cone>| LocalSource {
        id: LightId(LAMP_LIGHT_OWNER, lamp as u64 * 2 + part),
        active: true,
        color: Color::WHITE,
        strength: strength.min(FULL_SUN),
        range: RANGE,
        footprint: vec![from],
        cone,
    };
    let mut sources = Vec::with_capacity(2);
    let steady = power * (1.0 - beam.narrow);
    if steady > 0.0 {
        sources.push(light(0, steady, None));
    }
    let spot = power * beam.narrow;
    if spot > 0.0 {
        sources.push(light(
            1,
            spot,
            Some(Cone {
                axis: Vec2::new(beam.aim.cos(), beam.aim.sin()),
                full_cos: (beam.spread * FULL_SHARE).cos(),
                edge_cos: beam.spread.cos(),
            }),
        ));
    }
    sources
}
