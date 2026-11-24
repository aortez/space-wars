//! A loser's spaceling getting out as its tank blows up, and how it leaves.
//!
//! One loser in five gets out, by a roll the fight's seed gives each side
//! from a stream of its own. Half of those the blast throws clear, tumbling,
//! and they land, settle and get up as the physics crate's spaceling does.
//! The rest ride out in an escape pod that arcs away like a shell, lands and
//! may tip. A moment after it comes to rest, it opens and the spaceling
//! climbs out.
//!
//! Each spaceling that gets out draws its ending evenly from those that fit
//! the fight, by a roll from another stream of its own. The rescue beam
//! always fits: once up, the spaceling looks around and waves, then a beam
//! comes down on it and lifts it out as the winner beams up. With a winner
//! in view, it may stow away instead: it runs to the winner, jumping where
//! it must, climbs onto the back of its roof and rides it out. Or it
//! surrenders: it raises a white flag, the winner's turret turns onto it,
//! and a beam in the winner's colors comes down on it and draws it to the
//! winner's side, to go up with the winner. A stowaway with no way to the
//! winner, or no time left to reach it, waves for the rescue beam instead,
//! as does one still surrendering when its time runs out. The empty pods
//! stay until the hills withdraw, and go with them.
//!
//! The spacelings and the pods meet the hills, the dirt, the floor and the
//! winner, and pass the pieces of the wreck, the face and one another. One
//! that leaves the view retires there. A spaceling still down when its time
//! runs out is lifted out as it lies.
use super::*;
use engine_common::ClockTanksSurvivor;
use engine_rapier::{
    spaceling::{
        SpacelingAssembly, SpacelingBalance, SpacelingBalanceSpec, SpacelingControl,
        SpacelingGetUpResult, SpacelingSpec,
    },
    world::{
        BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole, ColliderSpec,
        RayCastOptions,
    },
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use scenario_scorched_earth::host::TankState;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use wreck::Puff;

pub use engine_common::ClockTanksEjection as Ejection;
pub use engine_common::ClockTanksEnding as Ending;
pub use engine_common::ClockTanksFallback as Fallback;
pub use engine_common::ClockTanksSurvivorPhase as Phase;

/// The bodies the survivors take in the arena: each side's spaceling and its
/// pod.
pub const MAX_SURVIVOR_BODIES: usize = 4;
/// The longest the aftermath waits for a spaceling that leaves from where it
/// stands, from the end of the blow-up until the beam-up.
pub const RESCUE_BY_TICKS: u64 = 6 * FIXED_HZ as u64;
/// The longest the aftermath lasts: as long as a stowaway may take to reach
/// the winner and climb on.
pub const AFTERMATH_TICKS: u64 = 8 * FIXED_HZ as u64;
/// Clock's spaceling beside the physics crate's 1.8-unit one: about 1 of the
/// duel's units, a third of a tank's height from the bottom of its wheels to
/// the top of its turret.
pub const PILOT_SCALE: f32 = 0.55;
/// The pod's outline in the duel's units, around its body's origin: a flat
/// base and a domed top. The dome above the hinge's height is the hatch.
pub const POD: [Vec2; 7] = [
    Vec2::new(-0.48, -0.45),
    Vec2::new(0.48, -0.45),
    Vec2::new(0.48, 0.05),
    Vec2::new(0.34, 0.37),
    Vec2::new(0.0, 0.5),
    Vec2::new(-0.34, 0.37),
    Vec2::new(-0.48, 0.05),
];
/// The area inside [`POD`], in the duel's units.
const POD_AREA: f32 = 0.79;
/// The hatch's hinge, on the pod's outline.
pub const HINGE: Vec2 = Vec2::new(-0.48, 0.05);
/// How long the hatch takes to swing open.
pub const OPEN_TICKS: u64 = 2 * FIXED_HZ as u64 / 5;
/// How long a spaceling looks around, then waves, before its beam may come.
pub const LOOK_TICKS: u64 = FIXED_HZ as u64;
pub const WAVE_TICKS: u64 = FIXED_HZ as u64;
/// How long the rescue beam takes to come down on the spaceling.
pub const RESCUE_DESCENT_TICKS: u64 = FIXED_HZ as u64 / 2;
/// How far the beam lifts the spaceling, in the duel's units.
const RESCUE_RISE: f32 = 10.0;
/// How long a pod lies still before it opens.
const POD_WAIT_TICKS: u32 = 2 * FIXED_HZ / 5;
/// The latest a pod opens after the blast, whether it is still or not.
const POD_OPEN_BY_TICKS: u64 = 5 * FIXED_HZ as u64 / 2;
/// How long a spaceling stands still before it has found its feet.
const STAND_TICKS: u32 = FIXED_HZ / 5;
/// The least time between two of a spaceling's requests to get up.
const PRESS_TICKS: u64 = FIXED_HZ as u64 / 2;
/// How long a spaceling with no room to get up crawls one way before it
/// tries the other.
const CRAWL_TICKS: u64 = 3 * FIXED_HZ as u64 / 2;
/// How often a pod in flight leaves a puff of smoke behind it.
const POD_TRAIL_TICKS: u64 = 4;
/// A pod or a spaceling slower than this, in the duel's units a second, is
/// still.
const REST_SPEED: f32 = 0.5;
/// One turning slower than this, in radians a second, is still.
const REST_SPIN: f32 = 1.5;
/// How fast a stowaway runs, in the duel's units a second: three times the
/// spaceling's walk, a scurry that crosses most of the battlefield in a few
/// seconds.
pub const RUN_SPEED: f32 = 15.0;
/// How much time a stowaway wants in hand to set out, so it never gives up
/// as it starts.
const SET_OUT_SPARE_TICKS: u64 = FIXED_HZ as u64 / 2;
/// How long a stowaway looks toward the winner before it runs.
pub const SPOT_TICKS: u64 = FIXED_HZ as u64 / 2;
/// How long a stowaway takes to climb onto the winner's roof, and how long
/// it sits there before the winner may leave.
pub const CLIMB_TICKS: u64 = FIXED_HZ as u64 / 2;
pub const RIDE_TICKS: u64 = FIXED_HZ as u64 / 2;
/// The most the ground may rise within a unit of a stowaway's run, in the
/// duel's units, for it to jump up there.
const MAX_STEP: f32 = 1.3;
/// The deepest drop a stowaway runs off, in the duel's units: from deeper
/// it would land knocked down.
const MAX_DROP: f32 = 3.5;
/// A step ahead higher than this, in the duel's units, a stowaway jumps.
const HOP_STEP: f32 = 0.3;
/// How far ahead of a step a stowaway jumps, in the duel's units, and in
/// seconds of its run, so it is still rising as it reaches it.
const HOP_LEAD: f32 = 0.6;
const HOP_LEAD_SECONDS: f32 = 0.2;
/// How near the winner's side a stowaway climbs on from, in the duel's
/// units.
const REACH: f32 = 0.5;
/// Within this of the winner's side, in the duel's units, a stowaway slows
/// to climb on.
const SLOWING: f32 = 3.0;
/// How long a running stowaway may make no headway before it gives up, and
/// the headway that counts, in the duel's units.
const STUCK_TICKS: u64 = 3 * FIXED_HZ as u64 / 2;
const HEADWAY: f32 = 0.5;
/// How long the white flag takes to go up.
pub const FLAG_TICKS: u64 = 2 * FIXED_HZ as u64 / 5;
/// How fast the winner's turret turns onto a spaceling that surrenders, in
/// radians a second.
pub const TURRET_SPEED: f32 = 2.0;
/// How long the winner's beam takes to come down on a spaceling that
/// surrenders, and how long it takes until it has drawn it to the winner's
/// side.
pub const TAKE_DESCENT_TICKS: u64 = FIXED_HZ as u64 / 2;
pub const TAKE_TICKS: u64 = 2 * FIXED_HZ as u64;
/// Where the winner's beam holds a spaceling that surrendered, in the
/// duel's units: out from the side of the winner's hull, and up from its
/// middle.
const BESIDE: Vec2 = Vec2::new(1.0, 0.6);
/// How high the winner's beam lifts a spaceling on its way to the winner's
/// side, in the duel's units: at least this, and at most enough for its
/// feet to clear the hills and the dirt between, within the highest.
const TAKE_ARC: f32 = 1.5;
const TAKE_ARC_MOST: f32 = 5.0;
const DT: f32 = 1.0 / FIXED_HZ as f32;

/// The endings in the order a roll picks among them: the rescue beam, which
/// always fits, then those that need a winner in view.
const ENDINGS: [Ending; 3] = [Ending::RescueBeam, Ending::Stowaway, Ending::Surrender];

/// Clock's spaceling: the physics crate's, shrunk by [`PILOT_SCALE`], at
/// the duel's `scale` and in its fall. In the duel's units it moves as Space
/// Wars' smaller pilot does in the same fall, so its lengths, speeds and
/// accelerations follow `scale`, and its turning follows its size.
pub fn spec(scale: f32) -> SpacelingSpec {
    let defaults = SpacelingSpec::default();
    SpacelingSpec {
        collision_groups: wreck::GROUPS,
        radius: defaults.radius * PILOT_SCALE * scale,
        half_segment: defaults.half_segment * PILOT_SCALE * scale,
        walk_speed: defaults.walk_speed * scale,
        ground_acceleration: defaults.ground_acceleration * scale,
        air_acceleration: defaults.air_acceleration * scale,
        jump_speed: defaults.jump_speed * scale,
        max_angular_speed: defaults.max_angular_speed / PILOT_SCALE,
        angular_acceleration: defaults.angular_acceleration * 2.0 / PILOT_SCALE,
        balance: SpacelingBalanceSpec {
            knockdown_velocity_change: defaults.balance.knockdown_velocity_change * scale,
            knockdown_angular_speed: defaults.balance.knockdown_angular_speed / PILOT_SCALE,
            settle_speed: defaults.balance.settle_speed * scale,
            ..defaults.balance
        },
        ..defaults
    }
}

/// A stowaway's spaceling once it breaks into a run: Clock's, running at
/// [`RUN_SPEED`] and quick to reach it, in the air as on the ground. At a
/// run every bump in the dirt knocks it about, so it rights itself faster,
/// and only a knock three times as hard as one that fells a walker knocks
/// it down.
pub fn runner_spec(scale: f32) -> SpacelingSpec {
    let walker = spec(scale);
    SpacelingSpec {
        walk_speed: RUN_SPEED * scale,
        ground_acceleration: 3.0 * walker.ground_acceleration,
        air_acceleration: 3.0 * walker.air_acceleration,
        max_angular_speed: 2.0 * walker.max_angular_speed,
        angular_acceleration: 8.0 * walker.angular_acceleration,
        balance: SpacelingBalanceSpec {
            knockdown_velocity_change: 3.0 * walker.balance.knockdown_velocity_change,
            knockdown_angular_speed: 3.0 * walker.balance.knockdown_angular_speed,
            ..walker.balance
        },
        ..walker
    }
}

/// How each side's spaceling gets out if its tank blows up, or none for one
/// that does not: one in five gets out, half of those thrown clear and half
/// in a pod. Both sides roll, whatever the fight brings, from a stream of
/// their own, so neither the pairing, the dodgers nor the fight moves a
/// roll.
pub fn rolls(seed: u64) -> [Option<Ejection>; 2] {
    let mut rng = StdRng::seed_from_u64(seed ^ 0x5350_4143_454c_494e);
    std::array::from_fn(|_| {
        let out = rng.random_ratio(1, 5);
        let pod = rng.random::<bool>();
        out.then_some(if pod { Ejection::Pod } else { Ejection::Thrown })
    })
}

/// Each side's roll for its spaceling's ending, from another stream of their
/// own: a fraction that picks evenly among the endings that fit the fight.
pub fn ending_rolls(seed: u64) -> [f64; 2] {
    let mut rng = StdRng::seed_from_u64(seed ^ 0x454e_4449_4e47_5321);
    std::array::from_fn(|_| rng.random::<f64>())
}

/// The ending a `roll` draws, evenly from those that fit the fight: any of
/// them with a winner in view, and the rescue beam alone without one.
pub fn ending(roll: f64, winner_in_view: bool) -> Ending {
    let fit = if winner_in_view { ENDINGS.len() } else { 1 };
    ENDINGS[((roll * fit as f64) as usize).min(fit - 1)]
}

/// The winner as it stands, for a stowaway running to it and a spaceling
/// surrendering to it.
#[derive(Debug, Clone, Copy)]
pub struct Winner {
    pub slot: usize,
    pub hull: BodyMotion,
    /// Half its hull box's width and height.
    pub half: Vec2,
    /// The lowest and highest corners of its hull and wheels.
    pub bounds: [Vec2; 2],
    /// Its barrel's pivot and muzzle.
    pub barrel: [Vec2; 2],
}

impl Winner {
    /// How far `x` lies outside the winner's hull and wheels, across.
    fn gap(&self, x: f32) -> f32 {
        (self.bounds[0].x - x).max(x - self.bounds[1].x).max(0.0)
    }
}

/// The direction a turret turns its `barrel` to in a tick, toward `target`
/// and no further than the turret turns in that time. It turns over the
/// top, never down through the hull.
pub fn turn_barrel([pivot, muzzle]: [Vec2; 2], target: Vec2) -> Vec2 {
    let over = |v: Vec2| {
        let angle = v.y.atan2(v.x);
        if angle < -FRAC_PI_2 {
            angle + TAU
        } else {
            angle
        }
    };
    let from = over(muzzle - pivot);
    let to = over(target - pivot);
    let most = TURRET_SPEED * DT;
    Vec2::from_radians(from + (to - from).clamp(-most, most))
}

/// Whether a `barrel` points at `target`, near enough.
fn on_target([pivot, muzzle]: [Vec2; 2], target: Vec2) -> bool {
    let along = (muzzle - pivot).normalized();
    along.dot((target - pivot).normalized()) > 0.05_f32.cos()
}

/// What a stowaway finds under a point on its way.
enum Ground {
    /// Ground at this height.
    At(f32),
    /// Ground higher than it steps or jumps up to.
    Wall,
    /// No ground within the deepest drop it runs off.
    Pit,
}

/// The ground under `x` for a spaceling whose feet stand at `feet`. The ray
/// comes down from just above the highest step it could take, so the face
/// high above never answers. It finds only what a spaceling meets, and never
/// the `spaceling` itself.
fn ground(world: &PhysicsWorld, x: f32, feet: f32, scale: f32, spaceling: PhysicsId) -> Ground {
    let above = (MAX_STEP + 0.2) * scale;
    let hit = world.cast_ray(
        Vec2::new(x, feet + above),
        Vec2::new(0.0, -1.0),
        RayCastOptions {
            max_distance: above + MAX_DROP * scale,
            collision_groups: wreck::GROUPS,
            exclude_entity: Some(spaceling),
            ..Default::default()
        },
    );
    match hit {
        Some(hit) if hit.distance <= 1e-3 * scale => Ground::Wall,
        Some(hit) => Ground::At(hit.point.y),
        None => Ground::Pit,
    }
}

/// Whether a stowaway with its feet at `feet` has a way over the hills to
/// `to`, beside the winner: within each unit of its run the ground rises no
/// higher than it jumps, it drops no deeper than it lands from on its feet,
/// and nothing stands in its way.
fn way_clear(world: &PhysicsWorld, feet: Vec2, to: f32, scale: f32, spaceling: PhysicsId) -> bool {
    const PER_UNIT: usize = 4;
    let step = scale / PER_UNIT as f32;
    let dir = (to - feet.x).signum();
    let mut behind = [feet.y; PER_UNIT];
    let (mut x, mut height) = (feet.x, feet.y);
    let mut index = 0;
    loop {
        x += dir * step;
        if (to - x) * dir <= 0.0 {
            return true;
        }
        let Ground::At(y) = ground(world, x, height, scale, spaceling) else {
            return false;
        };
        let lowest = behind.iter().copied().fold(f32::INFINITY, f32::min);
        if y - lowest > MAX_STEP * scale {
            return false;
        }
        behind[index % PER_UNIT] = y;
        height = y;
        index += 1;
    }
}

/// Whether a running stowaway with its feet at `feet`, running at `speed`
/// across toward `dir`, jumps now: just ahead, short of `clear` on, lies a
/// step steeper than it runs up, or something in its way. It runs up the
/// slopes it can, and jumps late enough to keep its speed and early enough
/// to be rising still as it reaches the step.
fn hop(
    world: &PhysicsWorld,
    feet: Vec2,
    dir: f32,
    speed: f32,
    clear: f32,
    scale: f32,
    spaceling: PhysicsId,
) -> bool {
    let reach = (HOP_LEAD * scale + speed * HOP_LEAD_SECONDS).min(clear);
    let step = 0.25 * scale;
    let mut last = feet.y;
    for ahead in (1..)
        .map(|index| index as f32 * step)
        .take_while(|&ahead| ahead <= reach)
    {
        match ground(world, feet.x + dir * ahead, last, scale, spaceling) {
            // A face steeper than a slope it runs up, rising to a step.
            Ground::At(y) if y - last > step && y - feet.y > HOP_STEP * scale => return true,
            Ground::At(y) => last = y,
            Ground::Wall => return true,
            Ground::Pit => return false,
        }
    }
    false
}

/// Whether a stowaway `gap` from the winner's side at `tick`, running, can
/// reach it and climb on before the aftermath ends at `deadline`, with
/// `spare` ticks to spare.
fn in_time(gap: f32, scale: f32, tick: u64, deadline: u64, spare: u64) -> bool {
    let run = (gap / (RUN_SPEED * scale) * FIXED_HZ as f32) as u64;
    tick + run + CLIMB_TICKS + FIXED_HZ as u64 / 4 + spare <= deadline
}

/// Where a stowaway is `ticks` into its climb onto the winner, along its
/// `path` in the frame of the winner's hull: from where it set out, up the
/// hull's side to the edge of its roof, over the turret if it must, and onto
/// the roof behind the turret.
fn climbing(path: [Vec2; 4], ticks: u64) -> Vec2 {
    let ease = |u: f32| u * u * (3.0 - 2.0 * u);
    let lengths = [0, 1, 2].map(|leg| path[leg].distance_to(path[leg + 1]));
    let total: f32 = lengths.iter().sum();
    let mut along = ease((ticks as f32 / CLIMB_TICKS as f32).min(1.0)) * total;
    for (leg, length) in lengths.into_iter().enumerate() {
        if along <= length && length > 0.0 {
            return path[leg] + (path[leg + 1] - path[leg]) * (along / length);
        }
        along -= length;
    }
    path[3]
}

/// Where a spaceling the winner's beam takes is `ticks` into it: where it
/// stood, `from`, while the beam comes down on it, then drawn over an `arc`
/// that high to `beside` the winner.
fn taking(from: Vec2, beside: Vec2, ticks: u64, arc: f32) -> Vec2 {
    let ease = |u: f32| u * u * (3.0 - 2.0 * u);
    let u = (ticks.saturating_sub(TAKE_DESCENT_TICKS) as f32
        / (TAKE_TICKS - TAKE_DESCENT_TICKS) as f32)
        .min(1.0);
    from + (beside - from) * ease(u) + Vec2::Y * (arc * (PI * u).sin())
}

/// How high the winner's beam lifts a spaceling on its way from `from` to
/// `to`, for its feet to clear the hills and the dirt between, every
/// quarter of a unit across. A ray that starts inside the face never
/// answers.
fn take_arc(world: &PhysicsWorld, from: Vec2, to: Vec2, scale: f32) -> f32 {
    let clear = spec(scale).half_height() + 0.2 * scale;
    let most = TAKE_ARC_MOST * scale;
    let steps = ((to.x - from.x).abs() / (0.25 * scale)).ceil().max(1.0) as usize;
    let mut arc = TAKE_ARC * scale;
    for step in 1..steps {
        // How far across, and how far into the draw it is there, undoing
        // the ease in [`taking`].
        let across = step as f32 / steps as f32;
        let u = 0.5 - ((1.0 - 2.0 * across).asin() / 3.0).sin();
        let lift = (PI * u).sin();
        if lift < 0.25 {
            continue;
        }
        let line = from + (to - from) * across;
        let hit = world.cast_ray(
            line + Vec2::Y * most,
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                max_distance: most + clear + to.y.max(from.y) - to.y.min(from.y),
                collision_groups: wreck::GROUPS,
                ..Default::default()
            },
        );
        if let Some(hit) = hit
            && hit.distance > 1e-3 * scale
        {
            arc = arc.max((hit.point.y + clear - line.y) / lift);
        }
    }
    arc.min(most)
}

