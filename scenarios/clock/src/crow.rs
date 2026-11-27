//! Cheap resident visitors: a crow alone or, now and then, a flock of up to
//! [`MAX_CROWS`], each with a body in the run's arena. No wall-clock
//! sampling. Perch feet use the same lit-cell geometry as rendering, and a
//! visit brings a perch body for each column its crows may land on.
pub(crate) mod body;
mod environment;
pub(crate) mod flight;
mod water_tolerance;
pub(crate) use engine_common::ClockCrowPhase as Phase;
use engine_common::{ClockCrowState, ClockCrowWaterTolerance, ClockEventKind};
use engine_core::Vec2;
use engine_rapier::world::PhysicsWorld;
pub(crate) use environment::Environment;
pub(crate) use water_tolerance::WaterTolerance;
#[cfg(test)]
mod behavior_tests;
pub(crate) use flight::Flight;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{DIGIT_SLOT_COUNT, SegmentRepresentation, SegmentState, layout::Layout};

pub const CROW_TICKS: u64 = 22 * 60;
/// The most crows one visit holds.
pub const MAX_CROWS: usize = engine_common::CLOCK_MAX_CROWS;
const MAX_FLIGHT_TICKS: u64 = 8 * 60;
const EXIT_TICKS: u64 = 5 * 60;
const PECK_TICKS: u64 = 56;
const GROUND_VISIT_CHANCE: f64 = 0.2;
const HOP_TICKS: u64 = 28;
/// The columns a crow may perch on: six in each digit's grid.
pub(crate) const PERCH_COLUMNS: usize = DIGIT_SLOT_COUNT * 6;
/// How far the feet may be from the spot they come down to, across and up
/// or down, in pitches, for the crow to stand there.
const SPOT_REACH: f32 = 0.35;
const SPOT_DEPTH: f32 = 0.25;
/// How far a standing crow may be pushed across, in pitches, before it has
/// been knocked off its spot.
const KNOCKED_OFF: f32 = 0.5;
/// How often an admission brings a flock instead of a crow alone.
const FLOCK_CHANCE: f64 = 0.25;
/// The salt of the stream that decides how many crows a visit brings,
/// "flocksiz".
const FLOCK_SALT: u64 = 0x666c_6f63_6b73_697a;
/// The room a crow takes up, in pitches: across, its silhouette's 1.47 and a
/// sliver more, and up and down, its height of 1.16 and a sliver more. No
/// crow of a visit chooses a spot within its room across of another, at any
/// height, so each reads on its own, and a crow in flight follows only one
/// within its room up or down, in its lane.
const ROOM_ACROSS: f32 = 1.6;
const ROOM_UP: f32 = 1.2;
/// How far behind the crow before it each crow of a flock comes in, in
/// pitches.
const ENTRY_GAP: f32 = 2.4;
/// How far ahead, in pitches, a crow in flight looks for another flying
/// the same way in its lane.
const FOLLOW_RANGE: f32 = 3.0;
/// The gap, in pitches, a crow keeps behind another it follows: more than its
/// room across, since it is slow to brake and comes in closer than it aims.
const FOLLOW_GAP: f32 = 2.0;
/// How fast across, in pitches a second, a crow ahead flies while it cruises.
/// One slower is slowing to come down or climbing out, and a crow behind
/// passes it rather than wait at its back.
const FOLLOW_SPEED: f32 = 4.0;
/// How quickly a crow closes on the gap it keeps behind another, per second
/// of the distance it has still to close.
const CLOSING_RATE: f32 = 4.0;
/// The most a flock's entry lanes rise above the cruising height, in
/// pitches, where the canopy leaves room.
const LANE_RISE: f32 = 0.9;
/// The share of that rise each crow of a flock comes in at, by its place in
/// the flock's line.
const LANES: [f32; MAX_CROWS] = [0.0, 0.6, 0.25, 1.0];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Perch {
    key: [u8; 3],
    feet: Vec2,
}

#[derive(Debug, Clone, Copy)]
enum Target {
    Digit(Perch),
    Ground(Vec2),
}

impl Target {
    fn feet(self) -> Vec2 {
        match self {
            Self::Digit(p) => p.feet,
            Self::Ground(p) => p,
        }
    }
    fn key(self) -> Option<[u8; 3]> {
        match self {
            Self::Digit(p) => Some(p.key),
            Self::Ground(_) => None,
        }
    }
}

