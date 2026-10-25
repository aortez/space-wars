use super::*;

pub(in crate::world::loading::response::gravity) fn world(
    ids: [u64; 3],
    reverse: bool,
    slices: [u16; 3],
    copies: u16,
) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::X * 10.0,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    for i in if reverse { [2, 1, 0] } else { [0, 1, 2] } {
        let mut colliders = Vec::new();
        for slice in 0..slices[i] {
            for copy in 0..copies {
                let id = ColliderId::new(
                    body(ids[i]).entity,
                    ColliderRole::PRIMARY,
                    slice * copies + copy,
                );
                let mut collider = ColliderSpec::cuboid(id, 0.5, 1.0 / f32::from(slices[i]));
                collider.local_position.y =
                    -1.0 + (2.0 * f32::from(slice) + 1.0) / f32::from(slices[i]);
                collider.friction = 0.0;
                collider.mass_properties = Some(ColliderMassProperties {
                    center: Vec2::ZERO,
                    mass: 20.0 / f32::from(slices[i] * copies),
                    inertia: 10.0 / f32::from(slices[i] * copies),
                });
                colliders.push(collider);
            }
        }
        if reverse {
            colliders.reverse();
        }
        assert!(world.insert_body(
            body(ids[i]),
            BodySpec {
                kind: if i == 2 {
                    BodyKind::Fixed
                } else {
                    BodyKind::Dynamic
                },
                position: Vec2::X * if i == 0 { -0.02 } else { i as f32 },
                linear_velocity: Vec2::X * if i == 0 { 4.0 } else { 0.0 },
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &colliders,
        ));
        world.refresh_mass_properties(body(ids[i]));
        let token = world
            .create_loading_region(
                RegionId {
                    entity: body(ids[i]).entity,
                    part: 0,
                },
                body(ids[i]),
            )
            .unwrap();
        world
            .register_loading_material(
                token,
                Region::new(if i == 0 { ARMOR } else { WOOD }, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        for collider in colliders {
            world
                .bind_loading_collider(
                    collider.id,
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
    }
    world
}
