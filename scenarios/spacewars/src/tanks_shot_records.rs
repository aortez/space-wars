//! Records of the shots Spacewars' tanks fire (#239).
//!
//! Each round a tank fires has one record: how it left the barrel, how its
//! flight has gone, and how it ended. A record observes. It holds what
//! happened, measured from the bodies' real motion after each step, and
//! nothing foreseen. The body a command names as its target only labels the
//! record of each shot fired under it. It changes nothing about the round or
//! what the round can hit, and nothing that decides a hit, a claim or a
//! surface force reads it.
//!
//! The records are bounded. A live tank keeps the records of its rounds in
//! flight and of the last [`ENDED_SHOTS`] of its shots to end. A tank that
//! leaves the world takes its ended records with it, and its rounds in flight
//! keep theirs until they end. The state's `tank_shots_ended` lists the
//! records that ended in the last step that advanced the world, whether their
//! tanks live or not.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_material::SourceLedger;
use engine_rapier::world::{BodyMotion, PhysicsId};

use super::{TankId, TankRound, Tanks};
use crate::{
    Allegiance, BodyId, DebrisId, DebrisState, SpacewarsState,
    physics::{self, MechanicalEntity},
};

/// How many of a live tank's ended shots keep their records.
pub const ENDED_SHOTS: usize = 4;
/// How many bodies a shot's record lists as harmed, at most.
pub const SHOT_HARMS: usize = 8;

/// A shot's identity, which no other shot ever takes: the tanks count their
/// shots, and neither removing nor clearing them rewinds the count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TankShotId(u64);

impl TankShotId {
    /// The id no shot takes, for a round that no tank's cannon fired.
    #[cfg(test)]
    pub(crate) const NONE: Self = Self(0);

    /// The next shot's id, which counts it in `fired`.
    pub(super) fn next(fired: &mut u64) -> Self {
        *fired += 1;
        Self(*fired)
    }
}

/// A body with an identity that a shot can be meant for, stop on or harm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShotBody {
    /// The ship or the pod in a seat.
    Ship(usize),
    /// The pilot on foot from a seat.
    Spaceling(usize),
    /// A rover, by its id.
    Rover(u64),
    Tank(TankId),
    /// A piece of debris, by the id [`DebrisState::id`] tells.
    Debris(DebrisId),
}

impl ShotBody {
    fn entity(self) -> MechanicalEntity {
        match self {
            Self::Ship(seat) => MechanicalEntity::Ship(seat),
            Self::Spaceling(seat) => MechanicalEntity::Spaceling(seat),
            Self::Rover(id) => MechanicalEntity::Rover(id),
            Self::Tank(id) => MechanicalEntity::Tank(id.entity()),
            Self::Debris(id) => MechanicalEntity::Debris(id.value()),
        }
    }

    fn of(entity: MechanicalEntity) -> Option<Self> {
        match entity {
            MechanicalEntity::Ship(seat) => Some(Self::Ship(seat)),
            MechanicalEntity::Spaceling(seat) => Some(Self::Spaceling(seat)),
            MechanicalEntity::Rover(id) => Some(Self::Rover(id)),
            MechanicalEntity::Tank(entity) => TankId::of_entity(entity).map(Self::Tank),
            MechanicalEntity::Debris(id) => DebrisId::from_value(id).map(Self::Debris),
            MechanicalEntity::World
            | MechanicalEntity::Body(_)
            | MechanicalEntity::TerrainFragment(_) => None,
        }
    }
}

/// Where a target's center of mass is, and how it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetMotion {
    pub position: Vec2,
    pub velocity: Vec2,
}

/// How a shot left its tank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotLaunch {
    /// The world's tick as the tank fired.
    pub tick: u64,
    /// The elevation the command aimed the barrel at, in radians.
    pub elevation: f32,
    /// The side the command faced the barrel toward.
    pub facing: f32,
    /// The power the command fired with.
    pub power: f32,
    /// The up the elevation was measured from.
    pub up: Vec2,
    /// The hull's motion.
    pub hull: BodyMotion,
    /// The barrel's pivot.
    pub pivot: Vec2,
    /// The way the barrel pointed.
    pub direction: Vec2,
    /// Where the round started.
    pub position: Vec2,
    /// The round's velocity as it started.
    pub velocity: Vec2,
    /// Whether something stood in the barrel, so that the round started
    /// there, short of the muzzle. The barrel is cast against the world as
    /// the last step left it, less the bodies of what has left the game
    /// since.
    pub blocked: bool,
    /// The target's motion as the step that fired the shot began, if the
    /// command named a target whose body the world then held.
    pub target: Option<TargetMotion>,
}

