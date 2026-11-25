//! A bounded Scorched duel in Clock's canonical arena. Only Clock steps physics.
use super::{EventContext, EventPhase};
use crate::{
    arena::{ClockArena, Owner, Population, Unavailable, ids::canonical},
    digit_surfaces::{SurfaceId, SurfaceLedger, SurfacePose},
    layout::Layout,
};
use engine_common::{ClockEventKind, ClockTanksState};
use engine_core::Vec2;
use engine_rapier::{
    terrain::{GrainSeed, GrainShape, RadialImpulse, TerrainGrain, TerrainSpec},
    world::{BodyId, BodyKind, BodyMotion, CollisionGroups, PhysicsId, PhysicsWorld},
};
use engine_terrain::{Cell, MaterialId};
use scenario_scorched_earth::{
    BankStrength, CpuPolicy, FIXED_HZ, FiringRange, SHELL_LIFETIME_TICKS, ScorchedConfig,
    host::{Bounds, Budget, HostedDuel, Placement},
};

pub const RAISE_TICKS: u64 = 2 * FIXED_HZ as u64;
pub const BEAM_DOWN_TICKS: u64 = 2 * FIXED_HZ as u64;
pub const SETTLE_TICKS: u64 = 2 * FIXED_HZ as u64;
pub const COMBAT_TICKS: u64 = 60 * FIXED_HZ as u64;
pub const BEAM_UP_TICKS: u64 = 2 * FIXED_HZ as u64;
/// How far the winner's beam lifts it, in the duel's units.
pub const BEAM_UP_RISE: f32 = 12.0;
pub const WITHDRAW_TICKS: u64 = 3 * FIXED_HZ as u64;
pub const MAX_TICKS: u64 = RAISE_TICKS
    + 2 * BEAM_DOWN_TICKS
    + SETTLE_TICKS
    + COMBAT_TICKS
    + SHELL_LIFETIME_TICKS as u64
    + BLOW_UP_TICKS
    + PARTING_TICKS
    + BEAM_UP_TICKS
    + WITHDRAW_TICKS;
pub const REUSE_TICKS: u64 = 120 * FIXED_HZ as u64;
pub const MAX_GRAINS: usize = 192;
/// Authored conversion: 24 half-unit square cells (six native area units).
pub const DIRT_PER_TANK: usize = 24;
const OWNER: Owner = Owner::Event(ClockEventKind::Tanks);
const BUDGET: Budget = Budget::for_terrain(
    MAX_GRAINS,
    true,
    scenario_scorched_earth::host::MAX_TERRAIN_WIDTH,
);
/// The duel's budget, the face's fixed body of boxes, the cells the shells
/// knock loose, the pieces the losers blow apart into, and the spacelings
/// that get out and their pods.
pub const POPULATION: Population = Population {
    bodies: BUDGET.bodies + 1 + MAX_LOOSE + MAX_PIECES + MAX_SURVIVOR_BODIES,
    colliders: BUDGET.colliders
        + digits::MAX_CELLS
        + MAX_LOOSE
        + MAX_PIECE_COLLIDERS
        + MAX_SURVIVOR_BODIES,
    joints: BUDGET.joints,
};

/// How far apart the CPUs fight, in the duel's units. A learning CPU's 50°
/// arc rises about 0.3 of the range, and the face starts about 17 units over
/// the hills, so at Scorched Earth's own 18 to 32 units every shell passes
/// below it. From about 72 units apart, the arcs meet the digits beside the
/// colon.
const RANGE: FiringRange = FiringRange {
    near: 72.0,
    far: 88.0,
};
/// Where each tank lands when the reading leaves a gap there, in the duel's
/// units from the middle: half the middle of the range.
const LANDING_X: f32 = (RANGE.near + RANGE.far) / 4.0;
/// The longest the tanks drive to their range before fire opens anyway.
const OPENING_TICKS: u64 = 12 * FIXED_HZ as u64;
/// How long the cells a parting shell knocks loose lie before they go home.
const LIE_TICKS: u64 = FIXED_HZ as u64;

pub fn config(shape: GrainShape, seed: u64) -> ScorchedConfig {
    ScorchedConfig {
        shape,
        max_grains: MAX_GRAINS,
        demo: true,
        cpu_dodging: dodging(seed),
        cpu_driving: true,
        cpu_policy: policies(seed),
        cpu_range: RANGE,
        // Every three seeds the other side fires first, so each pairing opens
        // from both sides.
        first_to_fire: (seed / 3 % 2) as usize,
        slumping: true,
        cohesion: BankStrength::Weak,
        suspension: true,
        ..Default::default()
    }
}

