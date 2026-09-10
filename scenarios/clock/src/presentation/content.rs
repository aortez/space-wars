use engine_core::Vec2;

use super::{Bounds, font};
use crate::{DisplaySnapshot, SegmentKind, digits};

pub(crate) const MAX_TEXT_BYTES: usize = 32;
pub(crate) const MAX_CONTENT_CELLS: usize = MAX_TEXT_BYTES * 35;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cell {
    pub center: Vec2,
    pub size: f32,
    pub glyph_pivot: Vec2,
    pub group: u8,
    /// Stable route position, not storage order or a list of currently lit cells.
    pub path: f32,
}

pub(crate) struct Content {
    pub cells: Vec<Cell>,
    pub bounds: Bounds,
    pub groups: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextError {
    Empty,
    TooLong,
    UnsupportedCharacter,
}

impl Content {
    pub fn clock(display: DisplaySnapshot) -> Self {
        let mut result = Self {
            cells: Vec::with_capacity(128),
            bounds: Bounds {
                min: Vec2::new(0.0, -1.2),
                max: Vec2::new(30.0, 10.2),
            },
            groups: 5,
        };
        result.update_clock(display);
        result
    }

    pub fn update_clock(&mut self, display: DisplaySnapshot) {
        self.cells.clear();
        for (slot, origin) in crate::layout::DIGIT_ORIGINS.into_iter().enumerate() {
            let Some(digit) = display.digits[slot] else {
                continue;
            };
            for kind in SegmentKind::ALL {
                if digits::digit_mask(digit) & (1 << kind as u8) == 0 {
                    continue;
                }
                for cell in digits::cells(kind) {
                    self.cells.push(Cell {
                        center: Vec2::new(
                            origin + f32::from(cell.x) + 0.5,
                            f32::from(cell.y) + 0.5,
                        ),
                        size: 1.0,
                        glyph_pivot: Vec2::new(origin + 3.0, 4.5),
                        group: slot as u8,
                        path: clock_path(kind, cell.x, cell.y),
                    });
                }
            }
        }
        if display.colon_lit {
            for (i, y) in [2.75, 6.25].into_iter().enumerate() {
                self.cells.push(Cell {
                    center: Vec2::new(15.0, y),
                    size: 1.0,
                    glyph_pivot: Vec2::new(15.0, 4.5),
                    group: 4,
                    path: i as f32 * 0.5,
                });
            }
        }
        // Include AM/PM in the content transform, not as an orphaned screen label.
        if let Some(meridiem) = display.meridiem {
            let first = if meridiem == "AM" {
                [2, 5, 7, 5, 5]
            } else {
                [6, 5, 6, 4, 4]
            };
            for (origin, width, rows) in [(0.0, 3, first), (4.0, 5, [17, 27, 21, 17, 17])] {
                for (row, bits) in rows.into_iter().enumerate() {
                    for col in 0..width {
                        if bits & (1 << (width - col - 1)) != 0 {
                            self.cells.push(Cell {
                                center: Vec2::new(
                                    30.0 - 9.0 * 0.16 + (origin + col as f32 + 0.5) * 0.16,
                                    -0.35 - (row as f32 + 0.5) * 0.16,
                                ),
                                size: 0.16,
                                glyph_pivot: Vec2::new(29.28, -0.75),
                                group: 5,
                                path: row as f32 / 5.0,
                            });
                        }
                    }
                }
            }
        }
        self.groups = if display.meridiem.is_some() { 6 } else { 5 };
        debug_assert!(self.cells.len() <= 128);
    }

    pub fn text(message: &str) -> Result<Self, TextError> {
        if message.len() > MAX_TEXT_BYTES {
            return Err(TextError::TooLong);
        }
        if message.is_empty() || message.bytes().all(|b| b == b' ') {
            return Err(TextError::Empty);
        }
        if !message.bytes().all(|b| font::glyph(b).is_some()) {
            return Err(TextError::UnsupportedCharacter);
        }
        let mut result = Self {
            cells: Vec::with_capacity(message.len() * 35),
            bounds: Bounds {
                min: Vec2::ZERO,
                max: Vec2::new((message.len() * 6 - 1) as f32, 7.0),
            },
            groups: message.len(),
        };
        for (group, byte) in message.bytes().enumerate() {
            for (row, bits) in font::glyph(byte).unwrap().into_iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) == 0 {
                        continue;
                    }
                    result.cells.push(Cell {
                        center: Vec2::new((group * 6 + col) as f32 + 0.5, 6.5 - row as f32),
                        size: 1.0,
                        glyph_pivot: Vec2::new((group * 6) as f32 + 2.5, 3.5),
                        group: group as u8,
                        // A serpentine scan through the font's fixed grid.
                        path: (row * 5 + if row % 2 == 0 { col } else { 4 - col }) as f32 / 35.0,
                    });
                }
            }
        }
        debug_assert!(result.cells.len() <= MAX_CONTENT_CELLS);
        Ok(result)
    }
}

fn clock_path(kind: SegmentKind, x: i8, y: i8) -> f32 {
    // Clockwise outside loop. The middle bar has its own left-to-right pass.
    let distance = match kind {
        SegmentKind::Top => x - 1,
        SegmentKind::UpperRight => 4 + 7 - y,
        SegmentKind::LowerRight => 7 + 3 - y,
        SegmentKind::Bottom => 10 + 4 - x,
        SegmentKind::LowerLeft => 14 + y - 1,
        SegmentKind::UpperLeft => 17 + y - 5,
        SegmentKind::Middle => return f32::from(x - 1) / 4.0,
    };
    f32::from(distance) / 20.0
}
