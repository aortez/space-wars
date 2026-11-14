//! The guest that gathers the coins: a magpie on the crow's flight
//! controller, with a body in the world the coins lie in while it is in view.
//! It picks a coin off the top of the pile as the pile actually lies, lands
//! on what holds its feet beside it, hops up to stand next to it, takes it in
//! its beak and carries it out of view, home, and comes back for another
//! while the batch's time allows. The coin it takes leaves the world, and is
//! drawn in its beak. The coins it keeps are the return's shortfall, which
//! the reassembly grows replacements for. One seeded stream decides the home
//! side, the looks and the ties.
use engine_common::{ClockMagpiePhase as Phase, ClockMagpieState};
use engine_core::Vec2;
use engine_rapier::world::PhysicsWorld;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::Cell;
use crate::{
    crow::{Flight, body::MAGPIE},
    events::duck::ArenaTerrain,
    floor::DrainGeometry,
    layout::Layout,
};

/// What holds a foot on the floor the coins lie on: the batch's own floor
/// with its drain, or the terrain of the visit whose arena the coins fell
/// into, with its course, its pit or its panels.
#[derive(Clone, Copy)]
pub(crate) enum Support<'a> {
    Drain(DrainGeometry),
    Terrain(&'a ArenaTerrain),
}

impl Support<'_> {
    /// The height of what would hold a foot at `x`, or nothing over the
    /// drain, a pit or an opening, or beyond the view.
    pub fn at(self, x: f32) -> Option<f32> {
        match self {
            Self::Drain(drain) => drain
                .slabs()
                .find(|(min, max)| min.x <= x && x <= max.x)
                .map(|(_, max)| max.y),
            Self::Terrain(terrain) => terrain.support_at(x),
        }
    }
}

/// How long the coins lie in the arena for the magpie. Its trips fit in
/// here, and the return starts when it is up whether it came back or not.
pub const GATHERING_TICKS: u64 = 23 * 60;
/// An approach that has not landed by now is given up.
const APPROACH_TICKS: u64 = 6 * 60;
/// Leaving takes at most this long before the bird is simply gone.
const EXIT_TICKS: u64 = 4 * 60;
/// A magpie is quicker than the crow: this much, on the crow's controller.
const HASTE: f32 = 1.5;
/// What a trip costs beyond crossing at cruising speed: braking and coming
/// down at the coin, the look, any hops, the take and climbing out. Measured
/// at 178 to 441 ticks over the crossings, 211 at the median, with seeds 1
/// to 12 at five layouts. A trip that runs longer than this allows is cut
/// short at `EXIT_RESERVE`.
const TRIP_OVERHEAD_TICKS: u64 = 330;
/// Inside this much of the end the bird leaves, whatever it was doing.
const EXIT_RESERVE: u64 = 150;
const HOP_TICKS: u64 = 28;
/// How long the bird finds its feet after a hop, before it reaches for the
/// coin or hops again.
const SETTLE_TICKS: u64 = 6;
/// The fastest a hop comes down, in pitches a second.
const HOP_DESCENT: f32 = 4.0;
/// The hops a bird makes for one coin before it gives the trip up.
const MAX_HOPS: u8 = 3;
/// The most coins the bird carries off in one gathering.
const MAX_TRIPS: u32 = 4;
const TAKE_TICKS: u64 = 40;
/// The tick of the take at which the coin leaves the pile for the beak.
const TAKE_AT: u64 = 22;
/// Where the beak's tip is with the head down, in pitches from the feet,
/// facing right, and how far beyond a coin's edge it reaches.
const BEAK: Vec2 = Vec2::new(0.945, 0.105);
const REACH_PITCH: f32 = 0.35;

/// The coin the bird is after, and where it lay when chosen: a coin that
/// has moved since, or left the view, is chosen again.
#[derive(Clone, Copy, Debug)]
struct Target {
    index: usize,
    position: Vec2,
    /// Which side of the coin the bird comes at it from.
    side: f32,
    /// Where it sets down for it: on what holds a foot beside the heap.
    landing: Vec2,
    /// The hops it has made for this coin.
    hops: u8,
}

pub(crate) struct Magpie {
    layout: Layout,
    rng: StdRng,
    home_right: bool,
    pub phase: Phase,
    pub phase_tick: u64,
    /// Its feet.
    pub position: Vec2,
    pub facing_right: bool,
    pub flight: Flight,
    from: Vec2,
    to: Vec2,
    duration: u64,
    target: Option<Target>,
    /// The side of the coin in its beak.
    pub carrying: Option<f32>,
    trips: u32,
    abandoned: u32,
    done: bool,
}