/// The room a visit's other crows take up: the columns of the spots they are
/// going to and of where they are, until each has gone. A crow chooses no
/// perch or spot on the ground there, so no two crows of a visit come down on
/// or beside each other, or into a column another is climbing out of or
/// coming down through. How they move tells a crow in flight which to follow.
#[derive(Clone, Copy, Default)]
pub(crate) struct Claimed {
    /// Where each other crow is going, by member.
    targets: [Option<Vec2>; MAX_CROWS],
    /// Where each other crow is, by member.
    positions: [Option<Vec2>; MAX_CROWS],
    /// How each other crow is moving, by member.
    velocities: [Option<Vec2>; MAX_CROWS],
}

impl Claimed {
    /// What the crows of `crows` other than `member` have claimed.
    fn by_others(crows: &[Option<Crow>; MAX_CROWS], member: usize) -> Self {
        let other = |index: usize| crows[index].as_ref().filter(|_| index != member);
        Self {
            targets: std::array::from_fn(|index| other(index).and_then(Crow::destination)),
            positions: std::array::from_fn(|index| other(index).map(|crow| crow.position)),
            velocities: std::array::from_fn(|index| other(index).map(|crow| crow.flight.velocity)),
        }
    }

    /// The most speed across that a crow at `position`, flying with
    /// `velocity`, may ask for so that it keeps [`FOLLOW_GAP`] behind each
    /// other crow cruising ahead of it in its lane, the same way: that crow's
    /// speed, and a closing speed that falls to nothing as the gap does, or
    /// less once it is too close. A crow coming the other way, standing, or
    /// slowing to come down or climbing out, or out of its lane, it passes.
    fn following(self, position: Vec2, velocity: Vec2, layout: Layout) -> f32 {
        let mut limit = f32::INFINITY;
        if velocity.x.abs() < layout.pitch * 0.2 {
            return limit;
        }
        for (at, moving) in self.positions.iter().zip(self.velocities) {
            let (Some(at), Some(moving)) = (at, moving) else {
                continue;
            };
            let gap = (at.x - position.x) * velocity.x.signum();
            if moving.x * velocity.x <= 0.0
                || moving.x.abs() < layout.pitch * FOLLOW_SPEED
                || gap <= 0.0
                || gap > layout.pitch * FOLLOW_RANGE
                || (at.y - position.y).abs() >= layout.pitch * ROOM_UP
            {
                continue;
            }
            let closing = (gap - layout.pitch * FOLLOW_GAP) * CLOSING_RATE;
            limit = limit.min((moving.x.abs() + closing).max(0.0));
        }
        limit
    }

    /// Whether a crow standing with its feet at `feet` would crowd another
    /// crow of the visit: be within [`ROOM_ACROSS`] of the spot that crow is
    /// going to, or of where it is, at any height, since each climbs out of
    /// its spot and comes down to it through the column above it.
    fn crowds(self, feet: Vec2, layout: Layout) -> bool {
        self.targets
            .iter()
            .chain(&self.positions)
            .flatten()
            .any(|at| (at.x - feet.x).abs() < layout.pitch * ROOM_ACROSS)
    }
}

/// Only the upper silhouette in each of the 24 digit columns is landable.
/// This excludes hidden lower bars and keeps flight descents clear of digits.
fn perches(
    layout: Layout,
    segments: &[SegmentState],
    event: Option<ClockEventKind>,
) -> [Option<Perch>; PERCH_COLUMNS] {
    let mut result: [Option<Perch>; PERCH_COLUMNS] = [None; PERCH_COLUMNS];
    if matches!(
        event,
        Some(
            ClockEventKind::Falling
                | ClockEventKind::Meltdown
                | ClockEventKind::DigitSlide
                | ClockEventKind::CellSwap
                | ClockEventKind::Marquee
                | ClockEventKind::Explosion
                | ClockEventKind::Coins
        )
    ) {
        return result;
    }
    for segment in segments
        .iter()
        .filter(|s| s.lit && s.representation == SegmentRepresentation::Anchored)
    {
        for cell in segment.cells() {
            let index = usize::from(segment.id.digit_slot) * 6 + cell.x as usize;
            let feet = layout.cell_center(segment.id, cell) + Vec2::new(0.0, layout.pitch * 0.4);
            if result[index].is_none_or(|old| feet.y > old.feet.y) {
                result[index] = Some(Perch {
                    key: [segment.id.digit_slot, cell.x as u8, cell.y as u8],
                    feet,
                });
            }
        }
    }
    result
}

