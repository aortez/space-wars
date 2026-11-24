//! A loser's spaceling getting out as its tank blows up, and the beam that
//! rescues it.
//!
//! One loser in five gets out, by a roll the fight's seed gives each side
//! from a stream of its own. Half of those the blast throws clear, tumbling,
//! and they land, settle and get up as the physics crate's spaceling does.
//! The rest ride out in an escape pod that arcs away like a shell, lands and
//! may tip. A moment after it comes to rest, it opens and the spaceling
//! climbs out. Once up, the spaceling looks around and waves, then a beam
//! comes down on it and lifts it out as the winner beams up. The empty pod
//! stays until the hills withdraw, and goes with them.
//!
//! The spacelings and the pods meet the hills, the dirt, the floor and the
//! winner, and pass the pieces of the wreck, the face and one another. One
//! that leaves the view retires there. A spaceling still down when its time
//! runs out is lifted out as it lies.
use super::*;
use engine_common::{ClockTanksEnding, ClockTanksSurvivor};
use engine_rapier::{
    spaceling::{
        SpacelingAssembly, SpacelingBalance, SpacelingBalanceSpec, SpacelingControl,
        SpacelingGetUpResult, SpacelingSpec,
    },
    world::{BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole, ColliderSpec},
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use scenario_scorched_earth::host::TankState;
use wreck::Puff;

pub use engine_common::ClockTanksEjection as Ejection;
pub use engine_common::ClockTanksSurvivorPhase as Phase;

/// The bodies the survivors take in the arena: each side's spaceling and its
/// pod.
pub const MAX_SURVIVOR_BODIES: usize = 4;
/// The longest the aftermath lasts, from the end of the blow-up until the
/// rescue.
pub const AFTERMATH_TICKS: u64 = 6 * FIXED_HZ as u64;
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
const DT: f32 = 1.0 / FIXED_HZ as f32;

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

/// A loser's spaceling that got out.
pub struct Survivor {
    /// The tank it came from.
    pub slot: usize,
    pub ejection: Ejection,
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
    /// Whether it has stood up since it got out.
    pub stood_up: bool,
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
}

/// The losers' spacelings that got out, from the blast to the rescue.
pub struct Survivors {
    /// How each side's spaceling gets out if its tank blows up.
    pub rolls: [Option<Ejection>; 2],
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
    /// last stood, gets out if its roll says so. It leaves toward the middle
    /// of the view, and falls as the duel's dirt does, by `gravity_scale`.
    pub fn eject(
        &mut self,
        world: &mut PhysicsWorld,
        slot: usize,
        tank: &TankState,
        gravity_scale: f32,
        tick: u64,
    ) {
        let Some(ejection) = self.rolls[slot] else {
            return;
        };
        self.gravity_scale = gravity_scale;
        let scale = self.scale;
        let hull = tank.motion;
        let [hatch, _] = tank.barrel;
        let arm = hatch - hull.position;
        let carried = hull.linear_velocity + Vec2::new(-arm.y, arm.x) * hull.angular_velocity;
        let inward = if hull.position.x > 0.0 { -1.0 } else { 1.0 };
        let mut survivor = Survivor {
            slot,
            ejection,
            phase: Phase::Flying,
            blast_tick: tick,
            since: tick,
            spaceling: None,
            pod: None,
            facing: inward,
            position: hatch,
            lifted: None,
            stood_up: false,
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
    /// the way it faces and then back the other way.
    pub fn control(&mut self, world: &mut PhysicsWorld, gravity: Vec2, tick: u64) {
        let settle = spec(self.scale).balance;
        for survivor in &mut self.out {
            if matches!(survivor.phase, Phase::BeamedUp | Phase::Lost) {
                continue;
            }
            let Some(spaceling) = &mut survivor.spaceling else {
                continue;
            };
            let snapshot = spaceling.snapshot(world).expect("a survivor's spaceling");
            let press = snapshot.needs_get_up()
                && snapshot.contacts > 0
                && snapshot.motion.linear_velocity.length() < settle.settle_speed
                && snapshot.motion.angular_velocity.abs() < settle.settle_angular_speed
                && survivor
                    .pressed
                    .is_none_or(|pressed| tick - pressed >= PRESS_TICKS);
            if press {
                survivor.pressed = Some(tick);
            }
            let walk = if snapshot.needs_get_up()
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
            spaceling.apply_control(
                world,
                SpacelingControl {
                    walk,
                    jump_held: press,
                },
                gravity,
                DT,
            );
        }
    }

    /// Follows the survivors after the arena's step at `tick`. A pod opens
    /// once it has lain still a moment, and its spaceling climbs out. A
    /// spaceling that has found its feet looks around, then waves. A
    /// spaceling, or a pod with its spaceling still in it, that leaves the
    /// view retires there.
    pub fn observe(&mut self, world: &mut PhysicsWorld, layout: Layout, tick: u64) {
        let scale = self.scale;
        let outside =
            |point: Vec2| point.y < layout.bounds_min.y || point.x.abs() > layout.bounds_max.x;
        for index in 0..self.out.len() {
            let survivor = &mut self.out[index];
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
                Phase::Flying
                | Phase::GettingUp
                | Phase::ClimbingOut
                | Phase::LookingAround
                | Phase::Waving => {
                    let spaceling = survivor.spaceling.as_ref().expect("a spaceling out");
                    let snapshot = spaceling.snapshot(world).expect("a survivor's spaceling");
                    survivor.position = snapshot.motion.position;
                    self.knockdowns[survivor.slot] = snapshot.knockdowns;
                    if outside(snapshot.motion.position) {
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
                    survivor.still = if standing { survivor.still + 1 } else { 0 };
                    let still = survivor.still;
                    match survivor.phase {
                        Phase::Flying | Phase::GettingUp | Phase::ClimbingOut
                            if still >= STAND_TICKS =>
                        {
                            survivor.stood_up = true;
                            survivor.enter(Phase::LookingAround, tick);
                        }
                        Phase::Flying if snapshot.contacts > 0 => {
                            survivor.enter(Phase::GettingUp, tick);
                        }
                        // Knocked over again, it gets up again.
                        Phase::LookingAround | Phase::Waving if snapshot.needs_get_up() => {
                            survivor.enter(Phase::GettingUp, tick);
                        }
                        Phase::LookingAround => {
                            let looked = tick - survivor.since;
                            if looked >= LOOK_TICKS {
                                survivor.enter(Phase::Waving, tick);
                            } else if looked == LOOK_TICKS / 3 || looked == 2 * LOOK_TICKS / 3 {
                                survivor.facing = -survivor.facing;
                            }
                        }
                        _ => (),
                    }
                }
                Phase::BeamedUp | Phase::Lost => (),
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

    /// Whether every survivor has waved long enough for its beam at `tick`,
    /// or is lost, so the aftermath can end. Without a survivor, it can.
    pub fn ready(&self, tick: u64) -> bool {
        self.out.iter().all(|survivor| match survivor.phase {
            Phase::Lost | Phase::BeamedUp => true,
            Phase::Waving => tick - survivor.since >= WAVE_TICKS,
            _ => false,
        })
    }

    /// Starts the beams that lift out the survivors still in view at `tick`,
    /// each spaceling standing or as it lies, or in its pod if it never
    /// climbed out. Answers whether there are any.
    pub fn begin_rescue(&mut self, world: &mut PhysicsWorld, tick: u64) -> bool {
        let mut any = false;
        for survivor in &mut self.out {
            if matches!(survivor.phase, Phase::Lost | Phase::BeamedUp) {
                continue;
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
    /// arena's step: once its beam has come down, up and out.
    pub fn lift(&mut self, world: &mut PhysicsWorld, phase_tick: u64) {
        let rise = BEAM_UP_TICKS - RESCUE_DESCENT_TICKS;
        for survivor in &mut self.out {
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

    /// How far into the beam-up a survivor rises and fades, at `phase_tick`
    /// ticks: none while its beam comes down, to gone.
    pub fn rescued(phase_tick: u64) -> f32 {
        let rise = BEAM_UP_TICKS - RESCUE_DESCENT_TICKS;
        (phase_tick.saturating_sub(RESCUE_DESCENT_TICKS) as f32 / rise as f32).min(1.0)
    }

    /// As the hills withdraw, the beamed-up survivors go. Answers the empty
    /// pods, which go with the hills.
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
            ending: ClockTanksEnding::RescueBeam,
            phase: survivor.phase,
            position_milli: [survivor.position.x, survivor.position.y]
                .map(|v| (v * 1000.0).round() as i32),
            stood_up: survivor.stood_up,
            knockdowns: self.knockdowns[slot] as u32,
        })
    }
}
