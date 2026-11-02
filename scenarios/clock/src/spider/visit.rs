//! The spider's visit: one autonomous spider that comes in on a line from the
//! beam above the hatch, climbs and swings on the digits for a while, and
//! leaves the way it came, up through the hatch. The visit is a seeded,
//! bounded state machine over the mechanic's intents. It reads the spider
//! back each tick and never moves it by hand: a digit that goes, a line that
//! is cut or a fall is recovered from, or ends the visit early.
//!
//! Where it ties its line is planned from the published surfaces: a shot is
//! taken only when a straight line from the spider to its mark passes no
//! other cell, by the cells' drawn boxes. The arena's own ray has the last
//! word, and a shot that still fails is tried again elsewhere.
//!
//! The visit is solo. Only Digit Slide and Color Cycle stay live beside it;
//! every other event waits for it, and it waits for ducks, crows and any
//! event it does not keep live. A kind this module has not been told about
//! does not compile, so a new event declares its compatibility here.

use engine_common::{
    ClockEventKind, ClockSpiderFramePart, ClockSpiderHold, ClockSpiderMode, ClockSpiderPhase,
    ClockSpiderState,
};
use engine_core::Vec2;
use engine_rapier::spider::{SpiderIntent, SpiderLine, SpiderMode};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{FramePart, HATCH_WIDTH_PITCH, Hold, SpiderFixture, SpiderFixtureState};
use crate::{
    digit_surfaces::{DigitSurface, Footprint, SurfaceId},
    events::FIXED_HZ,
    layout::Layout,
};

/// The whole visit, entrance and exit included.
pub const SPIDER_TICKS: u64 = 30 * 60;
/// Kept for the way out: the visit starts leaving with this much left past
/// what the way out is reckoned to take.
pub const EXIT_RESERVE_TICKS: u64 = 5 * 60;
/// How long past its budget a visit may still be on its way out before it is
/// retired where it stands and reported as a failed exit.
pub const EXIT_GRACE_TICKS: u64 = 10 * 60;
/// How long the spider hangs on its line after letting go of a digit.
const SWING_TICKS: u64 = 90;
/// Line let out before a swing, so there is room to swing on it.
const SLACK_TICKS: u64 = 24;
/// The least a swing drops, in radii, and how far past the line's length
/// the drop's bounce and the body reach, in radii, which keeps a swing off
/// the floor.
const SWING_DROP: f32 = 1.5;
const SWING_BOUNCE: f32 = 3.0;
/// How long a climb, a crossing or a walk may take before it is given up.
const STEP_LIMIT_TICKS: u64 = 6 * 60;
/// How long the way out crawls on after a shot up that came to nothing
/// before another is tried.
const SKY_SEARCH_TICKS: u64 = 3 * 60;
/// How long a crawl goes on before the spider decides what next.
const CRAWL_TICKS: [u64; 2] = [90, 240];
/// How far under the digits the spider comes in, in pitches: below their
/// bottom row, where every cell above has a clear underside to tie to.
const DROP_BELOW_PITCH: [f32; 2] = [0.5, 1.5];
/// How far from a cell a planned line keeps, in radii.
const LINE_MARGIN: f32 = 0.4;
/// The most swings a visit takes.
const MAX_SWINGS: u32 = 2;
/// The most marks a shot is tried at before the plan changes.
const MAX_RETRIES: u32 = 4;

/// Whether `kind` may run beside a spider's visit. The time's own transition
/// and a change of colour keep the digits where they are; everything else
/// moves the face, changes the arena or brings another animal.
pub(crate) fn compatible_event(kind: ClockEventKind) -> bool {
    use ClockEventKind::*;
    match kind {
        DigitSlide | ColorCycle => true,
        Falling | Meltdown | Duck | Marquee | Rain | Crow | Explosion | Aquarium | CellSwap
        | Grass | SlotMachine | Spider => false,
    }
}

/// What the spider is doing on the way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    /// Round the digit it is on to somewhere with clear sky, and a line up.
    ToTop,
    /// Up the line to the ceiling.
    ToCeiling,
    /// Along the ceiling to the hatch and up through it.
    ToHatch,
    /// A line to the beam, and out of the view.
    Out,
}

/// Where a shot is aimed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Target {
    slot: u8,
    point: Vec2,
}

/// The drawn part of a published cell, as a world box.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CellBox {
    id: SurfaceId,
    slot: u8,
    lo: Vec2,
    hi: Vec2,
}

impl CellBox {
    fn of(cell: &DigitSurface) -> Option<Self> {
        if !cell.eligible() {
            return None;
        }
        let Footprint { min, max } = cell.visible()?;
        let corners =
            [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)].map(|p| cell.world(p));
        let lo = corners
            .iter()
            .fold(corners[0], |a, c| Vec2::new(a.x.min(c.x), a.y.min(c.y)));
        let hi = corners
            .iter()
            .fold(corners[0], |a, c| Vec2::new(a.x.max(c.x), a.y.max(c.y)));
        Some(Self {
            id: cell.id,
            slot: cell.slot,
            lo,
            hi,
        })
    }

    fn center(&self) -> Vec2 {
        (self.lo + self.hi) * 0.5
    }

    fn spans_x(&self, x: f32) -> bool {
        (self.lo.x..=self.hi.x).contains(&x)
    }

    fn overlaps_y(&self, other: &Self) -> bool {
        self.lo.y < other.hi.y && other.lo.y < self.hi.y
    }

    /// How far along a ray from `from` along the unit `dir` this box, grown
    /// by `margin`, is entered, if it is within `limit`.
    fn entered(&self, from: Vec2, dir: Vec2, limit: f32, margin: f32) -> Option<f32> {
        let lo = self.lo - Vec2::new(margin, margin);
        let hi = self.hi + Vec2::new(margin, margin);
        let mut nearest = 0.0_f32;
        let mut farthest = limit;
        for (o, d, l, h) in [(from.x, dir.x, lo.x, hi.x), (from.y, dir.y, lo.y, hi.y)] {
            if d.abs() < 1e-6 {
                if o < l || o > h {
                    return None;
                }
                continue;
            }
            let (a, b) = ((l - o) / d, (h - o) / d);
            nearest = nearest.max(a.min(b));
            farthest = farthest.min(a.max(b));
            if nearest > farthest {
                return None;
            }
        }
        Some(nearest)
    }
}

