//! The arena fixture: a few balls that belong to the run, with no plants or
//! soil. It is the smallest resident that rests on the floor, and the arena
//! contract uses it to check that a resident keeps its bodies, their motion
//! and their identities while visits and events come and go. Each ball rests
//! on whatever actually supports it and leaves only by falling out of the
//! view.

use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
};

use crate::{
    arena::{ClockArena, Owner, ids::canonical::FIXTURE_BALLS},
    layout::Layout,
};

/// The most balls the fixture holds, one identity each.
pub const MAX_FIXTURE_BALLS: usize = 4;
const _: () = assert!(FIXTURE_BALLS.end - FIXTURE_BALLS.start == MAX_FIXTURE_BALLS as u64);

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

/// The fixture's balls now, and how many have fallen out of the view.
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaFixtureState {
    pub radius: f32,
    pub balls: Vec<FixtureBallState>,
    pub fallen: usize,
}

pub(crate) struct ArenaFixture {
    radius: f32,
    /// The slots of the balls still in the arena.
    slots: Vec<usize>,
    fallen: usize,
}

fn entity(slot: usize) -> PhysicsId {
    debug_assert!(slot < MAX_FIXTURE_BALLS);
    PhysicsId::new(FIXTURE_BALLS.start + slot as u64)
}

fn body(slot: usize) -> BodyId {
    BodyId::new(entity(slot), BodyRole::PRIMARY)
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
        let height = layout.floor_y + Self::radius(layout) * 6.0;
        [-0.55, 0.0, 0.55]
            .map(|x| FixtureBall {
                position: Vec2::new(layout.bounds_max.x * x, height),
                velocity: Vec2::ZERO,
            })
            .to_vec()
    }

    /// Claims the arena for `balls` and puts them in it. They fall at Clock's
    /// own gravity, whoever set the world's.
    pub fn admit(arena: &mut ClockArena, layout: Layout, balls: &[FixtureBall]) -> Self {
        assert!(!balls.is_empty() && balls.len() <= MAX_FIXTURE_BALLS);
        let radius = Self::radius(layout);
        arena.claim_resident(Owner::Fixture, layout);
        let gravity_scale = arena.resident_gravity_scale();
        let world = arena
            .held_mut(Owner::Fixture)
            .expect("the fixture's claim keeps a world");
        for (slot, ball) in balls.iter().enumerate() {
            assert!(world.insert_body(
                body(slot),
                BodySpec {
                    position: ball.position,
                    linear_velocity: ball.velocity,
                    gravity_scale,
                    ..BodySpec::default()
                },
                &[ColliderSpec {
                    restitution: 0.3,
                    ..ColliderSpec::ball(
                        ColliderId::new(entity(slot), ColliderRole::PRIMARY, 0),
                        radius,
                    )
                }],
            ));
        }
        Self {
            radius,
            slots: (0..balls.len()).collect(),
            fallen: 0,
        }
    }

    /// The fixture's part of the tick, just after the arena's step: a ball
    /// that has fallen below the view leaves the arena. Returns true once the
    /// last has gone and the fixture has let the arena go.
    pub fn observe(&mut self, arena: &mut ClockArena, layout: Layout) -> bool {
        let world = arena
            .held_mut(Owner::Fixture)
            .expect("the fixture's claim keeps a world");
        let below = layout.bounds_min.y - self.radius;
        let mut kept = Vec::with_capacity(self.slots.len());
        for &slot in &self.slots {
            let motion = world.motion(body(slot)).expect("a ball in the arena");
            // A position that is not finite is lost as surely as a fall.
            if motion.position.y >= below {
                kept.push(slot);
            } else {
                assert!(world.remove_entity(entity(slot)));
                self.fallen += 1;
            }
        }
        self.slots = kept;
        if self.slots.is_empty() {
            arena.release(Owner::Fixture);
        }
        self.slots.is_empty()
    }

    pub fn state(&self, arena: &ClockArena) -> ArenaFixtureState {
        let world = arena
            .held(Owner::Fixture)
            .expect("the fixture's claim keeps a world");
        ArenaFixtureState {
            radius: self.radius,
            balls: self
                .slots
                .iter()
                .map(|&slot| {
                    let motion = world.motion(body(slot)).expect("a ball in the arena");
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