/// One crow of a visit: where it is, what it is doing, and its own random
/// stream, temperament and wetness.
pub(crate) struct Crow {
    layout: Layout,
    visit_id: u64,
    /// The crow's identity in its visit, which it keeps when others leave.
    member: u8,
    body: body::Bird,
    /// Ticks since the visit was admitted. Every crow comes with its visit
    /// and steps with it, so this is the visit's age too.
    age: u64,
    pub phase: Phase,
    pub phase_tick: u64,
    pub position: Vec2,
    pub facing_right: bool,
    target: Option<Target>,
    from: Vec2,
    to: Vec2,
    duration: u64,
    /// How far above the cruising height the crow comes in: its lane in its
    /// flock's line, and none for a crow alone.
    lane: f32,
    rng: StdRng,
    hops: u32,
    escapes: u32,
    pub flight: Flight,
    ground_attempted: bool,
    ground_visits: u32,
    pecks: u32,
    wetness: f32,
    water_tolerance: WaterTolerance,
    wet_departures: u32,
}

impl Crow {
    /// Crow `member` of a visit, setting out for one of `candidates` that
    /// `environment` leaves it. It comes in from the left if `from_left`
    /// says so, as a flock's later crows come in from the side the first
    /// drew; a crow without it draws its side.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout: Layout,
        seed: u64,
        visit_id: u64,
        member: usize,
        water_tolerance: ClockCrowWaterTolerance,
        candidates: &[Option<Perch>; PERCH_COLUMNS],
        environment: Environment<'_>,
        from_left: Option<bool>,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let facing_right = from_left.unwrap_or_else(|| rng.random_bool(0.5));
        let position = Vec2::new(
            if facing_right {
                layout.bounds_min.x - layout.pitch * 2.0
            } else {
                layout.bounds_max.x + layout.pitch * 2.0
            },
            Self::cruise_height(layout),
        );
        let mut visit = Self {
            layout,
            visit_id,
            member: u8::try_from(member).expect("a visit's crow"),
            body: body::crow(member),
            age: 0,
            phase: Phase::Entering,
            phase_tick: 0,
            position,
            facing_right,
            target: None,
            from: position,
            to: position,
            duration: MAX_FLIGHT_TICKS,
            lane: 0.0,
            rng,
            hops: 0,
            escapes: 0,
            flight: Flight::default(),
            ground_attempted: false,
            ground_visits: 0,
            pecks: 0,
            wetness: 0.0,
            water_tolerance: WaterTolerance::new(water_tolerance, seed),
            wet_departures: 0,
        };
        let target = visit.choose(candidates, false, environment);
        if let Some(target) = target {
            visit.travel(Target::Digit(target), Phase::Entering);
        } else {
            visit.leave();
        }
        visit
    }

    fn cruise_height(layout: Layout) -> f32 {
        // Fly above the tallest possible digit, with room for raised wings.
        // Keeping this relative to the face avoids enormous portrait detours.
        (layout.pitch * 5.2).min(Self::ceiling(layout))
    }

    /// The highest a crow's feet go while it crosses the face, which leaves
    /// room under the canopy for its raised wings.
    fn ceiling(layout: Layout) -> f32 {
        layout.canopy_y - layout.pitch * 1.6 - 8.0
    }

    /// How far a flock's entry lanes may rise above the cruising height: as
    /// far as the ceiling leaves room, up to [`LANE_RISE`] pitches.
    fn lane_rise(layout: Layout) -> f32 {
        (Self::ceiling(layout) - Self::cruise_height(layout)).clamp(0.0, layout.pitch * LANE_RISE)
    }

    /// The height the crow crosses the face at: its lane on the way in, and
    /// the cruising height every crow keeps after that.
    fn cruise(&self) -> f32 {
        Self::cruise_height(self.layout)
            + if self.phase == Phase::Entering {
                self.lane
            } else {
                0.0
            }
    }

    /// Moves the crow back to `place` in its flock's line on the way in, that
    /// many gaps further out than the first, and up into a lane `lane` above
    /// the cruising height. It sets out from there as it would have from the
    /// first place.
    fn line_up(&mut self, place: usize, lane: f32) {
        let outward = self.position.x.signum();
        self.position += Vec2::new(outward * place as f32 * ENTRY_GAP * self.layout.pitch, lane);
        self.lane = lane;
        self.from = self.position;
        self.flight.retarget(self.position, false, self.layout);
    }

    fn choose(
        &mut self,
        candidates: &[Option<Perch>; PERCH_COLUMNS],
        nearby: bool,
        environment: Environment<'_>,
    ) -> Option<Perch> {
        let mut count = 0;
        let mut selected = None;
        for candidate in candidates.iter().flatten().copied() {
            if self.target.and_then(Target::key) == Some(candidate.key)
                || environment.claimed.crowds(candidate.feet, self.layout)
                || environment.wet_feet(candidate.feet, self.layout, self.water_tolerance)
            {
                continue;
            }
            // Same-level adjacent blocks make readable, short two-footed hops,
            // measured from the perch the crow stands on.
            let delta = candidate.feet - self.to;
            if nearby && (delta.y.abs() > 0.01 || delta.x.abs() > self.layout.pitch * 2.1) {
                continue;
            }
            count += 1;
            if self.rng.random_range(0..count) == 0 {
                selected = Some(candidate);
            }
        }
        selected
    }

    /// Whether a crow that has found nowhere to go has somewhere all the
    /// same, but for the room the visit's other crows take up: a perch other
    /// than its own that is dry enough for it.
    fn crowded_out(
        &self,
        candidates: &[Option<Perch>; PERCH_COLUMNS],
        environment: Environment<'_>,
    ) -> bool {
        candidates.iter().flatten().any(|candidate| {
            self.target.and_then(Target::key) != Some(candidate.key)
                && !environment.wet_feet(candidate.feet, self.layout, self.water_tolerance)
        })
    }

    fn travel(&mut self, target: Target, phase: Phase) {
        let grounded = matches!(self.phase, Phase::Perched | Phase::Pecking);
        self.target = Some(target);
        self.from = self.position;
        self.to = target.feet();
        self.facing_right = self.to.x >= self.from.x;
        self.phase = phase;
        self.phase_tick = 0;
        self.duration = if phase == Phase::Hopping {
            HOP_TICKS
        } else {
            MAX_FLIGHT_TICKS
        };
        if phase != Phase::Hopping {
            self.flight.retarget(self.position, grounded, self.layout);
        }
    }

    fn leave(&mut self) {
        let grounded = matches!(self.phase, Phase::Perched | Phase::Pecking);
        self.target = None;
        self.from = self.position;
        self.to = Vec2::new(
            if self.facing_right {
                self.layout.bounds_max.x + self.layout.pitch * 2.0
            } else {
                self.layout.bounds_min.x - self.layout.pitch * 2.0
            },
            Self::cruise_height(self.layout),
        );
        self.phase = Phase::Leaving;
        self.phase_tick = 0;
        self.duration = EXIT_TICKS;
        self.flight.retarget(self.position, grounded, self.layout);
    }

    /// React to support changes even in a zero-dt control/read synchronization.
    /// Position and age remain frozen until an actual simulation tick.
    pub fn synchronize(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
    ) {
        if self.phase == Phase::Leaving {
            return;
        }
        let candidates = perches(self.layout, segments, event);
        let valid = self.target.is_some_and(|target| match target {
            Target::Digit(p) => {
                candidates.iter().flatten().any(|c| c.key == p.key)
                    && (self.phase == Phase::Perched
                        || !environment.wet_feet(p.feet, self.layout, self.water_tolerance))
            }
            Target::Ground(p) => {
                candidates.iter().any(Option::is_some)
                    && environment.ground_available
                    && (self.phase == Phase::Pecking
                        || (!environment.wet_feet(p, self.layout, self.water_tolerance)
                            && (!self.water_tolerance.avoids_spray
                                || environment.spray(p, self.layout) == 0.0)))
            }
        });
        if valid {
            return;
        }
        self.escapes += 1;
        if self.phase == Phase::Pecking {
            self.leave();
        } else {
            self.return_to_perch(&candidates, environment);
        }
    }

    fn return_to_perch(
        &mut self,
        candidates: &[Option<Perch>; PERCH_COLUMNS],
        environment: Environment<'_>,
    ) {
        let nearby = matches!(self.target, Some(Target::Ground(_)))
            .then(|| {
                candidates
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|p| {
                        !environment.claimed.crowds(p.feet, self.layout)
                            && !environment.wet_feet(p.feet, self.layout, self.water_tolerance)
                    })
                    .min_by(|a, b| {
                        let cost = |p: Perch| {
                            (p.feet.x - self.position.x).abs()
                                + (Self::cruise_height(self.layout) - p.feet.y).abs() * 2.0
                        };
                        cost(*a).total_cmp(&cost(*b))
                    })
            })
            .flatten();
        if let Some(target) = nearby.or_else(|| self.choose(candidates, false, environment)) {
            self.travel(Target::Digit(target), Phase::Flying);
        } else {
            self.leave();
        }
    }

    fn try_ground(&mut self, environment: Environment<'_>) -> bool {
        if self.ground_attempted {
            return false;
        }
        self.ground_attempted = true;
        // Allow the full approach timeout and two pecks before the ordinary
        // departure deadline, which already reserves five seconds for the exit.
        let available = (CROW_TICKS - EXIT_TICKS).saturating_sub(self.age);
        // A tall layout must still allow a climb from the floor and a short
        // crossing offscreen within that exit window.
        let exit_seconds = (Self::cruise_height(self.layout) - self.layout.floor_y)
            / (self.layout.pitch * 5.0)
            + 1.5;
        if exit_seconds > EXIT_TICKS as f32 / 60.0
            || MAX_FLIGHT_TICKS + PECK_TICKS > available
            || !self.rng.random_bool(GROUND_VISIT_CHANCE)
        {
            return false;
        }
        let spots = environment.ground_spots(self.layout, self.water_tolerance);
        if let Some(target) = spots
            .into_iter()
            .flatten()
            .filter(|spot| !environment.claimed.crowds(*spot, self.layout))
            .min_by(|a, b| {
                (a.x - self.position.x)
                    .abs()
                    .total_cmp(&(b.x - self.position.x).abs())
            })
        {
            self.travel(Target::Ground(target), Phase::Flying);
            return true;
        }
        false
    }

    /// Where the crow is going: the feet of the perch or the spot on the
    /// ground it has chosen, if it has one.
    pub fn destination(&self) -> Option<Vec2> {
        self.target.map(Target::feet)
    }

    /// Puts the crow's body in the arena where the visit starts.
    pub fn enter(&self, world: &mut PhysicsWorld) {
        self.body
            .insert(world, self.position, self.flight.velocity, self.layout);
    }

    /// Takes the crow's body out of the arena, when it has left before the
    /// rest of its visit.
    fn exit(&self, world: &mut PhysicsWorld) {
        self.body.remove(world);
    }

    /// The crow's part of the tick before the arena's step: its body takes
    /// the velocity the crow chose.
    pub fn drive(&self, world: &mut PhysicsWorld) {
        self.body.drive(world, self.flight.velocity);
    }

    /// The crow's tick after the arena's step. It reads its body back, reacts
    /// to what holds its feet and to what the face and the water do, and
    /// chooses the velocity its body takes into the next step. Returns true
    /// when the visit is over.
    pub fn step(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
        world: &PhysicsWorld,
    ) -> bool {
        if let Some((position, velocity)) = self.body.motion(world) {
            self.position = position;
            self.flight.velocity = velocity;
        }
        let footing = self.body.footing(world, self.layout);
        self.synchronize(segments, event, environment);
        self.age += 1;
        if self.age >= CROW_TICKS {
            return true;
        }
        if self.age >= CROW_TICKS - EXIT_TICKS && self.phase != Phase::Leaving {
            self.leave();
        }
        let spray = environment.spray(self.position, self.layout);
        self.wetness = (self.wetness + spray / self.water_tolerance.soak_ticks
            - if spray == 0.0 { 1.0 / 180.0 } else { 0.0 })
        .clamp(0.0, 1.0);
        if self.phase != Phase::Leaving
            && (self.wetness >= 1.0
                || (matches!(self.phase, Phase::Perched | Phase::Pecking)
                    && environment.wet_feet(self.position, self.layout, self.water_tolerance)))
        {
            self.wet_departures += 1;
            self.leave();
        }
        self.phase_tick += 1;
        if matches!(self.phase, Phase::Perched | Phase::Pecking) && self.knocked_off(footing) {
            // Something pushed the crow off its spot. It takes to the air
            // for a perch, as it does when one goes from under it.
            self.escapes += 1;
            if self.phase == Phase::Pecking {
                self.leave();
            } else {
                self.return_to_perch(&perches(self.layout, segments, event), environment);
            }
            return false;
        }
        if self.phase == Phase::Pecking {
            if self.phase_tick % 28 == 16 {
                self.pecks += 1;
            }
            if self.phase_tick >= self.duration {
                // Ground pecking is the final activity of an occasional visit.
                self.leave();
            } else {
                self.stand();
            }
            return false;
        }
        if self.phase == Phase::Perched {
            // Consider the one ground excursion after a short look around,
            // before spending the visit's time budget on additional hops.
            if self.phase_tick >= 30 && self.try_ground(environment) {
                return false;
            }
            if self.phase_tick >= self.duration {
                let candidates = perches(self.layout, segments, event);
                if let Some(target) = self.choose(&candidates, true, environment) {
                    self.hops += 1;
                    self.travel(Target::Digit(target), Phase::Hopping);
                    self.hop();
                } else if let Some(target) = self.choose(&candidates, false, environment) {
                    self.travel(Target::Digit(target), Phase::Flying);
                } else if self.crowded_out(&candidates, environment) {
                    // Every other perch is in the room of another crow of
                    // the visit for now. It looks around again before it
                    // tries once more, until the visit's deadline.
                    self.phase_tick = 0;
                    self.duration = self.rng.random_range(50..=120);
                    self.stand();
                } else {
                    self.leave();
                }
            } else {
                self.stand();
            }
            return false;
        }
        if self.phase == Phase::Hopping {
            if self.phase_tick < self.duration {
                self.hop();
            } else if self.on_spot() {
                self.land();
            } else if let Some(target) = self.target {
                // Something stopped the hop short. The wings take it the rest
                // of the way.
                self.travel(target, Phase::Flying);
            }
            return false;
        }
        let leaving = self.phase == Phase::Leaving;
        if footing {
            if !leaving && self.on_spot() && self.flight.touchdown(self.layout) {
                self.land();
                return false;
            }
            // Retargeting or departure can interrupt a descent, and what the
            // feet came down on holds them: a perch, the floor or whatever
            // else is there. The wings start again from there. On its own
            // spot, a crow still too quick to stand settles first.
            if leaving || !self.on_spot() {
                self.flight.retarget(self.position, true, self.layout);
            }
        }
        let limit = environment
            .claimed
            .following(self.position, self.flight.velocity, self.layout);
        self.flight.steer_within(
            self.position,
            self.to,
            self.cruise(),
            self.layout,
            leaving,
            limit,
        );
        if self.flight.velocity.x.abs() > self.layout.pitch * 0.2 {
            self.facing_right = self.flight.velocity.x > 0.0;
        }
        if leaving {
            return self.position.x < self.layout.bounds_min.x - self.layout.pitch * 1.5
                || self.position.x > self.layout.bounds_max.x + self.layout.pitch * 1.5
                || self.phase_tick >= self.duration;
        }
        if self.phase_tick >= self.duration {
            self.leave();
        }
        false
    }

    /// Whether the feet are on the spot the crow is coming down to or
    /// standing on.
    fn on_spot(&self) -> bool {
        (self.position.x - self.to.x).abs() <= self.layout.pitch * SPOT_REACH
            && (self.position.y - self.to.y).abs() <= self.layout.pitch * SPOT_DEPTH
    }

    /// A standing crow that nothing holds up any more, and that has fallen or
    /// been pushed from its spot, was knocked off it.
    fn knocked_off(&self, footing: bool) -> bool {
        !footing
            && ((self.position.x - self.to.x).abs() > self.layout.pitch * KNOCKED_OFF
                || self.position.y < self.to.y - self.layout.pitch * SPOT_DEPTH)
    }

    /// Standing keeps the crow's weight on its feet, and whatever holds them
    /// takes it.
    fn stand(&mut self) {
        self.flight.velocity.y -= flight::gravity(self.layout) * flight::DT;
    }

    /// A hop is a short arc to the next perch: the legs and a flick of the
    /// wings carry the body along it, unless something is in the way.
    fn hop(&mut self) {
        self.flight.velocity = flight::hop(
            self.from,
            self.to,
            self.phase_tick,
            self.duration,
            self.position,
            self.layout,
        );
    }

    /// The feet are down on the spot the crow came to: it stands there,
    /// where the step left it, and looks about or pecks.
    fn land(&mut self) {
        self.flight = Flight::default();
        self.phase_tick = 0;
        if matches!(self.target, Some(Target::Ground(_))) {
            self.phase = Phase::Pecking;
            self.ground_visits += 1;
            self.duration = PECK_TICKS;
        } else {
            self.phase = Phase::Perched;
            self.duration = self.rng.random_range(50..=120);
        }
    }

    pub fn diagnostics(&self) -> ClockCrowState {
        ClockCrowState {
            visit_id: self.visit_id,
            member: self.member,
            water_tolerance: self.water_tolerance.kind,
            phase: self.phase,
            phase_tick: self.phase_tick,
            age_ticks: self.age,
            position_milli: [self.position.x, self.position.y].map(|v| (v * 1000.0).round() as i32),
            facing_right: self.facing_right,
            target: self.target.and_then(Target::key),
            ground_target_milli: self.target.and_then(|t| match t {
                Target::Ground(p) => Some([p.x, p.y].map(|v| (v * 1000.0).round() as i32)),
                Target::Digit(_) => None,
            }),
            ground_visits: self.ground_visits,
            pecks: self.pecks,
            wetness_milli: (self.wetness * 1000.0).round() as u16,
            wet_departures: self.wet_departures,
            hops: self.hops,
            escapes: self.escapes,
        }
    }
}

