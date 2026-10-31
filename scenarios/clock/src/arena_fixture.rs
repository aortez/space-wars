//! The arena fixture: a few balls that belong to the run, with no plants or
//! soil. It is the smallest resident that rests on the floor, and the arena
//! contract uses it to check that a resident keeps its bodies, their motion
//! and their identities while visits and events come and go. Each ball rests
//! on whatever actually supports it and leaves only by falling out of the
//! view. Several fixtures can hold the arena at once, each its own owner with
//! its own balls, so one can be taken out while the others stay.

use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
};

use crate::{
    arena::{ClockArena, MAX_RESIDENT_CLAIMS, Owner, ids::canonical::FIXTURE_OWNER_BALLS},
    layout::Layout,
};

/// The most balls one fixture holds, one identity each.
pub const MAX_FIXTURE_BALLS: usize = 4;
/// The most fixtures, each with identities of its own. There are as many as
/// the arena holds residents, so the arena's own limit is the one that
/// refuses another.
pub const MAX_FIXTURES: usize = MAX_RESIDENT_CLAIMS;

/// The arena fixtures a run starts with, so that the real client shows
/// residents. Ordinary Clock starts with none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FixtureStart {
    #[default]
    None,
    /// The ordinary fixture's three balls.
    Default,
    /// The ordinary fixture and a second owner with two balls of its own.
    /// Beside a visit they leave no room for Meltdown, Explosion or Aquarium.
    TwoOwners,
}

impl FixtureStart {
    /// The fixtures `SPACEWARS_CLOCK_ARENA_FIXTURE` names: `default` or
    /// `two-owners`.
    pub fn from_override(value: Option<&str>) -> Self {
        match value {
            Some("default") => Self::Default,
            Some("two-owners") => Self::TwoOwners,
            _ => Self::None,
        }
    }

    /// Each owner's balls at `layout`, in the order they are admitted.
    pub(crate) fn owners(self, layout: Layout) -> Vec<Vec<FixtureBall>> {
        match self {
            Self::None => Vec::new(),
            Self::Default => vec![ArenaFixture::default_balls(layout)],
            Self::TwoOwners => vec![
                ArenaFixture::default_balls(layout),
                ArenaFixture::outer_balls(layout),
            ],
        }
    }
}

/// A ball to admit: where it starts and how it moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixtureBall {
    pub position: Vec2,
    pub velocity: Vec2,
}

/// A ball the fixture holds now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixtureBallState {
    /// Which of the admitted balls this is. Its body keeps this identity for
    /// as long as it stays in the arena.
    pub slot: usize,
    pub position: Vec2,
    pub velocity: Vec2,
}

/// A fixture's balls now, and how many have fallen out of the view.
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaFixtureState {
    /// Which fixture this is. It keeps the index while it holds the arena.
    pub index: usize,
    pub radius: f32,
    pub balls: Vec<FixtureBallState>,
    pub fallen: usize,
}

pub(crate) struct ArenaFixture {
    index: u8,
    radius: f32,
    /// The slots of the balls still in the arena.
    slots: Vec<usize>,
    fallen: usize,
}

fn entity(index: u8, slot: usize) -> PhysicsId {
    debug_assert!(slot < MAX_FIXTURE_BALLS);
    PhysicsId::new(FIXTURE_OWNER_BALLS[usize::from(index)][0].start + slot as u64)
}

fn body(index: u8, slot: usize) -> BodyId {
    BodyId::new(entity(index, slot), BodyRole::PRIMARY)
}

impl ArenaFixture {
    /// The balls' radius at `layout`: half a pitch, so one fits through the
    /// open drain.
    pub fn radius(layout: Layout) -> f32 {
        layout.pitch * 0.5
    }

    /// Three balls dropped onto the floor, one to each side of the face and
    /// one over the drain, which falls when the floor opens under it.
    pub fn default_balls(layout: Layout) -> Vec<FixtureBall> {
        Self::dropped(layout, &[-0.55, 0.0, 0.55])
    }

