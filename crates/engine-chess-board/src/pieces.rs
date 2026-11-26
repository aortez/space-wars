//! The pieces, drawn from masks of cells: `#` is the piece, `+` a mark drawn
//! over it and `.` nothing. Both colours share the masks. The outline is the
//! piece grown by a cell in each of the four directions, so every piece keeps
//! a cell clear of the edge of its grid.

use std::sync::LazyLock;

use engine_chess::PieceKind;

use crate::Detail;
use crate::mask::{Corner, Mask};

/// Full pieces, sixteen cells high, in the order of [`PieceKind::ALL`].
const FULL: [[&str; 16]; 6] = [
    [
        "................",
        "................",
        "................",
        "................",
        "......####......",
        ".....######.....",
        ".....######.....",
        ".....######.....",
        "......####......",
        ".....######.....",
        "......####......",
        "......####......",
        ".....######.....",
        "....########....",
        "...##########...",
        "................",
    ],
    [
        "................",
        "................",
        ".......#.#......",
        "......######....",
        ".....########...",
        "....###+######..",
        "...###########..",
        "..####..######..",
        "..###...######..",
        "...#...#######..",
        "......#######...",
        ".....#######....",
        "....#########...",
        "...###########..",
        "...###########..",
        "................",
    ],
    [
        "................",
        ".......##.......",
        "......####......",
        ".......##.......",
        "......####......",
        ".....####+#.....",
        ".....###+##.....",
        ".....##+###.....",
        "......####......",
        ".....######.....",
        "......####......",
        "......####......",
        ".....######.....",
        "....########....",
        "...##########...",
        "................",
    ],
    [
        "................",
        "................",
        "................",
        "..##...##...##..",
        "..##...##...##..",
        "..############..",
        "...##########...",
        "....########....",
        "....########....",
        "....########....",
        "....########....",
        "....########....",
        "...##########...",
        "..############..",
        "..############..",
        "................",
    ],
    [
        "................",
        ".#.....##.....#.",
        ".##....##....##.",
        ".###..####..###.",
        "..############..",
        "...##########...",
        "....########....",
        ".....######.....",
        ".....######.....",
        "....########....",
        ".....######.....",
        ".....######.....",
        "....########....",
        "...##########...",
        "..############..",
        "................",
    ],
    [
        "................",
        ".......##.......",
        "......####......",
        ".......##.......",
        "...###.##.###...",
        "..############..",
        "..############..",
        "...##########...",
        "....########....",
        "....########....",
        "...##########...",
        "....########....",
        "....########....",
        "...##########...",
        "..############..",
        "................",
    ],
];

/// Compact pieces, ten cells high, in the order of [`PieceKind::ALL`]. A gap
/// within a row is two cells or more, which a renderer that widens each run
/// of pixels by one still shows.
const COMPACT: [[&str; 10]; 6] = [
    [
        "..........",
        "..........",
        "..........",
        "....##....",
        "...####...",
        "...####...",
        "....##....",
        "...####...",
        "..######..",
        "..........",
    ],
    [
        "..........",
        "....##....",
        "...#####..",
        "..###+##..",
        ".#######..",
        ".##..###..",
        "....####..",
        "...#####..",
        "..######..",
        "..........",
    ],
    [
        "..........",
        "....##....",
        "...####...",
        "..##+###..",
        "..#+####..",
        "...####...",
        "....##....",
        "...####...",
        "..######..",
        "..........",
    ],
    [
        "..........",
        "..........",
        ".###..###.",
        ".########.",
        "..######..",
        "..######..",
        "..######..",
        "..######..",
        "..######..",
        "..........",
    ],
    [
        "..........",
        ".#..##..#.",
        ".##.##.##.",
        "..######..",
        "...####...",
        "....##....",
        "...####...",
        "..######..",
        "..######..",
        "..........",
    ],
    [
        "..........",
        "....##....",
        "...####...",
        "....##....",
        ".########.",
        "..######..",
        "...####...",
        "...####...",
        "..######..",
        "..........",
    ],
];

/// The polygons that draw one piece, with corners in cells from the top left
/// of its square.
#[derive(Debug)]
pub(crate) struct Shape {
    /// How many cells lie along a side of the square.
    pub(crate) cells: u8,
    pub(crate) outline: Vec<Vec<Corner>>,
    pub(crate) body: Vec<Vec<Corner>>,
    pub(crate) marks: Vec<Vec<Corner>>,
}

impl Shape {
    fn new(rows: &[&str], tipped: bool) -> Self {
        let mut body = Mask::parse(rows, b"#+");
        let mut marks = Mask::parse(rows, b"+");
        if tipped {
            // Laid on its side, and down onto the row its base stood on.
            let base = body.bottom().expect("a piece has cells");
            body = body.turned();
            marks = marks.turned();
            let drop = base - body.bottom().expect("a piece has cells");
            body = body.lowered(drop);
            marks = marks.lowered(drop);
        }
        let polygons = |mask: &Mask| {
            mask.polygons()
                .expect("each part of a piece is one polygon")
        };
        Self {
            cells: body.size() as u8,
            outline: polygons(&body.grown()),
            body: polygons(&body),
            marks: polygons(&marks),
        }
    }

