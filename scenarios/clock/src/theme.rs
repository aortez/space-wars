//! The Clock's look (#455): which theme a choice wears at a reading, what
//! each theme draws, and the fade from one theme to the next (#457).
//!
//! A theme is presentation only. Choosing or resolving one takes no event
//! slot, draws nothing from the scheduler's random numbers, and leaves the
//! residents, the visitors and the physics as they are.

use engine_common::{ClockTheme, ClockThemeChoice, RenderColor};

use crate::ClockReading;
use crate::presentation::effects::{DigitPalette, smoothstep};

#[cfg(test)]
mod tests;

/// The hour a season's night starts, on its evening.
const EVENING: u8 = 18;
/// The hour the night ends, on the next morning.
const MORNING: u8 = 6;

/// The seasons Auto follows, each by the month its nights' evenings fall in.
/// A night belongs to the date of its evening, so the first morning after the
/// month still has its season and the month's first morning does not.
const SEASONS: [(u8, ClockTheme); 1] = [(12, ClockTheme::Christmas)];

/// The theme `choice` wears at `reading`, the host's local civil time, or
/// without one when no usable time is known. Auto needs a dated reading to
/// find a season and wears the ordinary look without one. Only the reading
/// counts: there is no timer, and no missed night is made up.
pub(crate) fn resolve(choice: ClockThemeChoice, reading: Option<ClockReading>) -> ClockTheme {
    match choice {
        ClockThemeChoice::Auto => reading.and_then(season).unwrap_or_default(),
        ClockThemeChoice::Normal => ClockTheme::Normal,
        ClockThemeChoice::Christmas => ClockTheme::Christmas,
    }
}

/// The season whose night `reading` falls in, if any.
fn season(reading: ClockReading) -> Option<ClockTheme> {
    let date = reading.date()?;
    let evening = if reading.hour() >= EVENING {
        date
    } else if reading.hour() < MORNING {
        date.previous()?
    } else {
        return None;
    };
    SEASONS
        .iter()
        .find(|(month, _)| *month == evening.month())
        .map(|&(_, theme)| theme)
}

/// What a theme draws. The wall, the floor, the frame's wood, the lamps and
/// everything in the room keep their own colors whatever the theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Appearance {
    /// The face's lit cells, their glow and the colon, and its dark cells.
    pub digits: DigitPalette,
    /// The date line and the AM/PM mark.
    pub label: RenderColor,
    /// How much of the string of lights shows under the canopy, from none at
    /// 0 to all of it at 1. The string is art only: nothing in it is a body, a
    /// light source or a surface anything stands on.
    pub lights: f32,
}

impl Appearance {
    pub const NORMAL: Self = Self {
        digits: DigitPalette::NORMAL,
        label: RenderColor::rgb(0.52, 0.72, 0.77),
        lights: 0.0,
    };

    /// Warm-white digits over pine-dark cells, a holly-red date line and the
    /// string of lights.
    pub const CHRISTMAS: Self = Self {
        digits: DigitPalette {
            fill: RenderColor::rgb(1.0, 0.91, 0.76),
            edge: RenderColor::rgb(1.0, 0.98, 0.91),
            unlit: RenderColor::rgb(0.028, 0.058, 0.040),
        },
        label: RenderColor::rgb(0.92, 0.42, 0.40),
        lights: 1.0,
    };

    pub const fn of(theme: ClockTheme) -> Self {
        match theme {
            ClockTheme::Normal => Self::NORMAL,
            ClockTheme::Christmas => Self::CHRISTMAS,
        }
    }

    /// The look `t` of the way from this one to `to`: every color mixes, and
    /// the string of lights fades in or out. The mix is exactly this look at
    /// 0 and exactly `to` at 1, so a finished fade draws what the theme draws
    /// from the start.
    pub fn toward(self, to: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a * (1.0 - t) + b * t;
        let color = |a: RenderColor, b: RenderColor| {
            RenderColor::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
        };
        Self {
            digits: DigitPalette {
                fill: color(self.digits.fill, to.digits.fill),
                edge: color(self.digits.edge, to.digits.edge),
                unlit: color(self.digits.unlit, to.digits.unlit),
            },
            label: color(self.label, to.label),
            lights: mix(self.lights, to.lights),
        }
    }
}

/// How long a change of look takes: half a second of the scene's own time.
pub(crate) const FADE_TICKS: u32 = 30;

/// The look on the face (#457): a fade from what was drawn when the theme
/// last changed toward the theme it heads for. It moves only with the scene's
/// own time, so a pause holds it where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    from: Appearance,
    to: ClockTheme,
    /// Ticks since the change. The fade is over at `FADE_TICKS`.
    tick: u32,
}

impl Look {
    /// `theme`'s look, with nothing left to fade.
    pub const fn settled(theme: ClockTheme) -> Self {
        Self {
            from: Appearance::of(theme),
            to: theme,
            tick: FADE_TICKS,
        }
    }

    /// What the face wears now.
    pub fn drawn(self) -> Appearance {
        let t = self.tick as f32 / FADE_TICKS as f32;
        self.from.toward(Appearance::of(self.to), smoothstep(t))
    }

    /// Heads for `theme` from whatever is drawn now, so a change in the
    /// middle of a fade carries on without a jump. The theme it already heads
    /// for leaves the fade where it is: repeated readings and settings never
    /// restart it.
    pub fn head_for(&mut self, theme: ClockTheme) {
        if theme != self.to {
            *self = Self {
                from: self.drawn(),
                to: theme,
                tick: 0,
            };
        }
    }

    /// One tick of the scene's own time.
    pub fn advance(&mut self) {
        self.tick = (self.tick + 1).min(FADE_TICKS);
    }
}