/// The CPUs a fight's seed sends in: two that plot each shot, or one that
/// plots against one that learns from its shots, on either side.
pub fn policies(seed: u64) -> [CpuPolicy; 2] {
    match seed % 3 {
        0 => [CpuPolicy::Plotted; 2],
        1 => [CpuPolicy::Plotted, CpuPolicy::Reactive],
        _ => [CpuPolicy::Reactive, CpuPolicy::Plotted],
    }
}

/// Which of the CPUs a fight's seed sends in dodge after a hit. Each side
/// dodges in half the fights, so every 24 seeds each pairing opens from either
/// side with each mix of dodgers.
pub fn dodging(seed: u64) -> [bool; 2] {
    [seed / 6 % 2 == 1, seed / 12 % 2 == 1]
}

mod damage;
mod digits;
mod entry;
pub mod survivor;
pub mod wreck;
pub use damage::MAX_LOOSE;
pub use entry::placement;
pub use survivor::{MAX_SURVIVOR_BODIES, PARTING_TICKS};
pub use wreck::{BLOW_UP_TICKS, MAX_PIECE_COLLIDERS, MAX_PIECES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    Left,
    Right,
    Draw,
    Neither,
}
impl ResultKind {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "left wins",
            Self::Right => "right wins",
            Self::Draw => "draw",
            Self::Neither => "no survivors",
        }
    }
    fn winners(self) -> [bool; 2] {
        match self {
            Self::Left => [true, false],
            Self::Right => [false, true],
            Self::Draw => [true, true],
            Self::Neither => [false, false],
        }
    }
    pub fn from_health(health: [f32; 2]) -> Self {
        match (health[0] > 0.0, health[1] > 0.0) {
            (false, false) => Self::Neither,
            (true, false) => Self::Left,
            (false, true) => Self::Right,
            (true, true) if health[0] == health[1] => Self::Draw,
            (true, true) if health[0] > health[1] => Self::Left,
            _ => Self::Right,
        }
    }
}