/// Whether a line from `from` to `to` passes no cell but `except`, keeping
/// `margin` from each.
fn line_clear(
    boxes: &[CellBox],
    from: Vec2,
    to: Vec2,
    except: Option<SurfaceId>,
    margin: f32,
) -> bool {
    let span = to - from;
    let distance = span.length();
    if distance <= margin {
        return true;
    }
    let dir = span / distance;
    boxes
        .iter()
        .filter(|b| Some(b.id) != except)
        .all(|b| b.entered(from, dir, distance - margin, margin).is_none())
}

pub(crate) struct SpiderVisit {
    visit_id: u64,
    seed: u64,
    rng: StdRng,
    layout: Layout,
    age: u64,
    phase: ClockSpiderPhase,
    phase_tick: u64,
    /// How far down the beam's line the spider comes in.
    drop_to: f32,
    /// The slot it crosses to first, and the one it prefers after that.
    slot: u8,
    exit: Exit,
    /// A shot's mark, while one is aimed for.
    target: Option<Target>,
    /// The cell last climbed to as a step on the way out, whether such a
    /// step is under way, and whether the way along the next top or
    /// underside is still to be chosen.
    last_step: Option<SurfaceId>,
    stepping: bool,
    pick_direction: bool,
    /// Where the climb under way started, and whether its legs are out for
    /// anything it passes.
    climb_from: Vec2,
    catch_all: bool,
    retries: u32,
    /// The crawl's direction and how long it goes on.
    direction: f32,
    crawl_until: u64,
    /// Set while a recovery began with the spider all but on what it was
    /// climbing to, and while it has landed on the floor in this one.
    catching: bool,
    landed: bool,
    climbs: u32,
    swings: u32,
    falls: u32,
    recoveries: u32,
    failed_exit: bool,
    done: bool,
}

impl SpiderVisit {
    /// A visit that starts under the beam, planned from `seed`.
    pub fn new(layout: Layout, seed: u64, visit_id: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let [low, _] = layout.digit_band();
        let [near, far] = DROP_BELOW_PITCH;
        let radius = SpiderFixture::radius(layout);
        let drop_to =
            (low - layout.pitch * rng.random_range(near..far)).max(layout.floor_y + 3.0 * radius);
        let slot = if rng.random_bool(0.5) { 1 } else { 2 };
        Self {
            visit_id,
            seed,
            rng,
            layout,
            age: 0,
            phase: ClockSpiderPhase::Descending,
            phase_tick: 0,
            drop_to,
            slot,
            exit: Exit::ToTop,
            target: None,
            last_step: None,
            stepping: false,
            pick_direction: false,
            climb_from: Vec2::ZERO,
            catch_all: false,
            retries: 0,
            direction: 1.0,
            crawl_until: 0,
            catching: false,
            landed: false,
            climbs: 0,
            swings: 0,
            falls: 0,
            recoveries: 0,
            failed_exit: false,
            done: false,
        }
    }

    /// The visit has ended, departed or failed, and the spider is to be
    /// taken out of the arena.
    pub fn done(&self) -> bool {
        self.done
    }

    pub fn failed_exit(&self) -> bool {
        self.failed_exit
    }

    fn enter(&mut self, phase: ClockSpiderPhase) {
        self.phase = phase;
        self.phase_tick = 0;
        self.retries = 0;
        self.target = None;
        self.catching = false;
        self.landed = false;
        self.catch_all = false;
    }

    /// Ticks left before the visit must be on its way out.
    fn remaining(&self) -> u64 {
        SPIDER_TICKS
            .saturating_sub(EXIT_RESERVE_TICKS)
            .saturating_sub(self.age)
    }

    /// A guess at how long the way out takes from `position`: the way round
    /// the digit to clear sky and the climb to the ceiling, the crawl along
    /// it to the hatch, and the way out above.
    fn exit_estimate(&self, position: Vec2, radius: f32) -> u64 {
        let crawl = SpiderFixture::spec(self.layout).crawl_speed * radius;
        let along = (position.x.abs() / crawl * FIXED_HZ as f32) as u64;
        u64::from(FIXED_HZ) * 7 + along
    }

    fn time_to_leave(&self, state: &SpiderFixtureState) -> bool {
        self.remaining() <= self.exit_estimate(state.spider.position, state.spider.radius)
    }

    fn boxes(published: &[DigitSurface]) -> Vec<CellBox> {
        published.iter().filter_map(CellBox::of).collect()
    }

    fn held_id(state: &SpiderFixtureState) -> Option<SurfaceId> {
        match state.support {
            Some(Hold::Surface { id, .. } | Hold::Retired(id)) => Some(id),
            _ => None,
        }
    }

    fn holds_digit(state: &SpiderFixtureState) -> bool {
        matches!(
            state.support,
            Some(Hold::Surface { .. } | Hold::Rigid(_) | Hold::Retired(_))
        )
    }

    fn tied_to_digit(state: &SpiderFixtureState) -> bool {
        state.spider.line.is_some()
            && matches!(
                state.anchor,
                Some(Hold::Surface { .. } | Hold::Rigid(_) | Hold::Retired(_))
            )
    }

    fn tied_to(state: &SpiderFixtureState, parts: &[FramePart]) -> bool {
        state.spider.line.is_some()
            && matches!(state.anchor, Some(Hold::Frame(part)) if parts.contains(&part))
    }

    /// Holding on under a cell, its back to the floor.
    fn under_cell(state: &SpiderFixtureState) -> bool {
        Self::holds_digit(state) && state.spider.up.y < -0.7
    }

