use engine_chess::{Action, Color, Game, Move, Position, Square};
use engine_common::{RenderPoint, RenderRect};

use crate::draw::squares;
use crate::fixture;
use crate::{Board, Detail, Orientation, Selection, Squares, TooSmall, Transition};

/// A square area of `side`, centred on the origin.
fn area(side: f32) -> RenderRect {
    RenderRect {
        min: RenderPoint::new(-side / 2.0, -side / 2.0),
        max: RenderPoint::new(side / 2.0, side / 2.0),
    }
}

fn square(name: &str) -> Square {
    Square::from_name(name).unwrap()
}

#[test]
fn a_board_takes_the_detail_its_squares_allow_in_the_view() {
    // In a view 480 units high a unit is a pixel on the shortest screen, and
    // a board is eight squares and a half, frame included.
    let fit = |side, view| Board::fit(area(side), view, Orientation::WhiteAtBottom);
    let full = fit(136.0, 480.0).unwrap();
    assert_eq!((full.detail(), full.square_side()), (Detail::Full, 16.0));
    assert_eq!(fit(135.9, 480.0).unwrap().detail(), Detail::Compact);
    let least = fit(85.0, 480.0).unwrap();
    assert_eq!(
        (least.detail(), least.square_side()),
        (Detail::Compact, 10.0)
    );
    assert_eq!(fit(84.9, 480.0), Err(TooSmall { needed: 85.0 }));
    // Twice the world in view makes the same board half as large on screen.
    assert_eq!(fit(85.0, 960.0), Err(TooSmall { needed: 170.0 }));
    assert_eq!(fit(272.0, 960.0).unwrap().detail(), Detail::Full);
}

#[test]
fn the_side_reported_as_needed_is_enough() {
    for view in [100.0, 333.3, 480.0, 720.0, 1000.0, 1234.5] {
        let Err(TooSmall { needed }) = Board::fit(area(1.0), view, Orientation::WhiteAtBottom)
        else {
            panic!("a board of one unit is too small in a view of {view}");
        };
        let board = Board::fit(area(needed), view, Orientation::WhiteAtBottom)
            .unwrap_or_else(|error| panic!("{error} in a view of {view}"));
        assert_eq!(board.detail(), Detail::Compact);
    }
}

#[test]
fn a_board_is_centred_in_its_area_and_bounded_by_it() {
    let area = RenderRect {
        min: RenderPoint::new(10.0, 20.0),
        max: RenderPoint::new(180.0, 105.0),
    };
    let board = Board::fit(area, 480.0, Orientation::WhiteAtBottom).unwrap();
    assert_eq!(
        board.bounds(),
        RenderRect {
            min: RenderPoint::new(52.5, 20.0),
            max: RenderPoint::new(137.5, 105.0),
        }
    );
}

#[test]
fn no_board_fits_a_view_or_an_area_that_is_not_a_number() {
    let fit = |area, view| Board::fit(area, view, Orientation::WhiteAtBottom);
    for view in [0.0, -480.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            fit(area(400.0), view),
            Err(TooSmall {
                needed: f32::INFINITY
            })
        );
    }
    let mut unknown = area(400.0);
    unknown.min.x = f32::NAN;
    assert!(fit(unknown, 480.0).is_err());
    let backwards = RenderRect {
        min: area(400.0).max,
        max: area(400.0).min,
    };
    assert!(fit(backwards, 480.0).is_err());
}

const ORIENTATIONS: [Orientation; 4] = [
    Orientation::WhiteAtBottom,
    Orientation::BlackAtBottom,
    Orientation::WhiteAtLeft,
    Orientation::WhiteAtRight,
];

#[test]
fn every_square_maps_to_its_place_and_back_every_way_round() {
    for orientation in ORIENTATIONS {
        let board = Board::fit(area(170.0), 480.0, orientation).unwrap();
        for square in squares() {
            let rect = board.square_rect(square);
            assert_eq!(rect.width(), board.square_side());
            assert_eq!(rect.height(), board.square_side());
            for (across, up) in [
                (0.5, 0.5),
                (0.02, 0.02),
                (0.98, 0.02),
                (0.02, 0.98),
                (0.98, 0.98),
            ] {
                let point = RenderPoint::new(
                    rect.min.x + across * rect.width(),
                    rect.min.y + up * rect.height(),
                );
                assert_eq!(
                    board.square_at(point),
                    Some(square),
                    "{orientation:?} {square}"
                );
            }
        }
    }
}

#[test]
fn white_at_bottom_has_a1_lower_left_and_black_at_bottom_turns_the_board() {
    let white = Board::fit(area(170.0), 480.0, Orientation::WhiteAtBottom).unwrap();
    let black = Board::fit(area(170.0), 480.0, Orientation::BlackAtBottom).unwrap();
    let corner = |board: &Board, name| board.square_rect(square(name)).min;
    // Inside a frame a quarter of a square wide.
    let frame = white.square_side() / 4.0;
    let bounds = white.bounds();
    assert_eq!(
        corner(&white, "a1"),
        RenderPoint::new(bounds.min.x + frame, bounds.min.y + frame)
    );
    assert!(corner(&white, "h1").x > corner(&white, "a1").x);
    assert!(corner(&white, "a8").y > corner(&white, "a1").y);
    for (name, opposite) in [("a1", "h8"), ("h1", "a8"), ("e4", "d5"), ("c7", "f2")] {
        assert_eq!(corner(&white, name), corner(&black, opposite));
    }
}

