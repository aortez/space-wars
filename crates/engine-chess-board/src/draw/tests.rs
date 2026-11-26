use engine_chess::{Color, Move, Piece, PieceKind, Square};
use engine_common::{Camera2, RenderFrame, RenderPoint, RenderPrimitive, RenderRect};

use super::*;
use crate::fixture::{self, Fixture};
use crate::{Board, Detail, Orientation, Presentation, Selection, Transition};

/// Sides of a board in a view 480 units high: compact at its least, and
/// full.
const COMPACT_SIDE: f32 = 85.0;
const FULL_SIDE: f32 = 400.0;

fn board(side: f32, orientation: Orientation) -> Board {
    let area = RenderRect {
        min: RenderPoint::new(-side / 2.0, -side / 2.0),
        max: RenderPoint::new(side / 2.0, side / 2.0),
    };
    Board::fit(area, 480.0, orientation).unwrap()
}

fn drawn(board: &Board, presentation: &Presentation) -> RenderFrame {
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 480.0));
    draw(&mut frame, 7, board, presentation);
    frame
}

fn named(name: &str) -> Fixture {
    fixture::all()
        .into_iter()
        .find(|fixture| fixture.name == name)
        .unwrap()
}

fn square(name: &str) -> Square {
    Square::from_name(name).unwrap()
}

/// The primitives of a frame, in an order that ignores the order drawn.
fn unordered(frame: &RenderFrame) -> Vec<String> {
    let mut primitives: Vec<_> = frame
        .layers
        .iter()
        .flat_map(|layer| &layer.primitives)
        .map(|primitive| format!("{primitive:?}"))
        .collect();
    primitives.sort();
    primitives
}

#[test]
fn every_fixture_draws_filled_finite_primitives_within_the_board() {
    for fixture in fixture::all() {
        for side in [COMPACT_SIDE, FULL_SIDE] {
            let board = board(side, fixture.orientation);
            let frame = drawn(&board, &fixture.presentation);
            let RenderRect { min, max } = board.bounds();
            let inside = |point: RenderPoint| {
                point.x.is_finite()
                    && point.y.is_finite()
                    && (min.x..=max.x).contains(&point.x)
                    && (min.y..=max.y).contains(&point.y)
            };
            assert_eq!(frame.layers.len(), 1);
            assert_eq!(frame.layers[0].z, 7);
            let primitives = &frame.layers[0].primitives;
            assert!(primitives.len() <= MAX_PRIMITIVES, "{}", fixture.name);
            for primitive in primitives {
                match primitive {
                    RenderPrimitive::Polygon(polygon) => {
                        assert!(polygon.points.len() >= 3);
                        assert!(polygon.fill.is_some() && polygon.stroke.is_none());
                        assert!(polygon.points.iter().all(|&point| inside(point)));
                    }
                    RenderPrimitive::Circle(circle) => {
                        assert!(circle.fill.is_some() && circle.stroke.is_none());
                        let RenderPoint { x, y } = circle.center;
                        let r = circle.radius;
                        assert!(r > 0.0);
                        assert!(inside(RenderPoint::new(x - r, y - r)));
                        assert!(inside(RenderPoint::new(x + r, y + r)));
                    }
                    other => panic!("{}: {other:?}", fixture.name),
                }
            }
        }
    }
}

#[test]
fn drawing_twice_gives_the_same_primitives_and_changes_nothing() {
    for fixture in fixture::all() {
        let before = fixture.presentation.clone();
        for side in [COMPACT_SIDE, FULL_SIDE] {
            let board = board(side, fixture.orientation);
            assert_eq!(
                drawn(&board, &fixture.presentation),
                drawn(&board, &fixture.presentation)
            );
        }
        assert_eq!(fixture.presentation, before);
    }
}