    /// How long a line to swing on from an anchor at `anchor_y` is: as much
    /// as the slack ticks let out, and never so much that the drop, with
    /// the bounce at the end of it, reaches the floor.
    fn swing_line(&self, anchor_y: f32, radius: f32) -> f32 {
        let spec = SpiderFixture::spec(self.layout);
        let let_out = SLACK_TICKS as f32 / FIXED_HZ as f32 * spec.pay_out_speed * radius;
        let room = anchor_y - self.layout.floor_y - SWING_BOUNCE * radius;
        (spec.min_line * radius + let_out).min(room)
    }

    /// Whether a swing from where the spider holds on has room to drop.
    fn can_swing(&self, state: &SpiderFixtureState) -> bool {
        let radius = state.spider.radius;
        Self::under_cell(state)
            && self.swing_line(state.spider.position.y + radius, radius)
                >= (SpiderFixture::spec(self.layout).min_line + SWING_DROP) * radius
    }

    fn on_ceiling(state: &SpiderFixtureState) -> bool {
        matches!(
            state.support,
            Some(Hold::Frame(
                FramePart::CeilingLeft | FramePart::CeilingRight
            ))
        )
    }

    /// The underside marks of `slot`'s cells above `from`: the middle of the
    /// bottom of a cell with no cell under it, that a line from `from`
    /// reaches past no other cell. Nearest first.
    fn underside_marks(&self, boxes: &[CellBox], slot: u8, from: Vec2, radius: f32) -> Vec<Target> {
        let pitch = self.layout.pitch;
        let mut marks: Vec<Target> = boxes
            .iter()
            .filter(|b| b.slot == slot)
            .filter_map(|b| {
                let mark = Vec2::new(b.center().x, b.lo.y);
                if mark.y < from.y + radius {
                    return None;
                }
                let covered = boxes.iter().any(|o| {
                    o.id != b.id
                        && o.spans_x(mark.x)
                        && o.hi.y <= b.lo.y + 0.01 * pitch
                        && o.hi.y > b.lo.y - 0.75 * pitch
                });
                (!covered && line_clear(boxes, from, mark, Some(b.id), LINE_MARGIN * radius))
                    .then_some(Target { slot, point: mark })
            })
            .collect();
        marks.sort_by(|a, b| {
            a.point
                .distance_to(from)
                .total_cmp(&b.point.distance_to(from))
        });
        marks
    }

    /// The side marks of `slot`'s cells for a crossing from `from`: the
    /// middle of the face toward `from` of a cell with nothing beside it that
    /// way, no higher than `from`, that a line reaches past no other cell.
    /// Nearest first.
    fn side_marks(&self, boxes: &[CellBox], slot: u8, from: Vec2, radius: f32) -> Vec<Target> {
        let pitch = self.layout.pitch;
        let mut marks: Vec<Target> = boxes
            .iter()
            .filter(|b| b.slot == slot)
            .filter_map(|b| {
                let right = from.x > b.center().x;
                let mark = Vec2::new(if right { b.hi.x } else { b.lo.x }, b.center().y);
                if mark.y > from.y + radius {
                    return None;
                }
                let covered = boxes.iter().any(|o| {
                    o.id != b.id
                        && o.overlaps_y(b)
                        && if right {
                            o.lo.x >= b.hi.x - 0.01 * pitch && o.lo.x < b.hi.x + 0.75 * pitch
                        } else {
                            o.hi.x <= b.lo.x + 0.01 * pitch && o.hi.x > b.lo.x - 0.75 * pitch
                        }
                });
                (!covered && line_clear(boxes, from, mark, Some(b.id), LINE_MARGIN * radius))
                    .then_some(Target { slot, point: mark })
            })
            .collect();
        marks.sort_by(|a, b| {
            a.point
                .distance_to(from)
                .total_cmp(&b.point.distance_to(from))
        });
        marks
    }

    /// The mark to climb a digit by from the floor: the middle of the bottom
    /// of the lowest cell of the slot nearest `from`, the two by the hatch
    /// preferred.
    fn floor_mark(boxes: &[CellBox], from: Vec2, pitch: f32) -> Option<Target> {
        (0..4u8)
            .filter_map(|slot| {
                let lowest = boxes
                    .iter()
                    .filter(|b| b.slot == slot)
                    .min_by(|a, b| a.lo.y.total_cmp(&b.lo.y))?;
                let point = Vec2::new(lowest.center().x, lowest.lo.y);
                let detour = if slot == 1 || slot == 2 {
                    0.0
                } else {
                    4.0 * pitch
                };
                Some(((point.x - from.x).abs() + detour, Target { slot, point }))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, target)| target)
    }

    /// The shot up from where the spider holds on: straight up from a top
    /// or a side, and none from an underside. It reaches the ceiling, the
    /// beam through the hatch, or a cell higher up, which is a step on the
    /// way. A shot leaning away from a side would reach the ceiling more
    /// often, but the swing up its line then crosses the next digit over.
    fn aim_up(state: &SpiderFixtureState) -> Option<Vec2> {
        (state.spider.up.y > -0.2).then_some(Vec2::Y)
    }

    /// The ends of `cell`'s row of cells, left and right: how far each is,
    /// and whether the spider can get round it from the row's underside or,
    /// with `underside` false, from its top. A cell just below the end
    /// keeps it from getting round from below, and one just above from the
    /// top.
    fn row_ends(boxes: &[CellBox], cell: &CellBox, underside: bool) -> [(f32, bool); 2] {
        let pitch = cell.hi.x - cell.lo.x;
        let row = |b: &&CellBox| b.slot == cell.slot && b.overlaps_y(cell);
        [-1.0_f32, 1.0].map(|step| {
            let mut edge = if step < 0.0 { cell.lo.x } else { cell.hi.x };
            loop {
                let next = boxes.iter().filter(row).find(|b| {
                    if step < 0.0 {
                        b.hi.x <= edge + 0.01 * pitch && b.hi.x > edge - 0.75 * pitch
                    } else {
                        b.lo.x >= edge - 0.01 * pitch && b.lo.x < edge + 0.75 * pitch
                    }
                });
                match next {
                    Some(b) => edge = if step < 0.0 { b.lo.x } else { b.hi.x },
                    None => break,
                }
            }
            let free = !boxes.iter().filter(|b| b.slot == cell.slot).any(|b| {
                let beyond = if underside {
                    b.hi.y <= cell.lo.y + 0.01 * pitch && b.hi.y > cell.lo.y - pitch
                } else {
                    b.lo.y >= cell.hi.y - 0.01 * pitch && b.lo.y < cell.hi.y + pitch
                };
                beyond
                    && if step < 0.0 {
                        b.lo.x < edge + 0.2 * pitch && b.hi.x > edge - pitch
                    } else {
                        b.hi.x > edge - 0.2 * pitch && b.lo.x < edge + pitch
                    }
            });
            (edge, free)
        })
    }