#[test]
fn turned_a_quarter_white_sits_at_the_left_or_at_the_right() {
    let fit = |orientation| Board::fit(area(170.0), 480.0, orientation).unwrap();
    let (bottom, left, right) = (
        fit(Orientation::WhiteAtBottom),
        fit(Orientation::WhiteAtLeft),
        fit(Orientation::WhiteAtRight),
    );
    let corner = |board: &Board, name| board.square_rect(square(name)).min;
    let frame = left.square_side() / 4.0;
    let bounds = left.bounds();
    // White's back rank runs up the left side, from h1 at its foot to a1
    // at its top, and Black's down the right.
    assert_eq!(
        corner(&left, "h1"),
        RenderPoint::new(bounds.min.x + frame, bounds.min.y + frame)
    );
    assert_eq!(corner(&left, "a1").x, corner(&left, "h1").x);
    assert!(corner(&left, "a1").y > corner(&left, "h1").y);
    assert!(corner(&left, "a8").x > corner(&left, "a1").x);
    assert_eq!(corner(&left, "a8").y, corner(&left, "a1").y);
    // Each is a quarter turn of White at the bottom, clockwise for White at
    // the left, about the centre of the board.
    let middle = |board: &Board, square| {
        let rect = board.square_rect(square);
        let bounds = board.bounds();
        (
            (rect.min.x + rect.max.x - bounds.min.x - bounds.max.x) / 2.0,
            (rect.min.y + rect.max.y - bounds.min.y - bounds.max.y) / 2.0,
        )
    };
    for square in squares() {
        let (x, y) = middle(&bottom, square);
        assert_eq!(middle(&left, square), (y, -x), "{square}");
        assert_eq!(middle(&right, square), (-y, x), "{square}");
    }
}

#[test]
fn the_frame_and_what_lies_beyond_it_are_on_no_square() {
    let board = Board::fit(area(170.0), 480.0, Orientation::WhiteAtBottom).unwrap();
    let bounds = board.bounds();
    let frame = board.square_side() / 4.0;
    let middle = (bounds.min.x + bounds.max.x) / 2.0;
    for point in [
        bounds.min,
        bounds.max,
        RenderPoint::new(middle, bounds.min.y + frame / 2.0),
        RenderPoint::new(middle, bounds.max.y - frame / 2.0),
        RenderPoint::new(bounds.min.x + frame / 2.0, middle),
        RenderPoint::new(bounds.max.x - frame / 2.0, middle),
        RenderPoint::new(1000.0, 0.0),
        RenderPoint::new(f32::NAN, 0.0),
    ] {
        assert_eq!(board.square_at(point), None, "{point:?}");
    }
}

#[test]
fn a_set_of_squares_holds_what_was_put_in_it() {
    let mut set: Squares = ["a1", "h8", "e4"].map(square).into_iter().collect();
    assert_eq!(set.len(), 3);
    assert!(set.contains(square("e4")) && !set.contains(square("e5")));
    set.insert(square("e4"));
    set.insert(square("e5"));
    assert_eq!(set.len(), 4);
    assert!(Squares::EMPTY.is_empty() && !set.is_empty());
    assert_eq!(squares().collect::<Squares>().len(), 64);
}

#[test]
fn a_selection_reaches_where_the_piece_may_legally_go() {
    let game = Game::new(1, Position::initial());
    let knight = Selection::of(&game, square("g1"));
    let reached = ["f3", "h3"].map(square).into_iter().collect();
    assert_eq!(knight.destinations, reached);
    assert!(Selection::of(&game, square("e8")).destinations.is_empty());
}

#[test]
fn only_a_move_is_put_in_motion() {
    let mut game = Game::new(1, Position::initial());
    assert_eq!(Transition::of_last_move(&game, 0.5), None);
    let e4 = Move::parse("e2e4").unwrap();
    game.apply(game.revision(), Color::White, Action::Move(e4))
        .unwrap();
    let transition = Transition::of_last_move(&game, 0.25).unwrap();
    assert_eq!(transition.before, Position::initial());
    assert_eq!((transition.mv, transition.revision), (e4, game.revision()));
    assert_eq!(transition.progress, 0.25);
    game.apply(game.revision(), Color::Black, Action::Resign)
        .unwrap();
    assert_eq!(Transition::of_last_move(&game, 0.5), None);
}

#[test]
fn the_fixtures_show_each_thing_a_board_draws() {
    let fixtures = fixture::all();
    let names: Vec<_> = fixtures.iter().map(|fixture| fixture.name).collect();
    assert_eq!(
        names,
        [
            "start",
            "middlegame",
            "check",
            "capture",
            "castling",
            "en-passant",
            "promotion",
            "promoted",
            "checkmate",
            "stalemate",
        ]
    );
    let shown = |name: &str| {
        &fixtures
            .iter()
            .find(|fixture| fixture.name == name)
            .unwrap()
            .presentation
    };
    assert_eq!(shown("start").position, Position::initial());
    assert!(shown("middlegame").selection.is_some());
    assert!(shown("check").position.is_check());
    for name in ["capture", "castling", "en-passant", "promotion"] {
        assert!(shown(name).motion().is_some(), "{name} is in motion");
    }
    assert!(shown("promoted").position.is_check());
    assert!(shown("checkmate").outcome.is_some() && shown("stalemate").outcome.is_some());
    assert!(
        fixtures
            .iter()
            .any(|fixture| fixture.orientation == Orientation::BlackAtBottom)
    );
}