/// A loser's spaceling that got out.
pub struct Survivor {
    /// The tank it came from.
    pub slot: usize,
    pub ejection: Ejection,
    /// The ending its roll drew.
    pub drawn: Ending,
    /// The ending it is on: the one it drew, or the rescue beam once that
    /// one falls back, and why it did.
    pub ending: Ending,
    pub fallback: Option<Fallback>,
    pub phase: Phase,
    /// The event's tick its tank blew up on.
    pub blast_tick: u64,
    /// The event's tick it began its phase on.
    pub since: u64,
    /// The spaceling, once it is out: thrown clear, or out of its pod.
    pub spaceling: Option<SpacelingAssembly>,
    /// Its pod, from the blast until the hills withdraw.
    pub pod: Option<BodyId>,
    /// Which way it faces: 1 to the right, -1 to the left.
    pub facing: f32,
    /// Where it last was: the spaceling, or its pod while it is sealed in.
    pub position: Vec2,
    /// What the beam lifts, and where from: the spaceling, or a pod it never
    /// climbed out of.
    pub lifted: Option<(BodyId, BodyMotion)>,
    /// For a spaceling the winner's beam takes: where it stood as the beam
    /// came, where the beam holds it, from the middle of the winner's hull,
    /// and how high it lifts it on the way.
    pub taken: Option<(BodyMotion, Vec2, f32)>,
    /// For a stowaway: its climb onto the winner's roof, in the frame of the
    /// winner's hull, from where it climbs on to where it rides.
    pub climb: Option<[Vec2; 4]>,
    /// Whether it has stood up since it got out.
    pub stood_up: bool,
    /// Whether it set out for the winner, so it runs on once back up.
    ran: bool,
    /// The tick it last made headway on, and where across it was.
    headway: (u64, f32),
    /// Whether it held its jump last tick, to jump a step.
    hopped: bool,
    /// The ticks its pod has lain still for, then it has stood still for.
    still: u32,
    /// Whether its pod has come down since it flew out.
    landed: bool,
    /// The event's tick of its last request to get up.
    pressed: Option<u64>,
    /// The event's tick it found it had no room to get up on.
    blocked: Option<u64>,
}