/// A visit of crows: one admission of up to [`MAX_CROWS`] birds, and the
/// perches they share. A flock comes in together from one side, in a loose
/// line, and then each crow goes its own way: it has its own body, flight,
/// target, random stream, temperament and wetness, keeps out of the others'
/// room, and leaves when it is done, and a crow that has left is not
/// replaced. The crows come with the visit and step with it, so all that
/// remain start for home by its deadline and none outlasts it. The visit is
/// over when its last crow has left.
pub(crate) struct CrowVisit {
    layout: Layout,
    /// The crows by member, which stays a crow's identity when others leave:
    /// a crow that has left leaves its place empty.
    crows: [Option<Crow>; MAX_CROWS],
    perches: body::Perches,
}

impl CrowVisit {
    /// A visit of `size` crows. Its first crow takes `seed`, so that a visit
    /// of one is the crow that has always come, and each other crow takes a
    /// seed of its own derived from it. Each chooses where it is going in
    /// turn, out of the room of those before it. A flock then comes in from
    /// the side its first crow drew, led by the crow going furthest, so none
    /// overtakes another on the way in, and each of the rest comes in a gap
    /// further back than the one before it, in a lane of its own.
    pub fn new(
        layout: Layout,
        seed: u64,
        visit_id: u64,
        size: usize,
        water_tolerance: ClockCrowWaterTolerance,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
    ) -> Self {
        assert!(
            (1..=MAX_CROWS).contains(&size),
            "a visit of one to {MAX_CROWS} crows, not {size}"
        );
        let candidates = perches(layout, segments, event);
        let mut crows = [const { None }; MAX_CROWS];
        let mut from_left = None;
        for member in 0..size {
            let environment = Environment {
                claimed: Claimed::by_others(&crows, member),
                ..Environment::default()
            };
            let crow = Crow::new(
                layout,
                member_seed(seed, member),
                visit_id,
                member,
                water_tolerance,
                &candidates,
                environment,
                from_left,
            );
            from_left = Some(crow.position.x < 0.0);
            crows[member] = Some(crow);
        }
        if let Some(from_left) = from_left.filter(|_| size > 1) {
            // How far in from the flock's side a crow is going; a crow with
            // nowhere to go crosses the whole face, so it leads.
            let reach = |member: usize| {
                crows[member]
                    .as_ref()
                    .and_then(Crow::destination)
                    .map_or(f32::INFINITY, |to| if from_left { to.x } else { -to.x })
            };
            let mut line: [usize; MAX_CROWS] = std::array::from_fn(|member| member);
            line[..size].sort_by(|a, b| reach(*b).total_cmp(&reach(*a)).then(a.cmp(b)));
            let rise = Crow::lane_rise(layout);
            for (place, member) in line[..size].iter().enumerate() {
                crows[*member]
                    .as_mut()
                    .expect("a crow of the flock")
                    .line_up(place, LANES[place] * rise);
            }
        }
        Self {
            layout,
            crows,
            perches: body::Perches::default(),
        }
    }