/// The closest a round has come to its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotApproach {
    /// The world's tick then.
    pub tick: u64,
    /// How far the round's center was from the target's center of mass.
    pub distance: f32,
    /// From the target's center of mass to the round's center.
    pub offset: Vec2,
}

/// How a shot's flight has gone, as its latest step left it. The step that
/// stops a round leaves it turned back from what it met, so an ended shot's
/// flight tells how its round rebounded, not how it came in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotFlight {
    /// How many steps the round has flown.
    pub steps: u64,
    /// Where the round is.
    pub position: Vec2,
    /// How the round moves.
    pub velocity: Vec2,
    /// The target's motion, if the shot has a target whose body the world
    /// held then. A step samples it before the shot's blast can push the
    /// target and before a target the step destroyed leaves the world, so an
    /// ended shot tells where its target was whether or not the target
    /// outlived it.
    pub target: Option<TargetMotion>,
    /// The closest the round has come to its target so far, from where each
    /// was as it fired and after each step. None while the shot has no target,
    /// or the world has not held the target's body.
    pub closest: Option<ShotApproach>,
}

impl ShotFlight {
    /// Notes how close a round at `position` is to `target` at `tick`, if
    /// it has come no closer.
    fn approach(&mut self, tick: u64, position: Vec2, target: TargetMotion) {
        let offset = position - target.position;
        let distance = offset.length();
        if self
            .closest
            .is_none_or(|closest| distance < closest.distance)
        {
            self.closest = Some(ShotApproach {
                tick,
                distance,
                offset,
            });
        }
    }
}

/// What a shot stopped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotStop {
    Body(ShotBody),
    /// A planet, its terrain, or a piece of terrain that broke off.
    Ground,
    /// Anything else, such as the sun or the edge of the universe.
    Other,
}

impl ShotStop {
    fn of(entity: PhysicsId) -> Self {
        physics::classify_entity(entity).map_or(Self::Other, Self::on)
    }

    fn on(entity: MechanicalEntity) -> Self {
        match entity {
            MechanicalEntity::Body(BodyId::Planet(_)) | MechanicalEntity::TerrainFragment(_) => {
                Self::Ground
            }
            entity => ShotBody::of(entity).map_or(Self::Other, Self::Body),
        }
    }
}

/// How a shot ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotOutcome {
    /// It burst where it stopped on `on`. With none, it burst in flight,
    /// where its shell died another way, such as to a laser.
    Burst { on: Option<ShotStop> },
    /// It flew for [`engine_rapier::cannon::ROUND_LIFETIME_TICKS`] and
    /// vanished.
    Expired,
    /// Its shell left the game another way, such as in a reset.
    Removed,
}

/// What a shot cost one body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotHarm {
    pub body: ShotBody,
    /// What its hit and its blast charged the body together, in the body's
    /// health.
    pub health: f32,
}

/// How a shot ended, and what it did.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotEnd {
    /// The world's tick once the step that ended it was over.
    pub tick: u64,
    /// Where it ended: where its blast started, or else where its round was
    /// last.
    pub position: Vec2,
    pub outcome: ShotOutcome,
    /// What it cost each body it harmed, the worst harmed first, and no more
    /// than [`SHOT_HARMS`] of them.
    pub harm: Vec<ShotHarm>,
    /// Where the shot's energy went, if the weapons carried it to its end.
    pub ledger: Option<SourceLedger>,
}

/// The record of one shot.
#[derive(Debug, Clone, PartialEq)]
pub struct TankShot {
    pub id: TankShotId,
    /// The tank that fired it, which may since have left the world.
    pub tank: TankId,
    /// The side that tank fought for, which the round's hits name.
    pub allegiance: Allegiance,
    /// The body the command that fired it named as its target. It labels the
    /// record, and changes nothing about the round.
    pub target: Option<ShotBody>,
    pub launch: ShotLaunch,
    pub flight: ShotFlight,
    /// None while the round flies.
    pub end: Option<ShotEnd>,
}

impl TankShot {
    /// The record of the shot whose `round` leaves as `launch` tells, meant
    /// for `target`.
    pub(super) fn fired(round: TankRound, target: Option<ShotBody>, launch: ShotLaunch) -> Self {
        Self {
            id: round.shot,
            tank: round.tank,
            allegiance: round.allegiance,
            target,
            launch,
            flight: ShotFlight {
                steps: 0,
                position: launch.position,
                velocity: launch.velocity,
                target: None,
                closest: None,
            },
            end: None,
        }
    }
}