    /// The crawl along the top or the underside the spider holds that
    /// reaches an end of the cell's row it can get round, the one toward
    /// the digit's outside where both can be, since the inner one is as
    /// likely to lead into a pocket as out of one: a positive crawl goes
    /// right along a top and left along an underside. None from a side,
    /// and where neither end can be got round, which is a pocket.
    fn to_row_end(boxes: &[CellBox], state: &SpiderFixtureState) -> Option<f32> {
        if state.spider.up.y.abs() < 0.7 {
            return None;
        }
        let held = boxes.iter().find(|b| Some(b.id) == Self::held_id(state))?;
        let underside = state.spider.up.y < 0.0;
        let [(left, left_free), (right, right_free)] = Self::row_ends(boxes, held, underside);
        let digit = boxes
            .iter()
            .filter(|b| b.slot == held.slot)
            .fold((held.lo.x, held.hi.x), |(lo, hi), b| {
                (lo.min(b.lo.x), hi.max(b.hi.x))
            });
        let middle = (digit.0 + digit.1) * 0.5;
        let go_left = match (left_free, right_free) {
            (true, true) => middle - left > right - middle,
            (true, false) => true,
            (false, true) => false,
            (false, false) => return None,
        };
        Some(if go_left == underside { 1.0 } else { -1.0 })
    }