pub struct TanksEvent {
    pub seed: u64,
    pub tick: u64,
    pub phase: EventPhase,
    pub phase_tick: u64,
    pub duel: HostedDuel,
    /// The dirt the losers' pieces have turned into.
    pub converted: Vec<TerrainGrain>,
    /// The losers, blown apart.
    pub wreck: wreck::Wreck,
    /// The losers' spacelings that get out.
    pub survivors: survivor::Survivors,
    pub result: Option<ResultKind>,
    combat_ticks: u64,
    /// The combat tick fire opened on.
    fire_opened: Option<u64>,
    /// Shells the face has stopped.
    face_hits: u32,
    layout: Layout,
    digits: digits::Solids,
    damage: damage::Damage,
    /// The CPUs' names, as Scorched Earth's CPU button shows them.
    cpu: [&'static str; 2],
    ground: BodyId,
    spawn: [(BodyId, BodyMotion); 2],
    /// Each tank's bodies with their offsets from its hull, which the beam
    /// carries together: the hull and its two sprung wheels.
    parts: [Vec<(BodyId, Vec2)>; 2],
    departure: Vec<(BodyId, BodyMotion)>,
    withdrawal: f32,
    final_health: Option<[f32; 2]>,
    retired_tank_mass: f32,
    entry_failure: Option<&'static str>,
    entry_restarts: u8,
}

impl TanksEvent {
    pub fn new(
        arena: &mut ClockArena,
        layout: Layout,
        display: crate::digits::DisplaySnapshot,
        seed: u64,
    ) -> Result<Self, Unavailable> {
        let mut owners = arena.owners().to_vec();
        owners.push(OWNER);
        arena.admit(&owners)?;
        // Recheck immediately before insertion, including residents admitted
        // between selection and this boundary.
        let place = placement(layout, display, arena.world()).ok_or(Unavailable::Placement)?;
        let shape = if seed & 1 == 0 {
            GrainShape::Round
        } else {
            GrainShape::Hexagon
        };
        let world = arena.claim_resident(OWNER, layout);
        let config = config(shape, seed);
        let cpu = [config.cpu_label(0), config.cpu_label(1)];
        let mut duel = match HostedDuel::attach(world, config, seed, place) {
            Ok(duel) => duel,
            Err(_) => {
                arena.release(OWNER);
                return Err(Unavailable::Capacity);
            }
        };
        duel.set_firing(false);
        duel.set_driving(false);
        let scale = duel.placement().scale;
        let ground = duel.terrain()[0].assembly.body();
        let spawn = duel.tanks(world).map(|tank| (tank.body, tank.motion));
        let parts: [Vec<(BodyId, Vec2)>; 2] = std::array::from_fn(|slot| {
            let hull = spawn[slot].1.position;
            duel.tank_bodies(slot)
                .map(|body| {
                    let motion = world.motion(body).expect("a tank's body");
                    (body, motion.position - hull)
                })
                .collect()
        });
        world.set_body_kind(ground, BodyKind::KinematicPosition, false);
        world.set_pose(
            ground,
            duel.placement().origin - Vec2::Y * 18.0 * scale,
            0.0,
            false,
        );
        // The tanks wait above the view, still enabled. The next step joins
        // each wheel's new spring to its hull's island, and Rapier asserts
        // that neither body is disabled when it does.
        for (slot, (_, motion)) in spawn.into_iter().enumerate() {
            let hull = Vec2::new(motion.position.x, layout.bounds_max.y + 3.0 * scale);
            for &(body, offset) in &parts[slot] {
                world.set_body_kind(body, BodyKind::KinematicPosition, false);
                world.set_pose(body, hull + offset, motion.angle, false);
            }
        }
        let damage = damage::Damage::new(seed);
        let mut digits = digits::Solids::default();
        digits.synchronize(world, layout, display, damage.standing());
        Ok(Self {
            seed,
            tick: 0,
            phase: EventPhase::Raising,
            phase_tick: 0,
            duel,
            converted: Vec::new(),
            wreck: wreck::Wreck::new(seed, scale, shape),
            survivors: survivor::Survivors::new(seed, scale),
            result: None,
            combat_ticks: 0,
            fire_opened: None,
            face_hits: 0,
            layout,
            digits,
            damage,
            cpu,
            ground,
            spawn,
            parts,
            departure: Vec::new(),
            withdrawal: 0.0,
            final_health: None,
            retired_tank_mass: 0.0,
            entry_failure: None,
            entry_restarts: 0,
        })
    }

    fn enter(&mut self, phase: EventPhase) {
        self.phase = phase;
        self.phase_tick = 0;
    }

    fn entry_top(&self) -> f32 {
        self.layout.bounds_max.y + 3.0 * self.duel.placement().scale
    }

    pub fn landing(&self, slot: usize) -> Vec2 {
        self.spawn[slot].1.position
    }

    /// Keeps the event to the reading: the damaged digits and the face's
    /// boxes throughout, and the entry lanes while the tanks arrive.
    pub fn synchronize(
        &mut self,
        arena: &mut ClockArena,
        display: crate::digits::DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) {
        let world = arena.held_mut(OWNER).expect("Tanks holds the arena");
        self.damage
            .synchronize(world, self.layout, display, surfaces);
        self.digits
            .synchronize(world, self.layout, display, self.damage.standing());
        self.synchronize_entry(arena, display);
    }

    /// A minute/font change can close a previously clear lane. Restart that
    /// transport once from the top; repeated changes still have a bounded exit.
    fn synchronize_entry(
        &mut self,
        arena: &mut ClockArena,
        display: crate::digits::DisplaySnapshot,
    ) {
        if !matches!(self.phase, EventPhase::Raising | EventPhase::BeamDown) {
            return;
        }
        let scale = self.duel.placement().scale;
        if self
            .spawn
            .iter()
            .all(|(_, pose)| entry::clear(self.layout, display, scale, pose.position.x))
        {
            return;
        }
        let world = arena.held_mut(OWNER).expect("Tanks holds the arena");
        let lanes = entry::lanes(self.layout, display, Some(world), scale);
        if lanes.is_none() || (self.phase == EventPhase::BeamDown && self.entry_restarts == 1) {
            self.entry_failure = Some("The current reading closed the tank entry lanes");
            self.begin_withdrawal(world);
            return;
        }
        let mut place = self.duel.placement().clone();
        place.spawn_x = lanes.unwrap().map(|x| x / scale);
        for (slot, position) in place.landing_positions(self.seed).into_iter().enumerate() {
            self.spawn[slot].1.position = position;
            let hull = Vec2::new(position.x, self.entry_top());
            for &(body, offset) in &self.parts[slot] {
                world.set_pose(body, hull + offset, 0.0, false);
                world.set_velocity(body, Vec2::ZERO, 0.0, false);
            }
        }
        if self.phase == EventPhase::BeamDown {
            self.entry_restarts += 1;
            self.phase_tick = 0;
        }
    }

