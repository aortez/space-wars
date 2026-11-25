//! A spaceling the hills of a Tanks fight left behind, staying on the clock
//! for a while.
//!
//! A loser's spaceling that gets out may draw no beam: the winner goes up,
//! the hills withdraw, and it is left. From then it is a visitor with a body
//! in the run's arena, as the crow is, one at a time. Its body is the fight's
//! spaceling, at the fight's size and in the fight's fall, standing and
//! walking as the physics crate's spaceling does on whatever holds it up. Its
//! jetpack carries it from place to place, in front of the face and past
//! whatever is in the way, and a beam carries it out the same way.
//!
//! It stands on the lit digit blocks that rest at their places in the face,
//! as the face's surfaces say, on the topmost of a column as the crow
//! perches, and it walks the closed floor. It brings a perch for the block
//! it stands on or is going to. Landing on a digit with no flag standing, it
//! plants one, which stands until that block goes, whether or not the
//! spaceling is still here. While it rains it shelters on a block with
//! another lit block of its column over it, if the face has one, and keeps
//! off the floor's pools. It keeps clear of a crow. A block that goes from
//! under it, a floor that opens or takes a course, or a knock that leaves it
//! down sends it to another place on its jetpack. With nowhere to stand for
//! the moment, as while a duck's course lies over the floor and the digits'
//! reels turn, its jetpack holds it aloft where it is.
//!
//! It leaves in a beam, as a spaceling the rescue beam finds does: when an
//! event that leaves it no digit to stand on begins, when it has had nowhere
//! to stand for as long as its jetpack holds it up, or at the first change of
//! the time once it has stayed 20 minutes. A resize ends its stay at once.
use std::{f32::consts::PI, ops::RangeInclusive};

use engine_common::{
    ClockEventKind, ClockStrandedAccounting, ClockStrandedFlag, ClockStrandedSpaceling,
    ClockStrandedState,
};
pub(crate) use engine_common::{ClockStrandedDeparture as Departure, ClockStrandedPhase as Phase};
use engine_core::Vec2;
use engine_rapier::{
    spaceling::{
        SpacelingAssembly, SpacelingBalance, SpacelingControl, SpacelingGetUpResult,
        SpacelingSnapshot, SpacelingSpec,
    },
    world::{
        BodyId, BodyKind, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
        CollisionGroups, PhysicsId, PhysicsWorld, RayCastOptions,
    },
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    DIGIT_SLOT_COUNT, GridCell,
    arena::ids::canonical,
    digit_surfaces::{
        CELL_HALF, DigitSurface, Footprint, GRID_CELLS, SurfaceId, SurfaceSource, bit,
    },
    events::{
        FIXED_HZ,
        tanks::{BEAM_UP_TICKS, survivor, wreck::Puff},
    },
    layout::Layout,
};

#[cfg(test)]
mod tests;

/// The perches a stranded spaceling has in the arena: one, under the block
/// it stands on or is going to.
pub(crate) const PERCHES: usize = 1;
const SECOND: u64 = FIXED_HZ as u64;
const DT: f32 = 1.0 / FIXED_HZ as f32;
/// The longest it stays before the next change of the time sends its beam.
const STAY_TICKS: u64 = 20 * 60 * SECOND;
/// How long past that its beam waits for the time to change, as it never
/// does while the time is unknown.
const GRACE_TICKS: u64 = 90 * SECOND;
/// How fast its jetpack carries it, in the fight's units a second, and the
/// shortest and the longest a flight takes.
const FLIGHT_SPEED: f32 = 28.0;
const FLIGHT_TICKS: [u64; 2] = [SECOND / 2, 3 * SECOND];
/// How high a flight arcs over the straight line to where it lands, in the
/// fight's units: this, and more with the distance, up to the most.
const ARC: f32 = 1.5;
const ARC_PER_DISTANCE: f32 = 0.12;
const ARC_MOST: f32 = 8.0;
/// How long it takes to right itself as it takes off.
const RIGHTING_TICKS: u64 = SECOND / 5;
/// How far above where it lands its jetpack sets it down, in the fight's
/// units.
const LANDING_GAP: f32 = 0.02;
/// How often its jetpack leaves a puff of smoke behind it, and how long a
/// puff lasts.
const JET_TRAIL_TICKS: u64 = 3;
const PUFF_TICKS: u64 = SECOND;
/// How far above where it stood it hovers with nowhere to stand, and how far
/// and how often it bobs there, in the fight's units and seconds, and the
/// longest its jetpack holds it aloft with nowhere to go before its beam
/// comes.
const HOVER_RISE: f32 = 3.0;
const HOVER_BOB: f32 = 0.3;
const HOVER_BOB_SECONDS: f32 = 1.2;
const HOVER_TICKS: u64 = 10 * SECOND;
/// How long it stands on a digit block before it moves on, and on the floor
/// between its walks there.
const BLOCK_STAY: RangeInclusive<u64> = 6 * SECOND..=14 * SECOND;
const FLOOR_PAUSE: RangeInclusive<u64> = 3 * SECOND / 2..=4 * SECOND;
/// How long it stays put once the rain has stopped.
const AFTER_RAIN: RangeInclusive<u64> = 2 * SECOND..=4 * SECOND;
/// How often it looks the other way as it stands.
const LOOK: RangeInclusive<u64> = 3 * SECOND / 2..=7 * SECOND / 2;
/// The walks it takes on the floor before it flies on, and how far each
/// goes, in the fight's units.
const WALKS: RangeInclusive<u32> = 1..=3;
const WALK: RangeInclusive<f32> = 6.0..=24.0;
/// Its stroll, as a share of its walking pace.
const STROLL: f32 = 0.45;
/// How near where it is going a walk ends, and within how far of it the
/// walk slows, in the fight's units.
const REACH: f32 = 0.5;
const SLOWING: f32 = 3.0;
/// How long a walk may make no headway before it stops where it is, and the
/// headway that counts, in the fight's units.
const STUCK_TICKS: u64 = 3 * SECOND / 2;
const HEADWAY: f32 = 0.5;
/// How often it goes down to the floor from a digit, when the floor is
/// there to go to.
const FLOOR_SHARE: f64 = 0.4;
/// How far across from where it stands it comes down to the floor, in the
/// fight's units, and how far it keeps from the frame's sides.
const FLOOR_DRIFT: f32 = 25.0;
const FLOOR_MARGIN: f32 = 2.0;
/// How far above the floor it looks for the ground it will stand on, and how
/// far below, in the fight's units.
const GROUND_ABOVE: f32 = 6.0;
const GROUND_BELOW: f32 = 2.0;
/// The nearest digit blocks it picks its next from.
const NEAR_BLOCKS: usize = 8;
/// How long it takes to plant its flag.
const PLANT_TICKS: u64 = 4 * SECOND / 5;
/// Where it plants its flag from the middle of its block, toward the way it
/// faces, in pitches.
const FLAG_OFFSET: f32 = 0.22;
/// How far it keeps from a crow, and from where a crow is going, in pitches,
/// and how long it waits before it looks for a clearer place again.
const BIRD_CLEAR: f32 = 3.0;
const WARY_TICKS: u64 = SECOND;
/// How far from the middle of its block, and how far below its top, its feet
/// may be, in pitches, before it has been knocked off it.
const OFF_BLOCK: f32 = CELL_HALF;
const BELOW_BLOCK: f32 = 0.25;
/// How long its beam takes to come down on it, how long until it has lifted
/// it out, and how far it lifts it, in the fight's units, as the rescue beam
/// does.
const BEAM_DESCENT_TICKS: u64 = survivor::RESCUE_DESCENT_TICKS;
const BEAM_TICKS: u64 = BEAM_UP_TICKS;
const BEAM_RISE: f32 = 10.0;
/// How long a spaceling knocked down tries to get up before its jetpack
/// rights it, the least time between two of its requests to, and how long it
/// stands still before it has found its feet.
const GET_UP_TICKS: u64 = 2 * SECOND;
const PRESS_TICKS: u64 = SECOND / 2;
const STAND_TICKS: u32 = FIXED_HZ / 5;
/// A spaceling slower than this, in the fight's units a second, and turning
/// slower than this, in radians a second, is still.
const REST_SPEED: f32 = 0.5;
const REST_SPIN: f32 = 1.5;
/// How far below the floor or beyond the frame's sides its feet may go, in
/// the fight's units, before it has left the view.
const LOST: f32 = 8.0;
/// Friction of its perch, as the digits' other solids have.
const PERCH_FRICTION: f32 = 0.65;

