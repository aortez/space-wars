//! Presentations to draw in tests and rendered fixtures: one of each thing a
//! board shows. Each position is reached by legal moves, so the rules could
//! reach it in a game.

use engine_chess::{Action, Game, Move, Position, Square};

use crate::{Orientation, Presentation, Selection, Transition};

/// How far a move in motion has gone, in the fixtures that show one.
pub const HALFWAY: f32 = 0.5;

/// A presentation to draw, and which way up.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    pub name: &'static str,
    pub orientation: Orientation,
    pub presentation: Presentation,
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Every fixture, in a fixed order.
pub fn all() -> Vec<Fixture> {
    let at_rest = |name, orientation, game: &Game| Fixture {
        name,
        orientation,
        presentation: Presentation::of(game),
    };
    let in_motion = |name, game: &Game| Fixture {
        name,
        orientation: Orientation::WhiteAtBottom,
        presentation: Presentation {
            transition: Transition::of_last_move(game, HALFWAY),
            ..Presentation::of(game)
        },
    };
    let ruy_lopez = game(
        START,
        "e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 b5a4 g8f6 e1g1 f8e7 f1e1 b7b5 a4b3 d7d6 c2c3 e8g8 h2h3",
    );
    let knight = square("f6");
    let promotion = game("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7a8q");
    vec![
        at_rest("start", Orientation::WhiteAtBottom, &game(START, "")),
        Fixture {
            name: "middlegame",
            orientation: Orientation::WhiteAtBottom,
            presentation: Presentation {
                selection: Some(Selection::of(&ruy_lopez, knight)),
                ..Presentation::of(&ruy_lopez)
            },
        },
        at_rest(
            "check",
            Orientation::BlackAtBottom,
            &game(START, "e2e4 d7d5 f1b5"),
        ),
        in_motion("capture", &game(START, "e2e4 d7d5 e4d5")),
        in_motion(
            "castling",
            &game(START, "e2e4 e7e5 g1f3 b8c6 f1c4 f8c5 e1g1"),
        ),
        in_motion("en-passant", &game(START, "e2e4 g8f6 e4e5 d7d5 e5d6")),
        in_motion("promotion", &promotion),
        at_rest("promoted", Orientation::WhiteAtBottom, &promotion),
        at_rest(
            "checkmate",
            Orientation::WhiteAtBottom,
            &game(START, "f2f3 e7e5 g2g4 d8h4"),
        ),
        at_rest(
            "stalemate",
            Orientation::WhiteAtBottom,
            &game("7k/8/6K1/8/8/8/8/5Q2 w - - 0 1", "f1f7"),
        ),
    ]
}

/// The game reached from `fen` by `moves`, given in coordinate notation and
/// separated by spaces.
fn game(fen: &str, moves: &str) -> Game {
    let start = Position::from_fen(fen).expect("a fixture starts from a valid position");
    let mut game = Game::new(1, start);
    for text in moves.split_whitespace() {
        let mv = Move::parse(text).expect("a fixture's moves are in coordinate notation");
        let player = game.position().side_to_move();
        game.apply(game.revision(), player, Action::Move(mv))
            .expect("a fixture's moves are legal");
    }
    game
}

fn square(name: &str) -> Square {
    Square::from_name(name).expect("a fixture names real squares")
}