    pub fn step(
        &mut self,
        arena: &mut ClockArena,
        display: crate::digits::DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) -> bool {
        self.synchronize(arena, display, surfaces);
        let world = arena.held_mut(OWNER).expect("Tanks holds the arena");
        let scale = self.duel.placement().scale;
        match self.phase {
            EventPhase::Raising => {
                let t = (self.phase_tick + 1) as f32 / RAISE_TICKS as f32;
                world.set_next_kinematic_pose(
                    self.ground,
                    self.duel.placement().origin - Vec2::Y * (18.0 * scale * (1.0 - t)),
                    0.0,
                );
            }
            EventPhase::BeamDown => {
                let t = (self.phase_tick + 1) as f32 / BEAM_DOWN_TICKS as f32;
                for (slot, (_, motion)) in self.spawn.into_iter().enumerate() {
                    let hull = Vec2::new(
                        motion.position.x,
                        self.entry_top() * (1.0 - t) + motion.position.y * t,
                    );
                    for &(body, offset) in &self.parts[slot] {
                        world.set_next_kinematic_pose(body, hull + offset, motion.angle);
                    }
                }
            }
            EventPhase::BeamUp | EventPhase::Withdrawing => {
                let (duration, direction, distance) = if self.phase == EventPhase::BeamUp {
                    (BEAM_UP_TICKS, 1.0, BEAM_UP_RISE * scale)
                } else {
                    (WITHDRAW_TICKS, -1.0, self.withdrawal)
                };
                let t = (self.phase_tick + 1) as f32 / duration as f32;
                for &(body, motion) in &self.departure {
                    world.set_next_kinematic_pose(
                        body,
                        motion.position + Vec2::Y * (direction * distance * t),
                        motion.angle,
                    );
                }
            }
            _ => (),
        }
        match self.phase {
            EventPhase::BlowingUp | EventPhase::Aftermath => {
                let gravity = world.gravity() * self.duel.gravity_scale(world);
                let winner = self.winner(world);
                self.survivors.control(world, gravity, self.tick, winner);
                // The winner's turret turns onto a spaceling that
                // surrenders, and keeps it covered as its beam draws it in,
                // and onto one it takes its parting shot at.
                if let Some(winner) = winner
                    && let Some(target) = self.survivors.target(winner.barrel[0])
                {
                    self.duel
                        .point_barrel(winner.slot, survivor::turn_barrel(winner.barrel, target));
                }
            }
            EventPhase::BeamUp => self.survivors.lift(world, self.phase_tick),
            _ => (),
        }
        if self.simulates_duel() {
            let drives = self
                .duel
                .prepare_step(world)
                .expect("one preparation per Clock tick");
            arena.queue_drives(drives);
        }
        false
    }

