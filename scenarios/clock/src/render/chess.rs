//! The chess board below the digits: packed in its case, opening out of it
//! into a board, set up with the match's pieces, and folding away again. And
//! the birds at it: the duck that brings it, plays at it and takes it away,
//! and the crow that flies in to play.
//!
//! The board stands on the floor in front of the plants, so it is drawn on
//! the floor's layer after them. The spotlights do not warm that layer, and
//! the board keeps the colours its pieces were qualified in. Each bird is
//! drawn as the Clock's other birds of its kind are, by the same silhouette
//! on the same layer: the duck in front of the board and of the case it
//! holds, and the crow in front of the board and behind the frame it flies
//! in over.

use engine_chess_board::{DARK_SQUARE, FRAME_COLOR, FRAME_SHARE, LIGHT_SQUARE};

use super::*;
use crate::chess::{Exhibition, Show, Visit, crow::Crow, duck::Duck, stage::Area};

#[cfg(test)]
mod tests;

// The duck is drawn in front of its board and of the case it holds, and
// behind the door it comes through.
const _: () = assert!(ARENA_LAYER < ACTIVE_CELL_LAYER && ACTIVE_CELL_LAYER < frame::DOOR_LAYER);
// The crow is drawn in front of the board, and behind the frame it flies in
// over.
const _: () = assert!(ARENA_LAYER < CREATURE_LAYER && CREATURE_LAYER < frame::FRAME_LAYER);

/// The case's lid line, as a share of its height from its foot, and its
/// width as a share of the case's height.
const LID: f32 = 0.62;
const LID_LINE: f32 = 0.08;
/// The clasp on the lid line, as shares of the case's width and height.
const CLASP_WIDTH: f32 = 0.12;
const CLASP_HEIGHT: f32 = 0.34;
const SEAM_COLOR: RenderColor = RenderColor::rgb(0.16, 0.11, 0.07);
const CLASP_COLOR: RenderColor = RenderColor::rgb(0.78, 0.62, 0.30);
/// The fold down the middle of an opening board, as a share of its width.
const FOLD: f32 = 0.012;

/// The visit's board, as it stands or as the duck has it.
pub(super) fn render(frame: &mut RenderFrame, visit: &Visit, exhibition: &Exhibition) {
    match visit.show(exhibition) {
        Show::Case(case) => self::case(frame, case),
        Show::Unfolding { opened } => {
            let stage = visit.stage();
            unfolding(frame, between(stage.case(), stage.board, ease(opened)));
        }
        Show::Board(board, presentation) => {
            engine_chess_board::draw(frame, ARENA_LAYER, &board, &presentation);
        }
    }
}

/// The visit's birds, drawn with the Clock's other visitors.
pub(super) fn render_birds(frame: &mut RenderFrame, visit: &Visit) {
    render_crow(frame, visit);
    let duck = visit.duck();
    if duck.present() {
        super::duck::silhouette(frame, &pose(duck), &|point| point);
    }
}

/// The visit's crow, while it is in the scene.
pub(super) fn render_crow(frame: &mut RenderFrame, visit: &Visit) {
    let crow = visit.crow();
    if crow.present() {
        super::crow::silhouette(frame, &crow_pose(crow));
    }
}

/// The chess crow as its silhouette is drawn.
fn crow_pose(crow: &Crow) -> super::crow::Pose {
    super::crow::Pose {
        position: crow.position(),
        pitch: crow.pitch(),
        facing_right: crow.facing_right(),
        pecking: crow.bowed(),
        wing: crow.wing(),
        lean: crow.lean(),
        hop: crow.hop(),
    }
}

/// The chess duck as its silhouette is drawn, in the world's own frame.
fn pose(duck: &Duck) -> super::duck::Pose {
    super::duck::Pose {
        position: duck.position(),
        radius: duck.radius(),
        facing: duck.facing(),
        stride: duck.stride(),
        hanging: false,
        lean: duck.lean(),
        hop: duck.hop(),
    }
}

/// The closed case, with its lid line and its clasp.
fn case(frame: &mut RenderFrame, case: Area) {
    fill(frame, case, FRAME_COLOR);
    let (width, height) = (case.width(), case.height());
    let lid = case.min.y + height * LID;
    let line = height * LID_LINE * 0.5;
    fill(
        frame,
        Area::new(
            Vec2::new(case.min.x, lid - line),
            Vec2::new(case.max.x, lid + line),
        ),
        SEAM_COLOR,
    );
    let middle = case.center().x;
    let (half_width, half_height) = (width * CLASP_WIDTH * 0.5, height * CLASP_HEIGHT * 0.5);
    fill(
        frame,
        Area::new(
            Vec2::new(middle - half_width, lid - half_height),
            Vec2::new(middle + half_width, lid + half_height),
        ),
        CLASP_COLOR,
    );
}

/// The board on its way out of the case or back into it, at `area`: its
/// frame and its squares, with no pieces, and the fold down its middle.
fn unfolding(frame: &mut RenderFrame, area: Area) {
    fill(frame, area, FRAME_COLOR);
    let inset = area.width().min(area.height()) * FRAME_SHARE;
    let squares = Area::new(area.min + Vec2::splat(inset), area.max - Vec2::splat(inset));
    fill(frame, squares, LIGHT_SQUARE);
    let (across, up) = (squares.width() / 8.0, squares.height() / 8.0);
    for row in 0..8 {
        for column in 0..8 {
            if (row + column) % 2 == 0 {
                let min = squares.min + Vec2::new(column as f32 * across, row as f32 * up);
                fill(
                    frame,
                    Area::new(min, min + Vec2::new(across, up)),
                    DARK_SQUARE,
                );
            }
        }
    }
    let middle = area.center().x;
    let fold = area.width() * FOLD * 0.5;
    fill(
        frame,
        Area::new(
            Vec2::new(middle - fold, area.min.y),
            Vec2::new(middle + fold, area.max.y),
        ),
        SEAM_COLOR,
    );
}

/// The area `t` of the way from `from` to `to`.
fn between(from: Area, to: Area, t: f32) -> Area {
    let at = |a: Vec2, b: Vec2| a + (b - a) * t;
    Area::new(at(from.min, to.min), at(from.max, to.max))
}

/// Slow at either end, as a lid lifted and laid down.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fill(frame: &mut RenderFrame, area: Area, color: RenderColor) {
    let (min, max) = (area.min, area.max);
    frame.push_primitive(
        ARENA_LAYER,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            vec![
                RenderPoint::new(min.x, min.y),
                RenderPoint::new(max.x, min.y),
                RenderPoint::new(max.x, max.y),
                RenderPoint::new(min.x, max.y),
            ],
            color,
        )),
    );
}