/// Its body: the fight's spaceling, which meets everything in the arena.
fn spec(scale: f32) -> SpacelingSpec {
    SpacelingSpec {
        collision_groups: CollisionGroups::ALL,
        ..survivor::spec(scale)
    }
}

const fn body() -> BodyId {
    BodyId::new(PhysicsId::new(canonical::STRANDED.start), BodyRole::PRIMARY)
}

const fn perch() -> PhysicsId {
    PhysicsId::new(canonical::STRANDED_PERCHES.start)
}

/// An angle as the same turn within half a turn of upright.
fn wrapped(angle: f32) -> f32 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

/// A lit digit block at rest at its place in the face, with nothing lit
/// directly over it, so a spaceling can stand on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Block {
    /// The surface of the face it is.
    pub surface: SurfaceId,
    /// Its digit slot, grid column and grid row.
    pub key: [u8; 3],
    /// The middle of its top.
    pub feet: Vec2,
    /// Whether another lit block of its column stands over it, and how many
    /// cells lie free between them.
    pub covered: bool,
    pub headroom: u8,
}

/// The digit slot and the cell of its grid whose place in the face `at` is,
/// if it is one.
fn cell_at(layout: Layout, at: Vec2) -> Option<(usize, GridCell)> {
    (0..DIGIT_SLOT_COUNT).find_map(|slot| {
        let corner = layout.digit_origin(slot);
        let x = (at.x - corner.x) / layout.pitch - 0.5;
        let y = (at.y - corner.y) / layout.pitch - 0.5;
        let (column, row) = (x.round(), y.round());
        let on = (x - column).abs() < 1e-3
            && (y - row).abs() < 1e-3
            && (0.0..6.0).contains(&column)
            && (0.0..9.0).contains(&row);
        on.then_some((
            slot,
            GridCell {
                x: column as i8,
                y: row as i8,
            },
        ))
    })
}

/// The blocks of the face a spaceling can stand on, as its `surfaces` stand:
/// in each digit slot whose every lit cell rests whole at a place in the
/// face, each cell with none lit directly over it. A cell counts where it
/// is, which after a Cell Swap is not where it was lit. A slot an event is
/// moving, fading or has broken has none.
pub(crate) fn blocks(layout: Layout, surfaces: &[DigitSurface]) -> Vec<Block> {
    let mut resting = [[None; GRID_CELLS]; DIGIT_SLOT_COUNT];
    let mut moving = [false; DIGIT_SLOT_COUNT];
    for surface in surfaces {
        let place = cell_at(layout, surface.origin);
        let at_rest = surface.source == SurfaceSource::Anchored
            && !surface.departing
            && surface.angle == 0.0
            && surface.opacity >= 1.0
            && surface.footprint == Footprint::CELL;
        match (at_rest, place) {
            (true, Some((slot, cell))) => resting[slot][bit(cell)] = Some(surface.id),
            // On its way: the slot it was lit in is not at rest, nor is the
            // one it is passing through.
            (_, place) => {
                moving[usize::from(surface.slot)] = true;
                if let Some((slot, _)) = place {
                    moving[slot] = true;
                }
            }
        }
    }
    let mut blocks = Vec::new();
    for slot in (0..DIGIT_SLOT_COUNT).filter(|slot| !moving[*slot]) {
        for x in 0..6 {
            let lit = |y: i8| resting[slot][bit(GridCell { x, y })];
            for y in 0..9 {
                let Some(surface) = lit(y) else {
                    continue;
                };
                if y < 8 && lit(y + 1).is_some() {
                    continue;
                }
                let cover = (y + 2..9).find(|above| lit(*above).is_some());
                blocks.push(Block {
                    surface,
                    key: [slot as u8, x as u8, y as u8],
                    feet: layout.slot_cell_center(slot, GridCell { x, y })
                        + Vec2::new(0.0, CELL_HALF * layout.pitch),
                    covered: cover.is_some(),
                    headroom: cover.map_or(0, |above| (above - y - 1) as u8),
                });
            }
        }
    }
    blocks
}