impl Magpie {
    pub fn new(layout: Layout, seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let home_right = rng.random_bool(0.5);
        let mut magpie = Self {
            layout,
            rng,
            home_right,
            phase: Phase::Away,
            phase_tick: 0,
            position: Vec2::ZERO,
            facing_right: !home_right,
            flight: Flight::hasty(HASTE),
            from: Vec2::ZERO,
            to: Vec2::ZERO,
            duration: 0,
            target: None,
            carrying: None,
            trips: 0,
            abandoned: 0,
            done: false,
        };
        magpie.position = magpie.home();
        magpie
    }

    /// Nothing between the frame and the pile is in the way, so the bird
    /// flies low, just over the heaps.
    fn cruise(&self) -> f32 {
        let layout = self.layout;
        (layout.floor_y + layout.pitch * 4.5).min(layout.canopy_y - layout.pitch * 1.6 - 8.0)
    }

    /// Where the bird waits between trips, out of view on its home side.
    fn home(&self) -> Vec2 {
        let layout = self.layout;
        Vec2::new(
            if self.home_right {
                layout.bounds_max.x + layout.pitch * 2.0
            } else {
                layout.bounds_min.x - layout.pitch * 2.0
            },
            self.cruise(),
        )
    }

    pub fn in_view(&self) -> bool {
        self.phase != Phase::Away
    }

    /// Whether this coin can be gone for: lying in the view, not down the
    /// drain and not already in the beak.
    fn available(&self, cell: &Cell) -> bool {
        let layout = self.layout;
        !cell.taken
            && cell.side > 0.0
            && cell.position.y > layout.floor_y - cell.side * 0.5
            && cell.position.x > layout.bounds_min.x
            && cell.position.x < layout.bounds_max.x
    }

    /// The top of a heap: one of the highest coins, chosen at random among
    /// those within a coin of the highest that have a floor to set down
    /// beside.
    fn choose(&mut self, cells: &[Cell], support: Support<'_>) -> Option<Target> {
        let top = cells
            .iter()
            .filter(|cell| self.available(cell))
            .map(|cell| cell.position.y)
            .fold(f32::NEG_INFINITY, f32::max);
        if top == f32::NEG_INFINITY {
            return None;
        }
        let mut count = 0;
        let mut chosen = None;
        for (index, cell) in cells.iter().enumerate() {
            if !self.available(cell) || cell.position.y < top - self.layout.pitch * 0.75 {
                continue;
            }
            let Some((side, landing)) = self.approach(cell, support) else {
                continue;
            };
            count += 1;
            if self.rng.random_range(0..count) == 0 {
                chosen = Some(Target {
                    index,
                    position: cell.position,
                    side,
                    landing,
                    hops: 0,
                });
            }
        }
        chosen
    }

    /// The side to come at `cell` from and where to set down there: the
    /// nearest spot beside the heap with a level stretch of floor under it,
    /// inside the frame, away from the middle of the view first. A coin with
    /// the drain, a pit or an opening on both sides has none.
    fn approach(&self, cell: &Cell, support: Support<'_>) -> Option<(f32, Vec2)> {
        let layout = self.layout;
        let margin = layout.frame_width + layout.pitch * 0.9;
        let preferred = if cell.position.x >= 0.0 { 1.0 } else { -1.0 };
        for side in [preferred, -preferred] {
            for step in 0..=10 {
                let x = (cell.position.x + side * layout.pitch * (1.5 + 0.25 * step as f32))
                    .clamp(layout.bounds_min.x + margin, layout.bounds_max.x - margin);
                // Level for half a pace either way, so that the last of the
                // descent cannot drift off an edge.
                let samples = [-0.5, 0.0, 0.5].map(|offset| support.at(x + offset * layout.pitch));
                if let [Some(left), Some(height), Some(right)] = samples
                    && (left - height).abs() <= layout.pitch * 0.15
                    && (right - height).abs() <= layout.pitch * 0.15
                {
                    return Some((side, Vec2::new(x, height)));
                }
            }
        }
        None
    }

