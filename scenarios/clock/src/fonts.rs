//! One bounded 6×9 cell grid for every face, shared by all event geometry.
use crate::{GridCell, SegmentKind, digits};
use engine_common::ClockFont;

include!(concat!(env!("OUT_DIR"), "/sampled_fonts.rs"));

pub(crate) const MAX_DIGIT_CELLS: usize = 6 * 9 * 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CellMask(pub u64);

impl CellMask {
    pub fn contains(self, cell: GridCell) -> bool {
        self.0 & (1 << (cell.y * 6 + cell.x)) != 0
    }
    pub fn cells(self) -> impl Iterator<Item = GridCell> + Clone {
        (0..54)
            .filter(move |bit| self.0 & (1 << bit) != 0)
            .map(|bit| GridCell {
                x: bit % 6,
                y: bit / 6,
            })
    }
}

pub(crate) fn glyph(font: ClockFont, digit: Option<u8>) -> CellMask {
    let Some(digit) = digit.filter(|d| *d < 10) else {
        return CellMask(0);
    };
    CellMask(match font {
        ClockFont::Classic => SegmentKind::ALL
            .into_iter()
            .filter(|kind| digits::digit_mask(digit) & (1 << *kind as u8) != 0)
            .fold(0, |mask, kind| mask | region(font, kind).0),
        ClockFont::Matrix => {
            let rows = crate::presentation::font::glyph(b'0' + digit).unwrap();
            let mut mask = 0;
            for y in 0..9 {
                for x in 0..6 {
                    if rows[(8 - y) * 7 / 9] & (1 << (4 - x * 5 / 6)) != 0 {
                        mask |= 1 << (y * 6 + x);
                    }
                }
            }
            mask
        }
        ClockFont::Sans => SANS[digit as usize],
        ClockFont::Serif => SERIF[digit as usize],
    })
}

pub(crate) fn guides(font: ClockFont) -> CellMask {
    if font == ClockFont::Classic {
        CellMask(
            SegmentKind::ALL
                .into_iter()
                .fold(0, |mask, kind| mask | region(font, kind).0),
        )
    } else {
        CellMask((0..10).fold(0, |mask, digit| mask | glyph(font, Some(digit)).0))
    }
}

/// Seven stable body IDs also partition sampled glyphs into compact pieces.
/// Classic retains its original bars, cell order, origins and physics seeds.
pub(crate) fn region(font: ClockFont, kind: SegmentKind) -> CellMask {
    if font == ClockFont::Classic {
        return CellMask(
            digits::cells(kind)
                .iter()
                .fold(0, |mask, cell| mask | (1 << (cell.y * 6 + cell.x))),
        );
    }
    CellMask(
        (0..54)
            .filter(|bit| {
                let x = bit % 6;
                let y = bit / 6;
                let owner = match y {
                    8 => SegmentKind::Top,
                    4 => SegmentKind::Middle,
                    0 => SegmentKind::Bottom,
                    5..=7 if x < 3 => SegmentKind::UpperLeft,
                    5..=7 => SegmentKind::UpperRight,
                    _ if x < 3 => SegmentKind::LowerLeft,
                    _ => SegmentKind::LowerRight,
                };
                owner == kind
            })
            .fold(0, |mask, bit| mask | (1 << bit)),
    )
}
