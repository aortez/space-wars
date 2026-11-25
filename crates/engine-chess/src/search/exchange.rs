//! What a move wins or loses once the square it lands on has been fought
//! over.
//!
//! Past the full search, where only captures are followed, the search leaves
//! out those that lose material. Everywhere else it tries them last.

use crate::bitboard::{
    bishop_attacks, diagonals, king_attacks, knight_attacks, pawn_attacks, rook_attacks, straights,
};
use crate::board::{Color, PieceKind, Square};
use crate::moves::Move;
use crate::position::Position;

use super::eval::VALUES;

fn value(kind: PieceKind) -> i32 {
    VALUES[kind.index()]
}

/// What `mv` wins in centipawns for the player who makes it, if both players
/// then capture on the square it lands on for as long as that pays them. It
/// moves a piece of kind `mover` and captures one of kind `captured`, if any.
///
/// Each capture is made by the least valuable piece that attacks the square,
/// and of two alike by the one on the lower square, counting from a1 along
/// the ranks. A piece behind another on the same line attacks once the one in
/// front has gone. A king captures only what is no longer defended.
///
/// Nothing else about the position is considered. A pinned piece captures as
/// freely as any other, and of the promotions only the one `mv` itself makes
/// is counted: a pawn that captures onto the last rank later stays a pawn.
fn exchange(position: &Position, mv: Move, mover: PieceKind, captured: Option<PieceKind>) -> i32 {
    let mut occupied = (position.occupied() ^ mv.from.bit()) | mv.to.bit();
    let mut won = 0;
    if let Some(captured) = captured {
        won = value(captured);
        if position.kind_at(mv.to).is_none() {
            // En passant: the pawn taken stands beside the capturer.
            let beside = Square::new(mv.to.file(), mv.from.rank()).expect("a square on the board");
            occupied ^= beside.bit();
        }
    }
    let mut standing = mover;
    if let Some(kind) = mv.promotion {
        won += value(kind) - value(PieceKind::Pawn);
        standing = kind;
    }
    let them = position.side_to_move().opposite();
    won - recapture(position, mv.to, them, occupied, value(standing))
}

/// Whether `mv` loses material by [`exchange`]. It moves a piece of kind
/// `mover` and captures one of kind `captured`, if any.
pub(super) fn loses(
    position: &Position,
    mv: Move,
    mover: PieceKind,
    captured: Option<PieceKind>,
) -> bool {
    if let (Some(captured), None) = (captured, mv.promotion) {
        // A piece that takes one worth as much cannot come out behind: at
        // worst it is taken in turn, and the exchange can stop there.
        if value(captured) >= value(mover) {
            return false;
        }
        // A pawn that takes the piece in turn is the cheapest capturer there
        // could be, and can itself cost no more than a pawn. If the piece is
        // worth more than what it took and a pawn together, it comes out
        // behind whatever follows.
        let us = position.side_to_move();
        let their_pawns = position.set(us.opposite(), PieceKind::Pawn);
        let defended = pawn_attacks(mv.to.bit(), us) & their_pawns != 0;
        if defended && value(captured) + value(PieceKind::Pawn) < value(mover) {
            return true;
        }
    }
    exchange(position, mv, mover, captured) < 0
}

/// The most `side` can win by capturing on `square`, where a piece worth
/// `prize` stands and only the `occupied` squares still hold pieces.
fn recapture(position: &Position, square: Square, side: Color, occupied: u64, prize: i32) -> i32 {
    let ours = attackers(position, square, occupied) & position.by_color[side.index()];
    let cheapest = PieceKind::ALL.into_iter().find_map(|kind| {
        let alike = ours & position.by_kind[kind.index()];
        // The lowest square of the set.
        (alike != 0).then(|| (kind, alike & alike.wrapping_neg()))
    });
    let Some((kind, from)) = cheapest else {
        return 0;
    };
    let occupied = occupied ^ from;
    let them = side.opposite();
    if kind == PieceKind::King {
        let defenders = attackers(position, square, occupied) & position.by_color[them.index()];
        if defenders != 0 {
            return 0;
        }
    }
    // The player may decline to capture.
    (prize - recapture(position, square, them, occupied, value(kind))).max(0)
}

