//! Shared public-API rectangle fixture, independent of native history internals.
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
pub fn shape(role: u64, part: u16, mass: f32, pieces: u16) -> ColliderSpec {
    let mut c = ColliderSpec::cuboid(
        ColliderId::new(body(role).entity, ColliderRole::PRIMARY, part),
        0.5,
        1.0,
    );
    c.friction = 0.0;
    c.mass_properties = Some(ColliderMassProperties {
        center: Vec2::ZERO,
        mass: mass / f32::from(pieces),
        inertia: mass / f32::from(pieces),
    });
    c
}

pub fn scene(
    mass: f32,
    weight: f32,
    parts: u16,
    copies: u16,
    turn: QuarterTurn,
    offset: Vec2,
    reverse: bool,
) -> (PhysicsWorld, RegionToken) {
    fixture(
        mass,
        weight,
        (parts, copies, reverse),
        (turn, offset),
        1200.0,
    )
}
pub fn fixture(
    mass: f32,
    weight: f32,
    layout: (u16, u16, bool),
    frame: (QuarterTurn, Vec2),
    remaining: f64,
) -> (PhysicsWorld, RegionToken) {
    let (parts, copies, reverse) = layout;
    let (turn, offset) = frame;
    let mut w = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: turn.rotate(Vec2::X * (weight / mass)),
        ..Default::default()
    });
    w.enable_loading_capture(CaptureConfig::default()).unwrap();
    w.enable_material_response().unwrap();
    let mut token = None;
    for role in if reverse { [1, 0] } else { [0, 1] } {
        let mut shapes = Vec::new();
        for part in 0..parts * copies {
            let mut c = shape(role, part, mass, parts * copies);
            c.shape = engine_rapier::world::ColliderShape::Cuboid {
                half_width: 0.5,
                half_height: 1.0 / f32::from(parts),
            };
            c.local_position.y = -1.0 + f32::from(2 * (part / copies) + 1) / f32::from(parts);
            shapes.push(c);
        }
        if reverse {
            shapes.reverse();
        }
        let position = offset + turn.rotate(Vec2::X * role as f32);
        w.insert_body(
            body(role),
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
            &shapes,
        );
        w.set_quarter_turn_pose(body(role), position, turn, true);
        w.refresh_mass_properties(body(role));
        let t = w
            .create_loading_region(
                RegionId {
                    entity: body(role).entity,
                    part: 0,
                },
                body(role),
            )
            .unwrap();
        if role == 1 {
            w.register_loading_material(
                t,
                Region::with_remaining(WOOD, 20.0, remaining).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
            token = Some(t);
        }
        for c in shapes {
            w.bind_loading_collider(
                c.id,
                ContactSource {
                    region: t,
                    patch: 0,
                },
            )
            .unwrap();
        }
    }
    (w, token.unwrap())
}