#[test]
fn no_presentation_draws_more_than_the_most_primitives() {
    assert!(pieces::most_polygons(Detail::Full) <= PIECE_POLYGONS);
    assert!(pieces::most_polygons(Detail::Compact) <= PIECE_POLYGONS);
    // Every piece still on the board, a check, a last move, and every square
    // offered to the selected piece.
    let mut crowded = named("check").presentation;
    crowded.selection = Some(Selection {
        square: square("e8"),
        destinations: squares().collect(),
    });
    for side in [COMPACT_SIDE, FULL_SIDE] {
        let count = drawn(&board(side, Orientation::WhiteAtBottom), &crowded).layers[0]
            .primitives
            .len();
        assert!(count <= MAX_PRIMITIVES, "{count}");
    }
    // The start, as the Clock would show it, takes far fewer.
    let start = named("start").presentation;
    let count = drawn(&board(COMPACT_SIDE, Orientation::WhiteAtBottom), &start).layers[0]
        .primitives
        .len();
    assert!(count <= 3 + 32 + 32 * 4, "{count}");
}

#[test]
fn a_move_in_motion_starts_from_the_position_it_left_and_arrives_at_the_next() {
    for fixture in fixture::all() {
        let Some(transition) = fixture.presentation.transition.clone() else {
            continue;
        };
        for side in [COMPACT_SIDE, FULL_SIDE] {
            let board = board(side, fixture.orientation);
            let at = |progress| Presentation {
                transition: Some(Transition {
                    progress,
                    ..transition.clone()
                }),
                ..fixture.presentation.clone()
            };
            let left = Presentation {
                position: transition.before.clone(),
                transition: None,
                ..fixture.presentation.clone()
            };
            assert_eq!(
                unordered(&drawn(&board, &at(0.0))),
                unordered(&drawn(&board, &left)),
                "{}",
                fixture.name
            );
            let reached = Presentation {
                transition: None,
                ..fixture.presentation.clone()
            };
            for progress in [1.0, 2.0, f32::NAN] {
                assert_eq!(
                    drawn(&board, &at(progress)),
                    drawn(&board, &reached),
                    "{} at {progress}",
                    fixture.name
                );
            }
            assert_ne!(drawn(&board, &at(0.5)), drawn(&board, &reached));
        }
    }
}

#[test]
fn a_transition_made_for_another_revision_or_move_is_not_drawn() {
    let fixture = named("capture");
    let board = board(FULL_SIDE, fixture.orientation);
    let reached = Presentation {
        transition: None,
        ..fixture.presentation.clone()
    };
    let mut stale = fixture.presentation.clone();
    stale.transition.as_mut().unwrap().revision.actions -= 1;
    assert_eq!(drawn(&board, &stale), drawn(&board, &reached));
    // A legal move from the same position, which leads elsewhere.
    let mut other = fixture.presentation.clone();
    other.transition.as_mut().unwrap().mv = Move::parse("e4e5").unwrap();
    assert_eq!(drawn(&board, &other), drawn(&board, &reached));
    let mut illegal = fixture.presentation.clone();
    illegal.transition.as_mut().unwrap().mv = Move::parse("e4e6").unwrap();
    assert_eq!(drawn(&board, &illegal), drawn(&board, &reached));
}

/// The pieces that stand and those in motion, as `draw` places them.
fn placed(board: &Board, presentation: &Presentation) -> (Vec<Placed>, Vec<Placed>) {
    let motion = presentation.motion();
    let shown = motion.map_or(&presentation.position, |transition| &transition.before);
    let outcome = motion.map_or(presentation.outcome, |_| None);
    placements(board, presentation, motion, shown, outcome)
}

fn on(placed: &[Placed], board: &Board, name: &str) -> Option<Placed> {
    let at = board.square_rect(square(name)).min;
    placed.iter().copied().find(|placed| placed.at == at)
}

fn near(a: RenderPoint, b: RenderPoint) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

fn piece(color: Color, kind: PieceKind) -> Piece {
    Piece { color, kind }
}

