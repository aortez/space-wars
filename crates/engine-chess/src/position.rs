//! One arrangement of the board, with the rights that go with it.

use crate::bitboard::{
    LIGHT_SQUARES, RANK_1, RANK_2, RANK_7, RANK_8, Squares, bishop_attacks, diagonals,
    king_attacks, knight_attacks, north, pawn_attacks, rook_attacks, south, straights,
};
use crate::board::{Color, Piece, PieceKind, Square};
use crate::moves::Move;
use crate::zobrist::KEYS;

pub(crate) const WHITE_KING_SIDE: u8 = 1;
pub(crate) const WHITE_QUEEN_SIDE: u8 = 2;
pub(crate) const BLACK_KING_SIDE: u8 = 4;
pub(crate) const BLACK_QUEEN_SIDE: u8 = 8;

const INITIAL: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The castling rights that end when a piece leaves `square` or is captured
/// there: the home squares of the kings and rooks.
const fn rights_ended_at(square: Square) -> u8 {
    match square.index() {
        0 => WHITE_QUEEN_SIDE,
        4 => WHITE_KING_SIDE | WHITE_QUEEN_SIDE,
        7 => WHITE_KING_SIDE,
        56 => BLACK_QUEEN_SIDE,
        60 => BLACK_KING_SIDE | BLACK_QUEEN_SIDE,
        63 => BLACK_KING_SIDE,
        _ => 0,
    }
}

/// Somewhere to put moves as they are generated.
pub(crate) trait MoveSink {
    fn add(&mut self, mv: Move);
}

impl MoveSink for Vec<Move> {
    fn add(&mut self, mv: Move) {
        self.push(mv);
    }
}

fn push_moves(moves: &mut impl MoveSink, from: Square, targets: u64) {
    for to in Squares(targets) {
        moves.add(Move {
            from,
            to,
            promotion: None,
        });
    }
}

/// The pieces on the board, the player to move, the castling and en passant
/// rights, and the move counters.
///
/// A position holds an en passant square only while a pawn may legally
/// capture there. Two positions that differ in nothing a player could do are
/// therefore equal in every field but the counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub(crate) by_color: [u64; 2],
    pub(crate) by_kind: [u64; 6],
    pub(crate) side_to_move: Color,
    pub(crate) castling: u8,
    pub(crate) en_passant: Option<Square>,
    pub(crate) halfmove_clock: u32,
    pub(crate) fullmove_number: u32,
    pub(crate) hash: u64,
}

impl Position {
    /// The position at the start of a game.
    pub fn initial() -> Self {
        Self::from_fen(INITIAL).expect("the initial position is valid")
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Half-moves since the last capture or pawn move.
    pub fn halfmove_clock(&self) -> u32 {
        self.halfmove_clock
    }

    /// The number of the move about to be played, which starts at one and
    /// rises after each move by Black.
    pub fn fullmove_number(&self) -> u32 {
        self.fullmove_number
    }

    /// The square on which a pawn may capture en passant, if one may.
    pub fn en_passant(&self) -> Option<Square> {
        self.en_passant
    }

    /// A value that is equal for positions that [`repeat`](Self::repeats)
    /// each other, and almost always different otherwise.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        let kind = self.kind_at(square)?;
        let color = if self.by_color[Color::White.index()] & square.bit() != 0 {
            Color::White
        } else {
            Color::Black
        };
        Some(Piece { color, kind })
    }