    /// Two balls dropped onto the floor nearer the view's edges than the
    /// ordinary ones, where every visit's course has ground.
    pub fn outer_balls(layout: Layout) -> Vec<FixtureBall> {
        Self::dropped(layout, &[-0.85, 0.85])
    }

    /// Balls at rest a little above the floor, at `across` fractions of the
    /// view's half width.
    fn dropped(layout: Layout, across: &[f32]) -> Vec<FixtureBall> {
        let height = layout.floor_y + Self::radius(layout) * 6.0;
        across
            .iter()
            .map(|x| FixtureBall {
                position: Vec2::new(layout.bounds_max.x * x, height),
                velocity: Vec2::ZERO,
            })
            .collect()
    }

    /// Claims the arena as fixture `index` and puts `balls` in it. They fall
    /// at Clock's own gravity, whoever set the world's. The arena admitted
    /// the claim first.
    pub fn admit(arena: &mut ClockArena, layout: Layout, index: u8, balls: &[FixtureBall]) -> Self {
        assert!(!balls.is_empty() && balls.len() <= MAX_FIXTURE_BALLS);
        let radius = Self::radius(layout);
        let owner = Owner::Fixture(index);
        arena.claim_resident(owner, layout);
        let gravity_scale = arena.resident_gravity_scale();
        let world = arena
            .held_mut(owner)
            .expect("the fixture's claim keeps a world");
        for (slot, ball) in balls.iter().enumerate() {
            assert!(world.insert_body(
                body(index, slot),
                BodySpec {
                    position: ball.position,
                    linear_velocity: ball.velocity,
                    gravity_scale,
                    ..BodySpec::default()
                },
                &[ColliderSpec {
                    restitution: 0.3,
                    ..ColliderSpec::ball(
                        ColliderId::new(entity(index, slot), ColliderRole::PRIMARY, 0),
                        radius,
                    )
                }],
            ));
        }
        Self {
            index,
            radius,
            slots: (0..balls.len()).collect(),
            fallen: 0,
        }
    }

    pub fn index(&self) -> usize {
        usize::from(self.index)
    }

    /// Takes the fixture out of the arena, balls and all. The others stay
    /// as they are.
    pub fn remove(self, arena: &mut ClockArena) {
        arena.release(Owner::Fixture(self.index));
    }

    /// The fixture's part of the tick, just after the arena's step: a ball
    /// that has fallen below the view leaves the arena. Returns true once the
    /// last has gone and the fixture has let the arena go.
    pub fn observe(&mut self, arena: &mut ClockArena, layout: Layout) -> bool {
        let owner = Owner::Fixture(self.index);
        let world = arena
            .held_mut(owner)
            .expect("the fixture's claim keeps a world");
        let below = layout.bounds_min.y - self.radius;
        let mut kept = Vec::with_capacity(self.slots.len());
        for &slot in &self.slots {
            let motion = world
                .motion(body(self.index, slot))
                .expect("a ball in the arena");
            // A position that is not finite is lost as surely as a fall.
            if motion.position.y >= below {
                kept.push(slot);
            } else {
                assert!(world.remove_entity(entity(self.index, slot)));
                self.fallen += 1;
            }
        }
        self.slots = kept;
        if self.slots.is_empty() {
            arena.release(owner);
        }
        self.slots.is_empty()
    }

    pub fn state(&self, arena: &ClockArena) -> ArenaFixtureState {
        let world = arena
            .held(Owner::Fixture(self.index))
            .expect("the fixture's claim keeps a world");
        ArenaFixtureState {
            index: self.index(),
            radius: self.radius,
            balls: self
                .slots
                .iter()
                .map(|&slot| {
                    let motion = world
                        .motion(body(self.index, slot))
                        .expect("a ball in the arena");
                    FixtureBallState {
                        slot,
                        position: motion.position,
                        velocity: motion.linear_velocity,
                    }
                })
                .collect(),
            fallen: self.fallen,
        }
    }
}
