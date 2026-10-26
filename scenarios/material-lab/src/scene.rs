//! The qualified full-face, downward-facing two-body fixture.
use crate::Case;
use engine_core::Vec2;
use engine_material::{Region, Section, WOOD};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole,
    ColliderSpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig, QuarterTurn,
    loading::{CaptureConfig, ContactSource, RegionId, RegionToken},
};

pub fn body(role: u64) -> BodyId {
    BodyId::new(PhysicsId::new(role + 1), BodyRole::PRIMARY)
}
pub fn build(case: Case) -> (PhysicsWorld, RegionToken) {
    let (weight, gap, _) = case.parameters();
    let turn = QuarterTurn::Deg270;
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: turn.rotate(Vec2::X * (weight / 20.0)),
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    let mut material = None;
    for role in 0..2 {
        let id = body(role);
        let mut shape = ColliderSpec::cuboid(
            ColliderId::new(id.entity, ColliderRole::PRIMARY, 0),
            0.5,
            1.0,
        );
        shape.friction = 0.0;
        shape.mass_properties = Some(ColliderMassProperties {
            center: Vec2::ZERO,
            mass: 20.0,
            inertia: 20.0,
        });
        let position = turn.rotate(Vec2::X * if role == 0 { -gap } else { 1.0 });
        world.insert_body(
            id,
            BodySpec {
                kind: if role == 0 {
                    BodyKind::Dynamic
                } else {
                    BodyKind::Fixed
                },
                position,
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &[shape.clone()],
        );
        world.set_quarter_turn_pose(id, position, turn, true);
        world.refresh_mass_properties(id);
        let token = world
            .create_loading_region(
                RegionId {
                    entity: id.entity,
                    part: 0,
                },
                id,
            )
            .unwrap();
        if role == 1 {
            world
                .register_loading_material(
                    token,
                    Region::with_remaining(WOOD, 20.0, 1200.0).unwrap(),
                    Section::new(1.0, 1.0),
                )
                .unwrap();
            material = Some(token);
        }
        world
            .bind_loading_collider(
                shape.id,
                ContactSource {
                    region: token,
                    patch: 0,
                },
            )
            .unwrap();
    }
    (world, material.unwrap())
}