impl Tanks {
    /// The records of tank `id`'s shots, oldest first: its rounds in flight,
    /// and the last [`ENDED_SHOTS`] of its shots to end while it lives.
    pub fn shots(&self, id: TankId) -> impl Iterator<Item = &TankShot> + '_ {
        self.shots.values().filter(move |shot| shot.tank == id)
    }

    /// Ends the record of shot `id` and returns it. A live tank keeps it, and
    /// drops the first of its records to end beyond [`ENDED_SHOTS`]. The
    /// record of a tank that is gone leaves with this.
    fn end_shot(&mut self, id: TankShotId, end: ShotEnd) -> Option<TankShot> {
        let shot = self.shots.get_mut(&id)?;
        shot.end = Some(end);
        let shot = shot.clone();
        if !self.actors.contains_key(&shot.tank) {
            self.shots.remove(&id);
            return Some(shot);
        }
        let mut ended: Vec<(u64, TankShotId)> = self
            .shots(shot.tank)
            .filter_map(|shot| Some((shot.end.as_ref()?.tick, shot.id)))
            .collect();
        ended.sort_unstable();
        let excess = ended.len().saturating_sub(ENDED_SHOTS);
        for (_, id) in ended.into_iter().take(excess) {
            self.shots.remove(&id);
        }
        Some(shot)
    }

    /// Drops the ended records of tank `id`, which has left the world. Its
    /// rounds in flight keep theirs.
    pub(super) fn forget_ended_shots(&mut self, id: TankId) {
        self.shots
            .retain(|_, shot| shot.tank != id || shot.end.is_none());
    }
}

