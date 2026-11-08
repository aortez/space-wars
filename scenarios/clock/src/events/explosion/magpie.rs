//! The guest that gathers the coins: a magpie on the crow's flight
//! controller. It is drawn, never a body. It picks a coin off the top of the
//! pile as the pile actually lies, lands on the floor beside it, hops up to
//! stand next to it, takes it in its beak and carries it out of view, home,
//! and comes back for another while the batch's time allows. The coins it
//! keeps are the return's shortfall, which the reassembly grows replacements
//! for. One seeded stream decides the home side, the looks and the ties.
use engine_common::{ClockMagpiePhase as Phase, ClockMagpieState};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::Cell;
use crate::{crow::Flight, layout::Layout};

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
/// down at the coin, the look, the hop, the take and climbing out. Measured
/// at 230 to 290 ticks over the crossings (`coins_tests`); the rest is
/// margin, so the bird is never still here when the return begins.
const TRIP_OVERHEAD_TICKS: u64 = 330;
/// Inside this much of the end the bird leaves, whatever it was doing.
const EXIT_RESERVE: u64 = 150;
const HOP_TICKS: u64 = 28;
const TAKE_TICKS: u64 = 40;
/// The tick of the take at which the coin leaves the pile for the beak.
const TAKE_AT: u64 = 22;