    /// Standing next to the coin, with the beak over it: on the heap if the
    /// coin is up on one, else on what is under the feet there. On the side
    /// the bird came from, unless nothing would hold a foot there, as over
    /// the drain or a pit, and something would on the other. Always inside
    /// the frame, whose walls hold the coins in view but not the bird.
    fn beside(&self, target: Target, cell: &Cell, support: Support<'_>) -> Vec2 {
        let layout = self.layout;
        let bottom = cell.position.y - cell.side * 0.5;
        let margin = layout.frame_width + layout.pitch * 0.9;
        let x = |side: f32| {
            (cell.position.x + side * layout.pitch * 0.8)
                .clamp(layout.bounds_min.x + margin, layout.bounds_max.x - margin)
        };
        let spot = |side: f32| {
            support
                .at(x(side))
                .map(|ground| Vec2::new(x(side), bottom.max(ground)))
        };
        spot(target.side)
            .or_else(|| spot(-target.side))
            .unwrap_or_else(|| Vec2::new(x(target.side), bottom.max(layout.floor_y)))
    }

    fn grounded(&self) -> bool {
        matches!(self.phase, Phase::Landed | Phase::Taking)
    }

    /// How far beyond `cell`'s edge the beak's tip is, with the head down,
    /// from where the bird stands now, facing it.
    fn beak_gap(&self, cell: &Cell) -> f32 {
        let facing = if cell.position.x >= self.position.x {
            1.0
        } else {
            -1.0
        };
        let beak = self.position + Vec2::new(BEAK.x * facing, BEAK.y) * self.layout.pitch;
        (cell.position - beak).length() - cell.side * 0.5
    }

    /// Whether the beak reaches `cell` from where the bird stands now.
    fn reaches(&self, cell: &Cell) -> bool {
        self.beak_gap(cell) <= self.layout.pitch * REACH_PITCH
    }