    /// The cell a climb straight up from `from` would come to, under
    /// `limit` away: the first a body of `radius` on its way up meets, the
    /// held cell aside. A line through a gap between cells narrower than
    /// the body is no way up.
    fn above<'a>(
        boxes: &'a [CellBox],
        from: Vec2,
        except: Option<SurfaceId>,
        limit: f32,
        radius: f32,
    ) -> Option<&'a CellBox> {
        boxes
            .iter()
            .filter(|b| Some(b.id) != except)
            .filter_map(|b| b.entered(from, Vec2::Y, limit, radius).map(|t| (t, b)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, b)| b)
    }

    /// Whether a shot up from where the spider is leads somewhere: the
    /// cell its line would tie to is the first the body meets on the way,
    /// so the climb arrives, and it is not a pocket, a cell whose row the
    /// spider could get round neither end of. The ceiling and the beam are
    /// always somewhere.
    fn climb_leads_up(&self, boxes: &[CellBox], state: &SpiderFixtureState) -> bool {
        let position = state.spider.position;
        let limit = self.layout.canopy_y - position.y;
        let held = Self::held_id(state);
        let tie = Self::above(boxes, position, held, limit, 0.0);
        let met = Self::above(boxes, position, held, limit, state.spider.radius);
        tie.map(|b| b.id) == met.map(|b| b.id)
            && tie.is_none_or(|cell| {
                Self::row_ends(boxes, cell, true)
                    .iter()
                    .any(|(_, free)| *free)
            })
    }

    /// The way along the held surface that goes up, where either does: along
    /// a side it is the one way, and from a top or an underside the row's
    /// nearer end leads round to a side.
    fn climb_direction(&mut self, state: &SpiderFixtureState, boxes: &[CellBox]) -> f32 {
        let up = state.spider.up;
        if up.x.abs() > 0.7 {
            -up.x.signum()
        } else if let Some(direction) = Self::to_row_end(boxes, state) {
            direction
        } else if self.rng.random_bool(0.5) {
            1.0
        } else {
            -1.0
        }
    }

    /// What the spider does this tick, from what it is and where the face
    /// is. Called once per moving tick, before the spider is driven.
    pub fn decide(
        &mut self,
        state: &SpiderFixtureState,
        published: &[DigitSurface],
    ) -> SpiderIntent {
        self.age += 1;
        self.phase_tick += 1;
        if self.failed_exit {
            // Reported for one tick where it stands, and then taken away.
            self.done = true;
            return SpiderIntent::default();
        }
        if self.age > SPIDER_TICKS + EXIT_GRACE_TICKS {
            self.failed_exit = true;
            return SpiderIntent::default();
        }
        if self.phase != ClockSpiderPhase::Leaving && self.time_to_leave(state) {
            self.leave(state);
        }
        match self.phase {
            ClockSpiderPhase::Descending => self.descend(state),
            ClockSpiderPhase::Crossing => self.cross(state, published),
            ClockSpiderPhase::Climbing => self.climb(state),
            ClockSpiderPhase::Crawling => self.crawl(state, published),
            ClockSpiderPhase::Swinging => self.swing(state),
            ClockSpiderPhase::Recovering => self.recover(state, published),
            ClockSpiderPhase::Leaving => self.exit_step(state, published),
        }
    }

    /// Down from the beam on a line, to under the digits.
    fn descend(&mut self, state: &SpiderFixtureState) -> SpiderIntent {
        let idle = SpiderIntent::default();
        match self.phase_tick {
            // The room's solids reach the queries at the next step.
            1 => idle,
            2 => SpiderIntent {
                shoot: Some(Vec2::Y),
                ..idle
            },
            _ => {
                if state.spider.line.is_none() {
                    if state.spider.shot.is_some() {
                        return idle;
                    }
                    // The beam is always there to tie to; try again, and
                    // give up only when something is badly wrong.
                    self.retries += 1;
                    if self.retries > MAX_RETRIES {
                        self.enter(ClockSpiderPhase::Recovering);
                        return SpiderIntent { grip: true, ..idle };
                    }
                    return SpiderIntent {
                        shoot: Some(Vec2::Y),
                        ..idle
                    };
                }
                if state.spider.position.y <= self.drop_to || self.phase_tick > STEP_LIMIT_TICKS {
                    self.enter(ClockSpiderPhase::Crossing);
                    return idle;
                }
                SpiderIntent { reel: -1.0, ..idle }
            }
        }
    }

    /// A line to a digit: from under them, to the underside of a cell of the
    /// chosen slot; from a digit, across to the near side of the other.
    fn cross(&mut self, state: &SpiderFixtureState, published: &[DigitSurface]) -> SpiderIntent {
        let idle = SpiderIntent::default();
        let from_digit = Self::holds_digit(state);
        let hold = SpiderIntent {
            grip: from_digit,
            ..idle
        };
        if state.spider.shot.is_some() {
            return hold;
        }
        if self.target.is_some() {
            if Self::tied_to_digit(state) {
                self.enter(ClockSpiderPhase::Climbing);
                self.climb_from = state.spider.position;
                self.catch_all = true;
                return SpiderIntent { reel: 1.0, ..idle };
            }
            // The shot did not tie: the next mark.
            self.retries += 1;
            self.target = None;
        }
        if !from_digit && state.support.is_none() && state.spider.line.is_none() {
            // The line it hung from is gone.
            self.enter(ClockSpiderPhase::Recovering);
            return SpiderIntent { grip: true, ..idle };
        }
        if self.phase_tick > STEP_LIMIT_TICKS {
            self.give_up_crossing(state);
            return hold;
        }
        let from = state.spider.position;
        let radius = state.spider.radius;
        let boxes = Self::boxes(published);
        let marks = if from_digit {
            self.side_marks(&boxes, self.slot, from, radius)
        } else {
            self.underside_marks(&boxes, self.slot, from, radius)
        };
        let Some(target) = marks.get(self.retries as usize).copied() else {
            // Nothing on this side to tie to: the other side once.
            if self.retries <= MAX_RETRIES && (self.slot == 1 || self.slot == 2) {
                self.slot = 3 - self.slot;
                self.retries = MAX_RETRIES + 1;
                return hold;
            }
            self.give_up_crossing(state);
            return hold;
        };
        self.target = Some(target);
        SpiderIntent {
            shoot: Some(target.point - from),
            ..hold
        }
    }

    /// No crossing to be had: carry on where it is, or let itself down to
    /// the floor and climb from there.
    fn give_up_crossing(&mut self, state: &SpiderFixtureState) {
        if Self::holds_digit(state) {
            self.enter(ClockSpiderPhase::Crawling);
        } else {
            self.enter(ClockSpiderPhase::Recovering);
        }
    }

    /// Up the line, with the grip off so the legs do not hold the spider
    /// where it is, and on near the anchor to catch it.
    fn climb(&mut self, state: &SpiderFixtureState) -> SpiderIntent {
        let idle = SpiderIntent::default();
        if Self::holds_digit(state) {
            self.climbs += 1;
            self.enter(ClockSpiderPhase::Crawling);
            return SpiderIntent { grip: true, ..idle };
        }
        let Some(line) = state.spider.line else {
            // Gone as the spider arrived, as often as not: its own momentum
            // pushes it into the anchor's cell, and a line from inside is
            // no line. The legs catch the cell on the next tick.
            self.enter(ClockSpiderPhase::Recovering);
            self.catching = true;
            return SpiderIntent { grip: true, ..idle };
        };
        if Self::on_ceiling(state) {
            self.leave(state);
            return SpiderIntent { grip: true, ..idle };
        }
        if self.phase_tick > STEP_LIMIT_TICKS {
            // Hanging under something it cannot catch: let the line go.
            self.enter(ClockSpiderPhase::Recovering);
            return SpiderIntent {
                cut: true,
                grip: true,
                ..idle
            };
        }
        self.climbing(line, state.spider.position, state.spider.radius)
    }

    /// Up the line, slowing as the anchor nears so it is not slammed into,
    /// and the legs out to catch what it is tied to. After a crossing they
    /// are out for whatever else the swing passes once it is well away from
    /// where it started, so a swing that overshoots its anchor ends on the
    /// digit beside it rather than on the far side with its line cut.
    fn climbing(&self, line: SpiderLine, position: Vec2, radius: f32) -> SpiderIntent {
        let away = position.distance_to(self.climb_from) > 2.5 * radius;
        SpiderIntent {
            reel: (line.distance / (4.0 * radius)).clamp(0.25, 1.0),
            grip: line.distance <= 1.6 * radius || (self.catch_all && self.phase_tick > 10 && away),
            ..SpiderIntent::default()
        }
    }

    /// Along the digit for a while, upward where the way is clear, and then
    /// a swing, another crawl or a crossing.
    fn crawl(&mut self, state: &SpiderFixtureState, published: &[DigitSurface]) -> SpiderIntent {
        let grip = SpiderIntent {
            grip: true,
            ..SpiderIntent::default()
        };
        if self.phase_tick == 1 {
            let boxes = Self::boxes(published);
            self.direction = self.climb_direction(state, &boxes);
            let [least, most] = CRAWL_TICKS;
            self.crawl_until = if self.can_swing(state) && self.swings < MAX_SWINGS {
                // A little way along the underside, and a swing from there.
                self.rng.random_range(least / 4..=least / 2)
            } else {
                self.rng.random_range(least..=most)
            };
        }
        if !Self::holds_digit(state) {
            if matches!(state.support, Some(Hold::Frame(_))) {
                // The frame is a place to leave from.
                self.leave(state);
                return grip;
            }
            self.enter(ClockSpiderPhase::Recovering);
            return grip;
        }
        // A line from the climb that brought it here is in the way of a
        // crawl round a corner.
        let crawling = SpiderIntent {
            crawl: self.direction,
            cut: state.spider.line.is_some(),
            ..grip
        };
        if self.phase_tick < self.crawl_until {
            return crawling;
        }
        // What next. A swing wants an underside to drop from, where the
        // line it hangs by cannot cross the cell it is tied to.
        let roll: f32 = self.rng.random_range(0.0..1.0);
        if self.can_swing(state) && self.swings < MAX_SWINGS && (self.swings == 0 || roll < 0.3) {
            self.enter(ClockSpiderPhase::Swinging);
            return grip;
        }
        if roll < 0.7 {
            self.enter(ClockSpiderPhase::Crawling);
            return crawling;
        }
        // Across to the other digit by the hatch, if a line reaches it.
        let other = match self.slot {
            1 => 2,
            _ => 1,
        };
        let boxes = Self::boxes(published);
        if self
            .side_marks(&boxes, other, state.spider.position, state.spider.radius)
            .is_empty()
        {
            self.enter(ClockSpiderPhase::Crawling);
            return crawling;
        }
        self.slot = other;
        self.enter(ClockSpiderPhase::Crossing);
        self.cross(state, published)
    }

    /// A line tied at its feet, some of it let out, and a drop to hang and
    /// swing on it below the digit.
    fn swing(&mut self, state: &SpiderFixtureState) -> SpiderIntent {
        let idle = SpiderIntent::default();
        let grip = SpiderIntent { grip: true, ..idle };
        let letting_out = SLACK_TICKS + 2;
        if self.phase_tick <= letting_out {
            if !Self::holds_digit(state) {
                self.enter(ClockSpiderPhase::Recovering);
                return grip;
            }
            if state.spider.shot.is_some() {
                return grip;
            }
            if state.spider.line.is_none() {
                if self.phase_tick > 2 {
                    // The line at its feet did not tie: crawl on instead.
                    self.enter(ClockSpiderPhase::Crawling);
                    return grip;
                }
                let aim = state.spider.support.map_or(-state.spider.up, |s| -s.normal);
                return SpiderIntent {
                    shoot: Some(aim),
                    ..grip
                };
            }
            let line = state.spider.line.expect("a line");
            if line.length < self.swing_line(line.anchor.y, state.spider.radius) {
                return SpiderIntent { reel: -1.0, ..grip };
            }
            // Enough let out: drop.
            self.phase_tick = letting_out;
            return grip;
        }
        if state.spider.line.is_none() {
            self.enter(ClockSpiderPhase::Recovering);
            return grip;
        }
        if self.phase_tick == letting_out + 1 {
            self.swings += 1;
        }
        if self.phase_tick >= letting_out + SWING_TICKS {
            self.enter(ClockSpiderPhase::Climbing);
            self.climb_from = state.spider.position;
            return SpiderIntent { reel: 1.0, ..idle };
        }
        idle
    }

    /// After a fall or a lost line: catch what is there, and from the floor
    /// walk under the nearest digit and climb it on a line.
    fn recover(&mut self, state: &SpiderFixtureState, published: &[DigitSurface]) -> SpiderIntent {
        let idle = SpiderIntent::default();
        let grip = SpiderIntent { grip: true, ..idle };
        if self.phase_tick == 1 {
            self.recoveries += 1;
        }
        if Self::holds_digit(state) {
            if self.catching {
                self.climbs += 1;
            }
            self.enter(ClockSpiderPhase::Crawling);
            return grip;
        }
        if matches!(state.support, Some(Hold::Floor)) && !self.landed {
            self.landed = true;
            self.falls += 1;
        }
        if state.spider.shot.is_some() {
            return grip;
        }
        if self.phase_tick > 3 * STEP_LIMIT_TICKS {
            self.leave(state);
            return grip;
        }
        match state.support {
            Some(Hold::Floor) => {
                if Self::tied_to_digit(state) {
                    self.enter(ClockSpiderPhase::Climbing);
                    self.climb_from = state.spider.position;
                    return SpiderIntent { reel: 1.0, ..idle };
                }
                // Under the nearest digit by its lowest cell, then straight up.
                let boxes = Self::boxes(published);
                let Some(target) =
                    Self::floor_mark(&boxes, state.spider.position, self.layout.pitch)
                else {
                    return grip;
                };
                self.slot = target.slot;
                let dx = target.point.x - state.spider.position.x;
                if dx.abs() > 0.3 * state.spider.radius {
                    return SpiderIntent {
                        crawl: dx.signum(),
                        cut: state.spider.line.is_some(),
                        ..grip
                    };
                }
                SpiderIntent {
                    shoot: Some(Vec2::Y),
                    cut: state.spider.line.is_some(),
                    ..grip
                }
            }
            Some(Hold::Frame(_)) => {
                // On a rail or the ceiling: a fine place to leave from.
                self.leave(state);
                grip
            }
            Some(Hold::Surface { .. } | Hold::Rigid(_) | Hold::Retired(_)) => grip,
            None => {
                if Self::tied_to_digit(state) {
                    self.enter(ClockSpiderPhase::Climbing);
                    self.climb_from = state.spider.position;
                    return SpiderIntent { reel: 1.0, ..idle };
                }
                // Falling, or hanging on a line to the beam or the ceiling:
                // let it down to the floor.
                SpiderIntent {
                    reel: if state.spider.line.is_some() {
                        -1.0
                    } else {
                        0.0
                    },
                    ..grip
                }
            }
        }
    }

    fn leave(&mut self, state: &SpiderFixtureState) {
        self.enter(ClockSpiderPhase::Leaving);
        self.direction = self.climb_direction(state, &[]);
        self.crawl_until = 0;
        self.pick_direction = true;
        self.stepping = false;
        self.exit = match state.support {
            Some(Hold::Frame(FramePart::CeilingLeft | FramePart::CeilingRight)) => Exit::ToHatch,
            Some(Hold::Frame(FramePart::Beam)) => Exit::Out,
            _ => Exit::ToTop,
        };
    }

    /// The next step of the way out, started afresh.
    fn step_exit(&mut self, exit: Exit, position: Vec2) {
        self.exit = exit;
        self.phase_tick = 0;
        self.retries = 0;
        self.target = None;
        self.crawl_until = 0;
        self.climb_from = position;
        self.pick_direction = true;
        self.stepping = false;
    }

    /// The way out, step by step.
    fn exit_step(
        &mut self,
        state: &SpiderFixtureState,
        published: &[DigitSurface],
    ) -> SpiderIntent {
        let idle = SpiderIntent::default();
        let grip = SpiderIntent { grip: true, ..idle };
        let layout = self.layout;
        let r = state.spider.radius;
        let position = state.spider.position;
        let out_of_view = position.y > layout.bounds_max.y;
        let ceiling = [FramePart::CeilingLeft, FramePart::CeilingRight];
        let climbing = |visit: &Self, line: SpiderLine| visit.climbing(line, position, r);
        match self.exit {
            Exit::ToTop => {
                if Self::on_ceiling(state) {
                    self.step_exit(Exit::ToHatch, position);
                    return grip;
                }
                if out_of_view {
                    self.step_exit(Exit::Out, position);
                    return grip;
                }
                if Self::tied_to(state, &ceiling) {
                    self.step_exit(Exit::ToCeiling, position);
                    return climbing(self, state.spider.line.expect("a line"));
                }
                if Self::tied_to(state, &[FramePart::Beam]) {
                    self.step_exit(Exit::Out, position);
                    return climbing(self, state.spider.line.expect("a line"));
                }
                if state.spider.shot.is_some() {
                    return grip;
                }
                match state.support {
                    Some(Hold::Floor) => return self.recover_for_exit(state, published),
                    Some(Hold::Frame(FramePart::LeftRail | FramePart::RightRail)) => {
                        // Into the ceiling from the top of the rail, and
                        // from lower down a line straight up to it, which a
                        // crawl up a tall view's rail would take too long
                        // to reach.
                        if position.y >= layout.canopy_y - 3.0 * r {
                            return SpiderIntent {
                                crawl: -state.spider.up.x.signum(),
                                cut: state.spider.line.is_some(),
                                ..grip
                            };
                        }
                        if self.phase_tick % 20 == 1 {
                            return SpiderIntent {
                                shoot: Some(Vec2::Y),
                                cut: state.spider.line.is_some(),
                                ..grip
                            };
                        }
                        return grip;
                    }
                    Some(Hold::Frame(_)) => return grip,
                    Some(Hold::Surface { .. } | Hold::Rigid(_) | Hold::Retired(_)) => {}
                    None => {
                        if let Some(line) = state.spider.line
                            && Self::tied_to_digit(state)
                        {
                            // Hanging under a digit: back onto it.
                            return climbing(self, line);
                        }
                        // Falling, or hanging on a line to nothing useful:
                        // down to the floor and up from there.
                        return SpiderIntent {
                            reel: if state.spider.line.is_some() {
                                -1.0
                            } else {
                                0.0
                            },
                            ..grip
                        };
                    }
                }
                // On a digit.
                if let Some(line) = state.spider.line
                    && let Some(Hold::Surface { id, .. }) = state.anchor
                    && line.anchor.y > position.y + 2.0 * r
                {
                    // A shot up that tied to a cell higher up: a step on the
                    // way, climbed once; a line back to the last step is
                    // going round in circles, so the crawl goes on instead.
                    if self.last_step == Some(id) {
                        self.target = None;
                        self.crawl_until = self.phase_tick + SKY_SEARCH_TICKS;
                    } else {
                        self.last_step = Some(id);
                        self.stepping = true;
                        return climbing(self, line);
                    }
                }
                if self.stepping {
                    // The step is climbed: the way on from here is chosen
                    // afresh.
                    self.stepping = false;
                    self.pick_direction = true;
                    self.target = None;
                }
                if self.target.take().is_some() {
                    // The shot did not tie: crawl on a while before another.
                    self.retries += 1;
                    self.crawl_until = self.phase_tick + SKY_SEARCH_TICKS / 4;
                }
                let boxes = Self::boxes(published);
                if self.phase_tick >= self.crawl_until
                    && let Some(aim) = Self::aim_up(state)
                    && self.climb_leads_up(&boxes, state)
                {
                    self.target = Some(Target {
                        slot: self.slot,
                        point: position + aim,
                    });
                    return SpiderIntent {
                        shoot: Some(aim),
                        cut: state.spider.line.is_some(),
                        ..grip
                    };
                }
                // Arriving on a top or under a cell: round to its side by an
                // end of its row it can get round. Then on the way it is
                // going, which keeps the digit on the same side and so,
                // round every corner and out of any pocket, goes the whole
                // way round the digit.
                if self.pick_direction
                    && let Some(direction) = Self::to_row_end(&boxes, state)
                {
                    self.direction = direction;
                    self.pick_direction = false;
                }
                SpiderIntent {
                    crawl: self.direction,
                    cut: state.spider.line.is_some(),
                    ..grip
                }
            }
            Exit::ToCeiling => {
                if Self::on_ceiling(state) {
                    self.step_exit(Exit::ToHatch, position);
                    return grip;
                }
                if Self::tied_to(state, &[FramePart::Beam]) {
                    self.step_exit(Exit::Out, position);
                    return climbing(self, state.spider.line.expect("a line"));
                }
                match state.spider.line {
                    Some(line) if Self::tied_to(state, &ceiling) => {
                        let stalled = self.phase_tick > u64::from(FIXED_HZ)
                            && state.spider.velocity.length() < 0.5 * r
                            && line.distance > 2.0 * r;
                        if stalled || self.phase_tick > STEP_LIMIT_TICKS {
                            // Wedged under a digit the line runs past, or
                            // hanging under a ceiling it cannot catch.
                            self.step_exit(Exit::ToTop, position);
                            return SpiderIntent { cut: true, ..grip };
                        }
                        if matches!(state.support, Some(Hold::Frame(_)))
                            && position.y >= layout.canopy_y - 3.0 * r
                        {
                            // Held to the top of a rail: into the ceiling.
                            return SpiderIntent {
                                crawl: -state.spider.up.x.signum(),
                                ..grip
                            };
                        }
                        climbing(self, line)
                    }
                    _ => {
                        // The line is gone: from wherever it is now.
                        self.step_exit(Exit::ToTop, position);
                        grip
                    }
                }
            }
            Exit::ToHatch => {
                if out_of_view {
                    self.step_exit(Exit::Out, position);
                    return grip;
                }
                if !Self::on_ceiling(state) {
                    // Off the ceiling: hanging on a line to it, or fallen.
                    self.step_exit(
                        if Self::tied_to(state, &ceiling) {
                            Exit::ToCeiling
                        } else {
                            Exit::ToTop
                        },
                        position,
                    );
                    return grip;
                }
                // Toward the hatch along the ceiling, and up its side: a
                // positive crawl goes leftward along a ceiling.
                let crawl = if position.x < 0.0 { -1.0 } else { 1.0 };
                SpiderIntent {
                    crawl,
                    cut: state.spider.line.is_some(),
                    ..grip
                }
            }
            Exit::Out => {
                let (beam, _) = FramePart::Beam.rect(layout);
                let gone = position.y - r >= layout.bounds_max.y
                    && ((Self::tied_to(state, &[FramePart::Beam])
                        && position.y >= beam.y - 2.0 * r)
                        || self.phase_tick > 2 * u64::from(FIXED_HZ));
                if gone {
                    self.done = true;
                    return idle;
                }
                if state.spider.shot.is_some() {
                    return idle;
                }
                if let Some(line) = state.spider.line
                    && Self::tied_to(state, &[FramePart::Beam])
                {
                    return SpiderIntent {
                        reel: 1.0,
                        grip: line.distance <= r,
                        ..idle
                    };
                }
                if !Self::on_ceiling(state) {
                    if !out_of_view {
                        // Fallen back in: the way out from the start.
                        self.step_exit(Exit::ToTop, position);
                    }
                    return grip;
                }
                let half_hatch = HATCH_WIDTH_PITCH * layout.pitch * 0.5;
                if position.x.abs() > half_hatch + layout.pitch - r {
                    // Past the end of the beam: back toward the hatch along
                    // the top of the ceiling.
                    return SpiderIntent {
                        crawl: -position.x.signum(),
                        ..grip
                    };
                }
                if self.phase_tick % 20 == 1 {
                    return SpiderIntent {
                        shoot: Some(Vec2::Y),
                        cut: state.spider.line.is_some(),
                        ..grip
                    };
                }
                grip
            }
        }
    }

    /// From the floor during the exit: the same walk and climb a recovery
    /// makes, after which clear sky is sought again.
    fn recover_for_exit(
        &mut self,
        state: &SpiderFixtureState,
        published: &[DigitSurface],
    ) -> SpiderIntent {
        let phase_tick = self.phase_tick;
        self.phase = ClockSpiderPhase::Recovering;
        let intent = self.recover(state, published);
        if self.phase == ClockSpiderPhase::Recovering {
            self.phase = ClockSpiderPhase::Leaving;
            self.phase_tick = phase_tick;
        } else if self.phase != ClockSpiderPhase::Leaving {
            // A climb or a crawl: back to the exit once it is on the digit.
            self.phase = ClockSpiderPhase::Leaving;
            self.exit = Exit::ToTop;
            self.phase_tick = phase_tick;
        }
        intent
    }

    pub fn diagnostics(&self, state: &SpiderFixtureState) -> ClockSpiderState {
        let milli = |value: f32| (value * 1000.0).round() as i32;
        let hold = |hold: Hold| match hold {
            Hold::Surface { id, departing, .. } => ClockSpiderHold::Surface {
                id: id.get(),
                departing,
            },
            Hold::Retired(id) => ClockSpiderHold::Surface {
                id: id.get(),
                departing: true,
            },
            Hold::Rigid(body) => ClockSpiderHold::Rigid {
                entity: body.entity.value(),
            },
            Hold::Frame(part) => ClockSpiderHold::Frame {
                part: match part {
                    FramePart::LeftRail => ClockSpiderFramePart::LeftRail,
                    FramePart::RightRail => ClockSpiderFramePart::RightRail,
                    FramePart::CeilingLeft => ClockSpiderFramePart::CeilingLeft,
                    FramePart::CeilingRight => ClockSpiderFramePart::CeilingRight,
                    FramePart::Beam => ClockSpiderFramePart::Beam,
                },
            },
            Hold::Floor => ClockSpiderHold::Floor,
        };
        ClockSpiderState {
            visit_id: self.visit_id,
            seed: self.seed,
            phase: self.phase,
            phase_tick: self.phase_tick,
            age_ticks: self.age,
            remaining_ticks: self.remaining(),
            position_milli: [
                milli(state.spider.position.x),
                milli(state.spider.position.y),
            ],
            velocity_milli: [
                milli(state.spider.velocity.x),
                milli(state.spider.velocity.y),
            ],
            up_milli: [milli(state.spider.up.x), milli(state.spider.up.y)],
            mode: match state.spider.mode {
                SpiderMode::Free => ClockSpiderMode::Free,
                SpiderMode::Crawling => ClockSpiderMode::Crawling,
                SpiderMode::Hanging => ClockSpiderMode::Hanging,
            },
            support: state.support.map(hold),
            anchor: state.anchor.map(hold),
            shooting: state.spider.shot.is_some(),
            line_length_milli: state
                .spider
                .line
                .map(|line| (line.length * 1000.0).round().max(0.0) as u32),
            climbs: self.climbs,
            swings: self.swings,
            falls: self.falls,
            recoveries: self.recoveries,
            failed_exit: self.failed_exit,
            proxies: state.proxies,
        }
    }
}

impl SpiderFixture {
    /// Where a visit starts: under the beam, above the hatch.
    pub(crate) fn visit_start(layout: Layout) -> Vec2 {
        Self::under_beam(layout)
    }
}
