use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig,
};
use rand::{Rng, rngs::StdRng};

use crate::{
    SegmentId, SegmentRepresentation, SegmentState, digits,
    floor::DrainGeometry,
    layout::Layout,
    meridiem::{LetterState, PIXEL_SIZE},
};

pub(crate) struct FallingWorld {
    world: PhysicsWorld,
    bodies: FallingBodies,
}

/// One bounded batch, reusable in the standalone arena or a player's world.
/// These IDs never overlap the duck (1), course/panels (1000s), or wall (2000).
pub(crate) struct FallingBodies {
    entities: Vec<PhysicsId>,
}

pub(crate) fn body_id(id: SegmentId) -> BodyId {
    BodyId::new(
        PhysicsId::new(3001 + u64::from(id.digit_slot) * 7 + id.kind as u64),
        BodyRole::PRIMARY,
    )
}

fn letter_body_id(slot: usize) -> BodyId {
    BodyId::new(PhysicsId::new(3032 + slot as u64), BodyRole::PRIMARY)
}

impl FallingWorld {
    pub fn new(
        drain: DrainGeometry,
        segments: &mut [SegmentState],
        letters: &[LetterState],
        rng: &mut StdRng,
    ) -> Self {
        let layout = drain.layout();
        let mut world = Self::arena(drain);
        let bodies = FallingBodies::insert(&mut world, layout, segments, letters, rng);
        Self { world, bodies }
    }

    pub fn into_parts(self) -> (PhysicsWorld, FallingBodies) {
        (self.world, self.bodies)
    }

    pub fn arena(drain: DrainGeometry) -> PhysicsWorld {
        let layout = drain.layout();
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -400.0),
            length_unit: layout.pitch,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(4, 4, 0);
        for (index, (min, max)) in drain
            .slabs()
            .chain([
                (
                    Vec2::new(layout.bounds_min.x - 20.0, layout.bounds_min.y),
                    Vec2::new(layout.bounds_min.x, layout.bounds_max.y + 100.0),
                ),
                (
                    Vec2::new(layout.bounds_max.x, layout.bounds_min.y),
                    Vec2::new(layout.bounds_max.x + 20.0, layout.bounds_max.y + 100.0),
                ),
            ])
            .enumerate()
        {
            // Shared arenas use 1000s for supporting terrain, 2000s for walls.
            let entity = PhysicsId::new(if index < 2 {
                1000 + index as u64
            } else {
                2000 + (index - 2) as u64
            });
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: (min + max) * 0.5,
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    (max.x - min.x) * 0.5,
                    (max.y - min.y) * 0.5
                )],
            ));
        }
        world
    }

    pub fn step(&mut self, segments: &mut [SegmentState], letters: &mut [LetterState]) {
        self.world.step(1.0 / 60.0);
        self.bodies.synchronize(&self.world, segments, letters);
    }

    pub fn body_count(&self) -> usize {
        self.world.body_count()
    }
    pub fn collider_count(&self) -> usize {
        self.world.collider_count()
    }
}

impl FallingBodies {
    pub fn match_gravity(&self, world: &mut PhysicsWorld) {
        let scale = 400.0 / -world.gravity().y;
        for entity in &self.entities {
            assert!(world.set_gravity_scale(BodyId::new(*entity, BodyRole::PRIMARY), scale, false));
        }
    }

