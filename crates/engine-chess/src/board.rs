//! Colours, pieces and squares.

use std::fmt;

/// One of the two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub const fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// What kind of piece stands on a square.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    pub const ALL: [Self; 6] = [
        Self::Pawn,
        Self::Knight,
        Self::Bishop,
        Self::Rook,
        Self::Queen,
        Self::King,
    ];

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The lower-case letter that names the kind in FEN and in a promotion.
    pub const fn letter(self) -> char {
        match self {
            Self::Pawn => 'p',
            Self::Knight => 'n',
            Self::Bishop => 'b',
            Self::Rook => 'r',
            Self::Queen => 'q',
            Self::King => 'k',
        }
    }

    /// The kind that a lower-case letter names.
    pub fn from_letter(letter: char) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.letter() == letter)
    }
}

/// A piece of one colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

/// A square of the board. Files and ranks count from zero, so a1 is file 0
/// on rank 0 and h8 is file 7 on rank 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Square(u8);

impl Square {
    pub const fn new(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Some(Self(rank * 8 + file))
        } else {
            None
        }
    }

    /// The square with this index, where a1 is 0, b1 is 1 and h8 is 63.
    pub(crate) const fn from_index(index: u8) -> Self {
        debug_assert!(index < 64);
        Self(index)
    }

    pub const fn file(self) -> u8 {
        self.0 % 8
    }

    pub const fn rank(self) -> u8 {
        self.0 / 8
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }

    /// The set that holds only this square.
    pub(crate) const fn bit(self) -> u64 {
        1 << self.0
    }

    /// Parses a name such as `e4`.
    pub fn from_name(name: &str) -> Option<Self> {
        let &[file, rank] = name.as_bytes() else {
            return None;
        };
        if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
            return None;
        }
        Self::new(file - b'a', rank - b'1')
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", (b'a' + self.file()) as char, self.rank() + 1)
    }
}

impl fmt::Debug for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
