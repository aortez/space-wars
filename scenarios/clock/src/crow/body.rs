//! A bird's body in a physics world, and the crow's perches.
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
//! do not stop it: it is in the arena's `BIRD` group, which they filter out.
//!
//! The digit cells are bodies only during the events that make them so, so
//! the crow brings a perch for each column it may land on: a fixed square
//! where the column's top lit cell is drawn. The perches follow the face
//! before each step. Like any body in the arena they are solid for whatever
//! meets them, as the cells they stand for would be, but they hold the crow
//! only from above: a crow beside or under one, knocked off its spot or under
//! a cell the face has just lit, flies up through it as through the column's
//! other cells. The floor is the one the arena keeps under whoever rests on
//! it, or a visit's terrain.
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld,
};

use super::{PERCH_COLUMNS, Perch};
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
/// How far below a perch's top the crow's feet may be, sunk into it or
/// slipping off its edge, for the perch still to hold them, in pitches.
/// Lower than that, the crow is beside or under the cell.
const HELD_BELOW_TOP: f32 = 0.1;
/// A perch the crow is beside or under: it holds whatever else meets it, as
/// the frame does, and the crow passes through it.
const UNDER: CollisionGroups = crate::arena::FRAME;

/// A bird's body, by its identity in the world it flies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bird {
    id: BodyId,
}

/// The crow's body in the arena.
pub(crate) const CROW: Bird = Bird::new(canonical::CROW.start);
/// The magpie's body, in whichever world the coins lie in.
pub(crate) const MAGPIE: Bird = Bird::new(canonical::MAGPIE.start);

impl Bird {
    const fn new(entity: u64) -> Self {
        Self {
            id: BodyId::new(PhysicsId::new(entity), BodyRole::PRIMARY),
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
        disc.collision_groups = crate::arena::BIRD;
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

/// The crow's perches in the arena, by column.
#[derive(Clone, Debug)]
pub(crate) struct Perches {
    placed: [Option<Perch>; PERCH_COLUMNS],
    /// Whether each placed perch holds the crow, as it does while the crow's
    /// feet are at its top or above it.
    holding: [bool; PERCH_COLUMNS],
}

impl Default for Perches {
    fn default() -> Self {
        Self {
            placed: [None; PERCH_COLUMNS],
            holding: [true; PERCH_COLUMNS],
        }
    }
}

fn perch_entity(column: usize) -> PhysicsId {
    debug_assert!(column < PERCH_COLUMNS);
    PhysicsId::new(canonical::CROW_PERCHES.start + column as u64)
}

impl Perches {
    /// Gives each column the perch the face has for it now, and takes away
    /// the perches it no longer has. Each holds the crow, whose feet are at
    /// `feet`, only if they are at its top or above it.
    pub fn place(
        &mut self,
        world: &mut PhysicsWorld,
        wanted: &[Option<Perch>; PERCH_COLUMNS],
        feet: Vec2,
        layout: Layout,
    ) {
        for (column, ((placed, holding), wanted)) in self
            .placed
            .iter_mut()
            .zip(&mut self.holding)
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
                    *holding = true;
                }
            }
            if let Some(perch) = placed {
                let holds = feet.y >= perch.feet.y - layout.pitch * HELD_BELOW_TOP;
                if holds != *holding {
                    let groups = if holds { CollisionGroups::ALL } else { UNDER };
                    assert!(
                        world.set_collision_groups(collider, groups),
                        "a placed perch"
                    );
                    *holding = holds;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.placed.iter().flatten().count()
    }
}
