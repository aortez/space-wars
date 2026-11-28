//! Where the crow and the duck set up their board: a stage below the digits.
//!
//! A visit finds its [`Stage`] before it changes anything. The board stands
//! square to the view on the floor's actual support, below the face and
//! clear of its AM/PM letters, with a room for a bird at each side of it and
//! a way in for each from the edge of the view on its side: the duck's along
//! the ground, which carries it, and the crow's through the air just above
//! the rooms, over whatever stands on the floor. Nothing is moved, cleared or
//! propped up to make room. Where no stage stands clear, supported and dry,
//! the visit is turned away.
//!
//! The board is as tall as the room below the face allows, but no wider than
//! a third of the floor, so that it stays small beside the digits and leaves
//! the birds their ways in. It is never smaller than the least board whose
//! pieces can be read, as [`Board::fit`] judges them. A view a little short
//! of room for that board gives up part of the gap it keeps below the face.
//! A view without room for it even so turns every visit away.
//!
//! The search is bounded. The board is tried in the middle of the face
//! first, then in steps of a quarter of its side to the left and to the
//! right, and at each place with the duck on the side the visit prefers,
//! then on the other.

use engine_chess_board::{Board, Orientation};
use engine_common::{RenderPoint, RenderRect};
use engine_core::Vec2;
use engine_rapier::world::{CollisionGroups, PhysicsId, PhysicsWorld};
use engine_water::WaterWorld;

use crate::layout::{CAMERA_HEIGHT, FACE_WIDTH_UNITS, Layout};
use crate::meridiem::{Glyph, PIXEL_SIZE};

#[cfg(test)]
mod tests;

/// The gap kept between the board and the face above it, and around the
/// AM/PM letters, in pitches of the face.
const BELOW_FACE: f32 = 0.25;
/// The least of that gap that is left above the least board whose pieces can
/// be read, in a view too short for both.
const LEAST_BELOW_FACE: f32 = 0.1;
/// How much larger than the least side a board is asked for, so that its
/// place, wherever it is, measures no less.
const LEAST_MARGIN: f32 = 1.001;
/// The most of the floor between the rails that the board's side may take.
const FLOOR_SHARE: f32 = 1.0 / 3.0;
/// A bird's room beside the board, and its way in, as shares of the board's
/// side: how wide the room is and how high both are.
const ROOM_WIDTH: f32 = 0.36;
const ROOM_HEIGHT: f32 = 0.45;
/// How tall the crow's way is, in pitches of the face: as tall as the
/// crow's body.
const LANE: f32 = 2.0 * crate::crow::body::RADIUS_PITCH;
/// The packed case's width and height, as shares of the board's side.
const CASE_WIDTH: f32 = 0.5;
const CASE_HEIGHT: f32 = 0.16;
/// The step between the places tried for the board, as a share of its side.
const STEP: f32 = 0.25;
/// How far above the ground an area is lifted when the arena is asked
/// whether it is clear, so that the support it stands on does not count.
const LIFT: f32 = 0.05;
/// The spacing of the points at which an area's foot is tried for water.
const WATER_SPACING: f32 = 2.0;

/// A side of the stage, as the view shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// An area of the world, between its lower left and upper right corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub min: Vec2,
    pub max: Vec2,
}

impl Area {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    /// The least area that holds every one of `points`, if there are any.
    pub fn around(points: &[Vec2]) -> Option<Self> {
        let (first, rest) = points.split_first()?;
        Some(rest.iter().fold(Self::new(*first, *first), |area, point| {
            Self::new(
                Vec2::new(area.min.x.min(point.x), area.min.y.min(point.y)),
                Vec2::new(area.max.x.max(point.x), area.max.y.max(point.y)),
            )
        }))
    }

    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    /// Whether the two areas share more than an edge.
    pub fn overlaps(self, other: Self) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    /// Whether `other` lies within this area, its edges included.
    pub fn contains(self, other: Self) -> bool {
        self.min.x <= other.min.x
            && other.max.x <= self.max.x
            && self.min.y <= other.min.y
            && other.max.y <= self.max.y
    }

    /// The area grown by `by` on every side.
    fn grown(self, by: f32) -> Self {
        Self::new(self.min - Vec2::splat(by), self.max + Vec2::splat(by))
    }

    /// Whether no body of `world` but those of `excluded` reaches into the
    /// area, just above whatever supports it.
    pub(crate) fn is_clear_in(self, world: &PhysicsWorld, excluded: &[PhysicsId]) -> bool {
        let lifted = Self::new(self.min + Vec2::new(0.0, LIFT), self.max);
        world.current_cuboid_is_clear(
            lifted.center(),
            0.0,
            (lifted.max - lifted.min) * 0.5,
            CollisionGroups::ALL,
            excluded,
        )
    }

    pub fn rect(self) -> RenderRect {
        RenderRect {
            min: RenderPoint::new(self.min.x, self.min.y),
            max: RenderPoint::new(self.max.x, self.max.y),
        }
    }
}