/// The pieces of both players that attack `square`, of those that stand on
/// `occupied` squares.
fn attackers(position: &Position, square: Square, occupied: u64) -> u64 {
    let target = square.bit();
    let of = |kind: PieceKind| position.by_kind[kind.index()];
    let [white, black] = position.by_color;
    let pawns = of(PieceKind::Pawn);
    let queens = of(PieceKind::Queen);
    // A piece attacks the square if the same kind of piece, standing on the
    // square, would attack it. For a pawn that means one of the other colour.
    let bishops = (of(PieceKind::Bishop) | queens) & occupied;
    let rooks = (of(PieceKind::Rook) | queens) & occupied;
    let mut attackers = (pawn_attacks(target, Color::Black) & pawns & white)
        | (pawn_attacks(target, Color::White) & pawns & black)
        | (knight_attacks(target) & of(PieceKind::Knight))
        | (king_attacks(target) & of(PieceKind::King));
    // What stands in the way of a sliding piece is looked at only if one
    // shares a line with the square.
    if diagonals(square) & bishops != 0 {
        attackers |= bishop_attacks(square, occupied) & bishops;
    }
    if straights(square) & rooks != 0 {
        attackers |= rook_attacks(square, occupied) & rooks;
    }
    attackers & occupied
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::board::Piece;
    use crate::search::mix;

    type Board = [Option<Piece>; 64];

    fn worth(kind: PieceKind) -> i32 {
        match kind {
            PieceKind::Pawn => 100,
            PieceKind::Knight => 330,
            PieceKind::Bishop => 350,
            PieceKind::Rook => 520,
            PieceKind::Queen => 980,
            // Dearer than anything, so that the king captures last.
            PieceKind::King => 10_000,
        }
    }

    fn squares() -> impl Iterator<Item = Square> {
        (0..64).map(Square::from_index)
    }

    /// What `mv` wins by [`exchange`], and whether it [`loses`].
    fn worked(position: &Position, mv: Move) -> (i32, bool) {
        let mover = position.kind_at(mv.from).unwrap();
        let captured = position.captured_kind(mv);
        (
            exchange(position, mv, mover, captured),
            loses(position, mv, mover, captured),
        )
    }

    /// Whether the piece on `from` attacks `to`.
    fn attacks(board: &Board, from: Square, to: Square) -> bool {
        let Some(piece) = board[from.index()] else {
            return false;
        };
        let files = i32::from(to.file()) - i32::from(from.file());
        let ranks = i32::from(to.rank()) - i32::from(from.rank());
        let clear = || {
            (1..files.abs().max(ranks.abs())).all(|step| {
                let file = i32::from(from.file()) + files.signum() * step;
                let rank = i32::from(from.rank()) + ranks.signum() * step;
                let between = Square::new(file as u8, rank as u8).unwrap();
                board[between.index()].is_none()
            })
        };
        let straight = (files == 0) != (ranks == 0);
        let diagonal = files.abs() == ranks.abs() && files != 0;
        match piece.kind {
            PieceKind::Pawn => {
                let forward = if piece.color == Color::White { 1 } else { -1 };
                files.abs() == 1 && ranks == forward
            }
            PieceKind::Knight => matches!((files.abs(), ranks.abs()), (1, 2) | (2, 1)),
            PieceKind::Bishop => diagonal && clear(),
            PieceKind::Rook => straight && clear(),
            PieceKind::Queen => (straight || diagonal) && clear(),
            PieceKind::King => files.abs().max(ranks.abs()) == 1,
        }
    }

    /// The most `side` can win by capturing the piece on `square`.
    fn plain_recapture(board: &mut Board, square: Square, side: Color) -> i32 {
        let prize = worth(board[square.index()].unwrap().kind);
        let attacker = squares()
            .filter(|&from| board[from.index()].is_some_and(|piece| piece.color == side))
            .filter(|&from| attacks(board, from, square))
            .min_by_key(|&from| (worth(board[from.index()].unwrap().kind), from.index()));
        let Some(from) = attacker else {
            return 0;
        };
        let piece = board[from.index()].take().unwrap();
        let taken = board[square.index()].replace(piece);
        let defended = squares().any(|other| {
            board[other.index()].is_some_and(|piece| piece.color != side)
                && attacks(board, other, square)
        });
        let won = if piece.kind == PieceKind::King && defended {
            0
        } else {
            (prize - plain_recapture(board, square, side.opposite())).max(0)
        };
        board[from.index()] = Some(piece);
        board[square.index()] = taken;
        won
    }

    /// The exchange again, worked out a square at a time on a plain board,
    /// for the real one to be compared with.
    pub(in crate::search) fn plain_exchange(position: &Position, mv: Move) -> i32 {
        let mut board: Board = [None; 64];
        for (square, piece) in position.pieces() {
            board[square.index()] = Some(piece);
        }
        let mover = board[mv.from.index()].take().unwrap();
        let mut won = board[mv.to.index()].map_or(0, |taken| worth(taken.kind));
        let sideways = mv.from.file() != mv.to.file();
        if mover.kind == PieceKind::Pawn && sideways && board[mv.to.index()].is_none() {
            // En passant.
            let beside = Square::new(mv.to.file(), mv.from.rank()).unwrap();
            board[beside.index()] = None;
            won = worth(PieceKind::Pawn);
        }
        let kind = mv.promotion.unwrap_or(mover.kind);
        won += worth(kind) - worth(mover.kind);
        board[mv.to.index()] = Some(Piece {
            color: mover.color,
            kind,
        });
        won - plain_recapture(&mut board, mv.to, mover.color.opposite())
    }

    #[test]
    fn an_exchange_is_worth_what_its_captures_come_to() {
        let cases = [
            (
                "a rook takes a pawn that nothing defends",
                "1k1r4/1pp4p/p7/4p3/8/P5P1/1PP4P/2K1R3 w - - 0 1",
                "e1e5",
                100,
            ),
            (
                "a knight takes a pawn and is taken, and going on would cost more",
                "1k1r3q/1ppn3p/p4b2/4p3/8/P2N2P1/1PP1R1BP/2K1Q3 w - - 0 1",
                "d3e5",
                100 - 330,
            ),
            (
                "a queen takes a pawn that a pawn defends",
                "4k3/8/3p4/4p3/8/8/4Q3/4K3 w - - 0 1",
                "e2e5",
                100 - 980,
            ),
            (
                "a queen steps where a pawn takes it",
                "6k1/8/3p4/8/8/8/4Q3/4K3 w - - 0 1",
                "e2e5",
                -980,
            ),
            (
                "the rook behind takes back, so the defender does not take",
                "3rk3/8/8/3p4/8/8/3R4/3RK3 w - - 0 1",
                "d2d5",
                100,
            ),
            (
                "en passant, where the king may not take a defended pawn",
                "4k3/8/8/8/2pPp3/8/4K3/8 b - d3 0 1",
                "c4d3",
                100,
            ),
            (
                "en passant, where the king takes back",
                "4k3/8/8/8/2pP4/8/4K3/8 b - d3 0 1",
                "c4d3",
                0,
            ),
            (
                // With the pawn gone from d4, the rook on d8 defends d3.
                "en passant opens the file behind the pawn it takes",
                "3r2k1/8/8/8/2pP4/8/8/5BK1 b - d3 0 1",
                "c4d3",
                100,
            ),
            (
                "a pawn promotes where a rook takes it",
                "1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                "a7a8q",
                980 - 100 - 980,
            ),
            (
                "a pawn takes a rook and promotes",
                "1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                "a7b8q",
                520 + 980 - 100,
            ),
            (
                "a pawn takes a rook and becomes a knight",
                "1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                "a7b8n",
                520 + 330 - 100,
            ),
            (
                "a king takes back what nothing else defends",
                "8/8/8/8/8/1k6/1p6/1Q2K3 w - - 0 1",
                "b1b2",
                100 - 980,
            ),
            (
                "a king may not take back what a bishop defends",
                "8/8/8/8/8/2k5/1p6/Q1B1K3 w - - 0 1",
                "a1b2",
                100,
            ),
            (
                // The queen on e3 takes the pawn before the one on h5 would,
                // and uncovers the rook behind it. Had the other queen gone
                // first, the pawn would have been won for nothing.
                "of two alike, the one on the lower square captures first",
                "6k1/8/3p4/4n2Q/5P2/4Q3/7K/4r3 w - - 0 1",
                "f4e5",
                330 - 100,
            ),
        ];
        for (what, fen, text, value) in cases {
            let position = Position::from_fen(fen).unwrap();
            let mv = Move::parse(text).unwrap();
            assert!(position.legal_moves().contains(&mv), "{what}: {text}");
            assert_eq!(worked(&position, mv), (value, value < 0), "{what}");
            assert_eq!(plain_exchange(&position, mv), value, "{what}, plainly");
        }
    }

    #[test]
    fn the_exchange_agrees_with_one_worked_out_on_a_plain_board() {
        let starts = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "r1bq1rk1/pp2ppbp/2np1np1/8/3NP3/2N1BP2/PPPQ2PP/R3KB1R w KQ - 3 9",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ];
        let (mut moves_checked, mut losing) = (0, 0);
        for (start, fen) in starts.into_iter().enumerate() {
            for game in 0..6 {
                let mut position = Position::from_fen(fen).unwrap();
                let mut state = mix((start * 100 + game) as u64);
                // Random moves leave pieces hanging everywhere.
                for _ in 0..80 {
                    let moves = position.legal_moves();
                    if moves.is_empty() {
                        break;
                    }
                    for &mv in &moves {
                        let king = position.kind_at(mv.from) == Some(PieceKind::King);
                        if king && mv.from.file().abs_diff(mv.to.file()) == 2 {
                            // Castling moves two pieces and captures nothing.
                            continue;
                        }
                        let value = plain_exchange(&position, mv);
                        assert_eq!(
                            worked(&position, mv),
                            (value, value < 0),
                            "{position}: {mv}"
                        );
                        moves_checked += 1;
                        losing += u32::from(value < 0);
                    }
                    state = mix(state);
                    let chosen = moves[(state % moves.len() as u64) as usize];
                    position = position.play(chosen).unwrap();
                }
            }
        }
        assert!(moves_checked > 20_000, "{moves_checked} moves");
        assert!(losing > 2_000, "{losing} of them lose material");
    }
}