/// Where a stranded spaceling stands, or is going.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    Block(Block),
    /// The floor, with its feet here.
    Floor(Vec2),
    /// The air, with its feet about here, while it has nowhere to stand.
    Air(Vec2),
}

impl Place {
    fn feet(self) -> Vec2 {
        match self {
            Self::Block(block) => block.feet,
            Self::Floor(feet) | Self::Air(feet) => feet,
        }
    }

    fn block(self) -> Option<Block> {
        match self {
            Self::Block(block) => Some(block),
            Self::Floor(_) | Self::Air(_) => None,
        }
    }
}

/// A jetpack flight, from where its feet were to where they will stand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Flight {
    from: Vec2,
    to: Vec2,
    /// How high it arcs over the straight line between them.
    arc: f32,
    /// The angle it took off at, which it rights itself from.
    angle: f32,
    ticks: u64,
}

impl Flight {
    fn new(from: Vec2, to: Vec2, angle: f32, scale: f32) -> Self {
        let distance = from.distance_to(to);
        let ticks = (distance / (FLIGHT_SPEED * scale) * FIXED_HZ as f32).ceil() as u64;
        Self {
            from,
            to,
            arc: (ARC * scale + ARC_PER_DISTANCE * distance).min(ARC_MOST * scale),
            angle: wrapped(angle),
            ticks: ticks.clamp(FLIGHT_TICKS[0], FLIGHT_TICKS[1]),
        }
    }

    /// Where its feet are `ticks` into the flight: up from where they were,
    /// over the arc, and down onto where they will stand.
    fn feet(&self, ticks: u64) -> Vec2 {
        let u = (ticks as f32 / self.ticks as f32).min(1.0);
        let ease = u * u * (3.0 - 2.0 * u);
        self.from + (self.to - self.from) * ease + Vec2::Y * (self.arc * (PI * u).sin())
    }

    /// Its angle `ticks` into the flight, as it rights itself.
    fn angle(&self, ticks: u64) -> f32 {
        self.angle * (1.0 - ticks as f32 / RIGHTING_TICKS as f32).max(0.0)
    }
}

/// The flag a stranded spaceling planted on a digit block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Flag {
    /// The side the spaceling that planted it fought on, and world units
    /// per unit of its fight.
    pub slot: usize,
    pub scale: f32,
    /// The block it stands on.
    pub surface: SurfaceId,
    pub key: [u8; 3],
    /// The foot of its stick, on the block's top.
    pub foot: Vec2,
    /// Which way its cloth flies: 1 to the right, -1 to the left.
    pub facing: f32,
    /// The run's tick it was planted on.
    pub planted: u64,
}

/// What a stranded spaceling goes by.
#[derive(Clone, Copy)]
pub(crate) struct Surroundings<'a> {
    /// The lit digit cells as the face last published them.
    pub surfaces: &'a [DigitSurface],
    /// The event under way.
    pub event: Option<ClockEventKind>,
    /// Whether the ordinary floor lies closed, with no visit's course in its
    /// place.
    pub floor: bool,
    /// Where a crow is, and where it is going.
    pub birds: [Option<Vec2>; 2],
    /// The digits the face shows.
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    /// The run's tick.
    pub tick: u64,
}

impl Surroundings<'_> {
    fn raining(&self) -> bool {
        self.event == Some(ClockEventKind::Rain)
    }
}

/// How a stranded spaceling's stay ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gone {
    /// Its beam lifted it out.
    Left(Departure),
    /// It left the view.
    Lost,
    /// A resize ended its stay.
    Cleared,
}

/// A spaceling as its fight leaves it behind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Strand {
    /// The side its tank fought on.
    pub slot: usize,
    /// Its body as it stood.
    pub motion: BodyMotion,
    /// Which way it faced: 1 to the right, -1 to the left.
    pub facing: f32,
    /// World units per unit of the fight.
    pub scale: f32,
    /// How fast the fight's bodies fall, in world units a second squared.
    pub fall: f32,
    /// The fight's seed.
    pub seed: u64,
}

/// A spaceling the hills left behind, from when they begin to withdraw
/// until its beam has lifted it out.
pub(crate) struct Stranded {
    layout: Layout,
    /// World units per unit of the fight that left it behind.
    pub scale: f32,
    /// How fast it falls, in world units a second squared.
    fall: f32,
    /// The side its tank fought on.
    pub slot: usize,
    fight_id: u64,
    /// Whether the fight that left it behind is still under way.
    home: bool,
    /// Its own stream, from the fight's seed.
    rng: StdRng,
    /// The ticks it has stayed.
    pub age: u64,
    pub phase: Phase,
    /// The age it began its phase at.
    pub since: u64,
    /// Its body, which stands and walks as the physics crate's spaceling
    /// does, and which its jetpack and its beam carry past everything.
    spaceling: SpacelingAssembly,
    /// The world's pull its body's fall was last scaled against.
    pull: Option<f32>,
    /// What its body is told at the next step.
    control: SpacelingControl,
    /// Where its feet are, and its angle from upright.
    pub feet: Vec2,
    pub angle: f32,
    /// Which way it faces: 1 to the right, -1 to the left.
    pub facing: f32,
    /// Where it stands, or is going.
    place: Place,
    flight: Option<Flight>,
    /// The block its perch in the arena is under.
    perch: Option<Block>,
    /// The age its stay where it stands ends at.
    until: u64,
    /// The age it next looks the other way at.
    look: u64,
    /// The walks it has left on the floor before it flies on.
    walks: u32,
    /// The age it last made headway on a walk at, and where across it was.
    headway: (u64, f32),
    /// The age it next looks for a place clear of a crow at.
    wary: u64,
    /// The age it last had somewhere to stand at, or to go.
    placed: u64,
    /// The age of its last request to get up.
    pressed: Option<u64>,
    /// The ticks it has stood still for.
    still: u32,
    /// The digits the face last showed it.
    digits: [Option<u8>; DIGIT_SLOT_COUNT],
    /// Why its beam came, and where its feet were when it did.
    leaving: Option<(Departure, Vec2)>,
    /// The smoke its jetpack left behind it, by the age it was left at.
    pub puffs: Vec<Puff>,
    flights: u32,
    flags: u32,
    knockdowns: u32,
}

