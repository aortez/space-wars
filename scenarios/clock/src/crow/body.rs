//! A bird's body in a physics world, and the perches of a visit of crows.
//!
//! A bird is a dynamic disc about as wide as its body, whose bottom is at its
//! feet, so the body's origin is where the bird stands. It keeps upright,
//! never sleeps and feels none of the world's gravity: its controller brings
//! the pull it flies against, as it always has, and sets the body's velocity
//! before each step. The step moves it, and whatever it meets stops it,
//! pushes it or is pushed by it. After the step the controller reads the
//! body back, contacts and all, and decides from there.
//!
//! A bird flies in and out of the view over its frame, so the frame's walls
//! do not stop it: it is in one of the arena's bird groups, each crow of a
//! visit in its own and the magpie and the chess board's crow in the first
//! crow's, which they filter out.
//!
//! The digit cells are bodies only during the events that make them so, so
//! a visit of crows brings a perch for each column they may land on: a fixed
//! square where the column's top lit cell is drawn. The crows share them, and
//! the perches follow the face before each step. Like any body in the arena
//! they are solid for whatever meets them, as the cells they stand for would
//! be, but each holds a crow only from above: a crow beside or under one,
//! knocked off its spot or under a cell the face has just lit, flies up
//! through it as through the column's other cells, while a crow standing on
//! it is held. The floor is the one the arena keeps under whoever rests on
//! it, or a visit's terrain.
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld,
};

use super::{MAX_CROWS, PERCH_COLUMNS, Perch};
use crate::{arena::ids::canonical, layout::Layout};

#[cfg(test)]
mod tests;

/// The disc's radius in pitches: the bird's body, without its head, tail
/// and legs.
pub(crate) const RADIUS_PITCH: f32 = 0.4;
/// A coin's, so a bird as wide as a coin weighs on a heap as another coin
/// would, and a coin it lands beside is nudged, not flung.
const DENSITY: f32 = 0.5;
const FRICTION: f32 = 0.8;
/// Friction of a perch, as the digits' other solids have.
const PERCH_FRICTION: f32 = 0.65;
/// Half a lit cell's side, in pitches.
const CELL_HALF_PITCH: f32 = 0.4;
/// What holds a bird up: its contacts in the last step pushed it up by at
/// least this much of the pull its controller gives it in a step. Standing,
/// they cancel all of it, on a floor or wedged between two coins alike.
const HELD: f32 = 0.25;
/// How far ahead, in pitches, a bird's contacts are predicted: more than a
/// step's travel at the fastest it comes down, so its feet stop on what
/// they reach instead of sinking into it for a step.
const PREDICTION_PITCH: f32 = 0.25;
/// How far below a perch's top a crow's feet may be, sunk into it or
/// slipping off its edge, for the perch still to hold them, in pitches.
/// Lower than that, the crow is beside or under the cell.
const HELD_BELOW_TOP: f32 = 0.1;

/// A bird's body, by its identity in the world it flies in, and what it
/// meets there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bird {
    id: BodyId,
    groups: CollisionGroups,
}

/// The body of crow `member` of a visit, in the arena.
pub(crate) const fn crow(member: usize) -> Bird {
    assert!(member < MAX_CROWS, "a visit's crow");
    Bird::new(
        canonical::CROW.start + member as u64,
        crate::arena::crow(member),
    )
}

/// The body of the crow at the chess board, in the arena.
pub(crate) const CHESS_CROW: Bird = Bird::new(canonical::CHESS_CROW.start, crate::arena::BIRD);

/// The magpie's body, in whichever world the coins lie in.
pub(crate) const MAGPIE: Bird = Bird::new(canonical::MAGPIE.start, crate::arena::BIRD);

impl Bird {
    const fn new(entity: u64, groups: CollisionGroups) -> Self {
        Self {
            id: BodyId::new(PhysicsId::new(entity), BodyRole::PRIMARY),
            groups,
        }
    }

    #[cfg(test)]
    pub fn id(self) -> BodyId {
        self.id
    }

    fn collider(self) -> ColliderId {
        ColliderId::new(self.id.entity, ColliderRole::PRIMARY, 0)
    }

    /// Puts the bird in `world`, standing or flying at `feet` with
    /// `velocity`.
    pub fn insert(self, world: &mut PhysicsWorld, feet: Vec2, velocity: Vec2, layout: Layout) {
        let radius = layout.pitch * RADIUS_PITCH;
        let mut disc = ColliderSpec::ball(self.collider(), radius);
        disc.local_position = Vec2::new(0.0, radius);
        disc.density = DENSITY;
        disc.friction = FRICTION;
        disc.restitution = 0.0;
        disc.collision_groups = self.groups;
        assert!(
            world.insert_body(
                self.id,
                BodySpec {
                    position: feet,
                    linear_velocity: velocity,
                    lock_rotation: true,
                    gravity_scale: 0.0,
                    can_sleep: false,
                    ccd_enabled: true,
                    soft_ccd_prediction: layout.pitch * PREDICTION_PITCH,
                    ..BodySpec::default()
                },
                &[disc],
            ),
            "{self:?} enters its world once"
        );
    }