    /// Every piece with its square, from a1 to h8.
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        Squares(self.occupied())
            .filter_map(|square| self.piece_at(square).map(|piece| (square, piece)))
    }

    pub fn king(&self, color: Color) -> Square {
        Square::from_index(self.set(color, PieceKind::King).trailing_zeros() as u8)
    }

    /// Whether the player to move is in check.
    pub fn is_check(&self) -> bool {
        self.is_attacked(self.king(self.side_to_move), self.side_to_move.opposite())
    }

    /// Every legal move of the player to move.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut legal = Vec::new();
        self.for_each_successor(|mv, _| legal.push(mv));
        legal
    }

    pub fn has_legal_move(&self) -> bool {
        let mut moves = Vec::with_capacity(64);
        self.candidate_moves(&mut moves);
        moves.into_iter().any(|mv| self.leaves_king_safe(mv))
    }

    /// Every legal move with the position it leads to.
    pub fn successors(&self) -> Vec<(Move, Position)> {
        let mut successors = Vec::new();
        self.for_each_successor(|mv, next| successors.push((mv, next)));
        successors
    }

    /// The position after `mv`, or `None` when the move is not legal.
    pub fn play(&self, mv: Move) -> Option<Position> {
        let mut moves = Vec::with_capacity(64);
        self.candidate_moves(&mut moves);
        if !moves.contains(&mv) {
            return None;
        }
        let next = self.make(mv);
        next.mover_is_safe().then_some(next)
    }

    /// Counts the sequences of `depth` legal half-moves that start here.
    pub fn perft(&self, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let mut count = 0;
        self.for_each_successor(|_, next| count += next.perft(depth - 1));
        count
    }

    /// Whether neither player could ever give checkmate with the pieces left:
    /// bare kings, a single bishop or knight, or bishops that all stand on
    /// squares of one colour.
    pub fn has_dead_material(&self) -> bool {
        let [pawns, knights, bishops, rooks, queens, _] = self.by_kind;
        if pawns | rooks | queens != 0 {
            return false;
        }
        if knights == 0 {
            bishops & LIGHT_SQUARES == 0 || bishops & !LIGHT_SQUARES == 0
        } else {
            knights.count_ones() == 1 && bishops == 0
        }
    }

    /// Whether the two are the same position for the repetition rules: the
    /// same pieces on the same squares, the same player to move, and the same
    /// castling and en passant rights.
    pub fn repeats(&self, other: &Self) -> bool {
        self.hash == other.hash
            && self.by_color == other.by_color
            && self.by_kind == other.by_kind
            && self.side_to_move == other.side_to_move
            && self.castling == other.castling
            && self.en_passant == other.en_passant
    }

    pub(crate) fn occupied(&self) -> u64 {
        self.by_color[0] | self.by_color[1]
    }

    /// The squares that hold `color`'s pieces of `kind`.
    pub(crate) fn set(&self, color: Color, kind: PieceKind) -> u64 {
        self.by_color[color.index()] & self.by_kind[kind.index()]
    }

    /// The kind of piece that `mv` would capture, en passant included.
    pub(crate) fn captured_kind(&self, mv: Move) -> Option<PieceKind> {
        self.kind_at(mv.to).or_else(|| {
            let en_passant =
                Some(mv.to) == self.en_passant && self.kind_at(mv.from) == Some(PieceKind::Pawn);
            en_passant.then_some(PieceKind::Pawn)
        })
    }

    pub(crate) fn kind_at(&self, square: Square) -> Option<PieceKind> {
        PieceKind::ALL
            .into_iter()
            .find(|kind| self.by_kind[kind.index()] & square.bit() != 0)
    }

    /// Puts a piece on an empty square or takes it off, keeping the hash in
    /// step.
    pub(crate) fn toggle(&mut self, color: Color, kind: PieceKind, square: Square) {
        self.by_color[color.index()] ^= square.bit();
        self.by_kind[kind.index()] ^= square.bit();
        self.hash ^= KEYS.pieces[color.index()][kind.index()][square.index()];
    }

    /// The hash worked out from nothing, which every position's stored hash
    /// must equal.
    pub(crate) fn computed_hash(&self) -> u64 {
        let mut hash = KEYS.castling[usize::from(self.castling)];
        for (square, piece) in self.pieces() {
            hash ^= KEYS.pieces[piece.color.index()][piece.kind.index()][square.index()];
        }
        if self.side_to_move == Color::Black {
            hash ^= KEYS.black_to_move;
        }
        if let Some(square) = self.en_passant {
            hash ^= KEYS.en_passant[usize::from(square.file())];
        }
        hash
    }

    pub(crate) fn is_attacked(&self, square: Square, by: Color) -> bool {
        let target = square.bit();
        let occupied = self.occupied();
        let attackers = |kind| self.set(by, kind);
        let queens = attackers(PieceKind::Queen);
        let bishops = attackers(PieceKind::Bishop) | queens;
        let rooks = attackers(PieceKind::Rook) | queens;
        // A piece attacks the square if the same kind of piece, standing on
        // the square, would attack it. For a pawn that means one of the
        // other colour. What stands in the way of a sliding piece is looked
        // at only if one shares a line with the square.
        pawn_attacks(target, by.opposite()) & attackers(PieceKind::Pawn) != 0
            || knight_attacks(target) & attackers(PieceKind::Knight) != 0
            || king_attacks(target) & attackers(PieceKind::King) != 0
            || (diagonals(square) & bishops != 0 && bishop_attacks(square, occupied) & bishops != 0)
            || (straights(square) & rooks != 0 && rook_attacks(square, occupied) & rooks != 0)
    }

    /// Whether `mv`, a candidate move of the player to move, could leave
    /// that player's king attacked. `in_check` says whether it is attacked
    /// now. Only if the answer is yes need the position after the move be
    /// tested with [`mover_is_safe`](Self::mover_is_safe).
    ///
    /// A king that is not attacked can come to be in three ways: it moves, a
    /// piece leaves a line it shared with the king and so opens the line, or
    /// a capture en passant takes a pawn off a square the capturer does not
    /// land on. Any other move leaves every line to the king as it was, or
    /// blocks one.
    pub(crate) fn may_expose_king(&self, mv: Move, in_check: bool) -> bool {
        let king = self.king(self.side_to_move);
        in_check
            || mv.from == king
            || Some(mv.to) == self.en_passant
            || (straights(king) | diagonals(king)) & mv.from.bit() != 0
    }

    /// Whether the player who has just moved left their own king unattacked.
    pub(crate) fn mover_is_safe(&self) -> bool {
        let mover = self.side_to_move.opposite();
        !self.is_attacked(self.king(mover), self.side_to_move)
    }

    fn leaves_king_safe(&self, mv: Move) -> bool {
        self.make(mv).mover_is_safe()
    }

    fn for_each_successor(&self, mut visit: impl FnMut(Move, Position)) {
        let mut moves = Vec::with_capacity(64);
        self.candidate_moves(&mut moves);
        let in_check = self.is_check();
        for mv in moves {
            let next = self.make(mv);
            if !self.may_expose_king(mv, in_check) || next.mover_is_safe() {
                visit(mv, next);
            }
        }
    }

    /// Every move that follows the rules of movement. Some leave the mover's
    /// own king attacked, which is tested once the move is made.
    pub(crate) fn candidate_moves(&self, moves: &mut impl MoveSink) {
        self.generate(moves, false);
    }

    /// The candidate moves that capture or promote.
    pub(crate) fn tactical_moves(&self, moves: &mut impl MoveSink) {
        self.generate(moves, true);
    }

    fn generate(&self, moves: &mut impl MoveSink, tactical: bool) {
        let us = self.side_to_move;
        let own = self.by_color[us.index()];
        let enemy = self.by_color[us.opposite().index()];
        let occupied = own | enemy;
        // Where a piece other than a pawn may go.
        let wanted = if tactical { enemy } else { !own };

        let (home_rank, last_rank) = match us {
            Color::White => (RANK_2, RANK_8),
            Color::Black => (RANK_7, RANK_1),
        };
        let forward = |set| match us {
            Color::White => north(set),
            Color::Black => south(set),
        };
        let en_passant = self.en_passant.map_or(0, Square::bit);
        for from in Squares(self.set(us, PieceKind::Pawn)) {
            let pawn = from.bit();
            let one_step = forward(pawn) & !occupied;
            let mut targets = pawn_attacks(pawn, us) & (enemy | en_passant);
            if tactical {
                targets |= one_step & last_rank;
            } else {
                targets |= one_step;
                if pawn & home_rank != 0 {
                    targets |= forward(one_step) & !occupied;
                }
            }
            if targets & last_rank == 0 {
                push_moves(moves, from, targets);
                continue;
            }
            for to in Squares(targets) {
                for kind in [
                    PieceKind::Queen,
                    PieceKind::Rook,
                    PieceKind::Bishop,
                    PieceKind::Knight,
                ] {
                    moves.add(Move {
                        from,
                        to,
                        promotion: Some(kind),
                    });
                }
            }
        }

        for from in Squares(self.set(us, PieceKind::Knight)) {
            push_moves(moves, from, knight_attacks(from.bit()) & wanted);
        }
        for from in Squares(self.set(us, PieceKind::Bishop)) {
            push_moves(moves, from, bishop_attacks(from, occupied) & wanted);
        }
        for from in Squares(self.set(us, PieceKind::Rook)) {
            push_moves(moves, from, rook_attacks(from, occupied) & wanted);
        }
        for from in Squares(self.set(us, PieceKind::Queen)) {
            let targets = bishop_attacks(from, occupied) | rook_attacks(from, occupied);
            push_moves(moves, from, targets & wanted);
        }
        let king = self.king(us);
        push_moves(moves, king, king_attacks(king.bit()) & wanted);
        if !tactical {
            self.castling_moves(moves);
        }
    }

    fn castling_moves(&self, moves: &mut impl MoveSink) {
        let us = self.side_to_move;
        let (king_side, queen_side, rank) = match us {
            Color::White => (WHITE_KING_SIDE, WHITE_QUEEN_SIDE, 0),
            Color::Black => (BLACK_KING_SIDE, BLACK_QUEEN_SIDE, 7),
        };
        if self.castling & (king_side | queen_side) == 0 {
            return;
        }
        // A right is only held while the king and that rook are at home.
        let square = |file| Square::new(file, rank).expect("a file on the board");
        let occupied = self.occupied();
        let empty = |files: &[u8]| files.iter().all(|&file| occupied & square(file).bit() == 0);
        let safe = |file| !self.is_attacked(square(file), us.opposite());
        // The king may not castle out of check or across an attacked square.
        // Its destination is tested like that of any other move.
        let mut castle = |to| {
            moves.add(Move {
                from: square(4),
                to: square(to),
                promotion: None,
            });
        };
        if self.castling & king_side != 0 && empty(&[5, 6]) && safe(4) && safe(5) {
            castle(6);
        }
        if self.castling & queen_side != 0 && empty(&[1, 2, 3]) && safe(4) && safe(3) {
            castle(2);
        }
    }

    /// Plays a candidate move without testing that it leaves the king safe.
    pub(crate) fn make(&self, mv: Move) -> Position {
        self.make_noting(mv, |_, _| {})
    }

    /// Plays a candidate move as [`make`](Self::make) does, and tells `note`
    /// of every piece it takes off a square (`false`) or puts on one
    /// (`true`).
    pub(crate) fn make_noting(
        &self,
        mv: Move,
        mut note: impl FnMut(Piece, (Square, bool)),
    ) -> Position {
        let us = self.side_to_move;
        let them = us.opposite();
        let kind = self
            .kind_at(mv.from)
            .expect("a candidate move starts on a piece");
        let on_file = |file, rank| Square::new(file, rank).expect("a square on the board");
        let mut next = self.clone();
        let mut change = |next: &mut Position, color, kind, square, put| {
            next.toggle(color, kind, square);
            note(Piece { color, kind }, (square, put));
        };

        let mut restarts_clock = kind == PieceKind::Pawn;
        if let Some(captured) = self.kind_at(mv.to) {
            change(&mut next, them, captured, mv.to, false);
            restarts_clock = true;
        }
        change(&mut next, us, kind, mv.from, false);
        change(&mut next, us, mv.promotion.unwrap_or(kind), mv.to, true);

        if kind == PieceKind::Pawn && Some(mv.to) == self.en_passant {
            // The captured pawn stands beside the capturer, not on its target.
            let beside = on_file(mv.to.file(), mv.from.rank());
            change(&mut next, them, PieceKind::Pawn, beside, false);
        }
        if kind == PieceKind::King && mv.from.file().abs_diff(mv.to.file()) == 2 {
            // Castling: the rook lands on the square the king crossed.
            let (rook_from, rook_to) = if mv.to.file() == 6 { (7, 5) } else { (0, 3) };
            let rank = mv.from.rank();
            change(
                &mut next,
                us,
                PieceKind::Rook,
                on_file(rook_from, rank),
                false,
            );
            change(&mut next, us, PieceKind::Rook, on_file(rook_to, rank), true);
        }

        next.castling &= !(rights_ended_at(mv.from) | rights_ended_at(mv.to));
        next.halfmove_clock = if restarts_clock {
            0
        } else {
            self.halfmove_clock.saturating_add(1)
        };
        if us == Color::Black {
            next.fullmove_number = next.fullmove_number.saturating_add(1);
        }
        next.side_to_move = them;
        next.en_passant = None;
        if kind == PieceKind::Pawn && mv.from.rank().abs_diff(mv.to.rank()) == 2 {
            let crossed = on_file(mv.from.file(), (mv.from.rank() + mv.to.rank()) / 2);
            next.en_passant = next.capturable_en_passant(crossed);
        }

        next.hash ^= KEYS.black_to_move
            ^ KEYS.castling[usize::from(self.castling)]
            ^ KEYS.castling[usize::from(next.castling)];
        for square in [self.en_passant, next.en_passant].into_iter().flatten() {
            next.hash ^= KEYS.en_passant[usize::from(square.file())];
        }
        next
    }

    /// The position with the move handed to the other player and nothing
    /// else changed. No game allows it: the search uses it to ask what the
    /// other player could do with a move for nothing. The player who passes
    /// must not be in check.
    pub(crate) fn pass(&self) -> Position {
        debug_assert!(!self.is_check(), "a player in check may not pass");
        let mut next = self.clone();
        next.side_to_move = self.side_to_move.opposite();
        next.halfmove_clock = self.halfmove_clock.saturating_add(1);
        if self.side_to_move == Color::Black {
            next.fullmove_number = next.fullmove_number.saturating_add(1);
        }
        next.hash ^= KEYS.black_to_move;
        // The chance to capture en passant lasts for one move only.
        if let Some(square) = next.en_passant.take() {
            next.hash ^= KEYS.en_passant[usize::from(square.file())];
        }
        next
    }

    /// Whether the player to move has a piece besides pawns and the king.
    pub(crate) fn mover_has_pieces(&self) -> bool {
        let pawns_and_kings =
            self.by_kind[PieceKind::Pawn.index()] | self.by_kind[PieceKind::King.index()];
        self.by_color[self.side_to_move.index()] & !pawns_and_kings != 0
    }

    /// `square` if the player to move has a pawn that may legally capture en
    /// passant there, given that a pawn has just crossed it.
    pub(crate) fn capturable_en_passant(&self, square: Square) -> Option<Square> {
        let us = self.side_to_move;
        let capturers = pawn_attacks(square.bit(), us.opposite()) & self.set(us, PieceKind::Pawn);
        if capturers == 0 {
            return None;
        }
        let mut crossed = self.clone();
        crossed.en_passant = Some(square);
        Squares(capturers)
            .any(|from| {
                crossed.leaves_king_safe(Move {
                    from,
                    to: square,
                    promotion: None,
                })
            })
            .then_some(square)
    }
}
