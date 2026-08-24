use engine_common::ClockTimeFormat;

use crate::ClockReading;

pub const DIGIT_SLOT_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SegmentKind {
    Top,
    UpperRight,
    LowerRight,
    Bottom,
    LowerLeft,
    UpperLeft,
    Middle,
}

impl SegmentKind {
    pub const ALL: [Self; 7] = [
        Self::Top,
        Self::UpperRight,
        Self::LowerRight,
        Self::Bottom,
        Self::LowerLeft,
        Self::UpperLeft,
        Self::Middle,
    ];

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId {
    pub digit_slot: u8,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRepresentation {
    Anchored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentState {
    pub id: SegmentId,
    pub lit: bool,
    pub representation: SegmentRepresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
    pub x: i8,
    pub y: i8,
}

const TOP_CELLS: [GridCell; 4] = horizontal_cells(8);
const MIDDLE_CELLS: [GridCell; 4] = horizontal_cells(4);
const BOTTOM_CELLS: [GridCell; 4] = horizontal_cells(0);
const UPPER_LEFT_CELLS: [GridCell; 3] = vertical_cells(0, 5);
const UPPER_RIGHT_CELLS: [GridCell; 3] = vertical_cells(5, 5);
const LOWER_LEFT_CELLS: [GridCell; 3] = vertical_cells(0, 1);
const LOWER_RIGHT_CELLS: [GridCell; 3] = vertical_cells(5, 1);

const fn horizontal_cells(y: i8) -> [GridCell; 4] {
    [
        GridCell { x: 1, y },
        GridCell { x: 2, y },
        GridCell { x: 3, y },
        GridCell { x: 4, y },
    ]
}

const fn vertical_cells(x: i8, first_y: i8) -> [GridCell; 3] {
    [
        GridCell { x, y: first_y },
        GridCell { x, y: first_y + 1 },
        GridCell { x, y: first_y + 2 },
    ]
}

pub fn cells(kind: SegmentKind) -> &'static [GridCell] {
    match kind {
        SegmentKind::Top => &TOP_CELLS,
        SegmentKind::UpperRight => &UPPER_RIGHT_CELLS,
        SegmentKind::LowerRight => &LOWER_RIGHT_CELLS,
        SegmentKind::Bottom => &BOTTOM_CELLS,
        SegmentKind::LowerLeft => &LOWER_LEFT_CELLS,
        SegmentKind::UpperLeft => &UPPER_LEFT_CELLS,
        SegmentKind::Middle => &MIDDLE_CELLS,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    pub colon_lit: bool,
    pub meridiem: Option<&'static str>,
}

impl DisplaySnapshot {
    pub const fn unsynchronized() -> Self {
        Self {
            digits: [None; DIGIT_SLOT_COUNT],
            colon_lit: false,
            meridiem: None,
        }
    }
}

pub fn snapshot(reading: ClockReading, format: ClockTimeFormat) -> DisplaySnapshot {
    let (display_hour, leading_zero, meridiem) = match format {
        ClockTimeFormat::TwentyFourHour => (reading.hour(), true, None),
        ClockTimeFormat::TwelveHour => {
            let display_hour = match reading.hour() % 12 {
                0 => 12,
                hour => hour,
            };
            let meridiem = if reading.hour() < 12 { "AM" } else { "PM" };
            (display_hour, false, Some(meridiem))
        }
    };
    let leading_hour = display_hour / 10;

    DisplaySnapshot {
        digits: [
            (leading_zero || leading_hour != 0).then_some(leading_hour),
            Some(display_hour % 10),
            Some(reading.minute() / 10),
            Some(reading.minute() % 10),
        ],
        colon_lit: reading.second().is_multiple_of(2),
        meridiem,
    }
}

pub fn create_segments() -> Vec<SegmentState> {
    (0..DIGIT_SLOT_COUNT)
        .flat_map(|digit_slot| {
            SegmentKind::ALL.map(move |kind| SegmentState {
                id: SegmentId {
                    digit_slot: digit_slot as u8,
                    kind,
                },
                lit: false,
                representation: SegmentRepresentation::Anchored,
            })
        })
        .collect()
}

pub fn apply_snapshot(segments: &mut [SegmentState], snapshot: DisplaySnapshot) {
    for segment in segments {
        let digit = snapshot.digits[usize::from(segment.id.digit_slot)];
        segment.lit = digit.is_some_and(|digit| digit_mask(digit) & segment.id.kind.bit() != 0);
    }
}

const fn digit_mask(digit: u8) -> u8 {
    use SegmentKind::{Bottom, LowerLeft, LowerRight, Middle, Top, UpperLeft, UpperRight};

    match digit {
        0 => {
            Top.bit()
                | UpperRight.bit()
                | LowerRight.bit()
                | Bottom.bit()
                | LowerLeft.bit()
                | UpperLeft.bit()
        }
        1 => UpperRight.bit() | LowerRight.bit(),
        2 => Top.bit() | UpperRight.bit() | Middle.bit() | LowerLeft.bit() | Bottom.bit(),
        3 => Top.bit() | UpperRight.bit() | Middle.bit() | LowerRight.bit() | Bottom.bit(),
        4 => UpperLeft.bit() | Middle.bit() | UpperRight.bit() | LowerRight.bit(),
        5 => Top.bit() | UpperLeft.bit() | Middle.bit() | LowerRight.bit() | Bottom.bit(),
        6 => {
            Top.bit()
                | UpperLeft.bit()
                | Middle.bit()
                | LowerLeft.bit()
                | LowerRight.bit()
                | Bottom.bit()
        }
        7 => Top.bit() | UpperRight.bit() | LowerRight.bit(),
        8 => {
            Top.bit()
                | UpperRight.bit()
                | LowerRight.bit()
                | Bottom.bit()
                | LowerLeft.bit()
                | UpperLeft.bit()
                | Middle.bit()
        }
        9 => {
            Top.bit()
                | UpperRight.bit()
                | LowerRight.bit()
                | Bottom.bit()
                | UpperLeft.bit()
                | Middle.bit()
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
        ClockReading::new(hour, minute, second).unwrap()
    }

    #[test]
    fn twenty_four_hour_display_keeps_leading_zero() {
        assert_eq!(
            snapshot(reading(4, 7, 2), ClockTimeFormat::TwentyFourHour),
            DisplaySnapshot {
                digits: [Some(0), Some(4), Some(0), Some(7)],
                colon_lit: true,
                meridiem: None,
            }
        );
    }

    #[test]
    fn twelve_hour_display_handles_midnight_noon_and_blank_leading_slot() {
        assert_eq!(
            snapshot(reading(0, 5, 1), ClockTimeFormat::TwelveHour),
            DisplaySnapshot {
                digits: [Some(1), Some(2), Some(0), Some(5)],
                colon_lit: false,
                meridiem: Some("AM"),
            }
        );
        assert_eq!(
            snapshot(reading(12, 5, 2), ClockTimeFormat::TwelveHour),
            DisplaySnapshot {
                digits: [Some(1), Some(2), Some(0), Some(5)],
                colon_lit: true,
                meridiem: Some("PM"),
            }
        );
        assert_eq!(
            snapshot(reading(13, 5, 2), ClockTimeFormat::TwelveHour).digits,
            [None, Some(1), Some(0), Some(5)]
        );
    }

    #[test]
    fn segments_have_stable_unique_ids_and_owned_cell_patterns() {
        let segments = create_segments();
        assert_eq!(segments.len(), DIGIT_SLOT_COUNT * SegmentKind::ALL.len());
        for (index, segment) in segments.iter().enumerate() {
            assert!(!segment.lit);
            assert_eq!(segment.representation, SegmentRepresentation::Anchored);
            assert!(!cells(segment.id.kind).is_empty());
            assert!(!segments[..index].iter().any(|other| other.id == segment.id));
        }
    }
}
