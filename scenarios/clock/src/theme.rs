//! The Clock's look (#455): which theme a choice wears at a reading, and
//! what each theme draws.
//!
//! A theme is presentation only. Choosing or resolving one takes no event
//! slot, draws nothing from the scheduler's random numbers, and leaves the
//! residents, the visitors and the physics as they are.

use engine_common::{ClockTheme, ClockThemeChoice, RenderColor};

use crate::ClockReading;
use crate::presentation::effects::DigitPalette;

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
    pub decoration: Decoration,
}

/// What a theme hangs on the frame, as art only: nothing here is a body, a
/// light source or a surface anything stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decoration {
    None,
    /// Colored bulbs on a green wire, swagged along the canopy's underside.
    StringLights,
}

impl Appearance {
    pub const NORMAL: Self = Self {
        digits: DigitPalette::NORMAL,
        label: RenderColor::rgb(0.52, 0.72, 0.77),
        decoration: Decoration::None,
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
        decoration: Decoration::StringLights,
    };

    pub const fn of(theme: ClockTheme) -> Self {
        match theme {
            ClockTheme::Normal => Self::NORMAL,
            ClockTheme::Christmas => Self::CHRISTMAS,
        }
    }
}
