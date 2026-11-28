//! Chess boards drawn through the production raster and vector adapters, and
//! prepared for the GPU scene, at the sizes `engine_chess_board` declares
//! legible. Set `SPACEWARS_CHESS_BOARD_ARTIFACTS` to a directory to keep the
//! captures.

use std::path::{Path, PathBuf};

use engine_chess::{Color, Piece, PieceKind, Position, Revision, Square};
use engine_chess_board::{Board, Detail, Orientation, Presentation, Squares, fixture};
use engine_common::{Camera2, RenderFrame, RenderPoint, RenderRect};
use i_slint_core::graphics::{ImageInner, SharedImageBuffer};
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};

use crate::gpu_scene::{mesh::MeshBuilder, plan};
use crate::render::{FrameLayout, Viewport};
use crate::thruster_visual_tests::{raster, svg, write_png};

/// The Clock's camera: 480 units high, a pixel a unit on an 800×480 screen.
const VIEW_HEIGHT: f32 = 480.0;

/// The least side of a board with compact pieces in that view, and of one
/// with full pieces.
const LEAST_COMPACT: f32 = 85.0;
const LEAST_FULL: f32 = 136.0;

const SCREENS: [(u32, u32); 3] = [(800, 480), (1024, 768), (480, 800)];

/// The fewest pixels of a square in which any two kinds or colours of piece
/// must differ at the least sizes. A part that tells two pieces apart, such
/// as a bishop's slit or a king's cross, covers more than this at 10 pixels
/// a square.
const FEWEST_APART: usize = 6;

fn artifacts() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("SPACEWARS_CHESS_BOARD_ARTIFACTS")?);
    let path = if path.is_absolute() {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    };
    std::fs::create_dir_all(&path).unwrap();
    Some(path)
}

fn camera() -> Camera2 {
    Camera2::new(RenderPoint::ZERO, VIEW_HEIGHT)
}

/// The board of `side` centred in the view.
fn centred(side: f32, orientation: Orientation) -> Board {
    let half = side / 2.0;
    let area = RenderRect {
        min: RenderPoint::new(-half, -half),
        max: RenderPoint::new(half, half),
    };
    Board::fit(area, VIEW_HEIGHT, orientation).unwrap()
}

fn frame(board: &Board, presentation: &Presentation) -> RenderFrame {
    let mut frame = RenderFrame::new(camera());
    engine_chess_board::draw(&mut frame, 0, board, presentation);
    frame
}

