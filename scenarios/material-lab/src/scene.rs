//! The qualified full-face two-body fixtures: a platen over a fixed layer, and a free pair.
use crate::{BodySetup, Case};
use engine_core::Vec2;
use engine_material::{Region, Section, WOOD};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole,
    ColliderSpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig, QuarterTurn,
    loading::{CaptureConfig, ContactSource, RegionId, RegionToken},
};

/// Represented volume of the ordinary fixed support.
const VOLUME: f32 = 20.0;

pub fn body(role: u64) -> BodyId {
    BodyId::new(PhysicsId::new(role + 1), BodyRole::PRIMARY)
}
/// Each pair layer starts undamaged, at its own represented volume.
pub fn pair_region(body: BodySetup) -> Region {
    Region::new(body.material, body.volume).unwrap()
}
pub fn support_region(case: Case) -> Region {
    if case == Case::SupportFailure {
        Region::new(WOOD, 2.0).unwrap()
    } else {
        Region::with_remaining(WOOD, VOLUME, 1200.0).unwrap()
    }
}
/// Material tokens are in body order; an ideal rigid platen has none.
pub fn build(case: Case) -> (PhysicsWorld, [Option<RegionToken>; 2]) {
    let Some(pair) = case.free_pair() else {
        return platen(case);
    };
    // A free pair meets along +x with no external force.
    let turn = QuarterTurn::Deg0;
    let mut world = empty_world(Vec2::ZERO);
    let tokens = [0, 1].map(|role| {
        let setup = pair[role as usize];
        Some(insert(
            &mut world,
            role,
            BodyKind::Dynamic,
            turn,
            Vec2::X * setup.position,
            setup.mass,
            Some(pair_region(setup)),
        ))
    });
    for role in [0, 1] {
        world.set_velocity(
            body(role),
            Vec2::X * pair[role as usize].velocity,
            0.0,
            true,
        );
    }
    (world, tokens)
}
fn platen(case: Case) -> (PhysicsWorld, [Option<RegionToken>; 2]) {
    let (weight, gap, _) = case.parameters();
    // The frame is turned so the platen's weight points down.
    let turn = QuarterTurn::Deg270;
    let mut world = empty_world(turn.rotate(Vec2::X * (weight / 20.0)));
    insert(
        &mut world,
        0,
        BodyKind::Dynamic,
        turn,
        turn.rotate(Vec2::X * -gap),
        20.0,
        None,
    );
    let material = insert(
        &mut world,
        1,
        BodyKind::Fixed,
        turn,
        turn.rotate(Vec2::X),
        20.0,
        Some(support_region(case)),
    );
    (world, [None, Some(material)])
}
fn empty_world(gravity: Vec2) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    world
}
fn insert(
    world: &mut PhysicsWorld,
    role: u64,
    kind: BodyKind,
    turn: QuarterTurn,
    position: Vec2,
    mass: f32,
    material: Option<Region>,
) -> RegionToken {
    let id = body(role);
    let mut shape = ColliderSpec::cuboid(
        ColliderId::new(id.entity, ColliderRole::PRIMARY, 0),
        0.5,
        1.0,
    );
    shape.friction = 0.0;
    shape.mass_properties = Some(ColliderMassProperties {
        center: Vec2::ZERO,
        mass,
        inertia: mass,
    });
    world.insert_body(
        id,
        BodySpec {
            kind,
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
    if let Some(region) = material {
        world
            .register_loading_material(token, region, Section::new(1.0, 1.0))
            .unwrap();
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
    token
}
