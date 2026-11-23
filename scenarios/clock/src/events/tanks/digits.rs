//! The face as the duel meets it: one fixed body with a box for every lit
//! digit cell that stands, both colon cells and the AM/PM pixels, rebuilt when
//! the reading changes or a cell comes loose. Rays meet the boxes, so shells
//! burst on them and a plotting CPU plots around them. Contacts with them push
//! nothing, so dirt and tanks pass the face as they always have.
use super::*;
use crate::{
    DIGIT_SLOT_COUNT,
    digit_surfaces::CELL_HALF,
    digits::DisplaySnapshot,
    fonts::{self, CellMask},
    meridiem,
};
use engine_rapier::world::{BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec};

pub(super) const BODY: PhysicsId = PhysicsId::new(canonical::TANKS_DIGITS.start);

/// The middles of the colon's two cells, in pitches from the face's lower
/// left corner.
const COLON_X: f32 = 15.0;
const COLON_ROWS: [f32; 2] = [2.75, 6.25];

/// The most boxes the face holds: every digit cell, the colon's two and the
/// AM/PM pixels.
pub const MAX_CELLS: usize = fonts::MAX_DIGIT_CELLS + COLON_ROWS.len() + meridiem::MAX_CELLS;

/// One square the face draws: its middle and half its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub center: Vec2,
    pub half: f32,
}

/// The squares the face draws for `display`: the lit digit cells, both colon
/// cells even while the colon blinks off, and the AM/PM pixels. A digit the
/// shells have damaged has only the cells `standing` gives it.
pub fn cells(
    layout: Layout,
    display: DisplaySnapshot,
    standing: [Option<CellMask>; DIGIT_SLOT_COUNT],
) -> Vec<Cell> {
    let mut cells = Vec::new();
    for (slot, digit) in display.digits.into_iter().enumerate() {
        let lit = standing[slot].unwrap_or_else(|| fonts::glyph(display.font, digit));
        for cell in lit.cells() {
            cells.push(Cell {
                center: layout.slot_cell_center(slot, cell),
                half: CELL_HALF * layout.pitch,
            });
        }
    }
    for row in COLON_ROWS {
        cells.push(Cell {
            center: Vec2::new(
                layout.face_origin.x + COLON_X * layout.pitch,
                layout.face_origin.y + row * layout.pitch,
            ),
            half: CELL_HALF * layout.pitch,
        });
    }
    if let Some(label) = display.meridiem {
        for glyph in meridiem::Glyph::for_label(label) {
            for cell in glyph.cells() {
                cells.push(Cell {
                    center: glyph.cell_center(layout, cell),
                    half: meridiem::PIXEL_SIZE * layout.pitch * 0.5,
                });
            }
        }
    }
    cells
}

/// The face's boxes in the arena, as last built.
#[derive(Default)]
pub struct Solids {
    cells: Vec<Cell>,
}

impl Solids {
    /// Builds the boxes for `display`, or rebuilds them when its cells differ
    /// from the last: a new minute, font or time format, or a cell come
    /// loose. The colon's blinking changes nothing.
    pub fn synchronize(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        display: DisplaySnapshot,
        standing: [Option<CellMask>; DIGIT_SLOT_COUNT],
    ) {
        let cells = cells(layout, display, standing);
        if cells == self.cells {
            return;
        }
        assert!(cells.len() <= MAX_CELLS);
        let colliders: Vec<_> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let mut collider = ColliderSpec::cuboid(
                    ColliderId::new(BODY, ColliderRole::PRIMARY, index as u16),
                    cell.half,
                    cell.half,
                );
                collider.local_position = cell.center;
                collider.solver_groups = CollisionGroups::NONE;
                collider
            })
            .collect();
        let body = BodyId::new(BODY, BodyRole::PRIMARY);
        if world.contains_entity(BODY) {
            assert!(world.replace_colliders(body, ColliderRole::PRIMARY, &colliders));
        } else {
            assert!(world.insert_body(
                body,
                BodySpec {
                    kind: BodyKind::Fixed,
                    ..BodySpec::default()
                },
                &colliders
            ));
        }
        self.cells = cells;
    }

    /// The squares the boxes stand for.
    #[cfg(test)]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}

#[cfg(test)]
mod tests;