impl Stranded {
    /// Takes the place in `world` of the spaceling a fight's hills leave
    /// behind as they begin to withdraw. Its jetpack lifts it off them and
    /// onto the nearest digit block it can stand on top of. With none, it
    /// hovers over them.
    pub fn new(
        world: &mut PhysicsWorld,
        layout: Layout,
        strand: Strand,
        fight_id: u64,
        around: Surroundings<'_>,
    ) -> Self {
        let spec = spec(strand.scale);
        let BodyMotion {
            position, angle, ..
        } = strand.motion;
        let feet = position - Vec2::Y.rotate_radians(angle) * spec.half_height();
        let spaceling = SpacelingAssembly::insert(world, body().entity, position, angle, spec)
            .expect("the room kept for a stranded spaceling");
        let mut stranded = Self {
            layout,
            scale: strand.scale,
            fall: strand.fall,
            slot: strand.slot,
            fight_id,
            home: true,
            rng: StdRng::seed_from_u64(strand.seed ^ 0x5354_5241_4e44_4544),
            age: 0,
            phase: Phase::Standing,
            since: 0,
            spaceling,
            pull: None,
            control: SpacelingControl::default(),
            feet,
            angle,
            facing: strand.facing,
            place: Place::Floor(feet),
            flight: None,
            perch: None,
            until: 0,
            look: 0,
            walks: 0,
            headway: (0, feet.x),
            wary: 0,
            placed: 0,
            pressed: None,
            still: 0,
            digits: around.digits,
            leaving: None,
            puffs: Vec::new(),
            flights: 0,
            flags: 0,
            knockdowns: 0,
        };
        let nearest = blocks(layout, around.surfaces)
            .into_iter()
            .filter(|block| !block.covered)
            .min_by(|a, b| {
                a.feet
                    .distance_to(feet)
                    .total_cmp(&b.feet.distance_to(feet))
            });
        match nearest {
            Some(block) => stranded.fly(world, Place::Block(block)),
            None => stranded.hover(world),
        }
        stranded
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.since = self.age;
        self.still = 0;
        self.control = SpacelingControl::default();
    }

    /// Its body, where it is drawn: its middle, and its angle.
    pub fn motion(&self) -> BodyMotion {
        BodyMotion {
            position: self.middle(self.feet, self.angle),
            angle: self.angle,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        }
    }

    fn middle(&self, feet: Vec2, angle: f32) -> Vec2 {
        feet + Vec2::Y.rotate_radians(angle) * spec(self.scale).half_height()
    }

    /// How far its beam has lifted it `ticks` into it, from not at all, while
    /// the beam comes down on it, to out.
    fn lifted(ticks: u64) -> f32 {
        (ticks.saturating_sub(BEAM_DESCENT_TICKS) as f32 / (BEAM_TICKS - BEAM_DESCENT_TICKS) as f32)
            .min(1.0)
    }

    /// How far it has faded from view in its beam, from not at all to gone.
    pub fn fade(&self) -> f32 {
        match self.leaving {
            Some(_) => Self::lifted(self.age - self.since),
            None => 0.0,
        }
    }

    /// Its beam: where its feet were when it came, and how far down from the
    /// top of the view it has come, from not at all to all the way.
    pub fn beam(&self) -> Option<(Vec2, f32)> {
        let (_, from) = self.leaving?;
        let descent = (self.age - self.since) as f32 / BEAM_DESCENT_TICKS as f32;
        Some((from, descent.min(1.0)))
    }

    /// Where the foot of the flag it plants on `block` stands.
    fn flag_foot(&self, block: Block) -> Vec2 {
        block.feet + Vec2::X * (self.facing * FLAG_OFFSET * self.layout.pitch)
    }

    /// The flag it is planting, while it is: where its foot stands, and how
    /// far up it is, from down to planted.
    pub fn planting(&self) -> Option<(Vec2, f32)> {
        match (self.phase, self.place) {
            (Phase::Planting, Place::Block(block)) => Some((
                self.flag_foot(block),
                ((self.age - self.since) as f32 / PLANT_TICKS as f32).min(1.0),
            )),
            _ => None,
        }
    }

    /// Its jetpack takes it to `to`, past everything on the way.
    fn fly(&mut self, world: &mut PhysicsWorld, to: Place) {
        self.ghost(world);
        let across = to.feet().x - self.feet.x;
        if across.abs() > f32::EPSILON {
            self.facing = across.signum();
        }
        self.flight = Some(Flight::new(self.feet, to.feet(), self.angle, self.scale));
        self.place = to;
        self.flights += 1;
        self.enter(Phase::Flying);
    }

    /// With nowhere to stand, its jetpack lifts it a little above where it
    /// is and holds it there. Aloft already, it stays so, until it has been
    /// without a place for as long as its jetpack holds it up, when its beam
    /// comes.
    fn hover(&mut self, world: &mut PhysicsWorld) {
        if self.age - self.placed >= HOVER_TICKS {
            self.leave(world, Departure::NoPlace);
        } else if !matches!(self.place, Place::Air(_)) {
            let aloft = self.feet + Vec2::Y * (HOVER_RISE * self.scale);
            self.fly(world, Place::Air(aloft));
        }
    }

    /// Where its feet are `ticks` into hovering about `at`, bobbing.
    fn bobbing(&self, at: Vec2, ticks: u64) -> Vec2 {
        let turn = 2.0 * PI * ticks as f32 * DT / HOVER_BOB_SECONDS;
        at + Vec2::Y * (HOVER_BOB * self.scale * turn.sin())
    }