    pub fn observe(&mut self, context: EventContext<'_>, arena: &mut ClockArena) -> bool {
        let EventContext {
            segments,
            surfaces,
            display,
            ..
        } = context;
        let world = arena.held_mut(OWNER).expect("Tanks holds the arena");
        let scale = self.duel.placement().scale;
        if self.simulates_duel() {
            self.duel
                .finish_step(world)
                .expect("one completion per Clock tick");
            // Loose cells fall as the dirt does.
            let gravity = self.duel.gravity_scale(world);
            for impact in self.duel.impacts() {
                // A parting shell's burst throws what the duel does not own.
                if self.result.is_some() {
                    self.survivors.shove(world, impact.point);
                    RadialImpulse {
                        center: impact.point,
                        radius: survivor::PUSH_REACH * scale,
                        speed: survivor::PUSH_SPEED * scale,
                    }
                    .apply(world, self.converted.iter().map(|grain| grain.body()));
                }
                if impact.collider.entity == digits::BODY {
                    self.face_hits += 1;
                    self.damage.knock_loose(
                        world,
                        self.layout,
                        segments,
                        surfaces,
                        gravity,
                        impact.point,
                        impact.velocity,
                    );
                }
            }
        }
        self.damage.observe(world, self.layout);
        self.damage.advance(segments, surfaces);
        self.tick += 1;
        self.phase_tick += 1;
        match self.phase {
            EventPhase::Raising if self.phase_tick == RAISE_TICKS => {
                world.set_body_kind(self.ground, BodyKind::Fixed, false);
                world.set_velocity(self.ground, Vec2::ZERO, 0.0, false);
                self.enter(EventPhase::BeamDown);
            }
            EventPhase::BeamDown if self.phase_tick == BEAM_DOWN_TICKS => {
                for &(body, _) in self.parts.iter().flatten() {
                    world.set_body_kind(body, BodyKind::Dynamic, true);
                    world.set_velocity(body, Vec2::ZERO, 0.0, true);
                }
                self.enter(EventPhase::Settling);
            }
            EventPhase::Settling => {
                let ready = self.duel.tanks(world).iter().all(|tank| {
                    tank.supported
                        && tank.health > 0.0
                        && tank.motion.angle.cos() > 0.7
                        && tank.motion.linear_velocity.length() < 0.6 * self.duel.placement().scale
                });
                if ready {
                    // Fire opens once the tanks hold range.
                    self.duel.set_driving(true);
                    self.enter(EventPhase::Fighting);
                } else if self.phase_tick == SETTLE_TICKS {
                    self.entry_failure = Some("Tanks could not settle on supported positions");
                    self.begin_withdrawal(world);
                }
            }
            EventPhase::Fighting => {
                self.combat_ticks += 1;
                self.open_fire(world);
                if self.combat_ticks == COMBAT_TICKS
                    || self
                        .duel
                        .tanks(world)
                        .iter()
                        .filter(|tank| tank.health > 0.0)
                        .count()
                        <= 1
                {
                    self.duel.set_firing(false);
                    self.duel.set_driving(false);
                    self.enter(EventPhase::FinishingShots);
                    if self.duel.active_shells() == 0 {
                        self.resolve(world);
                    }
                }
            }
            EventPhase::FinishingShots if self.duel.active_shells() == 0 => self.resolve(world),
            EventPhase::BlowingUp => {
                let ending = self.phase_tick == BLOW_UP_TICKS;
                let winner = self.winner(world);
                // Were the wreck done now, the aftermath would end this long
                // from now at the latest.
                let deadline = self.tick + self.survivors.aftermath_ticks();
                self.survivors.observe(
                    world,
                    &mut self.duel,
                    self.layout,
                    self.tick,
                    winner,
                    deadline,
                );
                if !self
                    .wreck
                    .observe(world, self.layout, self.tick, ending, &mut self.converted)
                {
                    // A survivor still getting up, looking around, waving,
                    // running or surrendering has the aftermath to finish in,
                    // as does a parting shell still in the air.
                    if self.survivors.ready(self.tick) {
                        self.begin_beam_up(world);
                    } else {
                        self.enter(EventPhase::Aftermath);
                    }
                }
            }
            EventPhase::Aftermath => {
                let winner = self.winner(world);
                let longest = self.survivors.aftermath_ticks();
                let deadline = self.tick - self.phase_tick + longest;
                self.survivors.observe(
                    world,
                    &mut self.duel,
                    self.layout,
                    self.tick,
                    winner,
                    deadline,
                );
                if self.survivors.ready(self.tick) || self.phase_tick >= longest {
                    self.begin_beam_up(world);
                }
            }
            EventPhase::BeamUp if self.phase_tick == BEAM_UP_TICKS => self.begin_withdrawal(world),
            EventPhase::Withdrawing if self.phase_tick == WITHDRAW_TICKS => {
                debug_assert!(self.damage.whole(), "every cell is home");
                return true;
            }
            _ => (),
        }
        // Once the last shell has landed, every damaged digit comes home. The
        // cells a parting shell knocks loose lie a moment first.
        if self.result.is_some() && !self.survivors.shelling(self.tick, LIE_TICKS) {
            let world = arena.held_mut(OWNER).expect("Tanks holds the arena");
            self.damage
                .bring_home(world, self.layout, display, surfaces);
        }
        debug_assert!(self.tick <= MAX_TICKS);
        false
    }

    /// Opens fire once the tanks stand in range, or when the opening runs out
    /// with them still driving to it.
    fn open_fire(&mut self, world: &PhysicsWorld) {
        if self.fire_opened.is_some() {
            return;
        }
        let place = self.duel.placement();
        let [left, right] = self
            .duel
            .tanks(world)
            .map(|tank| place.point_from_world(tank.motion.position).x);
        if RANGE.holds((right - left).abs()) || self.combat_ticks >= OPENING_TICKS {
            self.duel.set_firing(true);
            self.fire_opened = Some(self.combat_ticks);
        }
    }