    pub fn insert(
        world: &mut PhysicsWorld,
        layout: Layout,
        segments: &mut [SegmentState],
        letters: &[LetterState],
        rng: &mut StdRng,
    ) -> Self {
        let mut entities = Vec::with_capacity(30);
        let lit = segments.iter().filter(|s| s.lit);
        let collider_count = lit
            .clone()
            .map(|s| digits::cells(s.id.kind).len())
            .sum::<usize>()
            + letters
                .iter()
                .map(|l| l.glyph.cells().count())
                .sum::<usize>();
        world.reserve(lit.count() + letters.len(), collider_count, 0);
        // Preserve Falling's established acceleration without changing the
        // smaller duck's calibrated gravity/jump on narrow displays.
        let gravity_scale = 400.0 / -world.gravity().y;
        for segment in segments.iter_mut().filter(|segment| segment.lit) {
            let id = body_id(segment.id);
            let position = layout.segment_center(segment.id);
            let colliders = digits::cells(segment.id.kind)
                .iter()
                .enumerate()
                .map(|(part, cell)| {
                    let mut collider = ColliderSpec::cuboid(
                        ColliderId::new(id.entity, ColliderRole::PRIMARY, part as u16),
                        layout.pitch * 0.4,
                        layout.pitch * 0.4,
                    );
                    collider.local_position = layout.cell_center(segment.id, *cell) - position;
                    collider.friction = 0.65;
                    collider.restitution = 0.35;
                    collider
                })
                .collect::<Vec<_>>();
            assert!(world.insert_body(
                id,
                BodySpec {
                    position,
                    gravity_scale,
                    linear_velocity: Vec2::new(
                        rng.random_range(-45.0..45.0),
                        rng.random_range(15.0..70.0)
                    ),
                    angular_velocity: rng.random_range(-2.8..2.8),
                    linear_damping: 0.1,
                    angular_damping: 0.25,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &colliders
            ));
            entities.push(id.entity);
            segment.representation = SegmentRepresentation::Rigid {
                position,
                angle: 0.0,
            };
        }
        for (slot, letter) in letters.iter().enumerate() {
            let id = letter_body_id(slot);
            let half = layout.pitch * PIXEL_SIZE * 0.5;
            let colliders = letter
                .glyph
                .cells()
                .enumerate()
                .map(|(part, cell)| {
                    let mut collider = ColliderSpec::cuboid(
                        ColliderId::new(id.entity, ColliderRole::PRIMARY, part as u16),
                        half,
                        half,
                    );
                    collider.local_position =
                        letter.glyph.cell_center(layout, cell) - letter.position;
                    collider.friction = 0.65;
                    collider.restitution = 0.35;
                    collider
                })
                .collect::<Vec<_>>();
            assert!(world.insert_body(
                id,
                BodySpec {
                    position: letter.position,
                    gravity_scale,
                    linear_velocity: Vec2::new(
                        rng.random_range(-45.0..45.0),
                        rng.random_range(15.0..70.0)
                    ),
                    angular_velocity: rng.random_range(-2.8..2.8),
                    linear_damping: 0.1,
                    angular_damping: 0.25,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &colliders
            ));
            entities.push(id.entity);
        }
        Self { entities }
    }

    pub fn synchronize(
        &self,
        world: &PhysicsWorld,
        segments: &mut [SegmentState],
        letters: &mut [LetterState],
    ) {
        for segment in segments {
            if let Some(motion) = world.motion(body_id(segment.id)) {
                segment.representation = SegmentRepresentation::Rigid {
                    position: motion.position,
                    angle: motion.angle,
                };
            }
        }
        for (slot, letter) in letters.iter_mut().enumerate() {
            let motion = world
                .motion(letter_body_id(slot))
                .expect("live falling letter");
            letter.position = motion.position;
            letter.angle = motion.angle;
        }
    }

    pub fn remove(self, world: &mut PhysicsWorld) {
        for entity in self.entities {
            assert!(world.remove_entity(entity), "live Falling batch");
        }
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    use crate::events::duck::DuckEvent;
    use rand::SeedableRng;

    #[test]
    fn moving_the_falling_world_preserves_every_body_motion_and_mass() {
        for aspect in [4.0 / 3.0, 0.6] {
            let layout = Layout::new(aspect);
            let drain = crate::floor::test_drain(layout);
            let mut segments = digits::create_segments();
            for segment in &mut segments {
                segment.lit = true;
            }
            let mut falling =
                FallingWorld::new(drain, &mut segments, &[], &mut StdRng::seed_from_u64(42));
            for _ in 0..100 {
                falling.step(&mut segments, &mut []);
            }
            let motions = falling.world.motions().collect::<Vec<_>>();
            let masses = motions
                .iter()
                .map(|r| falling.world.body_mass(r.id))
                .collect::<Vec<_>>();
            let collider_count = falling.world.collider_count();
            let (world, bodies) = falling.into_parts();
            let mut duck = DuckEvent::new_drain_player(drain, 42, 1, 1, world);
            bodies.match_gravity(duck.arena_world_mut());
            let world = duck.arena_world();
            assert_eq!(world.motions().collect::<Vec<_>>(), motions);
            assert_eq!(world.collider_count(), collider_count);
            assert_eq!(
                motions
                    .iter()
                    .map(|r| world.body_mass(r.id))
                    .collect::<Vec<_>>(),
                masses
            );
        }
    }
}