    /// Its beam comes for it, `why`.
    fn leave(&mut self, world: &mut PhysicsWorld, why: Departure) {
        self.ghost(world);
        self.flight = None;
        self.leaving = Some((why, self.feet));
        self.enter(Phase::BeamedUp);
    }

    /// Its body goes where it is taken, and meets nothing on the way.
    fn ghost(&mut self, world: &mut PhysicsWorld) {
        let body = self.spaceling.body();
        assert!(
            world.set_body_kind(body, BodyKind::KinematicPosition, true)
                && world.set_collision_groups(self.spaceling.collider(), CollisionGroups::NONE),
            "a stranded spaceling's body"
        );
    }

    /// Its jetpack sets it down where it was going: its body is the
    /// spaceling's again, on its feet there. In the air, with nowhere to
    /// stand, the jetpack holds it there instead.
    fn land(&mut self, world: &mut PhysicsWorld, raining: bool, flagged: bool) {
        self.feet = self.place.feet();
        self.angle = 0.0;
        self.flight = None;
        if !matches!(self.place, Place::Air(_)) {
            let spec = spec(self.scale);
            let entity = self.spaceling.body().entity;
            assert!(world.remove_entity(entity), "a stranded spaceling's body");
            let middle = self.feet + Vec2::Y * (spec.half_height() + LANDING_GAP * self.scale);
            self.spaceling = SpacelingAssembly::insert(world, entity, middle, 0.0, spec)
                .expect("the room kept for a stranded spaceling");
            self.pull = None;
        }
        self.settle(raining, flagged);
    }

    /// What it does where it has come to stand: out of the rain there, it
    /// shelters. On a digit with no flag standing, it plants one. Otherwise
    /// it stands a while, and on the floor it has some walks ahead of it. In
    /// the air it hovers.
    fn settle(&mut self, raining: bool, flagged: bool) {
        self.look = self.age + self.rng.random_range(LOOK);
        match self.place {
            Place::Block(block) if raining && block.covered => self.enter(Phase::Sheltering),
            Place::Block(block) if !raining && !flagged && !block.covered => {
                self.enter(Phase::Planting);
            }
            Place::Block(_) => {
                self.until = self.age + self.rng.random_range(BLOCK_STAY);
                self.enter(Phase::Standing);
            }
            Place::Floor(_) => {
                self.walks = self.rng.random_range(WALKS);
                self.until = self.age + self.rng.random_range(FLOOR_PAUSE);
                self.enter(Phase::Standing);
            }
            Place::Air(_) => self.enter(Phase::Hovering),
        }
    }