impl SpacewarsState {
    /// The records of tank `id`'s shots, oldest first: its rounds in flight,
    /// and the last [`ENDED_SHOTS`] of its shots to end while it lives. Once
    /// the tank is gone, only its rounds still in flight have records.
    pub fn tank_shots(&self, id: TankId) -> impl Iterator<Item = &TankShot> + '_ {
        self.physics.tanks.shots(id)
    }

    /// Where `body`'s center of mass is and how it moves, if the world holds
    /// its body.
    fn shot_target_motion(&self, body: ShotBody) -> Option<TargetMotion> {
        let (position, velocity) = self.physics.carried_motion(body.entity())?;
        Some(TargetMotion { position, velocity })
    }

    /// Starts keeping the record of each shot fired in this step, with its
    /// target's motion as it fired. Runs once the world holds a body for just
    /// what the game holds as the step begins, so a target that left the game
    /// in the last step has none.
    pub(crate) fn record_tank_shots(&mut self, fired: Vec<TankShot>) {
        for mut shot in fired {
            let target = shot.target.and_then(|body| self.shot_target_motion(body));
            shot.launch.target = target;
            shot.flight.target = target;
            if let Some(target) = target {
                shot.flight
                    .approach(shot.launch.tick, shot.launch.position, target);
            }
            self.physics.tanks.shots.insert(shot.id, shot);
        }
    }

    /// Notes where the step left each round in flight and its target. Runs
    /// once the step's motion has reached the debris, before anything the
    /// step destroyed leaves the world.
    pub(crate) fn track_tank_shots(&mut self) {
        let tick = self.tick + 1;
        let shots = &self.physics.tanks.shots;
        let seen: Vec<_> = self
            .debris
            .iter()
            // A shell that was dead as the step began had no body to fly.
            .filter(|shell| !shell.dead)
            .filter_map(|shell| {
                let id = shell.tank_round?.shot;
                // An ended record is final, though a host may put its shell
                // back.
                let shot = shots.get(&id).filter(|shot| shot.end.is_none())?;
                let target = shot.target.and_then(|body| self.shot_target_motion(body));
                Some((id, shell.position, shell.velocity, target))
            })
            .collect();
        for (id, position, velocity, target) in seen {
            let Some(shot) = self.physics.tanks.shots.get_mut(&id) else {
                continue;
            };
            shot.flight.steps += 1;
            shot.flight.position = position;
            shot.flight.velocity = velocity;
            shot.flight.target = target;
            if let Some(target) = target {
                shot.flight.approach(tick, position, target);
            }
        }
    }

    /// Ends the record of each shot whose round the step ended, and lists
    /// those records in `tank_shots_ended`. Runs once the step's hits are
    /// charged and its destroyed tanks are gone, while its dead shells are
    /// still in the debris.
    pub(crate) fn finish_tank_shots(&mut self) {
        self.tank_shots_ended.clear();
        // The shell of each tank's round, by its shot.
        let shells: BTreeMap<TankShotId, &DebrisState> = self
            .debris
            .iter()
            .filter_map(|shell| Some((shell.tank_round?.shot, shell)))
            .collect();
        let open = self.physics.tanks.shots.values();
        let ends: Vec<(TankShotId, ShotEnd)> = open
            .filter(|shot| shot.end.is_none())
            .filter_map(|shot| {
                let shell = shells.get(&shot.id).copied();
                if shell.is_some_and(|shell| !shell.dead) {
                    return None;
                }
                Some((shot.id, self.tank_shot_end(shot, shell)))
            })
            .collect();
        for (id, end) in ends {
            if let Some(shot) = self.physics.tanks.end_shot(id, end) {
                self.tank_shots_ended.push(shot);
            }
        }
    }

    /// How `shot` ended in this step, from what the step's weapons, terrain
    /// and charges tell of its dead `shell`. A shot whose shell is gone was
    /// removed.
    fn tank_shot_end(&self, shot: &TankShot, shell: Option<&DebrisState>) -> ShotEnd {
        let key = shell.map(|shell| shell.physics_id);
        let ended = key
            .and_then(|key| self.round_ends.get(&key).copied())
            .unwrap_or_default();
        // Terrain stops a shell before its shot can, so the shot tells of no
        // stop there.
        let ground = key.is_some_and(|key| self.terrain.cannon_struck.contains(&key));
        let on = ended
            .stopped
            .map(ShotStop::of)
            .or(ground.then_some(ShotStop::Ground));
        // A round bursts wherever it stops, so a shot that the weapons ended
        // of themselves without a burst ran out of time.
        let outcome = if ended.burst.is_some() {
            ShotOutcome::Burst { on }
        } else if ended.unprompted {
            ShotOutcome::Expired
        } else {
            ShotOutcome::Removed
        };
        // What the shot's hit and its blast charged each body.
        let mut harm: Vec<ShotHarm> = Vec::new();
        let hit = key
            .and_then(|key| self.shot_hits.get(&key))
            .map(|hit| (hit.target, hit.health));
        let blasts = self
            .blast_hits
            .iter()
            .filter(|hit| Some(hit.source) == key)
            .map(|hit| (hit.target, hit.health));
        for (target, health) in hit.into_iter().chain(blasts) {
            if !health.is_finite() || health <= 0.0 {
                continue;
            }
            let Some(body) = ShotBody::of(target) else {
                continue;
            };
            match harm.iter_mut().find(|harm| harm.body == body) {
                Some(harm) => harm.health += health,
                None => harm.push(ShotHarm { body, health }),
            }
        }
        harm.sort_by(|a, b| b.health.total_cmp(&a.health).then(a.body.cmp(&b.body)));
        harm.truncate(SHOT_HARMS);
        ShotEnd {
            tick: self.tick + 1,
            // The round's flight tells where its shell was last.
            position: ended.burst.unwrap_or(shot.flight.position),
            outcome,
            harm,
            ledger: ended.ledger,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stop names the body with an identity that the round met, the ground
    /// for a planet or a piece of terrain that broke off, and no more than
    /// that for anything else.
    #[test]
    fn a_stop_names_a_body_the_ground_or_something_else() {
        let tank = TankId(3);
        let debris = DebrisId::from_value(100_007).unwrap();
        let cases = [
            (MechanicalEntity::Ship(1), ShotStop::Body(ShotBody::Ship(1))),
            (
                MechanicalEntity::Spaceling(0),
                ShotStop::Body(ShotBody::Spaceling(0)),
            ),
            (
                MechanicalEntity::Rover(7),
                ShotStop::Body(ShotBody::Rover(7)),
            ),
            (
                MechanicalEntity::Tank(tank.entity()),
                ShotStop::Body(ShotBody::Tank(tank)),
            ),
            (
                MechanicalEntity::Debris(debris.value()),
                ShotStop::Body(ShotBody::Debris(debris)),
            ),
            (MechanicalEntity::Body(BodyId::Planet(2)), ShotStop::Ground),
            (MechanicalEntity::TerrainFragment(5), ShotStop::Ground),
            (MechanicalEntity::Body(BodyId::Sun), ShotStop::Other),
            (MechanicalEntity::World, ShotStop::Other),
        ];
        for (entity, stop) in cases {
            assert_eq!(ShotStop::on(entity), stop, "{entity:?}");
            // A body's name leads back to the entity it was given for.
            if let ShotStop::Body(body) = stop {
                assert_eq!(body.entity(), entity);
            }
        }
    }
}