/// The coin the bird is after, and where it lay when chosen: a coin that
/// has moved since, or left the view, is chosen again.
#[derive(Clone, Copy, Debug)]
struct Target {
    index: usize,
    position: Vec2,
    /// Which side of the coin the bird approaches from: away from the
    /// drain, so its feet are on the floor.
    side: f32,
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
    /// those within a coin of the highest.
    fn choose(&mut self, cells: &[Cell]) -> Option<Target> {
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
            count += 1;
            if self.rng.random_range(0..count) == 0 {
                chosen = Some((index, cell.position));
            }
        }
        chosen.map(|(index, position)| Target {
            index,
            position,
            side: if position.x >= 0.0 { 1.0 } else { -1.0 },
        })
    }

    /// The floor beside the heap, on the drain-free side of the coin and
    /// inside the frame.
    fn landing(&self, target: Target) -> Vec2 {
        let layout = self.layout;
        let margin = layout.frame_width + layout.pitch * 0.9;
        let spot = |side: f32| {
            (target.position.x + side * layout.pitch * 1.5)
                .clamp(layout.bounds_min.x + margin, layout.bounds_max.x - margin)
        };
        let mut x = spot(target.side);
        if x.abs() < layout.drain_half_width() + layout.pitch * 0.5 {
            x = spot(-target.side);
        }
        Vec2::new(x, layout.floor_y)
    }

    /// Standing next to the coin, on the heap if the coin is up on one, with
    /// the beak over it.
    fn beside(&self, target: Target, cell: &Cell) -> Vec2 {
        let layout = self.layout;
        Vec2::new(
            cell.position.x + target.side * layout.pitch * 0.8,
            (cell.position.y - cell.side * 0.5).max(layout.floor_y),
        )
    }

    fn grounded(&self) -> bool {
        matches!(self.phase, Phase::Landed | Phase::Taking)
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
    fn pursue(&mut self, target: Target, cells: &[Cell]) {
        self.target = Some(target);
        let beside = self.beside(target, &cells[target.index]);
        if self.grounded() && (beside - self.position).length() <= self.layout.pitch * 3.0 {
            self.travel(beside, Phase::Hopping);
        } else {
            let landing = self.landing(target);
            self.travel(
                landing,
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
    /// started, so the bird is away before the return and never cut off.
    fn trip_ticks(&self, target: Target, cell: &Cell) -> u64 {
        let layout = self.layout;
        let width = layout.bounds_max.x - layout.bounds_min.x;
        let speed = (width * 0.55).max(layout.pitch * 9.0) * HASTE;
        let edge = if self.home_right {
            layout.bounds_max.x
        } else {
            layout.bounds_min.x
        };
        let approach = (self.home().x - self.landing(target).x).abs();
        let exit = (edge - self.beside(target, cell).x).abs() + layout.pitch * 1.5;
        ((approach + exit) / speed * 60.0) as u64 + TRIP_OVERHEAD_TICKS
    }

    /// Lands at `to`: the feet set down there and the bird looks about for a
    /// seeded moment before it moves on.
    fn land(&mut self) {
        self.position = self.to;
        self.flight = Flight::hasty(HASTE);
        self.phase = Phase::Landed;
        self.phase_tick = 0;
        self.duration = self.rng.random_range(15..=45);
    }

    /// One tick, given the batch's cells as they lie and how many ticks of
    /// gathering remain. Returns the index of a coin the bird has just taken
    /// in its beak, which the caller takes out of the world.
    pub fn step(&mut self, cells: &[Cell], remaining: u64) -> Option<usize> {
        if self.phase == Phase::Away {
            if self.done {
                return None;
            }
            self.position = self.home();
            self.flight = Flight::hasty(HASTE);
            match self.choose(cells) {
                Some(target) if remaining >= self.trip_ticks(target, &cells[target.index]) => {
                    self.pursue(target, cells);
                }
                _ => self.done = true,
            }
            return None;
        }
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
                match self.choose(cells) {
                    Some(target) => self.pursue(target, cells),
                    None => self.abandon(),
                }
            }
        }
        let layout = self.layout;
        match self.phase {
            Phase::Away => unreachable!("handled above"),
            Phase::Entering | Phase::Flying => {
                let cruise = self.cruise();
                let mut landed =
                    self.flight
                        .step(&mut self.position, self.to, cruise, layout, false);
                // The floor is the destination, and it catches feet that come
                // down a little off the mark too.
                if self.position.y <= self.to.y
                    && (self.position.x - self.to.x).abs() <= layout.pitch * 1.5
                {
                    self.position.y = self.to.y;
                    landed = true;
                }
                if self.flight.velocity.x.abs() > layout.pitch * 0.2 {
                    self.facing_right = self.flight.velocity.x > 0.0;
                }
                if landed {
                    self.to.x = self.position.x;
                    self.land();
                } else if self.phase_tick >= self.duration {
                    self.abandon();
                }
            }
            Phase::Landed => {
                if self.phase_tick >= self.duration {
                    match self.target {
                        Some(target) => {
                            let beside = self.beside(target, &cells[target.index]);
                            self.travel(beside, Phase::Hopping);
                        }
                        None => self.abandon(),
                    }
                }
            }
            Phase::Hopping => {
                let t = (self.phase_tick as f32 / self.duration as f32).min(1.0);
                self.position = self.from * (1.0 - t)
                    + self.to * t
                    + Vec2::new(0.0, layout.pitch * 0.9 * 4.0 * t * (1.0 - t));
                if self.phase_tick >= self.duration {
                    self.position = self.to;
                    match self.target {
                        Some(target) if self.available(&cells[target.index]) => {
                            self.facing_right = cells[target.index].position.x >= self.position.x;
                            self.phase = Phase::Taking;
                            self.phase_tick = 0;
                            self.duration = TAKE_TICKS;
                        }
                        _ => {
                            self.phase = Phase::Landed;
                            match self.choose(cells) {
                                Some(target) => self.pursue(target, cells),
                                None => self.abandon(),
                            }
                        }
                    }
                }
            }
            Phase::Taking => {
                if self.phase_tick == TAKE_AT
                    && let Some(target) = self.target.take()
                {
                    self.carrying = Some(cells[target.index].side);
                    return Some(target.index);
                }
                if self.phase_tick >= self.duration {
                    if self.carrying.is_some() {
                        self.leave();
                    } else {
                        self.abandon();
                    }
                }
            }
            Phase::Leaving => {
                let cruise = self.cruise();
                self.flight
                    .step(&mut self.position, self.to, cruise, layout, true);
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