#[test]
fn castling_carries_the_king_and_the_rook_halfway_together() {
    let fixture = named("castling");
    let board = board(FULL_SIDE, fixture.orientation);
    let (standing, moving) = placed(&board, &fixture.presentation);
    let midway = |from: &str, to: &str| {
        let (from, to) = (
            board.square_rect(square(from)).min,
            board.square_rect(square(to)).min,
        );
        RenderPoint::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0)
    };
    assert_eq!(moving.len(), 2);
    assert_eq!(moving[0].piece, piece(Color::White, PieceKind::King));
    assert!(near(moving[0].at, midway("e1", "g1")));
    assert_eq!(moving[1].piece, piece(Color::White, PieceKind::Rook));
    assert!(near(moving[1].at, midway("h1", "f1")));
    for name in ["e1", "h1", "f1", "g1"] {
        assert_eq!(on(&standing, &board, name), None, "{name}");
    }
    assert_eq!(standing.len(), 32 - 2);
}

#[test]
fn a_pawn_taken_stands_until_the_move_that_takes_it_arrives() {
    for (name, taken) in [("en-passant", "d5"), ("capture", "d5")] {
        let fixture = named(name);
        let board = board(FULL_SIDE, fixture.orientation);
        let (standing, moving) = placed(&board, &fixture.presentation);
        assert_eq!(moving.len(), 1, "{name}");
        assert_eq!(moving[0].piece, piece(Color::White, PieceKind::Pawn));
        let pawn = on(&standing, &board, taken).map(|placed| placed.piece);
        assert_eq!(pawn, Some(piece(Color::Black, PieceKind::Pawn)), "{name}");
        let reached = Presentation {
            transition: None,
            ..fixture.presentation
        };
        let (standing, moving) = placed(&board, &reached);
        assert!(moving.is_empty());
        let after = on(&standing, &board, taken).map(|placed| placed.piece);
        let mover = (name == "capture").then(|| piece(Color::White, PieceKind::Pawn));
        assert_eq!(after, mover, "{name}");
    }
}

#[test]
fn a_promoting_pawn_moves_as_a_pawn_and_arrives_as_a_queen() {
    let fixture = named("promotion");
    let board = board(FULL_SIDE, fixture.orientation);
    let (standing, moving) = placed(&board, &fixture.presentation);
    assert_eq!(moving[0].piece, piece(Color::White, PieceKind::Pawn));
    let rook = on(&standing, &board, "a8").map(|placed| placed.piece);
    assert_eq!(rook, Some(piece(Color::Black, PieceKind::Rook)));
    let (standing, _) = placed(&board, &named("promoted").presentation);
    let queen = on(&standing, &board, "a8").map(|placed| placed.piece);
    assert_eq!(queen, Some(piece(Color::White, PieceKind::Queen)));
}

#[test]
fn a_king_that_lost_is_tipped_over_on_a_marked_square() {
    let fixture = named("checkmate");
    let board = board(FULL_SIDE, fixture.orientation);
    let (standing, _) = placed(&board, &fixture.presentation);
    let tipped: Vec<_> = standing.iter().filter(|placed| placed.tipped).collect();
    assert_eq!(tipped.len(), 1);
    assert_eq!(tipped[0].piece, piece(Color::White, PieceKind::King));
    assert_eq!(Some(*tipped[0]), on(&standing, &board, "e1"));
    let colors = colors_of(&board, &fixture.presentation);
    assert_eq!(colors[index(square("e1"))], mix(DARK, LOST));
}

#[test]
fn a_draw_marks_both_kings_and_tips_neither() {
    let fixture = named("stalemate");
    let board = board(FULL_SIDE, fixture.orientation);
    let (standing, _) = placed(&board, &fixture.presentation);
    assert!(standing.iter().all(|placed| !placed.tipped));
    let colors = colors_of(&board, &fixture.presentation);
    assert_eq!(colors[index(square("h8"))], mix(DARK, DRAWN));
    assert_eq!(colors[index(square("g6"))], mix(LIGHT, DRAWN));
}

fn colors_of(board: &Board, presentation: &Presentation) -> [RenderColor; 64] {
    let motion = presentation.motion();
    let shown = motion.map_or(&presentation.position, |transition| &transition.before);
    let outcome = motion.map_or(presentation.outcome, |_| None);
    square_colors(board, presentation, shown, outcome)
}