impl Survivor {
    fn enter(&mut self, phase: Phase, tick: u64) {
        self.phase = phase;
        self.since = tick;
        self.still = 0;
    }

    /// Settles on the winner's roof at `tick`, facing the turret.
    fn ride(&mut self, tick: u64) {
        if let Some([.., deck]) = self.climb {
            self.facing = -deck.x.signum();
        }
        self.enter(Phase::Riding, tick);
    }

    /// Gives up its own ending for the rescue beam, `why`, and waves for it.
    fn fall_back(&mut self, why: Fallback, tick: u64) {
        self.ending = Ending::RescueBeam;
        self.fallback = Some(why);
        self.enter(Phase::Waving, tick);
    }

    /// How far its pod's hatch has swung open, from shut to wide open.
    pub fn hatch(&self, tick: u64) -> f32 {
        match self.phase {
            Phase::Sealed => 0.0,
            Phase::Opening => ((tick - self.since) as f32 / OPEN_TICKS as f32).min(1.0),
            // A pod the beam lifts out sealed stays shut.
            Phase::BeamedUp if self.pod_lifted() => 0.0,
            _ => 1.0,
        }
    }

    /// Whether the beam is lifting out the pod itself, still sealed.
    pub fn pod_lifted(&self) -> bool {
        self.lifted.is_some_and(|(body, _)| self.pod == Some(body))
    }

