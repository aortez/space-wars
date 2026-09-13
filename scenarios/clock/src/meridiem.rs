//! Shared code-native AM/PM geometry, in clock-face units. Rendering, compound
//! letter colliders and melted pixels all use the same small glyphs.
use engine_core::Vec2;

use crate::{
    digits::GridCell,
    layout::{FACE_WIDTH_UNITS, Layout},
};

pub(crate) const PIXEL_PITCH: f32 = 0.16;
pub(crate) const PIXEL_SIZE: f32 = PIXEL_PITCH * 0.9;
pub(crate) const MAX_CELLS: usize = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    A,
    P,
    M,
}

impl Glyph {
    pub fn for_label(label: &str) -> [Self; 2] {
        [if label == "AM" { Self::A } else { Self::P }, Self::M]
    }

    pub fn width(self) -> i8 {
        if self == Self::M { 5 } else { 3 }
    }

    pub fn lit(self, cell: GridCell) -> bool {
        let rows = match self {
            Self::A => [2, 5, 7, 5, 5],
            Self::P => [6, 5, 6, 4, 4],
            Self::M => [17, 27, 21, 17, 17],
        };
        rows[(4 - cell.y) as usize] & (1 << (self.width() - cell.x - 1)) != 0
    }

    pub fn cells(self) -> impl Iterator<Item = GridCell> {
        // Match the ordinary label's top-to-bottom, left-to-right drawing order.
        (0..5)
            .rev()
            .flat_map(move |y| (0..self.width()).map(move |x| GridCell { x, y }))
            .filter(move |cell| self.lit(*cell))
    }

    pub fn cell_min(self, cell: GridCell) -> Vec2 {
        let offset = if self == Self::M { 4.0 } else { 0.0 };
        Vec2::new(
            FACE_WIDTH_UNITS - 9.0 * PIXEL_PITCH + (offset + f32::from(cell.x)) * PIXEL_PITCH,
            -0.35 - f32::from(5 - cell.y) * PIXEL_PITCH,
        )
    }

    pub fn cell_center(self, layout: Layout, cell: GridCell) -> Vec2 {
        let units = self.cell_min(cell) + Vec2::splat(PIXEL_SIZE * 0.5);
        Vec2::new(layout.face_origin.x, layout.face_origin.y) + units * layout.pitch
    }

    pub fn center(self, layout: Layout) -> Vec2 {
        (self.cell_center(layout, GridCell { x: 0, y: 0 })
            + self.cell_center(
                layout,
                GridCell {
                    x: self.width() - 1,
                    y: 4,
                },
            ))
            * 0.5
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LetterState {
    pub glyph: Glyph,
    pub position: Vec2,
    pub angle: f32,
}

impl LetterState {
    pub fn new(glyph: Glyph, layout: Layout) -> Self {
        Self {
            glyph,
            position: glyph.center(layout),
            angle: 0.0,
        }
    }
}