#[test]
fn a_check_the_last_move_and_a_selection_tint_their_squares() {
    let check = named("check");
    let colors = colors_of(&board(FULL_SIDE, check.orientation), &check.presentation);
    assert_eq!(colors[index(square("e8"))], mix(LIGHT, CHECK));
    assert_eq!(colors[index(square("f1"))], mix(LIGHT, LAST_MOVE));
    assert_eq!(colors[index(square("b5"))], mix(LIGHT, LAST_MOVE));
    assert_eq!(colors[index(square("e4"))], LIGHT);
    assert_eq!(colors[index(square("e5"))], DARK);

    let middlegame = named("middlegame");
    let full = colors_of(
        &board(FULL_SIDE, middlegame.orientation),
        &middlegame.presentation,
    );
    let compact = colors_of(
        &board(COMPACT_SIDE, middlegame.orientation),
        &middlegame.presentation,
    );
    for colors in [&full, &compact] {
        assert_eq!(colors[index(square("f6"))], mix(DARK, SELECTED));
        // A piece the selected knight may take.
        assert_eq!(colors[index(square("e4"))], mix(LIGHT, REACHABLE));
        assert_eq!(colors[index(square("h3"))], mix(LIGHT, LAST_MOVE));
    }
    // An empty square it may reach is tinted only when compact; in full
    // detail it gets a dot instead.
    assert_eq!(full[index(square("d5"))], LIGHT);
    assert_eq!(compact[index(square("d5"))], mix(LIGHT, REACHABLE));
    let dots = |side| {
        drawn(
            &board(side, middlegame.orientation),
            &middlegame.presentation,
        )
        .layers[0]
            .primitives
            .iter()
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count()
    };
    let reachable = middlegame.presentation.selection.unwrap().destinations;
    assert_eq!(reachable.len(), 6);
    assert_eq!((dots(FULL_SIDE), dots(COMPACT_SIDE)), (5, 0));
}

#[test]
fn the_frame_is_lit_on_the_side_of_the_player_to_move_while_the_game_is_on() {
    let lit = |fixture: &Fixture| {
        let board = board(FULL_SIDE, fixture.orientation);
        let bounds = board.bounds();
        drawn(&board, &fixture.presentation).layers[0]
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                RenderPrimitive::Polygon(polygon)
                    if polygon.fill.map(|fill| fill.color) == Some(TURN) =>
                {
                    let bottom = polygon.points.iter().any(|point| point.y == bounds.min.y);
                    Some(if bottom { "bottom" } else { "top" })
                }
                _ => None,
            })
    };
    // White to move with White at the bottom, and Black to move with Black
    // at the bottom: both at the bottom.
    assert_eq!(lit(&named("start")), Some("bottom"));
    assert_eq!(lit(&named("check")), Some("bottom"));
    let mut black_above = named("check");
    black_above.orientation = Orientation::WhiteAtBottom;
    assert_eq!(lit(&black_above), Some("top"));
    let mut unmarked = named("start");
    unmarked.presentation.mark_turn = false;
    assert_eq!(lit(&unmarked), None);
    assert_eq!(lit(&named("checkmate")), None);
    // While a move is on its way, it is still the turn of its player.
    assert_eq!(lit(&named("capture")), Some("bottom"));
}

#[test]
fn the_squares_of_a_colour_are_drawn_together() {
    let fixture = named("middlegame");
    let board = board(FULL_SIDE, fixture.orientation);
    let colors: Vec<_> = drawn(&board, &fixture.presentation).layers[0]
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Polygon(polygon) => polygon.fill.map(|fill| fill.color),
            _ => None,
        })
        .take(1 + 1 + 1 + 32 + 3)
        .collect();
    // The frame, the mark of the turn and the light squares, then the dark
    // squares at once, but for the selected f6 and h2, which the last move
    // left.
    let dark = colors
        .iter()
        .skip(3)
        .take_while(|&&color| color == DARK)
        .count();
    assert_eq!(dark, 32 - 2);
}
