//! The qualified full-face fixtures: a platen over a fixed layer, a free pair,
//! and the split case's striker and wood.
use crate::{BodySetup, Case, split};
use engine_core::Vec2;
use engine_material::{ARMOR, Region, Section, WOOD};
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
/// Each body has one block collider.
pub fn collider(role: u64) -> ColliderId {
    ColliderId::new(body(role).entity, ColliderRole::PRIMARY, 0)
}
/// The split case's armor striker, at full capacity.
pub fn striker() -> Region {
    Region::new(ARMOR, 20.0).unwrap()
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
    if case == Case::RegionSplit {
        return striker_and_wood();
    }
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
/// A free armor striker 1/16 from a fixed wood block, along +x with no
/// external force. Only ordinary steps move it, through the native response.
fn striker_and_wood() -> (PhysicsWorld, [Option<RegionToken>; 2]) {
    let turn = QuarterTurn::Deg0;
    let mut world = empty_world(Vec2::ZERO);
    let striker = insert(
        &mut world,
        0,
        BodyKind::Dynamic,
        turn,
        Vec2::X * (-0.5 - split::GAP),
        split::STRIKER_MASS,
        Some(striker()),
    );
    let wood = insert(
        &mut world,
        1,
        BodyKind::Fixed,
        turn,
        Vec2::X * 0.5,
        20.0,
        Some(split::whole()),
    );
    world.set_velocity(body(0), Vec2::X * split::STRIKER_SPEED, 0.0, true);
    (world, [Some(striker), Some(wood)])
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
    let shape = shape(world, role, kind, turn, position, Vec2::new(0.5, 1.0), mass);
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
            shape,
            ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
    token
}
/// A fixed block with no region: the split case adds its fragment and the
/// wood's replacement this way, and the split or transfer gives them material.
pub fn block(world: &mut PhysicsWorld, role: u64, position: Vec2, half: Vec2) {
    shape(
        world,
        role,
        BodyKind::Fixed,
        QuarterTurn::Deg0,
        position,
        half,
        20.0,
    );
}
/// A frictionless, rotation-locked body of one cuboid, with no region.
fn shape(
    world: &mut PhysicsWorld,
    role: u64,
    kind: BodyKind,
    turn: QuarterTurn,
    position: Vec2,
    half: Vec2,
    mass: f32,
) -> ColliderId {
    let id = body(role);
    let mut shape = ColliderSpec::cuboid(collider(role), half.x, half.y);
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
    shape.id
}