    /// The ground under `x` for its feet, where the floor is: none where
    /// nothing holds it up there.
    fn ground(&self, world: &PhysicsWorld, x: f32) -> Option<Vec2> {
        let above = GROUND_ABOVE * self.scale;
        let hit = world.cast_ray(
            Vec2::new(x, self.layout.floor_y + above),
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                max_distance: above + GROUND_BELOW * self.scale,
                exclude_entity: Some(self.spaceling.body().entity),
                ..Default::default()
            },
        )?;
        (hit.distance > 1e-3 * self.scale).then_some(hit.point)
    }

    /// How far across the floor it may stand: inside the frame's sides.
    fn floor_span(&self) -> [f32; 2] {
        let inset = self.layout.frame_width + FLOOR_MARGIN * self.scale;
        [
            self.layout.bounds_min.x + inset,
            self.layout.bounds_max.x - inset,
        ]
    }

    /// Where it next goes: under cover while it rains, if the face has a
    /// block under another, the closest under its cover first, and of those
    /// the nearest. Otherwise one of the nearest blocks it can stand on top
    /// of, other than its own, or the floor some of the time, where it lies
    /// closed and dry. It keeps clear of the `birds`. With nowhere new, it
    /// stays where it stands if that still holds, and with nowhere at all,
    /// there is none: not the air it hovers in.
    fn choose(
        &mut self,
        world: &PhysicsWorld,
        blocks: &[Block],
        around: Surroundings<'_>,
    ) -> Option<Place> {
        let reach = BIRD_CLEAR * self.layout.pitch;
        let clear = |at: Vec2| {
            around
                .birds
                .iter()
                .flatten()
                .all(|bird| bird.distance_to(at) > reach)
        };
        let from = self.feet;
        let nearer = |a: &Block, b: &Block| {
            a.feet
                .distance_to(from)
                .total_cmp(&b.feet.distance_to(from))
        };
        let own = self.place.block().map(|block| block.surface);
        if around.raining()
            && let Some(cover) = blocks
                .iter()
                .filter(|block| block.covered && clear(block.feet))
                .min_by(|a, b| a.headroom.cmp(&b.headroom).then(nearer(a, b)))
        {
            return Some(Place::Block(*cover));
        }
        let mut tops: Vec<Block> = blocks
            .iter()
            .copied()
            .filter(|block| !block.covered && Some(block.surface) != own && clear(block.feet))
            .collect();
        tops.sort_by(nearer);
        tops.truncate(NEAR_BLOCKS);
        let on_floor = matches!(self.place, Place::Floor(_));
        let down = if around.floor && !around.raining() && !on_floor {
            let [left, right] = self.floor_span();
            let drift = self.rng.random_range(-FLOOR_DRIFT..=FLOOR_DRIFT) * self.scale;
            self.ground(world, (from.x + drift).clamp(left, right))
                .filter(|spot| clear(*spot))
        } else {
            None
        };
        if let Some(spot) = down
            && (tops.is_empty() || self.rng.random_bool(FLOOR_SHARE))
        {
            return Some(Place::Floor(spot));
        }
        if !tops.is_empty() {
            return Some(Place::Block(tops[self.rng.random_range(0..tops.len())]));
        }
        let held = match self.place {
            Place::Block(own) => blocks.iter().any(|block| block.surface == own.surface),
            Place::Floor(_) => around.floor,
            Place::Air(_) => false,
        };
        held.then_some(self.place)
    }

    /// Flies to the place it next chooses, stays where it stands if that is
    /// the place, or hovers with nowhere to go.
    fn move_on(&mut self, world: &mut PhysicsWorld, blocks: &[Block], around: Surroundings<'_>) {
        match self.choose(world, blocks, around) {
            Some(place) if place == self.place => {
                self.until = self.age + self.rng.random_range(BLOCK_STAY);
            }
            Some(place) => self.fly(world, place),
            None => self.hover(world),
        }
    }

    /// Whether the event under way leaves it no digit to stand on: one that
    /// takes the digits apart or away, floods them, or the next fight.
    fn evicted(&self, event: Option<ClockEventKind>) -> bool {
        match event {
            Some(ClockEventKind::Tanks) => !self.home,
            Some(kind) => matches!(
                kind,
                ClockEventKind::Falling
                    | ClockEventKind::Meltdown
                    | ClockEventKind::Explosion
                    | ClockEventKind::Coins
                    | ClockEventKind::Marquee
                    | ClockEventKind::Aquarium
            ),
            None => false,
        }
    }

    /// Whether something has knocked it off the block it stands on.
    fn off_block(&self) -> bool {
        let pitch = self.layout.pitch;
        match (self.phase, self.place) {
            (Phase::Flying | Phase::BeamedUp | Phase::GettingUp, _)
            | (_, Place::Floor(_) | Place::Air(_)) => false,
            (_, Place::Block(block)) => {
                (self.feet.x - block.feet.x).abs() > OFF_BLOCK * pitch
                    || self.feet.y < block.feet.y - BELOW_BLOCK * pitch
            }
        }
    }

    /// Goes by what is around it now, whether or not time has moved: its
    /// beam comes when an event that leaves it no digit begins, and once it
    /// has stayed as long as it may and the time changes. It takes off for
    /// another place when the one it stands on or is going to no longer
    /// holds, when a crow comes too near it, when it rains with cover to be
    /// had, and when it has been knocked off its block, back onto which it
    /// flies. With no place to go to it hovers, and from the air it takes
    /// the first place there is.
    fn react(&mut self, world: &mut PhysicsWorld, around: Surroundings<'_>) {
        if self.leaving.is_some() {
            return;
        }
        if around.event != Some(ClockEventKind::Tanks) {
            self.home = false;
        }
        let changed = std::mem::replace(&mut self.digits, around.digits) != around.digits;
        if self.evicted(around.event) {
            return self.leave(world, Departure::Event);
        }
        if (self.age >= STAY_TICKS && changed) || self.age >= STAY_TICKS + GRACE_TICKS {
            return self.leave(world, Departure::TimeUp);
        }
        let blocks = blocks(self.layout, around.surfaces);
        let held = match self.place {
            Place::Block(own) => blocks.iter().any(|block| block.surface == own.surface),
            Place::Floor(_) => around.floor,
            Place::Air(_) => false,
        };
        if held {
            self.placed = self.age;
        }
        // A crow too near where it stands, or where it is going.
        let reach = BIRD_CLEAR * self.layout.pitch;
        let near = |at: Vec2| {
            around
                .birds
                .iter()
                .flatten()
                .any(|bird| bird.distance_to(at) <= reach)
        };
        let crowded = self.age >= self.wary && (near(self.place.feet()) || near(self.feet));
        // In the rain with no block over it, while one it could shelter on
        // is there.
        let covered = self.place.block().is_some_and(|block| block.covered);
        let exposed = around.raining()
            && !covered
            && blocks
                .iter()
                .any(|block| block.covered && !near(block.feet));
        if held && !crowded && !exposed {
            if self.off_block() {
                self.fly(world, self.place);
            }
            return;
        }
        self.wary = self.age + WARY_TICKS;
        match self.choose(world, &blocks, around) {
            Some(place) if place == self.place => {
                if self.off_block() {
                    self.fly(world, place);
                }
            }
            Some(place) => {
                self.placed = self.age;
                self.fly(world, place);
            }
            None => self.hover(world),
        }
    }

    /// Its part of the tick before the arena's step: its perch goes under
    /// the block it stands on or is going to, and its body takes what its
    /// jetpack, its beam or its own legs do next.
    pub fn drive(&mut self, world: &mut PhysicsWorld) {
        self.bring_perch(world);
        let body = self.spaceling.body();
        let ticks = self.age + 1 - self.since;
        match (self.phase, self.flight, self.leaving) {
            (Phase::Flying, Some(flight), _) => {
                let angle = flight.angle(ticks);
                let middle = self.middle(flight.feet(ticks), angle);
                world.set_next_kinematic_pose(body, middle, angle);
            }
            (Phase::BeamedUp, _, Some((_, from))) => {
                let feet = from + Vec2::Y * (BEAM_RISE * self.scale * Self::lifted(ticks));
                world.set_next_kinematic_pose(body, self.middle(feet, self.angle), self.angle);
            }
            (Phase::Hovering, ..) => {
                let feet = self.bobbing(self.place.feet(), ticks);
                world.set_next_kinematic_pose(body, self.middle(feet, 0.0), 0.0);
            }
            _ => {
                // It keeps its fight's fall, whoever set the world's.
                let pull = -world.gravity().y;
                if self.pull != Some(pull) {
                    assert!(world.set_gravity_scale(body, self.fall / pull, true));
                    self.pull = Some(pull);
                }
                self.spaceling
                    .apply_control(world, self.control, Vec2::new(0.0, -self.fall), DT);
            }
        }
    }

    /// Its perch: a fixed square where the block it stands on or is going to
    /// is drawn, solid for whatever meets it but the birds, which have
    /// perches of their own.
    fn bring_perch(&mut self, world: &mut PhysicsWorld) {
        let wanted = match self.leaving {
            Some(_) => None,
            None => self.place.block(),
        };
        if self.perch == wanted {
            return;
        }
        if self.perch.take().is_some() {
            assert!(world.remove_entity(perch()), "a placed perch");
        }
        if let Some(block) = wanted {
            let half = CELL_HALF * self.layout.pitch;
            let mut cell = ColliderSpec::cuboid(
                ColliderId::new(perch(), ColliderRole::PRIMARY, 0),
                half,
                half,
            );
            cell.friction = PERCH_FRICTION;
            cell.restitution = 0.0;
            cell.collision_groups = crate::arena::FRAME;
            assert!(world.insert_body(
                BodyId::new(perch(), BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: block.feet - Vec2::new(0.0, half),
                    ..BodySpec::default()
                },
                &[cell],
            ));
            self.perch = Some(block);
        }
    }

    /// Its tick after the arena's step. It reads its body back, goes by what
    /// is around it, and carries on with what it was doing. `flag` is the
    /// flag that stands, which it plants when none does. Answers how its
    /// stay ended, once it has.
    pub fn step(
        &mut self,
        world: &mut PhysicsWorld,
        around: Surroundings<'_>,
        flag: &mut Option<Flag>,
    ) -> Option<Gone> {
        self.age += 1;
        let age = self.age;
        self.puffs.retain(|puff| age - puff.tick < PUFF_TICKS);
        let ticks = self.age - self.since;
        match (self.phase, self.flight, self.leaving) {
            (Phase::BeamedUp, _, Some((why, from))) => {
                self.feet = from + Vec2::Y * (BEAM_RISE * self.scale * Self::lifted(ticks));
                return (ticks >= BEAM_TICKS).then_some(Gone::Left(why));
            }
            (Phase::Flying, Some(flight), _) => {
                self.feet = flight.feet(ticks);
                self.angle = flight.angle(ticks);
                if ticks.is_multiple_of(JET_TRAIL_TICKS) {
                    self.smoke();
                }
                if ticks >= flight.ticks {
                    self.land(world, around.raining(), flag.is_some());
                }
            }
            (Phase::Hovering, ..) => {
                self.feet = self.bobbing(self.place.feet(), ticks);
                self.angle = 0.0;
                if ticks.is_multiple_of(JET_TRAIL_TICKS) {
                    self.smoke();
                }
            }
            _ => {
                let snapshot = self
                    .spaceling
                    .snapshot(world)
                    .expect("a stranded spaceling's body");
                self.angle = snapshot.motion.angle;
                self.feet = snapshot.motion.position
                    - Vec2::Y.rotate_radians(self.angle) * spec(self.scale).half_height();
                let beyond = LOST * self.scale;
                if self.feet.y < self.layout.floor_y - beyond
                    || self.feet.x.abs() > self.layout.bounds_max.x + beyond
                {
                    return Some(Gone::Lost);
                }
                self.react(world, around);
                if !matches!(
                    self.phase,
                    Phase::Flying | Phase::Hovering | Phase::BeamedUp
                ) {
                    self.act(world, around, snapshot, flag);
                }
                return None;
            }
        }
        self.react(world, around);
        None
    }

    /// Its jetpack leaves a puff of smoke behind it.
    fn smoke(&mut self) {
        let nozzle = Vec2::new(-self.facing * 0.33, -0.9) * (survivor::PILOT_SCALE * self.scale);
        self.puffs.push(Puff {
            point: self.middle(self.feet, self.angle) + nozzle.rotate_radians(self.angle),
            tick: self.age,
        });
    }

    /// Goes by what is around it between ticks, as when the time or the
    /// format changes while the clock is paused, without moving or aging.
    pub fn synchronize(&mut self, world: &mut PhysicsWorld, around: Surroundings<'_>) {
        self.react(world, around);
    }

    /// Carries on with what it was doing on its feet, or gets back onto
    /// them, and settles what its body is told at the next step.
    fn act(
        &mut self,
        world: &mut PhysicsWorld,
        around: Surroundings<'_>,
        snapshot: SpacelingSnapshot,
        flag: &mut Option<Flag>,
    ) {
        let scale = self.scale;
        let ticks = self.age - self.since;
        let raining = around.raining();
        let standing = snapshot.balance == SpacelingBalance::Balanced
            && snapshot.grounded()
            && !snapshot.needs_get_up()
            && snapshot.relative_speed.abs() < REST_SPEED * scale
            && snapshot.motion.angular_velocity.abs() < REST_SPIN;
        self.still = if standing { self.still + 1 } else { 0 };
        self.control = SpacelingControl::default();
        if self.phase != Phase::GettingUp && snapshot.needs_get_up() {
            self.knockdowns += 1;
            self.pressed = None;
            return self.enter(Phase::GettingUp);
        }
        match self.phase {
            Phase::GettingUp => {
                let walker = spec(scale);
                if self.still >= STAND_TICKS {
                    // On its feet again where it fell: its place, if it is
                    // the floor, and otherwise it flies back to it.
                    if let Place::Floor(_) = self.place {
                        self.place = Place::Floor(self.feet);
                    }
                    self.until = self.age + self.rng.random_range(FLOOR_PAUSE);
                    self.enter(Phase::Standing);
                } else if ticks >= GET_UP_TICKS
                    || snapshot.get_up_result == SpacelingGetUpResult::Blocked
                {
                    // Its jetpack rights it, and takes it back to where it
                    // stood.
                    self.fly(world, self.place);
                } else {
                    let press = snapshot.needs_get_up()
                        && snapshot.contacts > 0
                        && snapshot.motion.linear_velocity.length() < walker.balance.settle_speed
                        && snapshot.motion.angular_velocity.abs()
                            < walker.balance.settle_angular_speed
                        && self
                            .pressed
                            .is_none_or(|pressed| self.age - pressed >= PRESS_TICKS);
                    if press {
                        self.pressed = Some(self.age);
                    }
                    self.control.jump_held = press;
                }
            }
            Phase::Planting => {
                if let (true, Place::Block(block)) = (ticks >= PLANT_TICKS, self.place) {
                    *flag = Some(Flag {
                        slot: self.slot,
                        scale,
                        surface: block.surface,
                        key: block.key,
                        foot: self.flag_foot(block),
                        facing: self.facing,
                        planted: around.tick,
                    });
                    self.flags += 1;
                    self.until = self.age + self.rng.random_range(BLOCK_STAY);
                    self.enter(Phase::Standing);
                }
            }
            Phase::Sheltering => {
                if !raining {
                    self.until = self.age + self.rng.random_range(AFTER_RAIN);
                    self.enter(Phase::Standing);
                }
            }
            Phase::Walking => {
                let to = self.place.feet();
                let gap = to.x - self.feet.x;
                if (self.feet.x - self.headway.1).abs() >= HEADWAY * scale {
                    self.headway = (self.age, self.feet.x);
                }
                let stuck = self.age - self.headway.0 >= STUCK_TICKS;
                if (gap.abs() <= REACH * scale && snapshot.grounded()) || stuck {
                    self.place = Place::Floor(self.feet);
                    self.until = self.age + self.rng.random_range(FLOOR_PAUSE);
                    self.look = self.age + self.rng.random_range(LOOK);
                    self.enter(Phase::Standing);
                } else {
                    self.facing = gap.signum();
                    let pace = (gap.abs() / (SLOWING * scale)).clamp(0.5, 1.0);
                    self.control.walk = self.facing * STROLL * pace;
                }
            }
            Phase::Standing => {
                if self.age >= self.look {
                    self.facing = -self.facing;
                    self.look = self.age + self.rng.random_range(LOOK);
                }
                // It waits out the rain where it is.
                if self.age < self.until || raining {
                    return;
                }
                match self.place {
                    Place::Floor(_) if self.walks > 0 && around.floor => {
                        self.walks -= 1;
                        let [left, right] = self.floor_span();
                        let way = if self.rng.random_bool(0.5) { 1.0 } else { -1.0 };
                        let length = self.rng.random_range(WALK) * scale;
                        let x = (self.feet.x + way * length).clamp(left, right);
                        self.place = Place::Floor(Vec2::new(x, self.feet.y));
                        self.headway = (self.age, self.feet.x);
                        self.enter(Phase::Walking);
                    }
                    _ => {
                        let blocks = blocks(self.layout, around.surfaces);
                        self.move_on(world, &blocks, around);
                    }
                }
            }
            Phase::Flying | Phase::Hovering | Phase::BeamedUp => {}
        }
    }

    /// How long it has walked for, in seconds, while it walks.
    pub fn stride(&self) -> Option<f32> {
        (self.phase == Phase::Walking).then(|| (self.age - self.since) as f32 * DT)
    }

    /// How long its jetpack has burned for, in seconds, while it flies or
    /// hovers.
    pub fn jet(&self) -> Option<f32> {
        matches!(self.phase, Phase::Flying | Phase::Hovering)
            .then(|| (self.age - self.since) as f32 * DT)
    }

    pub fn diagnostics(&self) -> ClockStrandedSpaceling {
        ClockStrandedSpaceling {
            fight_id: self.fight_id,
            slot: self.slot as u8,
            phase: self.phase,
            phase_tick: self.age - self.since,
            age_ticks: self.age,
            position_milli: [self.feet.x, self.feet.y].map(|v| (v * 1000.0).round() as i32),
            facing_right: self.facing > 0.0,
            perch: self.place.block().map(|block| block.key),
            flags: self.flags,
            flights: self.flights,
            knockdowns: self.knockdowns,
            leaving: self.leaving.map(|(why, _)| why),
        }
    }
}