    fn resolve(&mut self, world: &mut PhysicsWorld) {
        assert!(self.result.is_none() && self.duel.active_shells() == 0);
        let tanks = self.duel.tanks(world);
        let health = tanks.map(|tank| tank.health);
        let result = ResultKind::from_health(health);
        self.result = Some(result);
        self.final_health = Some(health);
        let winners = result.winners();
        let winner_in_view = self.winner(world).is_some();
        let mut ids = self.duel.conversion_ids();
        let gravity_scale = self.duel.gravity_scale(world);
        for (slot, tank) in tanks
            .into_iter()
            .enumerate()
            .filter(|(slot, _)| !winners[*slot])
        {
            self.retired_tank_mass += world.body_mass(tank.body).unwrap_or(0.0);
            let gear = self
                .duel
                .running_gear(world, slot)
                .expect("a loser's running gear");
            self.duel
                .retire_tank(world, slot)
                .expect("post-step retirement");
            if tank.motion.position.y < self.layout.bounds_min.y
                || tank.motion.position.x.abs() > self.layout.bounds_max.x
            {
                continue;
            }
            let loser = wreck::Loser { slot, tank, gear };
            self.wreck
                .blow_up(world, loser, gravity_scale, self.tick, &mut ids);
            self.survivors
                .eject(world, slot, &tank, winner_in_view, gravity_scale, self.tick);
        }
        if self.wreck.pieces.is_empty() {
            self.begin_beam_up(world);
        } else {
            self.enter(EventPhase::BlowingUp);
        }
    }

    // Once the last shell resolves, Rapier continues the physical aftermath,
    // and the duel steps again only while the winner's parting shell flies.
    // Its CPUs neither aim nor drive by then, so nothing changes the latched
    // result, and nothing makes bodies once the withdrawal has captured them.
    fn simulates_duel(&self) -> bool {
        self.phase != EventPhase::Withdrawing
            && (self.result.is_none() || self.duel.active_shells() > 0)
    }

    /// The winner, once the fight is over, while it stands in view: none
    /// after a draw or a fight without one.
    fn winner(&self, world: &PhysicsWorld) -> Option<survivor::Winner> {
        let slot = match self.result? {
            ResultKind::Left => 0,
            ResultKind::Right => 1,
            ResultKind::Draw | ResultKind::Neither => return None,
        };
        let tank = self.duel.tanks(world)[slot];
        let position = tank.motion.position;
        if !tank.present
            || position.y < self.layout.bounds_min.y
            || position.x.abs() > self.layout.bounds_max.x
        {
            return None;
        }
        let bounds = tank.bounds?;
        let gear = self.duel.running_gear(world, slot)?;
        Some(survivor::Winner {
            slot,
            hull: tank.motion,
            half: gear.hull_half_extents,
            bounds: [bounds.min, bounds.max],
            barrel: tank.barrel,
        })
    }

    fn begin_beam_up(&mut self, world: &mut PhysicsWorld) {
        debug_assert!(self.wreck.pieces.is_empty(), "every piece is dirt");
        self.departure.clear();
        let winners = self.result.expect("result settled").winners();
        for (slot, tank) in self.duel.tanks(world).into_iter().enumerate() {
            if winners[slot] && tank.present {
                for body in self.duel.tank_bodies(slot) {
                    if let Some(motion) = world.motion(body) {
                        world.set_body_kind(body, BodyKind::KinematicPosition, true);
                        self.departure.push((body, motion));
                    }
                }
            }
        }
        // A stowaway on the winner's roof, or a spaceling its beam has taken,
        // goes up with the winner, and the other survivors' beams come down
        // as the winner's lifts it.
        let riders = self.survivors.riders(world, self.tick);
        self.departure.extend(riders);
        let winner = self.winner(world);
        let rescued = self.survivors.begin_rescue(world, self.tick, winner);
        if self.departure.is_empty() && !rescued {
            self.begin_withdrawal(world);
        } else {
            self.enter(EventPhase::BeamUp);
        }
    }