    /// The crows that have not left, by member.
    pub fn crows(&self) -> impl Iterator<Item = &Crow> {
        self.crows.iter().flatten()
    }

    /// The crow with the lowest member of those that have not left.
    pub fn first(&self) -> &Crow {
        self.crows()
            .next()
            .expect("a visit holds a crow until it is over")
    }

    /// The crow with the lowest member of those that have not left, to
    /// change.
    #[cfg(test)]
    pub fn first_mut(&mut self) -> &mut Crow {
        self.crows
            .iter_mut()
            .flatten()
            .next()
            .expect("a visit holds a crow until it is over")
    }

    /// How many perches the visit has placed in the arena.
    #[cfg(test)]
    pub fn perch_count(&self) -> usize {
        self.perches.count()
    }

    /// Sends crow `member` home at once: at its next step it has left, as a
    /// crow does at the end of its departure.
    #[cfg(test)]
    pub fn send_home(&mut self, member: usize) {
        let crow = self.crows[member].as_mut().expect("a crow of the visit");
        crow.leave();
        crow.phase_tick = crow.duration;
    }

    /// Puts the crows' bodies in the arena where the visit starts.
    pub fn enter(&self, world: &mut PhysicsWorld) {
        for crow in self.crows() {
            crow.enter(world);
        }
    }