    /// How far its white flag is up at `tick`, from down to all the way.
    pub fn flag(&self, tick: u64) -> f32 {
        match self.phase {
            Phase::Surrendering => ((tick - self.since) as f32 / FLAG_TICKS as f32).min(1.0),
            Phase::Taken => 1.0,
            _ => 0.0,
        }
    }

    /// How far it has faded from view `phase_tick` ticks into the beam-up,
    /// from not at all to gone: as its own beam lifts it out, or as the
    /// winner it rides or goes with fades.
    pub fn fade(&self, phase_tick: u64) -> f32 {
        match self.phase {
            Phase::Riding | Phase::Taken => phase_tick as f32 / BEAM_UP_TICKS as f32,
            Phase::BeamedUp => Survivors::rescued(phase_tick),
            _ => 0.0,
        }
    }
}

/// The losers' spacelings that got out, from the blast until they leave.
pub struct Survivors {
    /// How each side's spaceling gets out if its tank blows up.
    pub rolls: [Option<Ejection>; 2],
    /// Each side's roll for its spaceling's ending.
    pub ending_rolls: [f64; 2],
    pub out: Vec<Survivor>,
    /// The smoke the pods left behind them as they flew.
    pub puffs: Vec<Puff>,
    /// Host units per duel unit.
    scale: f32,
    gravity_scale: f32,
    /// The launches' own stream, so they never move a roll.
    rng: StdRng,
    /// The spacelings knocked down since they got out, kept for the
    /// diagnostics once the beam takes them.
    knockdowns: [u64; 2],
}

impl Survivors {
    pub fn new(seed: u64, scale: f32) -> Self {
        Self {
            rolls: rolls(seed),
            ending_rolls: ending_rolls(seed),
            out: Vec::new(),
            puffs: Vec::new(),
            scale,
            gravity_scale: 1.0,
            rng: StdRng::seed_from_u64(seed ^ 0x4c41_554e_4348_4553),
            knockdowns: [0; 2],
        }
    }

    /// The survivor from the tank in `slot`, if its spaceling got out.
    pub fn get(&self, slot: usize) -> Option<&Survivor> {
        self.out.iter().find(|survivor| survivor.slot == slot)
    }

    /// The spaceling of the loser in `slot`, blowing up at `tick` as `tank`
    /// last stood, gets out if its roll says so, and draws its ending from
    /// those that fit, whether or not there is a winner in view. It leaves
    /// toward the middle of the view, and falls as the duel's dirt does, by
    /// `gravity_scale`.
    pub fn eject(
        &mut self,
        world: &mut PhysicsWorld,
        slot: usize,
        tank: &TankState,
        winner_in_view: bool,
        gravity_scale: f32,
        tick: u64,
    ) {
        let Some(ejection) = self.rolls[slot] else {
            return;
        };
        self.gravity_scale = gravity_scale;
        let scale = self.scale;
        let drawn = ending(self.ending_rolls[slot], winner_in_view);
        let hull = tank.motion;
        let [hatch, _] = tank.barrel;
        let arm = hatch - hull.position;
        let carried = hull.linear_velocity + Vec2::new(-arm.y, arm.x) * hull.angular_velocity;
        let inward = if hull.position.x > 0.0 { -1.0 } else { 1.0 };
        let mut survivor = Survivor {
            slot,
            ejection,
            drawn,
            ending: drawn,
            fallback: None,
            phase: Phase::Flying,
            blast_tick: tick,
            since: tick,
            spaceling: None,
            pod: None,
            facing: inward,
            position: hatch,
            lifted: None,
            taken: None,
            climb: None,
            stood_up: false,
            ran: false,
            headway: (tick, hatch.x),
            hopped: false,
            still: 0,
            landed: false,
            pressed: None,
            blocked: None,
        };
        match ejection {
            Ejection::Thrown => {
                // Thrown up and away from the blast, past where most of the
                // pieces land, tumbling faster than the spaceling can keep
                // its feet, so it lands knocked down.
                let launch = carried
                    + Vec2::new(
                        inward * self.rng.random_range(7.0..10.0),
                        self.rng.random_range(9.0..12.0),
                    ) * scale;
                let spin =
                    self.rng.random_range(16.0..20.0) * if self.rng.random() { 1.0 } else { -1.0 };
                let angle = self.rng.random_range(-1.0..1.0);
                let motion = BodyMotion {
                    position: hatch,
                    angle,
                    linear_velocity: launch,
                    angular_velocity: spin,
                };
                survivor.spaceling = Some(self.insert_spaceling(world, slot, motion));
            }
            Ejection::Pod => {
                // Fired off like a shell, in a lower, shorter arc, and
                // turning slowly, so it mostly comes down on its base.
                let launch = carried
                    + Vec2::new(
                        inward * self.rng.random_range(7.0..10.0),
                        self.rng.random_range(11.0..14.0),
                    ) * scale;
                let motion = BodyMotion {
                    position: hatch,
                    angle: hull.angle,
                    linear_velocity: launch,
                    angular_velocity: self.rng.random_range(-1.2..1.2),
                };
                survivor.pod = Some(self.insert_pod(world, slot, motion));
                survivor.phase = Phase::Sealed;
            }
        }
        self.out.push(survivor);
    }