    /// How many polygons draw the piece.
    #[cfg(test)]
    pub(crate) fn polygons(&self) -> usize {
        self.outline.len() + self.body.len() + self.marks.len()
    }
}

struct Shapes {
    /// By detail, then by kind.
    standing: [[Shape; 6]; 2],
    /// A king laid on its side, by detail.
    tipped: [Shape; 2],
}

static SHAPES: LazyLock<Shapes> = LazyLock::new(|| Shapes {
    standing: [
        FULL.map(|rows| Shape::new(&rows, false)),
        COMPACT.map(|rows| Shape::new(&rows, false)),
    ],
    tipped: [
        Shape::new(&FULL[king()], true),
        Shape::new(&COMPACT[king()], true),
    ],
});

fn king() -> usize {
    kind_index(PieceKind::King)
}

fn kind_index(kind: PieceKind) -> usize {
    PieceKind::ALL
        .iter()
        .position(|&other| other == kind)
        .expect("every kind is in ALL")
}

fn detail_index(detail: Detail) -> usize {
    match detail {
        Detail::Full => 0,
        Detail::Compact => 1,
    }
}

/// How a piece of `kind` is drawn at `detail`, standing or, for a king that
/// has lost, tipped over.
pub(crate) fn shape(detail: Detail, kind: PieceKind, tipped: bool) -> &'static Shape {
    if tipped && kind == PieceKind::King {
        &SHAPES.tipped[detail_index(detail)]
    } else {
        &SHAPES.standing[detail_index(detail)][kind_index(kind)]
    }
}

/// The most polygons that any one piece is drawn with at `detail`.
#[cfg(test)]
pub(crate) fn most_polygons(detail: Detail) -> usize {
    let shapes = &*SHAPES;
    let index = detail_index(detail);
    shapes.standing[index]
        .iter()
        .chain([&shapes.tipped[index]])
        .map(Shape::polygons)
        .max()
        .expect("there are pieces")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_masks() -> Vec<(&'static str, Vec<&'static str>)> {
        let names = ["pawn", "knight", "bishop", "rook", "queen", "king"];
        let mut masks = Vec::new();
        for (name, rows) in names.iter().zip(FULL) {
            masks.push((*name, rows.to_vec()));
        }
        for (name, rows) in names.iter().zip(COMPACT) {
            masks.push((*name, rows.to_vec()));
        }
        masks
    }

    #[test]
    fn every_piece_is_one_group_clear_of_the_edge_with_its_marks_inside() {
        for (name, rows) in all_masks() {
            assert!(
                rows.iter()
                    .all(|row| row.bytes().all(|cell| b"#+.".contains(&cell))),
                "{name}"
            );
            let body = Mask::parse(&rows, b"#+");
            let polygons = body.polygons().unwrap_or_else(|| panic!("{name}"));
            assert_eq!(polygons.len(), 1, "{name} is one piece");
            let last = rows.len() - 1;
            assert!(
                !rows[0].contains(['#', '+']) && !rows[last].contains(['#', '+']),
                "{name} leaves room above and below for its outline"
            );
            assert!(
                rows.iter()
                    .all(|row| !row.starts_with(['#', '+']) && !row.ends_with(['#', '+'])),
                "{name} leaves room at the sides for its outline"
            );
        }
    }

    #[test]
    fn every_shape_traces_and_stays_in_its_square() {
        for detail in [Detail::Full, Detail::Compact] {
            for kind in PieceKind::ALL {
                for tipped in [false, true] {
                    let shape = shape(detail, kind, tipped);
                    assert_eq!(shape.outline.len(), 1, "{detail:?} {kind:?}");
                    assert_eq!(shape.body.len(), 1, "{detail:?} {kind:?}");
                    for polygon in shape.outline.iter().chain(&shape.body).chain(&shape.marks) {
                        assert!(polygon.len() >= 4);
                        assert!(
                            polygon
                                .iter()
                                .all(|&(x, y)| x <= shape.cells && y <= shape.cells)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_tipped_king_lies_on_the_row_its_base_stood_on() {
        for (detail, rows) in [
            (Detail::Full, &FULL[king()][..]),
            (Detail::Compact, &COMPACT[king()][..]),
        ] {
            let tipped = shape(detail, PieceKind::King, true);
            let standing = shape(detail, PieceKind::King, false);
            let lowest = |polygons: &Vec<Vec<Corner>>| {
                polygons.iter().flatten().map(|&(_, y)| y).max().unwrap()
            };
            assert_eq!(lowest(&tipped.body), lowest(&standing.body), "{detail:?}");
            assert_eq!(usize::from(lowest(&standing.body)), rows.len() - 1);
            assert_ne!(tipped.body, standing.body);
        }
    }
}