/// Where a visit to the board takes place. See the
/// [module documentation](self).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// The board's bounds, frame included, standing on the ground.
    pub board: Area,
    /// The side the duck stands on. The crow stands on the other.
    pub duck: Side,
    /// Each bird's room beside the board, and its way in, by side.
    rooms: [Area; 2],
    ways: [Area; 2],
}

impl Stage {
    /// The level of the ground the board and the birds stand on.
    pub fn ground(&self) -> f32 {
        self.board.min.y
    }

    /// The board's side, frame included.
    pub fn side(&self) -> f32 {
        self.board.width()
    }

    /// The room the bird on `side` has beside the board, from the board's
    /// edge outward.
    pub fn room(&self, side: Side) -> Area {
        self.rooms[side as usize]
    }

    /// The way in and out of the bird on `side`, from the edge of the view
    /// on that side: along the ground to its room for the duck, which walks,
    /// and for the crow, which flies, through the air just above the rooms
    /// to above its own, where it comes down.
    pub fn way(&self, side: Side) -> Area {
        self.ways[side as usize]
    }

    /// The board packed in its case, standing on the ground in the middle of
    /// the board's place.
    pub fn case(&self) -> Area {
        let (side, middle) = (self.side(), self.board.center().x);
        Area::new(
            Vec2::new(middle - side * CASE_WIDTH * 0.5, self.ground()),
            Vec2::new(
                middle + side * CASE_WIDTH * 0.5,
                self.ground() + side * CASE_HEIGHT,
            ),
        )
    }

    /// The board laid out on the stage, turned a quarter so that the player
    /// of White sits at its `white` side.
    pub fn board_for(&self, white: Side) -> Board {
        let orientation = match white {
            Side::Left => Orientation::WhiteAtLeft,
            Side::Right => Orientation::WhiteAtRight,
        };
        Board::fit(self.board.rect(), CAMERA_HEIGHT, orientation)
            .expect("a stage holds a board whose pieces can be read")
    }

    /// The first stage `scene` leaves clear, supported and dry, with the
    /// duck at its `prefer` side where it can be. Changes nothing.
    pub(crate) fn find(scene: &Scene<'_>, prefer: Side) -> Result<Self, Refusal> {
        let layout = scene.layout;
        let ground = layout.floor_y;
        let gap = BELOW_FACE * layout.pitch;
        let face = face(layout);
        let rails = [
            layout.bounds_min.x + layout.frame_width,
            layout.bounds_max.x - layout.frame_width,
        ];
        let room = face.min.y - ground;
        let most = (rails[1] - rails[0]) * FLOOR_SHARE;
        let mut side = (room - gap).min(most);
        let legible = |side: f32| {
            let square = Area::new(Vec2::new(0.0, 0.0), Vec2::new(side, side));
            Board::fit(square.rect(), CAMERA_HEIGHT, Orientation::WhiteAtBottom)
        };
        if let Err(short) = legible(side) {
            // The least board, where giving up some of the gap makes room.
            let least = short.needed * LEAST_MARGIN;
            let fits = least <= (room - LEAST_BELOW_FACE * layout.pitch).min(most);
            if !fits {
                return Err(Refusal::TooSmall);
            }
            side = least;
        }
        if !side.is_finite() || side > face.width() {
            return Err(Refusal::TooSmall);
        }
        let letters = letters(layout).grown(gap);
        let step = side * STEP;
        // The board's middle stays below the face, so that the board does.
        let reach = ((face.width() - side) * 0.5 / step).floor() as i32;
        for index in 0..=2 * reach {
            // In the middle first, then a step to the left, a step to the
            // right, and so on outward.
            let steps = (index + 1) / 2 * if index % 2 == 1 { -1 } else { 1 };
            let middle = face.center().x + steps as f32 * step;
            let board = Area::new(
                Vec2::new(middle - side * 0.5, ground),
                Vec2::new(middle + side * 0.5, ground + side),
            );
            if board.overlaps(letters)
                || Board::fit(board.rect(), CAMERA_HEIGHT, Orientation::WhiteAtBottom).is_err()
                || !scene.stands(board)
            {
                continue;
            }
            let width = side * ROOM_WIDTH;
            let height = side * ROOM_HEIGHT;
            let rooms = [
                Area::new(
                    Vec2::new(board.min.x - width, ground),
                    Vec2::new(board.min.x, ground + height),
                ),
                Area::new(
                    Vec2::new(board.max.x, ground),
                    Vec2::new(board.max.x + width, ground + height),
                ),
            ];
            if rooms[0].min.x < rails[0]
                || rooms[1].max.x > rails[1]
                || rooms
                    .iter()
                    .any(|room| room.overlaps(letters) || !scene.stands(*room))
            {
                continue;
            }
            let above = face.grown(gap);
            for duck in [prefer, prefer.other()] {
                let walk = match duck {
                    Side::Left => Area::new(
                        Vec2::new(rails[0], ground),
                        Vec2::new(rooms[0].min.x, ground + height),
                    ),
                    Side::Right => Area::new(
                        Vec2::new(rooms[1].max.x, ground),
                        Vec2::new(rails[1], ground + height),
                    ),
                };
                let (left, right) = match duck.other() {
                    Side::Left => (rails[0], board.min.x),
                    Side::Right => (board.max.x, rails[1]),
                };
                let lane = ground + height;
                let flight = Area::new(
                    Vec2::new(left, lane),
                    Vec2::new(right, lane + layout.pitch * LANE),
                );
                let open = |way: Area| {
                    way.width() <= 0.0 || !(way.overlaps(letters) || way.overlaps(above))
                };
                if open(walk)
                    && open(flight)
                    && (walk.width() <= 0.0 || scene.stands(walk))
                    && (flight.width() <= 0.0 || scene.clear(flight))
                {
                    let mut ways = [walk; 2];
                    ways[duck.other() as usize] = flight;
                    return Ok(Self {
                        board,
                        duck,
                        rooms,
                        ways,
                    });
                }
            }
        }
        Err(Refusal::NoRoom)
    }
}