    /// Where a body `half` across fits clear of the hills, the dirt and the
    /// winner, from `position` up.
    fn clear_of(world: &PhysicsWorld, position: Vec2, half: Vec2) -> Vec2 {
        (0..=10)
            .map(|step| position + Vec2::Y * (step as f32 * 0.2 * half.y))
            .find(|&at| world.current_cuboid_is_clear(at, 0.0, half, wreck::GROUPS, &[]))
            .unwrap_or(position + Vec2::Y * (2.0 * half.y))
    }

    /// The spaceling from the tank in `slot`, set off with `motion`.
    fn insert_spaceling(
        &self,
        world: &mut PhysicsWorld,
        slot: usize,
        motion: BodyMotion,
    ) -> SpacelingAssembly {
        let spec = spec(self.scale);
        let position = Self::clear_of(
            world,
            motion.position,
            Vec2::new(spec.radius, spec.half_height()),
        );
        let entity = PhysicsId::new(canonical::TANKS_SURVIVORS.start + slot as u64);
        let spaceling = SpacelingAssembly::insert(world, entity, position, motion.angle, spec)
            .expect("the room kept for a survivor");
        world.set_velocity(
            spaceling.body(),
            motion.linear_velocity,
            motion.angular_velocity,
            true,
        );
        world.set_gravity_scale(spaceling.body(), self.gravity_scale, true);
        spaceling
    }