    /// Takes the bird out of `world`, if it is there.
    pub fn remove(self, world: &mut PhysicsWorld) {
        world.remove_entity(self.id.entity);
    }

    /// The velocity the bird takes into the next step.
    pub fn drive(self, world: &mut PhysicsWorld, velocity: Vec2) {
        assert!(
            world.set_velocity(self.id, velocity, 0.0, true),
            "{self:?} flies in this world"
        );
    }

    /// Where the bird's feet are and how fast it is going.
    pub fn motion(self, world: &PhysicsWorld) -> Option<(Vec2, Vec2)> {
        world
            .motion(self.id)
            .map(|motion| (motion.position, motion.linear_velocity))
    }

    /// Whether something held the bird up in the last step: what it touches
    /// pushed it up, as a floor, a perch or a heap of coins does under its
    /// feet, and not as a wall it brushes or a surface it leaves does.
    pub fn footing(self, world: &PhysicsWorld, layout: Layout) -> bool {
        world.body_contact_push(self.id).is_some_and(|push| {
            push.velocity_change.y >= HELD * super::flight::gravity(layout) * super::flight::DT
        })
    }
}

/// A visit's perches in the arena, by column, which its crows share.
#[derive(Clone, Debug)]
pub(crate) struct Perches {
    placed: [Option<Perch>; PERCH_COLUMNS],
    /// What each placed perch meets: everything while it holds every crow of
    /// the visit, as a new perch does, and otherwise everything but the crows
    /// it does not hold.
    groups: [CollisionGroups; PERCH_COLUMNS],
}

impl Default for Perches {
    fn default() -> Self {
        Self {
            placed: [None; PERCH_COLUMNS],
            groups: [CollisionGroups::ALL; PERCH_COLUMNS],
        }
    }
}

fn perch_entity(column: usize) -> PhysicsId {
    debug_assert!(column < PERCH_COLUMNS);
    PhysicsId::new(canonical::CROW_PERCHES.start + column as u64)
}

impl Perches {
    /// Gives each column the perch the face has for it now, and takes away
    /// the perches it no longer has. Each holds a crow of the visit, whose
    /// feet are in `crows` by member, only if they are at its top or above
    /// it.
    pub fn place(
        &mut self,
        world: &mut PhysicsWorld,
        wanted: &[Option<Perch>; PERCH_COLUMNS],
        crows: &[Option<Vec2>; MAX_CROWS],
        layout: Layout,
    ) {
        let present = crows_where(crows, |_| true);
        for (column, ((placed, groups), wanted)) in self
            .placed
            .iter_mut()
            .zip(&mut self.groups)
            .zip(wanted)
            .enumerate()
        {
            let same = match (placed.as_ref(), wanted) {
                (Some(placed), Some(wanted)) => {
                    placed.key == wanted.key && placed.feet == wanted.feet
                }
                (None, None) => true,
                _ => false,
            };
            let entity = perch_entity(column);
            let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
            if !same {
                if placed.take().is_some() {
                    assert!(world.remove_entity(entity), "a placed perch");
                }
                if let Some(perch) = wanted {
                    let half = layout.pitch * CELL_HALF_PITCH;
                    let mut cell = ColliderSpec::cuboid(collider, half, half);
                    cell.friction = PERCH_FRICTION;
                    cell.restitution = 0.0;
                    assert!(world.insert_body(
                        BodyId::new(entity, BodyRole::PRIMARY),
                        BodySpec {
                            kind: BodyKind::Fixed,
                            position: perch.feet - Vec2::new(0.0, half),
                            ..BodySpec::default()
                        },
                        &[cell],
                    ));
                    *placed = Some(*perch);
                    *groups = CollisionGroups::ALL;
                }
            }
            if let Some(perch) = placed {
                let held = crows_where(crows, |feet| {
                    feet.y >= perch.feet.y - layout.pitch * HELD_BELOW_TOP
                });
                let meets = if held == present {
                    CollisionGroups::ALL
                } else {
                    crate::arena::holding(held)
                };
                if meets != *groups {
                    assert!(
                        world.set_collision_groups(collider, meets),
                        "a placed perch"
                    );
                    *groups = meets;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.placed.iter().flatten().count()
    }
}

/// The union of the collision groups of the visit's crows whose feet, in
/// `crows` by member, pass `test`.
fn crows_where(crows: &[Option<Vec2>; MAX_CROWS], test: impl Fn(Vec2) -> bool) -> u32 {
    crows
        .iter()
        .enumerate()
        .filter(|(_, feet)| feet.is_some_and(&test))
        .fold(0, |groups, (member, _)| {
            groups | crate::arena::crow(member).memberships
        })
}