    fn begin_withdrawal(&mut self, world: &mut PhysicsWorld) {
        self.duel.set_driving(false);
        self.duel.set_firing(false);
        for slot in 0..2 {
            self.duel
                .retire_tank(world, slot)
                .expect("post-step retirement");
        }
        // The survivors have gone up their beams; their empty pods go with
        // the hills.
        let pods = self.survivors.withdraw(world);
        self.departure = self
            .duel
            .body_ids()
            .chain(self.converted.iter().map(|g| g.body()))
            .chain(pods)
            .filter_map(|body| world.motion(body).map(|motion| (body, motion)))
            .collect();
        let highest = self
            .departure
            .iter()
            .filter_map(|(body, _)| world.body_solid_bounds(*body).map(|(_, max)| max.y))
            .fold(self.layout.floor_y, f32::max);
        self.withdrawal = highest - self.layout.bounds_min.y + 4.0 * self.duel.placement().scale;
        for &(body, _) in &self.departure {
            world.set_body_kind(body, BodyKind::KinematicPosition, true);
        }
        self.enter(EventPhase::Withdrawing);
    }

    pub fn release(&mut self, arena: &mut ClockArena) {
        if let Some(world) = arena.held_mut(OWNER) {
            self.damage.release(world);
            self.wreck.release(world);
            self.survivors.release(world);
            self.duel.retire(world);
        }
        self.converted.clear();
        arena.release(OWNER);
    }

    /// The surfaces of the digits the shells have damaged, and of the cells
    /// they knocked loose.
    pub fn surfaces(&self, layout: Layout, out: &mut Vec<(SurfaceId, SurfacePose)>) {
        // A resize ends the event, so the face never moves under it.
        debug_assert_eq!(layout.pitch, self.layout.pitch);
        self.damage.surfaces(self.layout, out);
    }

    /// Gives the face back the digits the event still holds.
    pub fn release_surfaces(&mut self, surfaces: &mut SurfaceLedger) {
        self.damage.release_surfaces(surfaces);
    }

    /// The digit cells the event draws for the face, as (middle, side,
    /// angle).
    pub fn face_cells(&self) -> Vec<(Vec2, f32, f32)> {
        self.damage.cells(self.layout)
    }

    pub fn diagnostics(&self, arena: &ClockArena) -> ClockTanksState {
        let world = arena.held(OWNER).expect("Tanks holds the arena");
        let tanks = self.duel.tanks(world);

        ClockTanksState {
            seed: self.seed,
            tick: self.tick,
            combat_ticks: self.combat_ticks,
            scale_milli: (self.duel.placement().scale * 1000.0).round() as u32,
            health_milli: self
                .final_health
                .unwrap_or(tanks.map(|t| t.health))
                .map(|v| (v * 1000.0).round() as u32),
            present: tanks.map(|t| t.present),
            supported: tanks.map(|t| t.supported),
            positions_milli: tanks.map(|t| {
                [t.motion.position.x, t.motion.position.y].map(|v| (v * 1000.0).round() as i32)
            }),
            tank_ids: tanks.map(|t| t.body.entity.value()),
            terrain_ids: self.duel.terrain().iter().map(|f| f.id.value()).collect(),
            bodies: (self.duel.body_ids().count()
                + self.converted.len()
                + self.wreck.pieces.len()
                + self.survivors.bodies()) as u32,
            terrain_cells: self
                .duel
                .terrain()
                .iter()
                .map(|field| {
                    field
                        .terrain
                        .cells()
                        .iter()
                        .filter(|cell| cell.material != MaterialId::VOID)
                        .count() as u64
                })
                .sum(),
            loose_cells: self.duel.grains().count() as u32,
            shells: self.duel.active_shells() as u32,
            shots: self.duel.shots(),
            result: self.result.map(|r| r.label().into()),
            converted_cells: self.converted.len() as u32,
            pieces: self.wreck.pieces.len() as u32,
            retired_tank_mass_milli: (self.retired_tank_mass * 1000.0).round() as u32,
            conversion_area_milli: (1000.0
                * self.converted.len() as f32
                * (0.5 * self.duel.placement().scale).powi(2))
            .round() as u32,
            entry_failure: self.entry_failure.map(str::to_owned),
            cpu: self.cpu.map(str::to_owned),
            dodging: self.duel.dodging(),
            face_hits: self.face_hits,
            knocked_loose: self.damage.knocked_loose(),
            ejections: self.survivors.rolls,
            survivors: [0, 1].map(|slot| self.survivors.state(slot)),
        }
    }
}

#[cfg(test)]
mod tests;