    /// What the bird takes from where it stands: the coin it came for if it
    /// reaches it, else the nearest other coin it does. Standing up on a
    /// heap, its feet are on coins higher than the one it came for, and the
    /// ones in front of its beak are there to take.
    fn within_reach(&self, cells: &[Cell]) -> Option<usize> {
        if let Some(target) = self.target
            && self.reaches(&cells[target.index])
        {
            return Some(target.index);
        }
        cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| self.available(cell) && self.reaches(cell))
            .min_by(|(_, a), (_, b)| self.beak_gap(a).total_cmp(&self.beak_gap(b)))
            .map(|(index, _)| index)
    }

    /// The bird finds its feet where it is, briefly, before it reaches for a
    /// coin or hops again.
    fn settle(&mut self) {
        self.to = self.position;
        self.phase = Phase::Landed;
        self.phase_tick = 0;
        self.duration = SETTLE_TICKS;
        self.stand();
    }

    /// Standing keeps the bird's weight on its feet, and whatever holds them
    /// takes it.
    fn stand(&mut self) {
        self.flight.velocity.y -=
            crate::crow::flight::gravity(self.layout) * crate::crow::flight::DT;
    }

    /// A hop is a short arc to the spot beside the coin: the legs and a
    /// flick of the wings carry the body along it, unless something is in
    /// the way. It comes down no faster than `HOP_DESCENT` pitches a second,
    /// so the bird settles onto a heap instead of knocking it about.
    fn hop(&mut self) {
        let mut velocity = crate::crow::flight::hop(
            self.from,
            self.to,
            self.phase_tick,
            self.duration,
            self.position,
            self.layout,
        );
        velocity.y = velocity.y.max(-self.layout.pitch * HOP_DESCENT);
        self.flight.velocity = velocity;
    }

    fn travel(&mut self, to: Vec2, phase: Phase) {
        let grounded = self.grounded();
        self.from = self.position;
        self.to = to;
        self.facing_right = to.x >= self.position.x;
        self.phase = phase;
        self.phase_tick = 0;
        self.duration = if phase == Phase::Hopping {
            HOP_TICKS
        } else {
            APPROACH_TICKS
        };
        if phase != Phase::Hopping {
            self.flight.retarget(self.position, grounded, self.layout);
        }
    }

    /// Where a departure aims: well beyond the frame, so the bird crosses the
    /// edge at speed instead of braking toward a point just outside it.
    fn exit(&self) -> Vec2 {
        let layout = self.layout;
        let beyond = (layout.bounds_max.x - layout.bounds_min.x) * 0.5 + layout.pitch * 4.0;
        Vec2::new(
            if self.home_right {
                layout.bounds_max.x + beyond
            } else {
                layout.bounds_min.x - beyond
            },
            self.cruise(),
        )
    }

    fn leave(&mut self) {
        let grounded = self.grounded();
        self.target = None;
        self.from = self.position;
        self.to = self.exit();
        self.facing_right = self.home_right;
        self.phase = Phase::Leaving;
        self.phase_tick = 0;
        self.duration = EXIT_TICKS;
        self.flight.retarget(self.position, grounded, self.layout);
    }

    /// Gives this trip up without a coin.
    fn abandon(&mut self) {
        self.abandoned += 1;
        self.leave();
    }

    /// Goes for `target` from wherever the bird is: a hop if it is standing
    /// near, a flight otherwise.
    fn pursue(&mut self, target: Target, cells: &[Cell], support: Support<'_>) {
        self.target = Some(target);
        let beside = self.beside(target, &cells[target.index], support);
        if self.grounded() && (beside - self.position).length() <= self.layout.pitch * 3.0 {
            self.travel(beside, Phase::Hopping);
        } else {
            self.travel(
                target.landing,
                if self.in_view() {
                    Phase::Flying
                } else {
                    Phase::Entering
                },
            );
        }
    }

    /// What a trip for `target` takes, from entering to being gone, at the
    /// controller's cruising speed: a trip the budget cannot hold is not
    /// started. One that runs long is cut short at `EXIT_RESERVE`, which
    /// leaves the bird the reserve and the return to fly out of view.
    fn trip_ticks(&self, target: Target, cell: &Cell, support: Support<'_>) -> u64 {
        let layout = self.layout;
        let width = layout.bounds_max.x - layout.bounds_min.x;
        let speed = (width * 0.55).max(layout.pitch * 9.0) * HASTE;
        let edge = if self.home_right {
            layout.bounds_max.x
        } else {
            layout.bounds_min.x
        };
        let approach = (self.home().x - target.landing.x).abs();
        let exit = (edge - self.beside(target, cell, support).x).abs() + layout.pitch * 1.5;
        ((approach + exit) / speed * 60.0) as u64 + TRIP_OVERHEAD_TICKS
    }

    /// Lands where the feet are down: the bird looks about there for a
    /// seeded moment before it moves on.
    fn land(&mut self) {
        self.to = self.position;
        self.flight = Flight::hasty(HASTE);
        self.phase = Phase::Landed;
        self.phase_tick = 0;
        self.duration = self.rng.random_range(15..=45);
    }

    /// One tick before the step of the world the coins lie in, given the
    /// batch's cells as they lie, how many ticks of gathering remain, what
    /// holds a foot on the floor they lie on, and that world. The bird enters
    /// it when a trip starts and leaves it when the bird is out of view.
    /// Returns the index of a coin the bird has just taken in its beak, which
    /// the caller takes out of the world.
    pub fn step(
        &mut self,
        cells: &[Cell],
        remaining: u64,
        support: Support<'_>,
        world: Option<&mut PhysicsWorld>,
    ) -> Option<usize> {
        if self.phase == Phase::Away {
            if self.done {
                return None;
            }
            self.position = self.home();
            self.flight = Flight::hasty(HASTE);
            match (self.choose(cells, support), world) {
                (Some(target), Some(world))
                    if self.trips < MAX_TRIPS
                        && remaining >= self.trip_ticks(target, &cells[target.index], support) =>
                {
                    self.pursue(target, cells, support);
                    MAGPIE.insert(world, self.position, self.flight.velocity, self.layout);
                }
                _ => self.done = true,
            }
            return None;
        }
        let Some(world) = world else {
            // The world went with the coins: so did the bird.
            self.phase = Phase::Away;
            self.carrying = None;
            return None;
        };
        if let Some((position, velocity)) = MAGPIE.motion(world) {
            self.position = position;
            self.flight.velocity = velocity;
        }
        let footing = MAGPIE.footing(world, self.layout);
        let taken = self.act(cells, remaining, support, footing);
        if self.phase == Phase::Away {
            MAGPIE.remove(world);
        } else {
            MAGPIE.drive(world, self.flight.velocity);
        }
        taken
    }

    /// The bird's decisions this tick, from where the last step left it.
    fn act(
        &mut self,
        cells: &[Cell],
        remaining: u64,
        support: Support<'_>,
        footing: bool,
    ) -> Option<usize> {
        self.phase_tick += 1;
        if self.phase != Phase::Leaving && remaining <= EXIT_RESERVE {
            if self.carrying.is_some() {
                self.leave();
            } else {
                self.abandon();
            }
        }
        // A coin that has moved, gone down the drain or been kicked out of
        // view since it was chosen is chosen again from where the bird is.
        if let Some(target) = self.target
            && self.phase != Phase::Hopping
        {
            let cell = &cells[target.index];
            let moved = (cell.position - target.position).length() > self.layout.pitch * 0.6;
            if !self.available(cell) || moved {
                match self.choose(cells, support) {
                    Some(target) => self.pursue(target, cells, support),
                    None => self.abandon(),
                }
            }
        }
        let layout = self.layout;
        match self.phase {
            Phase::Away => unreachable!("handled above"),
            Phase::Entering | Phase::Flying => {
                // What holds the feet decides the touchdown, not the
                // controller: near the spot it is the landing, a little off
                // the mark or not, on the floor, a coin or a platform's edge.
                if footing {
                    if (self.position.x - self.to.x).abs() <= layout.pitch * 1.5 {
                        self.land();
                        return None;
                    }
                    // A descent cut short by a retarget keeps its downward
                    // momentum. Whatever is under the feet holds them up
                    // wherever they are, and the wings start again from
                    // there.
                    self.flight.retarget(self.position, true, layout);
                }
                let cruise = self.cruise();
                self.flight
                    .steer(self.position, self.to, cruise, layout, false);
                if self.flight.velocity.x.abs() > layout.pitch * 0.2 {
                    self.facing_right = self.flight.velocity.x > 0.0;
                }
                if self.phase_tick >= self.duration {
                    self.abandon();
                }
            }
            Phase::Landed => {
                if self.phase_tick >= self.duration {
                    match (self.within_reach(cells), self.target) {
                        (Some(index), Some(target)) => {
                            let cell = &cells[index];
                            self.target = Some(Target {
                                index,
                                position: cell.position,
                                ..target
                            });
                            self.facing_right = cell.position.x >= self.position.x;
                            self.phase = Phase::Taking;
                            self.phase_tick = 0;
                            self.duration = TAKE_TICKS;
                        }
                        (None, Some(mut target)) if target.hops < MAX_HOPS => {
                            target.hops += 1;
                            self.target = Some(target);
                            let beside = self.beside(target, &cells[target.index], support);
                            self.travel(beside, Phase::Hopping);
                            self.hop();
                            return None;
                        }
                        _ => {
                            self.abandon();
                            return None;
                        }
                    }
                }
                self.stand();
            }
            Phase::Hopping => {
                if self.phase_tick < self.duration {
                    self.hop();
                } else if footing || self.phase_tick >= self.duration * 2 {
                    // The bird finds its feet where the hop ends, beside the
                    // coin unless something was in the way.
                    self.settle();
                } else {
                    // Its weight takes it down onto whatever will hold it.
                    self.stand();
                }
            }
            Phase::Taking => {
                if self.phase_tick == TAKE_AT
                    && let Some(target) = self.target
                {
                    if !self.reaches(&cells[target.index]) {
                        // Something moved the bird off the coin before its
                        // beak closed on it. It finds its feet where it is
                        // and reaches again for what is in front of it.
                        self.settle();
                        return None;
                    }
                    self.target = None;
                    self.carrying = Some(cells[target.index].side);
                    self.stand();
                    return Some(target.index);
                }
                if self.phase_tick >= self.duration {
                    if self.carrying.is_some() {
                        self.leave();
                    } else {
                        self.abandon();
                    }
                } else {
                    self.stand();
                }
            }
            Phase::Leaving => {
                // Leaving from a descent, the feet are held up the same way.
                if footing {
                    self.flight.retarget(self.position, true, layout);
                }
                let cruise = self.cruise();
                self.flight
                    .steer(self.position, self.to, cruise, layout, true);
                if self.flight.velocity.x.abs() > layout.pitch * 0.2 {
                    self.facing_right = self.flight.velocity.x > 0.0;
                }
                let gone = self.position.x < layout.bounds_min.x - layout.pitch * 1.5
                    || self.position.x > layout.bounds_max.x + layout.pitch * 1.5
                    || self.phase_tick >= self.duration;
                if gone {
                    if self.carrying.take().is_some() {
                        self.trips += 1;
                    }
                    self.phase = Phase::Away;
                    self.phase_tick = 0;
                    self.position = self.home();
                }
            }
        }
        None
    }

    pub fn diagnostics(&self) -> ClockMagpieState {
        ClockMagpieState {
            phase: self.phase,
            phase_tick: self.phase_tick,
            position_milli: [self.position.x, self.position.y].map(|v| (v * 1000.0).round() as i32),
            facing_right: self.facing_right,
            home_right: self.home_right,
            target: self.target.map(|target| target.index as u32),
            carrying: self.carrying.is_some(),
            trips: self.trips,
            abandoned: self.abandoned,
        }
    }
}
