use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig,
};
use rand::{Rng, rngs::StdRng};

use crate::{SegmentId, SegmentRepresentation, SegmentState, digits, layout::Layout};

pub(crate) struct FallingWorld {
    world: PhysicsWorld,
}

fn body_id(id: SegmentId) -> BodyId {
    BodyId::new(
        PhysicsId::new(1 + u64::from(id.digit_slot) * 7 + id.kind as u64),
        BodyRole::PRIMARY,
    )
}

impl FallingWorld {
    pub fn new(layout: Layout, segments: &mut [SegmentState], rng: &mut StdRng) -> Self {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -400.0),
            length_unit: layout.pitch,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(32, 100, 0);
        let drain = layout.drain_half_width();
        for (index, (min, max)) in [
            (
                Vec2::new(layout.bounds_min.x, layout.bounds_min.y),
                Vec2::new(-drain, layout.floor_y),
            ),
            (
                Vec2::new(drain, layout.bounds_min.y),
                Vec2::new(layout.bounds_max.x, layout.floor_y),
            ),
            (
                Vec2::new(layout.bounds_min.x - 20.0, layout.bounds_min.y),
                Vec2::new(layout.bounds_min.x, layout.bounds_max.y + 100.0),
            ),
            (
                Vec2::new(layout.bounds_max.x, layout.bounds_min.y),
                Vec2::new(layout.bounds_max.x + 20.0, layout.bounds_max.y + 100.0),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let entity = PhysicsId::new(100 + index as u64);
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
            segment.representation = SegmentRepresentation::Rigid {
                position,
                angle: 0.0,
            };
        }
        Self { world }
    }

    pub fn step(&mut self, segments: &mut [SegmentState]) {
        self.world.step(1.0 / 60.0);
        for segment in segments {
            if let Some(motion) = self.world.motion(body_id(segment.id)) {
                segment.representation = SegmentRepresentation::Rigid {
                    position: motion.position,
                    angle: motion.angle,
                };
            }
        }
    }

    pub fn body_count(&self) -> usize {
        self.world.body_count()
    }
    pub fn collider_count(&self) -> usize {
        self.world.collider_count()
    }
}