    /// Each crow reacts to support changes, as [`Crow::synchronize`] does,
    /// in turn, so that each knows what those before it have claimed.
    pub fn synchronize(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
    ) {
        for member in 0..MAX_CROWS {
            let environment = self.around(member, environment);
            if let Some(crow) = &mut self.crows[member] {
                crow.synchronize(segments, event, environment);
            }
        }
    }

    /// `environment` as crow `member` finds it, with what the visit's other
    /// crows have claimed.
    fn around<'a>(&self, member: usize, environment: Environment<'a>) -> Environment<'a> {
        Environment {
            claimed: Claimed::by_others(&self.crows, member),
            ..environment
        }
    }

    /// The visit's part of the tick before the arena's step: its perches take
    /// the face as it is now, each holding the crows that stand on it, and
    /// each crow's body the velocity the crow chose.
    pub fn drive(
        &mut self,
        world: &mut PhysicsWorld,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
    ) {
        let feet = self
            .crows
            .each_ref()
            .map(|crow| crow.as_ref().map(|crow| crow.position));
        self.perches.place(
            world,
            &perches(self.layout, segments, event),
            &feet,
            self.layout,
        );
        for crow in self.crows() {
            crow.drive(world);
        }
    }

    /// The visit's tick after the arena's step: each crow's in turn, so that
    /// each knows what those before it have claimed. A crow that leaves
    /// before the others takes its body out of the arena. Returns true when
    /// the visit is over, as its last crows leave; their bodies go with the
    /// visit's claim on the arena.
    pub fn step(
        &mut self,
        segments: &[SegmentState],
        event: Option<ClockEventKind>,
        environment: Environment<'_>,
        world: &mut PhysicsWorld,
    ) -> bool {
        let mut left = [false; MAX_CROWS];
        for (member, left) in left.iter_mut().enumerate() {
            let environment = self.around(member, environment);
            if let Some(crow) = &mut self.crows[member] {
                *left = crow.step(segments, event, environment, world);
            }
        }
        if self
            .crows
            .iter()
            .zip(left)
            .all(|(crow, left)| crow.is_none() || left)
        {
            return true;
        }
        for (crow, left) in self.crows.iter_mut().zip(left) {
            if left && let Some(crow) = crow.take() {
                crow.exit(world);
            }
        }
        false
    }

    /// Each crow's diagnostics, by member.
    pub fn diagnostics(&self) -> Vec<ClockCrowState> {
        self.crows().map(Crow::diagnostics).collect()
    }
}

/// How many crows a visit whose seed is `seed` brings: one, as a rule, and
/// one time in four a flock of two, three or four, each as likely. The draw
/// has a stream of its own, so it changes nothing that the visit's crows or
/// the rest of the Clock draw.
pub fn visit_size(seed: u64) -> usize {
    let mut rng = StdRng::seed_from_u64(seed ^ FLOCK_SALT);
    if rng.random_bool(FLOCK_CHANCE) {
        rng.random_range(2..=MAX_CROWS)
    } else {
        1
    }
}

/// The seed of crow `member` of a visit whose seed is `seed`. The first
/// crow's is the visit's, so a visit of one draws as a crow alone always has.
/// Each other crow's is the visit's mixed with a salt of its own, "FLOCK!",
/// and the member, so it draws nothing from the first crow's streams.
fn member_seed(seed: u64, member: usize) -> u64 {
    if member == 0 {
        return seed;
    }
    // SplitMix64's finalizer.
    let mut z =
        (seed ^ 0x464c_4f43_4b21).wrapping_add((member as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