    fn insert_pod(&self, world: &mut PhysicsWorld, slot: usize, motion: BodyMotion) -> BodyId {
        let scale = self.scale;
        let position = Self::clear_of(world, motion.position, Vec2::new(0.48, 0.5) * scale);
        let entity = PhysicsId::new(canonical::TANKS_SURVIVORS.start + 2 + slot as u64);
        let mut shell = ColliderSpec::convex_polygon(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            POD.map(|corner| corner * scale).to_vec(),
        );
        shell.collision_groups = wreck::GROUPS;
        shell.friction = 0.6;
        shell.restitution = 0.0;
        // Its weight lies in its base, so it takes a steep landing to tip it
        // over, as a weighted toy's does.
        let mass = POD_AREA * scale * scale;
        shell.mass_properties = Some(ColliderMassProperties {
            center: Vec2::new(0.0, -0.3) * scale,
            mass,
            inertia: mass * 0.15 * scale * scale,
        });
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        assert!(
            world.insert_body(
                body,
                BodySpec {
                    position,
                    angle: motion.angle,
                    linear_velocity: motion.linear_velocity,
                    angular_velocity: motion.angular_velocity,
                    gravity_scale: self.gravity_scale,
                    linear_damping: 0.1,
                    // A pod that lands on its base soon stops rocking.
                    angular_damping: 1.5,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[shell],
            ),
            "the room kept for a survivor"
        );
        body
    }

    /// Steers the spacelings before the arena's step at `tick`, in the fall
    /// the duel gives its bodies, `gravity`. Each stands where it is, and one
    /// that lies settled asks to get up, at most twice a second. One with no
    /// room to get up, under the dirt or a ledge, crawls out from under it,
    /// the way it faces and then back the other way. A stowaway runs for the
    /// `winner`, slowing as it comes up to it, and jumps the steps ahead of
    /// it. Climbing on, and on the winner's roof, it goes where the hull
    /// carries it. One the winner's beam has taken goes where the beam draws
    /// it.
    pub fn control(
        &mut self,
        world: &mut PhysicsWorld,
        gravity: Vec2,
        tick: u64,
        winner: Option<Winner>,
    ) {
        let scale = self.scale;
        let walker = spec(scale);
        for survivor in &mut self.out {
            let Some(spaceling) = &mut survivor.spaceling else {
                continue;
            };
            // Where it is once the arena has stepped: the event's tick has
            // moved on by then.
            let ticks = tick + 1 - survivor.since;
            match survivor.phase {
                Phase::BeamedUp | Phase::Lost => continue,
                Phase::Climbing | Phase::Riding => {
                    if let (Some(winner), Some(climb)) = (winner, survivor.climb) {
                        let local = if survivor.phase == Phase::Riding {
                            climb[3]
                        } else {
                            climbing(climb, ticks)
                        };
                        world.set_next_kinematic_pose(
                            spaceling.body(),
                            winner.hull.position + local.rotate_radians(winner.hull.angle),
                            winner.hull.angle,
                        );
                    }
                    continue;
                }
                Phase::Taken => {
                    if let (Some(winner), Some((from, beside, arc))) = (winner, survivor.taken) {
                        world.set_next_kinematic_pose(
                            spaceling.body(),
                            taking(from.position, winner.hull.position + beside, ticks, arc),
                            from.angle,
                        );
                    }
                    continue;
                }
                _ => (),
            }
            let snapshot = spaceling.snapshot(world).expect("a survivor's spaceling");
            let press = snapshot.needs_get_up()
                && snapshot.contacts > 0
                && snapshot.motion.linear_velocity.length() < walker.balance.settle_speed
                && snapshot.motion.angular_velocity.abs() < walker.balance.settle_angular_speed
                && survivor
                    .pressed
                    .is_none_or(|pressed| tick - pressed >= PRESS_TICKS);
            if press {
                survivor.pressed = Some(tick);
            }
            let mut walk = if snapshot.needs_get_up()
                && snapshot.get_up_result == SpacelingGetUpResult::Blocked
            {
                let since = *survivor.blocked.get_or_insert(tick);
                if ((tick - since) / CRAWL_TICKS).is_multiple_of(2) {
                    survivor.facing
                } else {
                    -survivor.facing
                }
            } else {
                survivor.blocked = None;
                0.0
            };
            let mut hopping = false;
            // A runner knocked about but not down rights itself as it runs.
            if survivor.phase == Phase::Running
                && snapshot.balance == SpacelingBalance::Balanced
                && let Some(winner) = winner
            {
                let at = snapshot.motion.position;
                let dir = (winner.hull.position.x - at.x).signum();
                let gap = winner.gap(at.x);
                survivor.facing = dir;
                walk = dir * (gap / (SLOWING * scale)).clamp(0.3, 1.0);
                if snapshot.grounded() {
                    hopping = hop(
                        world,
                        at - Vec2::Y * walker.half_height(),
                        dir,
                        snapshot.motion.linear_velocity.x.abs(),
                        gap - REACH * scale,
                        scale,
                        spaceling.body().entity,
                    );
                }
            }
            // A jump takes a fresh press, so it lets go between jumps.
            let jump = hopping && !survivor.hopped;
            survivor.hopped = jump;
            spaceling.apply_control(
                world,
                SpacelingControl {
                    walk,
                    jump_held: press || jump,
                },
                gravity,
                DT,
            );
        }
    }

    /// Follows the survivors after the arena's step at `tick`, with the
    /// `winner` as it stands and the aftermath at the latest over by
    /// `deadline`. A pod opens once it has lain still a moment, and its
    /// spaceling climbs out. A spaceling that has found its feet looks
    /// around, then waves for its beam. One that surrenders raises its white
    /// flag instead, and once it has held it up a moment with the winner's
    /// turret on it, the winner's beam takes it. A stowaway looks toward the
    /// winner, then runs to it if it has a way and the time, and climbs onto
    /// its roof. Without a winner, a way or the time, either waves for the
    /// rescue beam. A spaceling, or a pod with its spaceling still in it,
    /// that leaves the view retires there.
    pub fn observe(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        tick: u64,
        winner: Option<Winner>,
        deadline: u64,
    ) {
        let scale = self.scale;
        let outside =
            |point: Vec2| point.y < layout.bounds_min.y || point.x.abs() > layout.bounds_max.x;
        for index in 0..self.out.len() {
            let survivor = &mut self.out[index];
            let mut run = false;
            let mut climb = false;
            let mut take = false;
            match survivor.phase {
                Phase::Sealed | Phase::Opening => {
                    let pod = survivor.pod.expect("a sealed pod");
                    let motion = world.motion(pod).expect("a pod");
                    survivor.position = motion.position;
                    if outside(motion.position) {
                        world.remove_entity(pod.entity);
                        survivor.pod = None;
                        survivor.enter(Phase::Lost, tick);
                        continue;
                    }
                    if survivor.phase == Phase::Opening {
                        if tick - survivor.since >= OPEN_TICKS {
                            self.climb_out(world, index, motion, tick);
                        }
                        continue;
                    }
                    survivor.landed |= world
                        .surface_contacts(ColliderId::new(pod.entity, ColliderRole::PRIMARY, 0))
                        .next()
                        .is_some();
                    if motion.linear_velocity.length() < REST_SPEED * scale
                        && motion.angular_velocity.abs() < REST_SPIN
                    {
                        survivor.still += 1;
                    } else {
                        survivor.still = 0;
                    }
                    if survivor.still >= POD_WAIT_TICKS
                        || tick - survivor.blast_tick >= POD_OPEN_BY_TICKS
                    {
                        survivor.enter(Phase::Opening, tick);
                    } else if !survivor.landed
                        && (tick - survivor.blast_tick).is_multiple_of(POD_TRAIL_TICKS)
                    {
                        // From the base, where it was fired from.
                        let base = motion.position
                            + (Vec2::Y * (-0.45 * scale)).rotate_radians(motion.angle);
                        self.puffs.push(Puff { point: base, tick });
                    }
                }
                Phase::Climbing | Phase::Riding | Phase::Taken => {
                    let spaceling = survivor.spaceling.as_ref().expect("a spaceling carried");
                    survivor.position = world
                        .motion(spaceling.body())
                        .expect("a spaceling carried")
                        .position;
                    if survivor.phase == Phase::Climbing && tick - survivor.since >= CLIMB_TICKS {
                        survivor.ride(tick);
                    }
                }
                Phase::Flying
                | Phase::GettingUp
                | Phase::ClimbingOut
                | Phase::LookingAround
                | Phase::Waving
                | Phase::Running
                | Phase::Surrendering => {
                    let spaceling = survivor.spaceling.as_ref().expect("a spaceling out");
                    let snapshot = spaceling.snapshot(world).expect("a survivor's spaceling");
                    let at = snapshot.motion.position;
                    survivor.position = at;
                    self.knockdowns[survivor.slot] = snapshot.knockdowns;
                    if outside(at) {
                        world.remove_entity(spaceling.body().entity);
                        survivor.spaceling = None;
                        survivor.enter(Phase::Lost, tick);
                        continue;
                    }
                    let standing = snapshot.balance == SpacelingBalance::Balanced
                        && snapshot.grounded()
                        && !snapshot.needs_get_up()
                        && snapshot.relative_speed.abs() < REST_SPEED * scale
                        && snapshot.motion.angular_velocity.abs() < REST_SPIN;
                    // A runner knocked about but not down rights itself as
                    // it goes.
                    let fallen = if survivor.ran {
                        snapshot.balance != SpacelingBalance::Balanced
                    } else {
                        snapshot.needs_get_up()
                    };
                    survivor.still = if standing { survivor.still + 1 } else { 0 };
                    let toward = winner.map(|winner| (winner.hull.position.x - at.x).signum());
                    match survivor.phase {
                        Phase::Flying | Phase::GettingUp | Phase::ClimbingOut
                            if survivor.still >= STAND_TICKS =>
                        {
                            survivor.stood_up = true;
                            if survivor.ending == Ending::Stowaway {
                                // A stowaway eyes the winner, and once it has
                                // set out, runs on.
                                survivor.facing = toward.unwrap_or(survivor.facing);
                                run = survivor.ran;
                            }
                            if !run {
                                survivor.enter(Phase::LookingAround, tick);
                            }
                        }
                        Phase::Flying if snapshot.contacts > 0 => {
                            survivor.enter(Phase::GettingUp, tick);
                        }
                        // Knocked over again, it gets up again.
                        Phase::LookingAround
                        | Phase::Waving
                        | Phase::Running
                        | Phase::Surrendering
                            if fallen =>
                        {
                            survivor.enter(Phase::GettingUp, tick);
                        }
                        Phase::LookingAround => {
                            let looked = tick - survivor.since;
                            match (survivor.ending, toward) {
                                (Ending::Stowaway, _) => run = looked >= SPOT_TICKS,
                                (Ending::Surrender, Some(toward)) if looked >= LOOK_TICKS => {
                                    survivor.facing = toward;
                                    if tick + FLAG_TICKS + WAVE_TICKS + TAKE_TICKS <= deadline {
                                        survivor.enter(Phase::Surrendering, tick);
                                    } else {
                                        survivor.fall_back(Fallback::OutOfTime, tick);
                                    }
                                }
                                (Ending::Surrender, None) if looked >= LOOK_TICKS => {
                                    survivor.fall_back(Fallback::NoWay, tick);
                                }
                                _ if looked >= LOOK_TICKS => survivor.enter(Phase::Waving, tick),
                                _ if looked == LOOK_TICKS / 3 || looked == 2 * LOOK_TICKS / 3 => {
                                    survivor.facing = -survivor.facing;
                                }
                                _ => (),
                            }
                        }
                        Phase::Running => match winner {
                            None => survivor.fall_back(Fallback::NoWay, tick),
                            Some(winner) => {
                                let gap = winner.gap(at.x);
                                if (at.x - survivor.headway.1).abs() >= HEADWAY * scale {
                                    survivor.headway = (tick, at.x);
                                }
                                if gap <= REACH * scale {
                                    climb = true;
                                } else if tick - survivor.headway.0 >= STUCK_TICKS {
                                    survivor.fall_back(Fallback::NoWay, tick);
                                } else if !in_time(gap, scale, tick, deadline, 0) {
                                    survivor.fall_back(Fallback::OutOfTime, tick);
                                }
                            }
                        },
                        // Its flag held up a moment with the winner's turret
                        // on it, the winner's beam takes it, if there is
                        // still time for it to.
                        Phase::Surrendering => match winner {
                            None => survivor.fall_back(Fallback::NoWay, tick),
                            Some(_) if tick + TAKE_TICKS > deadline => {
                                survivor.fall_back(Fallback::OutOfTime, tick);
                            }
                            Some(winner) => {
                                take = tick - survivor.since >= FLAG_TICKS + WAVE_TICKS
                                    && on_target(winner.barrel, at);
                            }
                        },
                        _ => (),
                    }
                }
                Phase::BeamedUp | Phase::Lost => (),
            }
            if run {
                self.set_out(world, index, winner, tick, deadline);
            } else if climb {
                self.climb_on(world, index, winner.expect("a winner to climb"), tick);
            } else if take {
                self.take(world, index, winner.expect("a winner to take it"), tick);
            }
        }
    }

    /// The spaceling of survivor `index` climbs out of its pod, lying at
    /// `pod` with its hatch wide open, at `tick`. It hops out over the side
    /// it faces.
    fn climb_out(&mut self, world: &mut PhysicsWorld, index: usize, pod: BodyMotion, tick: u64) {
        let scale = self.scale;
        let survivor = &self.out[index];
        let (slot, facing) = (survivor.slot, survivor.facing);
        let up = Vec2::Y.rotate_radians(pod.angle);
        let spec = spec(scale);
        let hatch = pod.position + up * (0.5 * scale) + Vec2::Y * spec.half_height();
        let motion = BodyMotion {
            position: hatch,
            angle: 0.0,
            linear_velocity: (up * 1.5 + Vec2::new(facing * 1.5, 2.5)) * scale,
            angular_velocity: 0.0,
        };
        let spaceling = self.insert_spaceling(world, slot, motion);
        let survivor = &mut self.out[index];
        survivor.position = world.motion(spaceling.body()).unwrap().position;
        survivor.spaceling = Some(spaceling);
        survivor.enter(Phase::ClimbingOut, tick);
    }

    /// The stowaway of survivor `index` sets out for the `winner` at `tick`,
    /// at a run, if it has a way over the hills to it and the time to get
    /// there before `deadline`, and waves for the rescue beam if it has not.
    fn set_out(
        &mut self,
        world: &PhysicsWorld,
        index: usize,
        winner: Option<Winner>,
        tick: u64,
        deadline: u64,
    ) {
        let scale = self.scale;
        let half_height = spec(scale).half_height();
        let survivor = &mut self.out[index];
        let spaceling = survivor.spaceling.as_mut().expect("a stowaway");
        let at = survivor.position;
        let clear = winner.is_some_and(|winner| {
            // Only a winner standing on its wheels has a roof to ride.
            if winner.hull.angle.cos() < 0.8 {
                return false;
            }
            let near = if at.x < winner.hull.position.x {
                winner.bounds[0].x - REACH * scale
            } else {
                winner.bounds[1].x + REACH * scale
            };
            way_clear(
                world,
                at - Vec2::Y * half_height,
                near,
                scale,
                spaceling.body().entity,
            )
        });
        if !clear {
            survivor.fall_back(Fallback::NoWay, tick);
        } else if winner.is_some_and(|winner| {
            !in_time(winner.gap(at.x), scale, tick, deadline, SET_OUT_SPARE_TICKS)
        }) {
            survivor.fall_back(Fallback::OutOfTime, tick);
        } else {
            assert!(spaceling.set_spec(runner_spec(scale)), "a runner's spec");
            survivor.ran = true;
            survivor.headway = (tick, at.x);
            survivor.enter(Phase::Running, tick);
        }
    }

    /// The stowaway of survivor `index`, beside the `winner` at `tick`,
    /// climbs onto its roof, behind the turret, clear of the barrel. One that
    /// came up on the barrel's side climbs over the turret. From here the
    /// hull carries it, and it pushes nothing.
    fn climb_on(&mut self, world: &mut PhysicsWorld, index: usize, winner: Winner, tick: u64) {
        let scale = self.scale;
        let walker = spec(scale);
        let survivor = &mut self.out[index];
        let spaceling = survivor.spaceling.as_ref().expect("a stowaway");
        let body = spaceling.body();
        let at = world.motion(body).expect("a stowaway").position;
        let local = |point: Vec2| (point - winner.hull.position).rotate_radians(-winner.hull.angle);
        let side = (at.x - winner.hull.position.x).signum();
        let [pivot, muzzle] = winner.barrel.map(local);
        let aim = muzzle.x - pivot.x;
        let back = if aim.abs() < 0.1 * scale {
            side
        } else {
            -aim.signum()
        };
        let roof = winner.half.y + walker.half_height();
        let edge = Vec2::new(
            side * (winner.half.x + walker.radius + 0.05 * scale),
            roof + 0.05 * scale,
        );
        let over = if back == side {
            edge
        } else {
            Vec2::new(
                pivot.x,
                pivot.y + wreck::TURRET_RADIUS * scale + walker.half_height() + 0.1 * scale,
            )
        };
        // On the roof between the turret and its back edge.
        let deck = Vec2::new(back * (winner.half.x - 0.55 * scale), roof + 0.02 * scale);
        world.set_body_kind(body, BodyKind::KinematicPosition, true);
        world.set_collision_groups(spaceling.collider(), CollisionGroups::NONE);
        survivor.climb = Some([local(at), edge, over, deck]);
        survivor.facing = -side;
        survivor.enter(Phase::Climbing, tick);
    }

    /// The `winner`'s beam takes the spaceling of survivor `index`, which
    /// surrendered to it, at `tick`, to draw it to the winner's side. From
    /// here the beam carries it, and it pushes nothing.
    fn take(&mut self, world: &mut PhysicsWorld, index: usize, winner: Winner, tick: u64) {
        let scale = self.scale;
        let survivor = &mut self.out[index];
        let spaceling = survivor
            .spaceling
            .as_ref()
            .expect("a spaceling that surrendered");
        let body = spaceling.body();
        let from = world.motion(body).expect("a spaceling that surrendered");
        let side = (from.position.x - winner.hull.position.x).signum();
        world.set_body_kind(body, BodyKind::KinematicPosition, true);
        world.set_collision_groups(spaceling.collider(), CollisionGroups::NONE);
        let beside = Vec2::new(side * (winner.half.x + BESIDE.x * scale), BESIDE.y * scale);
        let arc = take_arc(world, from.position, winner.hull.position + beside, scale);
        survivor.taken = Some((from, beside, arc));
        survivor.enter(Phase::Taken, tick);
    }

    /// Whether every survivor is ready to leave at `tick`, or lost, so the
    /// aftermath can end: one has waved long enough for its beam, sat on
    /// the winner's roof a moment, or been drawn to the winner's side.
    /// Without a survivor, it can.
    pub fn ready(&self, tick: u64) -> bool {
        self.out.iter().all(|survivor| match survivor.phase {
            Phase::Lost | Phase::BeamedUp => true,
            Phase::Waving => tick - survivor.since >= WAVE_TICKS,
            Phase::Riding => tick - survivor.since >= RIDE_TICKS,
            Phase::Taken => tick - survivor.since >= TAKE_TICKS,
            _ => false,
        })
    }

    /// The longest the aftermath waits for the survivors: long enough for a
    /// stowaway's run or a surrender, or for a spaceling that waves for the
    /// rescue beam.
    pub fn aftermath_ticks(&self) -> u64 {
        if self
            .out
            .iter()
            .all(|survivor| survivor.drawn == Ending::RescueBeam)
        {
            RESCUE_BY_TICKS
        } else {
            AFTERMATH_TICKS
        }
    }

    /// The survivors the winner's beam carries up with its hull, with their
    /// bodies as they stand: the stowaways on its roof, or still climbing
    /// on, and those it has taken.
    pub fn riders(&mut self, world: &PhysicsWorld, tick: u64) -> Vec<(BodyId, BodyMotion)> {
        let mut riders = Vec::new();
        for survivor in &mut self.out {
            if !matches!(
                survivor.phase,
                Phase::Climbing | Phase::Riding | Phase::Taken
            ) {
                continue;
            }
            if survivor.phase == Phase::Climbing {
                survivor.ride(tick);
            }
            let body = survivor.spaceling.as_ref().expect("a rider").body();
            riders.push((body, world.motion(body).expect("a rider")));
        }
        riders
    }

    /// Starts the beams that lift out the survivors still in view at `tick`
    /// that the winner's beam does not carry: each spaceling standing or as
    /// it lies, or in its pod if it never climbed out. One whose own ending
    /// had not finished falls back to the rescue beam: out of time, or with
    /// no way once the `winner` has gone. Answers whether there are any.
    pub fn begin_rescue(
        &mut self,
        world: &mut PhysicsWorld,
        tick: u64,
        winner: Option<Winner>,
    ) -> bool {
        let mut any = false;
        for survivor in &mut self.out {
            if matches!(
                survivor.phase,
                Phase::Lost | Phase::BeamedUp | Phase::Climbing | Phase::Riding | Phase::Taken
            ) {
                continue;
            }
            if survivor.ending != Ending::RescueBeam {
                survivor.ending = Ending::RescueBeam;
                survivor.fallback = Some(if winner.is_some() {
                    Fallback::OutOfTime
                } else {
                    Fallback::NoWay
                });
            }
            let body = survivor
                .spaceling
                .as_ref()
                .map(SpacelingAssembly::body)
                .or(survivor.pod)
                .expect("a survivor in view");
            let motion = world.motion(body).expect("a survivor's body");
            world.set_body_kind(body, BodyKind::KinematicPosition, true);
            survivor.lifted = Some((body, motion));
            survivor.enter(Phase::BeamedUp, tick);
            any = true;
        }
        any
    }

    /// Lifts each survivor `phase_tick` ticks into the beam-up, before the
    /// arena's step: its own beam comes down on it, then lifts it up and
    /// out. Those the winner's beam carries, it follows as they go.
    pub fn lift(&mut self, world: &mut PhysicsWorld, phase_tick: u64) {
        let rise = BEAM_UP_TICKS - RESCUE_DESCENT_TICKS;
        for survivor in &mut self.out {
            if let (Phase::Riding | Phase::Taken, Some(spaceling)) =
                (survivor.phase, &survivor.spaceling)
            {
                survivor.position = world.motion(spaceling.body()).expect("a rider").position;
            }
            let Some((body, from)) = survivor.lifted else {
                continue;
            };
            let t = ((phase_tick + 1).saturating_sub(RESCUE_DESCENT_TICKS) as f32 / rise as f32)
                .min(1.0);
            let position = from.position + Vec2::Y * (RESCUE_RISE * self.scale * t);
            world.set_next_kinematic_pose(body, position, from.angle);
            survivor.position = position;
        }
    }

    /// How far into the beam-up a survivor its own beam lifts rises and
    /// fades, at `phase_tick` ticks: none while its beam comes down, to
    /// gone.
    pub fn rescued(phase_tick: u64) -> f32 {
        let rise = BEAM_UP_TICKS - RESCUE_DESCENT_TICKS;
        (phase_tick.saturating_sub(RESCUE_DESCENT_TICKS) as f32 / rise as f32).min(1.0)
    }

    /// Where the winner's turret turns: onto a spaceling that surrenders,
    /// from when its flag goes up, and as the winner's beam draws it in.
    pub fn target(&self) -> Option<Vec2> {
        self.out
            .iter()
            .find(|survivor| matches!(survivor.phase, Phase::Surrendering | Phase::Taken))
            .map(|survivor| survivor.position)
    }

    /// The spaceling the winner's beam has taken, if any.
    pub fn taken(&self) -> Option<&Survivor> {
        self.out
            .iter()
            .find(|survivor| survivor.phase == Phase::Taken)
    }

    /// As the hills withdraw, the survivors have gone up the beams. Answers
    /// the empty pods, which go with the hills.
    pub fn withdraw(&mut self, world: &mut PhysicsWorld) -> Vec<BodyId> {
        let mut pods = Vec::new();
        for survivor in &mut self.out {
            if let Some(spaceling) = survivor.spaceling.take() {
                world.remove_entity(spaceling.body().entity);
            }
            if survivor.pod_lifted()
                && let Some(pod) = survivor.pod.take()
            {
                world.remove_entity(pod.entity);
            }
            pods.extend(survivor.pod);
        }
        pods
    }

    /// Removes the spacelings and pods still here, for a fight cut short.
    pub fn release(&mut self, world: &mut PhysicsWorld) {
        for survivor in &mut self.out {
            if let Some(spaceling) = survivor.spaceling.take() {
                world.remove_entity(spaceling.body().entity);
            }
            if let Some(pod) = survivor.pod.take() {
                world.remove_entity(pod.entity);
            }
        }
    }

    /// The spacelings and pods in the arena.
    pub fn bodies(&self) -> usize {
        self.out
            .iter()
            .map(|survivor| {
                usize::from(survivor.spaceling.is_some()) + usize::from(survivor.pod.is_some())
            })
            .sum()
    }

    /// The survivor from the tank in `slot`, for `clock state`.
    pub fn state(&self, slot: usize) -> Option<ClockTanksSurvivor> {
        let survivor = self.get(slot)?;
        Some(ClockTanksSurvivor {
            ejection: survivor.ejection,
            ending: survivor.ending,
            drawn: survivor.drawn,
            fallback: survivor.fallback,
            phase: survivor.phase,
            position_milli: [survivor.position.x, survivor.position.y]
                .map(|v| (v * 1000.0).round() as i32),
            stood_up: survivor.stood_up,
            knockdowns: self.knockdowns[slot] as u32,
        })
    }
}