/// Why no stage was found. Nothing changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The view has no room below the face for a board whose pieces can be
    /// read.
    TooSmall,
    /// Every place for the board is taken, unsupported or wet, or leaves a
    /// bird without its room or its way in.
    NoRoom,
}

/// What a stage stands on and keeps clear of.
pub(crate) struct Scene<'a> {
    pub layout: Layout,
    /// The floor's fixed support: its slabs, by their lower left and upper
    /// right corners.
    pub support: &'a [(Vec2, Vec2)],
    /// The worlds whose bodies a stage keeps clear of: the arena's, and
    /// that of an event which simulates apart from it.
    pub worlds: &'a [&'a PhysicsWorld],
    /// Where the plants' tissue rises above the ground.
    pub plants: &'a [Area],
    /// The scene's water, if it has any.
    pub water: Option<&'a WaterWorld>,
}

impl Scene<'_> {
    /// Whether `area` stands on the ground, which supports it all along its
    /// foot, and is clear and dry.
    pub fn stands(&self, area: Area) -> bool {
        self.supports(area) && self.clear(area)
    }

    /// Whether the floor's support runs level all along the foot of `area`.
    pub fn supports(&self, area: Area) -> bool {
        let mut spans: Vec<(f32, f32)> = self
            .support
            .iter()
            .filter(|(_, max)| (max.y - area.min.y).abs() <= LIFT)
            .map(|(min, max)| (min.x, max.x))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut reached = area.min.x;
        for (left, right) in spans {
            if left > reached {
                break;
            }
            reached = reached.max(right);
        }
        reached >= area.max.x
    }

    /// Whether `area` is clear of every world's bodies and the plants, and
    /// dry, just above whatever supports it.
    pub fn clear(&self, area: Area) -> bool {
        area.width() > 0.0
            && area.height() > LIFT
            && !self.plants.iter().any(|plant| plant.overlaps(area))
            && self.worlds.iter().all(|world| area.is_clear_in(world, &[]))
            && self.water.is_none_or(|water| dry(water, area))
    }
}

/// Whether no water stands at the foot of `area` and no drop is in it.
fn dry(water: &WaterWorld, area: Area) -> bool {
    let samples = (area.width() / WATER_SPACING).ceil().max(1.0) as usize;
    let foot = area.min.y + LIFT;
    (0..=samples).all(|index| {
        let x = area.min.x + area.width() * index as f32 / samples as f32;
        water.sample(Vec2::new(x, foot)).is_none()
    }) && water.parcels().iter().all(|parcel| {
        let at = parcel.position;
        !(area.min.x..=area.max.x).contains(&at.x) || !(area.min.y..=area.max.y).contains(&at.y)
    })
}

/// The face's digits, the band they and the colon occupy.
fn face(layout: Layout) -> Area {
    let [bottom, top] = layout.digit_band();
    Area::new(
        Vec2::new(layout.face_origin.x, bottom),
        Vec2::new(layout.face_origin.x + FACE_WIDTH_UNITS * layout.pitch, top),
    )
}

/// The AM/PM letters below the face's lower right corner, whichever they
/// spell, shown or not, so that a stage does not move with the format of
/// the time.
fn letters(layout: Layout) -> Area {
    let origin = Vec2::new(layout.face_origin.x, layout.face_origin.y);
    let corners: Vec<Vec2> = [Glyph::A, Glyph::P, Glyph::M]
        .into_iter()
        .flat_map(|glyph| glyph.cells().map(move |cell| glyph.cell_min(cell)))
        .flat_map(|min| [min, min + Vec2::splat(PIXEL_SIZE)])
        .map(|units| origin + units * layout.pitch)
        .collect();
    Area::around(&corners).expect("the letters have cells")
}