/// The run's stranded spaceling, if a fight's hills have left one behind,
/// the flag one planted, and what became of those the run has had.
#[derive(Default)]
pub(crate) struct Scene {
    pub spaceling: Option<Stranded>,
    pub flag: Option<Flag>,
    pub accounting: ClockStrandedAccounting,
}

impl Scene {
    /// A fight's hills leave `strand` behind in `world`, which the caller
    /// has claimed for it.
    pub fn admit(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        strand: Strand,
        fight_id: u64,
        around: Surroundings<'_>,
    ) {
        assert!(self.spaceling.is_none(), "one stranded spaceling at a time");
        self.spaceling = Some(Stranded::new(world, layout, strand, fight_id, around));
        self.accounting.stranded += 1;
    }

    /// The flag stands until its block goes: until its digit changes, or an
    /// event moves it or takes it apart.
    pub fn tend_flag(&mut self, layout: Layout, surfaces: &[DigitSurface]) {
        if let Some(flag) = self.flag
            && !blocks(layout, surfaces)
                .iter()
                .any(|block| block.surface == flag.surface)
        {
            self.flag = None;
        }
    }

    /// The spaceling's tick after the arena's step in `world`, which answers
    /// whether its stay has ended, so that the caller lets its claim go.
    pub fn step(&mut self, world: &mut PhysicsWorld, around: Surroundings<'_>) -> bool {
        let Some(spaceling) = &mut self.spaceling else {
            return false;
        };
        let Some(gone) = spaceling.step(world, around, &mut self.flag) else {
            return false;
        };
        self.end(gone);
        true
    }

    /// The spaceling's stay is over, as `gone` says. Its flag stays.
    pub fn end(&mut self, gone: Gone) {
        if self.spaceling.take().is_none() {
            return;
        }
        let count = match gone {
            Gone::Left(Departure::TimeUp) => &mut self.accounting.time_up,
            Gone::Left(Departure::Event) => &mut self.accounting.event,
            Gone::Left(Departure::NoPlace) => &mut self.accounting.no_place,
            Gone::Lost => &mut self.accounting.lost,
            Gone::Cleared => &mut self.accounting.cleared,
        };
        *count += 1;
    }

    /// The scene as Clock's state publishes it, at the run's `tick`: none
    /// until a fight has left a spaceling behind.
    pub fn state(&self, tick: u64) -> Option<ClockStrandedState> {
        (self.accounting.stranded > 0).then(|| ClockStrandedState {
            spaceling: self.spaceling.as_ref().map(Stranded::diagnostics),
            flag: self.flag.map(|flag| ClockStrandedFlag {
                slot: flag.slot as u8,
                block: flag.key,
                age_ticks: tick.saturating_sub(flag.planted),
            }),
            accounting: self.accounting,
        })
    }
}
