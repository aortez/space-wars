//! What a tank can tell of the units around it: which of them oppose it,
//! where they are and how they move, and whether it can see them. Telling a
//! tank's opponents casts nothing. Each sight line is one cast, and the
//! tanks' work counts every tank's together.

use engine_core::Vec2;
use engine_rapier::world::{ColliderId, PhysicsWorld, RayCastOptions};

use super::{ShotBody, ShotStop, Tank, TankId, TargetMotion};
use crate::{
    Allegiance, DebrisKind, PlayerId, SPACEWARS_PLAYER_COUNT, SpacewarsState,
    physics::{self, MechanicalEntity},
};

/// The most opponents a tank tells at once.
pub const MAX_OPPONENTS: usize = 8;

/// A unit that opposes a tank, as the tank tells it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opponent {
    pub body: ShotBody,
    /// Where its center of mass is, and how it moves.
    pub motion: TargetMotion,
    /// How far its center of mass is from the barrel's pivot.
    pub distance: f32,
}

/// What a tank sees along the line from its barrel's pivot to a body's
/// center of mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sight {
    /// The line meets nothing before the body.
    Clear,
    /// The line meets something else first, at `at`.
    Blocked { by: ShotStop, at: Vec2 },
}

impl Tank {
    /// Where the barrel turns, from the chassis as the world holds it.
    fn pivot(&self, world: &PhysicsWorld) -> Option<Vec2> {
        let chassis = world.motion(self.assembly.bodies()[0])?;
        Some(self.barrel(chassis.position, chassis.angle).1.pivot)
    }
}

impl SpacewarsState {
    /// The units that oppose tank `id` with their centers of mass within
    /// `range` of its barrel's pivot: the nearest [`MAX_OPPONENTS`], nearest
    /// first, and in the order of their bodies where two are as near. A unit
    /// opposes the tank when it fights for another side. Ships and pods,
    /// pilots on foot and rovers fight for their players, and tanks for their
    /// own sides. A unit counts while the game holds it, the world holds its
    /// body, and its player, if it fights for one, is still in the game.
    pub fn tank_opponents(&self, id: TankId, range: f32) -> Vec<Opponent> {
        let actors = &self.physics.tanks.actors;
        let Some((tank, pivot)) = actors
            .get(&id)
            .and_then(|tank| Some((tank, tank.pivot(&self.physics.world)?)))
        else {
            return Vec::new();
        };
        let player = |index| PlayerId::from_index(index).map(Allegiance::Player);
        let ships = self.ships.iter().enumerate().filter(|(_, ship)| !ship.dead);
        let ships =
            ships.filter_map(|(seat, ship)| Some((ShotBody::Ship(seat), player(ship.owner_id)?)));
        let pilots = (0..SPACEWARS_PLAYER_COUNT)
            .filter_map(|seat| Some((ShotBody::Spaceling(seat), player(seat)?)));
        let rovers = self.rovers.iter().filter(|rover| !rover.dead);
        let rovers =
            rovers.filter_map(|rover| Some((ShotBody::Rover(rover.id), player(rover.owner_id)?)));
        let tanks = actors.iter().filter(|&(&other, _)| other != id);
        let tanks =
            tanks.map(|(&other, other_tank)| (ShotBody::Tank(other), other_tank.allegiance));
        let mut opponents: Vec<Opponent> = ships
            .chain(pilots)
            .chain(rovers)
            .chain(tanks)
            .filter(|&(_, side)| side != tank.allegiance && !self.eliminated(side))
            .filter_map(|(body, _)| {
                let motion = self.shot_target_motion(body)?;
                let distance = motion.position.distance_to(pivot);
                (distance <= range).then_some(Opponent {
                    body,
                    motion,
                    distance,
                })
            })
            .collect();
        opponents.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.body.cmp(&b.body)));
        opponents.truncate(MAX_OPPONENTS);
        opponents
    }

    /// Whether `side` is a player the game holds eliminated. A match
    /// eliminates the player whose pilot dies, and the world goes on holding
    /// the pilot's body.
    fn eliminated(&self, side: Allegiance) -> bool {
        let Allegiance::Player(player) = side else {
            return false;
        };
        self.players
            .get(player.index())
            .is_some_and(|state| state.eliminated)
    }

    /// What tank `id` sees along the line from its barrel's pivot to the
    /// center of mass of `body`, while the world holds both. The line meets
    /// what a round meets, but passes through the tank itself and through
    /// shells, a tank's round or a ship's: a shell is small and soon gone, so
    /// it hides nothing for long. A call that tells a sight casts once,
    /// counted in the tanks' work.
    ///
    /// The world answers as its last step left it, less the bodies it has let
    /// go since, and may miss a body it has taken up or moved since until the
    /// next step. Between steps it may still hold the bodies of some of what
    /// left the game in the last step, such as a rover destroyed then. Within
    /// a step, once it holds just what the game holds, it holds none of them.
    pub fn tank_sight(&mut self, id: TankId, body: ShotBody) -> Option<Sight> {
        let target = self.shot_target_motion(body)?.position;
        let tank = self.physics.tanks.actors.get(&id)?;
        let world = &self.physics.world;
        let pivot = tank.pivot(world)?;
        let along = target - pivot;
        let options = RayCastOptions {
            max_distance: along.length(),
            collision_groups: physics::tank_round_groups(),
            exclude_entity: Some(tank.assembly.entity()),
            ..Default::default()
        };
        let shell = |collider: ColliderId| match physics::classify_entity(collider.entity) {
            Some(MechanicalEntity::Debris(piece)) => {
                self.physics.debris_kind(piece) == Some(DebrisKind::Shell)
            }
            _ => false,
        };
        let hit = world.cast_ray_keeping(pivot, along, options, |collider| !shell(collider));
        self.physics.tanks.work.sight_casts += 1;
        Some(
            match hit.map(|hit| (ShotStop::of(hit.collider.entity), hit.point)) {
                None => Sight::Clear,
                Some((ShotStop::Body(met), _)) if met == body => Sight::Clear,
                Some((by, at)) => Sight::Blocked { by, at },
            },
        )
    }
}