/// The frame drawn by each backend: the raster adapter's pixels, and the
/// vector adapter's paths rasterized through Slint's SVG loader.
pub(crate) fn captures(
    frame: &RenderFrame,
    viewport: Viewport,
) -> [(&'static str, SharedPixelBuffer<Rgb8Pixel>); 2] {
    let paths = svg(frame, viewport);
    assert!(!paths.contains("NaN") && !paths.contains("inf"));
    let image = Image::load_from_svg_data(paths.as_bytes()).unwrap();
    // Image::to_rgba8 scales premultiplied channels by 255 once too often,
    // which turns every pixel white, so read the loader's own buffer. The
    // SVG paints an opaque background, so its premultiplied colours are the
    // colours.
    let inner: &ImageInner = (&image).into();
    let Some(SharedImageBuffer::RGBA8Premultiplied(rgba)) = inner.render_to_buffer(None) else {
        panic!("the SVG loader renders premultiplied pixels");
    };
    let mut vector = SharedPixelBuffer::<Rgb8Pixel>::new(rgba.width(), rgba.height());
    for (to, from) in vector.make_mut_slice().iter_mut().zip(rgba.as_slice()) {
        assert_eq!(from.a, 255);
        *to = Rgb8Pixel::new(from.r, from.g, from.b);
    }
    [("raster", raster(frame, viewport)), ("vector", vector)]
}

/// Where a point of the world falls on the screen, in pixels from the top
/// left.
fn on_screen(point: RenderPoint, viewport: Viewport) -> (f32, f32) {
    let at = camera().world_to_viewport(point, viewport.aspect_ratio());
    (at.x * viewport.width, at.y * viewport.height)
}

fn pixel(pixels: &SharedPixelBuffer<Rgb8Pixel>, (x, y): (f32, f32)) -> Rgb8Pixel {
    pixels.as_slice()[y as usize * pixels.width() as usize + x as usize]
}

/// The pixels within a square, but for a pixel along each edge.
fn inside(
    pixels: &SharedPixelBuffer<Rgb8Pixel>,
    board: &Board,
    square: Square,
    viewport: Viewport,
) -> Vec<Rgb8Pixel> {
    let rect = board.square_rect(square);
    let (left, top) = on_screen(RenderPoint::new(rect.min.x, rect.max.y), viewport);
    let (right, bottom) = on_screen(RenderPoint::new(rect.max.x, rect.min.y), viewport);
    let [left, top, right, bottom] = [left, top, right, bottom].map(|edge| edge.round());
    let columns = left as u32 + 1..right as u32 - 1;
    (top as u32 + 1..bottom as u32 - 1)
        .flat_map(|y| columns.clone().map(move |x| (x as f32, y as f32)))
        .map(|at| pixel(pixels, at))
        .collect()
}

fn center(rect: RenderRect) -> RenderPoint {
    RenderPoint::new(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    )
}

fn differs(a: &Rgb8Pixel, b: &Rgb8Pixel) -> bool {
    a.r.abs_diff(b.r)
        .max(a.g.abs_diff(b.g))
        .max(a.b.abs_diff(b.b))
        > 32
}

fn squares() -> impl Iterator<Item = Square> {
    (0..8).flat_map(|rank| (0..8).filter_map(move |file| Square::new(file, rank)))
}

fn is_dark(square: Square) -> bool {
    (square.file() + square.rank()).is_multiple_of(2)
}

/// Squares a move in motion leaves or reaches, and those beside them, which
/// a castling rook or a pawn taken en passant may cross or stand on.
fn near_motion(presentation: &Presentation, square: Square) -> bool {
    presentation.transition.as_ref().is_some_and(|transition| {
        [transition.mv.from, transition.mv.to].iter().any(|end| {
            end.file().abs_diff(square.file()) <= 1 && end.rank().abs_diff(square.rank()) <= 1
        })
    })
}

/// Squares tinted by a mark: the last move's, the selection's, and the
/// kings' while one is in check or once the game is over.
fn marked(presentation: &Presentation, square: Square) -> bool {
    let king = presentation
        .position
        .piece_at(square)
        .is_some_and(|piece| piece.kind == PieceKind::King);
    presentation
        .last_move
        .is_some_and(|mv| mv.from == square || mv.to == square)
        || presentation.selection.is_some_and(|selection| {
            selection.square == square || selection.destinations.contains(square)
        })
        || (king && (presentation.outcome.is_some() || presentation.position.is_check()))
}

fn occupied(presentation: &Presentation, square: Square) -> bool {
    let before = presentation
        .transition
        .as_ref()
        .map(|transition| &transition.before);
    presentation.position.piece_at(square).is_some()
        || before.is_some_and(|before| before.piece_at(square).is_some())
}

#[test]
fn chess_fixtures_put_squares_and_pieces_where_the_board_maps_them_on_both_backends() {
    let output = artifacts();
    let mut index = String::from("<!doctype html><title>Chess board fixtures</title>\n");
    let mut failures = Vec::new();
    for (width, height) in SCREENS {
        let viewport = Viewport::new(width as f32, height as f32);
        let world = camera().world_bounds(viewport.aspect_ratio());
        let full = 0.9 * world.width().min(world.height());
        for (size, side, detail) in [
            ("compact", LEAST_COMPACT, Detail::Compact),
            ("full", full, Detail::Full),
        ] {
            for fixture in fixture::all() {
                let board = centred(side, fixture.orientation);
                assert_eq!(board.detail(), detail);
                let presentation = &fixture.presentation;
                let frame = frame(&board, presentation);
                let name = format!("{}-{size}-{width}x{height}", fixture.name);
                for (backend, pixels) in captures(&frame, viewport) {
                    let case = format!("{name} {backend}");
                    assert_eq!((pixels.width(), pixels.height()), (width, height), "{case}");
                    let calm = |square: Square| {
                        !marked(presentation, square) && !near_motion(presentation, square)
                    };
                    // Every empty square shows its own colour at its centre:
                    // one for the light squares and another for the dark.
                    let mut colors = [None, None];
                    for square in squares().filter(|&square| calm(square)) {
                        if occupied(presentation, square) {
                            continue;
                        }
                        let at = on_screen(center(board.square_rect(square)), viewport);
                        let shown = pixel(&pixels, at);
                        let expected = *colors[usize::from(is_dark(square))].get_or_insert(shown);
                        if shown != expected {
                            failures.push(format!("{case} {square}: {shown:?}, not {expected:?}"));
                        }
                    }
                    let [Some(light), Some(dark)] = colors else {
                        panic!("{case} has empty squares of both colours");
                    };
                    assert!(differs(&light, &dark), "{case}");
                    // Every piece at rest covers a share of its own square.
                    for square in squares().filter(|&square| calm(square)) {
                        if !occupied(presentation, square) {
                            continue;
                        }
                        let empty = if is_dark(square) { dark } else { light };
                        let block = inside(&pixels, &board, square, viewport);
                        let covered = block.iter().filter(|pixel| differs(pixel, &empty)).count();
                        if covered * 5 < block.len() {
                            failures.push(format!(
                                "{case} {square}: the piece covers {covered} of {} pixels",
                                block.len()
                            ));
                        }
                    }
                    if let Some(directory) = &output {
                        let file = format!("{name}-{backend}.png");
                        write_png(&directory.join(&file), &pixels);
                        index.push_str(&format!(
                            "<p>{name} {backend}<br><img src=\"{file}\"></p>\n"
                        ));
                    }
                }
                // The GPU scene prepares the same frame.
                let scene = plan::build(
                    std::slice::from_ref(&frame),
                    viewport,
                    FrameLayout::EqualHorizontal,
                    1.0,
                )
                .unwrap();
                let mut mesh = MeshBuilder::default();
                mesh.rebuild(&scene).unwrap();
                assert!(!mesh.buffers.indices.is_empty(), "{name}");
            }
        }
    }
    if let Some(directory) = &output {
        std::fs::write(directory.join("index.html"), index).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A position with `piece` alone on `square`, but for the kings, which
/// stand in corners off the lines through it. A king is its own king.
fn alone(piece: Piece, square: Square) -> Position {
    let mut cells = [[None; 8]; 8];
    cells[usize::from(square.rank())][usize::from(square.file())] = Some(piece);
    // The diagonal a1 to h8 runs through d4 and the one from a8 to h1
    // through e4, so the kings stand on the other two corners.
    let corners = if square.file() == 3 {
        [(7, 0), (0, 7)]
    } else {
        [(0, 0), (7, 7)]
    };
    for (color, (file, rank)) in [Color::White, Color::Black].into_iter().zip(corners) {
        if !(piece.kind == PieceKind::King && piece.color == color) {
            cells[rank][file] = Some(Piece {
                color,
                kind: PieceKind::King,
            });
        }
    }
    let ranks: Vec<String> = cells
        .iter()
        .rev()
        .map(|rank| {
            let mut text = String::new();
            let mut empty = 0;
            for cell in rank {
                let Some(piece) = cell else {
                    empty += 1;
                    continue;
                };
                if empty > 0 {
                    text.push_str(&empty.to_string());
                    empty = 0;
                }
                let letter = piece.kind.letter();
                text.push(match piece.color {
                    Color::White => letter.to_ascii_uppercase(),
                    Color::Black => letter.to_ascii_lowercase(),
                });
            }
            if empty > 0 {
                text.push_str(&empty.to_string());
            }
            text
        })
        .collect();
    let fen = format!("{} w - - 0 1", ranks.join("/"));
    Position::from_fen(&fen).unwrap_or_else(|error| panic!("{fen}: {error:?}"))
}

#[test]
fn every_kind_and_colour_of_piece_differs_from_the_rest_at_the_least_sizes() {
    let output = artifacts();
    let viewport = Viewport::new(800.0, 480.0);
    let pieces: Vec<Piece> = [Color::White, Color::Black]
        .into_iter()
        .flat_map(|color| PieceKind::ALL.map(|kind| Piece { color, kind }))
        .collect();
    let mut failures = Vec::new();
    for (detail, side) in [(Detail::Compact, LEAST_COMPACT), (Detail::Full, LEAST_FULL)] {
        let board = centred(side, Orientation::WhiteAtBottom);
        assert_eq!(board.detail(), detail);
        for name in ["d4", "e4"] {
            let square = Square::from_name(name).unwrap();
            let mut blocks: [Vec<Vec<Rgb8Pixel>>; 2] = [Vec::new(), Vec::new()];
            for &piece in &pieces {
                let presentation = Presentation {
                    position: alone(piece, square),
                    revision: Revision {
                        session: 0,
                        actions: 0,
                    },
                    last_move: None,
                    selection: None,
                    mark_turn: false,
                    outcome: None,
                    transition: None,
                    hidden: Squares::EMPTY,
                };
                let frame = frame(&board, &presentation);
                for (index, (backend, pixels)) in captures(&frame, viewport).into_iter().enumerate()
                {
                    blocks[index].push(inside(&pixels, &board, square, viewport));
                    if let Some(directory) = &output {
                        let file = format!(
                            "alone-{detail:?}-{name}-{:?}-{:?}-{backend}.png",
                            piece.color, piece.kind
                        );
                        write_png(&directory.join(file), &pixels);
                    }
                }
            }
            for (backend, blocks) in ["raster", "vector"].iter().zip(&blocks) {
                let apart = |i: usize, j: usize| {
                    let differ = blocks[i]
                        .iter()
                        .zip(&blocks[j])
                        .filter(|(a, b)| differs(a, b))
                        .count();
                    (differ, i, j)
                };
                let (fewest, i, j) = (0..blocks.len())
                    .flat_map(|i| (i + 1..blocks.len()).map(move |j| (i, j)))
                    .map(|(i, j)| apart(i, j))
                    .min()
                    .unwrap();
                let case = format!(
                    "{detail:?} {name} {backend}: {:?} and {:?}, the closest two, differ in \
                     {fewest} of {} pixels",
                    pieces[i],
                    pieces[j],
                    blocks[i].len()
                );
                eprintln!("{case}");
                if fewest < FEWEST_APART {
                    failures.push(case);
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
