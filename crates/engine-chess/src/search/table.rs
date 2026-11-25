//! What the search has already learned about positions, so that one reached
//! by another order of moves need not be searched again.

use crate::board::{PieceKind, Square};
use crate::moves::Move;

/// How a stored score relates to the true value of its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bound {
    /// The score is the value.
    Exact,
    /// The value is at least the score: a move was good enough to stop.
    Lower,
    /// The value is at most the score: no move reached the window.
    Upper,
}

/// What the table keeps about one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) score: i16,
    /// The best move found, if there was one.
    pub(super) mv: Option<Move>,
    /// How many half-moves deep the position was searched.
    pub(super) depth: u8,
    pub(super) bound: Bound,
}

impl Entry {
    /// Sixteen bits each for the score and the move, eight for the depth,
    /// and the bound above them. No bound is packed as zero, so a word of
    /// zero is an empty slot.
    fn pack(self) -> u64 {
        let bound = match self.bound {
            Bound::Exact => 1,
            Bound::Lower => 2,
            Bound::Upper => 3,
        };
        u64::from(self.score as u16)
            | u64::from(self.mv.map_or(0, pack)) << 16
            | u64::from(self.depth) << 32
            | bound << 40
    }

    fn unpack(word: u64) -> Option<Self> {
        let bound = match word >> 40 {
            1 => Bound::Exact,
            2 => Bound::Lower,
            3 => Bound::Upper,
            _ => return None,
        };
        Some(Self {
            score: word as u16 as i16,
            mv: unpack((word >> 16) as u16),
            depth: (word >> 32) as u8,
            bound,
        })
    }
}

/// A table of a fixed number of slots. A new entry replaces whatever shared
/// its slot, so the table never grows.
#[derive(Clone)]
pub(super) struct Table {
    /// For each slot, the key of its position and its packed entry. Slots
    /// of plain zeros let the allocator hand over memory that is already
    /// clear, so a large table costs nothing to make.
    slots: Vec<[u64; 2]>,
}

impl Table {
    /// The size of one slot in bytes.
    pub(super) const SLOT_BYTES: usize = std::mem::size_of::<[u64; 2]>();

    /// A table of two to the power `bits` slots, or none when `bits` is
    /// zero.
    pub(super) fn new(bits: u8) -> Self {
        let slots = if bits == 0 {
            Vec::new()
        } else {
            vec![[0; 2]; 1 << bits]
        };
        Self { slots }
    }

    pub(super) fn len(&self) -> usize {
        self.slots.len()
    }

    fn slot(&self, key: u64) -> Option<usize> {
        // The length is a power of two, so the mask keeps the low bits.
        let mask = self.slots.len().checked_sub(1)?;
        Some(key as usize & mask)
    }

    pub(super) fn probe(&self, key: u64) -> Option<Entry> {
        let [stored, word] = self.slots[self.slot(key)?];
        if stored != key {
            return None;
        }
        Entry::unpack(word)
    }

    pub(super) fn store(&mut self, key: u64, entry: Entry) {
        if let Some(slot) = self.slot(key) {
            self.slots[slot] = [key, entry.pack()];
        }
    }
}

const PROMOTIONS: [PieceKind; 4] = [
    PieceKind::Knight,
    PieceKind::Bishop,
    PieceKind::Rook,
    PieceKind::Queen,
];

/// Six bits for each square and three for the promotion. No move starts and
/// ends on a1, so zero is free to mean no move.
fn pack(mv: Move) -> u16 {
    let promotion = mv.promotion.map_or(0, |kind| {
        1 + PROMOTIONS
            .iter()
            .position(|&other| other == kind)
            .expect("a pawn promotes to a knight, bishop, rook or queen") as u16
    });
    mv.from.index() as u16 | (mv.to.index() as u16) << 6 | promotion << 12
}

fn unpack(packed: u16) -> Option<Move> {
    if packed == 0 {
        return None;
    }
    let square = |bits: u16| Square::from_index((bits & 63) as u8);
    let promotion = match packed >> 12 {
        0 => None,
        kind => Some(*PROMOTIONS.get(usize::from(kind) - 1)?),
    };
    Some(Move {
        from: square(packed),
        to: square(packed >> 6),
        promotion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_survives_packing() {
        for text in ["e2e4", "a1h8", "h8a1", "e7e8q", "a2a1n", "h7g8r", "b2c1b"] {
            let mv = Move::parse(text).unwrap();
            assert_eq!(unpack(pack(mv)), Some(mv), "{text}");
        }
        assert_eq!(unpack(0), None);
    }

    #[test]
    fn an_entry_is_found_only_under_its_own_key() {
        let mut table = Table::new(4);
        assert_eq!(table.len(), 16);
        for (score, bound) in [
            (-120, Bound::Lower),
            (i16::MAX, Bound::Exact),
            (i16::MIN, Bound::Upper),
        ] {
            let entry = Entry {
                score,
                mv: Move::parse("g1f3"),
                depth: 3,
                bound,
            };
            table.store(0x51, entry);
            assert_eq!(table.probe(0x51), Some(entry));
        }
        // The same slot, another position.
        assert_eq!(table.probe(0x41), None);
        assert_eq!(table.probe(0x52), None);
    }

    #[test]
    fn an_empty_slot_holds_no_entry_even_for_the_key_zero() {
        let table = Table::new(4);
        assert_eq!(table.probe(0), None);
        assert_eq!(table.probe(16), None);
    }

    #[test]
    fn a_table_of_no_slots_stores_nothing() {
        let mut table = Table::new(0);
        let entry = Entry {
            score: 0,
            mv: None,
            depth: 1,
            bound: Bound::Exact,
        };
        table.store(7, entry);
        assert_eq!(table.probe(7), None);
        assert_eq!(table.len(), 0);
    }
}
