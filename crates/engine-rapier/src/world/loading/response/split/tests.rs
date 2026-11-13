use super::*;
use engine_material::{ARMOR, WOOD};

const DT: f32 = 1.0 / 120.0;
/// Each strike at this speed charges about a fifth of the wood's capacity.
const SPEED: f32 = 5.0;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64, part: u16) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, part)
}
fn region(n: u64, part: u32) -> RegionId {
    RegionId {
        entity: PhysicsId::new(n),
        part,
    }
}

/// A ball of `parts` equal colliders: body 1 moves, every other body is fixed.
fn insert_ball(world: &mut PhysicsWorld, n: u64, parts: u16, position: Vec2, speed: f32) {
    let specs: Vec<_> = (0..parts)
        .map(|part| {
            let mut spec = ColliderSpec::ball(collider(n, part), 0.5);
            spec.friction = 0.0;
            spec.mass_properties = Some(ColliderMassProperties {
                center: Vec2::ZERO,
                mass: 20.0 / f32::from(parts),
                inertia: 10.0 / f32::from(parts),
            });
            spec
        })
        .collect();
    assert!(world.insert_body(
        body(n),
        BodySpec {
            position,
            linear_velocity: Vec2::X * speed,
            kind: if n == 1 {
                BodyKind::Dynamic
            } else {
                BodyKind::Fixed
            },
            can_sleep: false,
            ..Default::default()
        },
        &specs
    ));
    world.refresh_mass_properties(body(n));
}

/// An armor ball about to strike a fixed wood ball.
fn strike(parts: u16) -> (PhysicsWorld, [RegionToken; 2]) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    insert_ball(&mut world, 1, parts, Vec2::ZERO, SPEED);
    insert_ball(&mut world, 2, parts, Vec2::X, 0.0);
    let tokens = [(1, ARMOR), (2, WOOD)].map(|(n, material)| {
        let token = world.create_loading_region(region(n, 0), body(n)).unwrap();
        world
            .register_loading_material(
                token,
                Region::new(material, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        for part in 0..parts {
            world
                .bind_loading_collider(
                    collider(n, part),
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        token
    });
    (world, tokens)
}

fn stored(world: &PhysicsWorld, token: RegionToken) -> &StoredRegion {
    &world
        .loading
        .as_ref()
        .unwrap()
        .response
        .as_ref()
        .unwrap()
        .materials[&token]
}
fn balance(world: &PhysicsWorld, token: RegionToken) -> Region {
    stored(world, token).region().unwrap()
}
fn in_contact(world: &PhysicsWorld, token: RegionToken) -> bool {
    world.loading.as_ref().unwrap().in_contact(token)
}
fn source(world: &PhysicsWorld, collider: ColliderId) -> Option<ContactSource> {
    world
        .loading
        .as_ref()
        .unwrap()
        .bindings
        .get(&collider)
        .copied()
}

/// Step until the strike has charged the wood and the pair has separated.
fn strike_and_separate(world: &mut PhysicsWorld, wood: RegionToken) {
    let before = balance(world, wood).remaining_precise();
    for _ in 0..240 {
        world.step(DT);
        if balance(world, wood).remaining_precise() < before && !in_contact(world, wood) {
            return;
        }
    }
    panic!("the strike did not charge the wood and separate");
}

fn separate(world: &mut PhysicsWorld, wood: RegionToken) {
    for _ in 0..240 {
        world.step(DT);
        if !in_contact(world, wood) {
            return;
        }
    }
    panic!("the pair did not separate");
}

fn assert_restores_and_replays(world: &PhysicsWorld) {
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    let mut world = world.clone();
    for _ in 0..30 {
        world.step(DT);
        restored.step(DT);
    }
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        world.snapshot_bytes().unwrap()
    );
}

#[test]
fn a_split_partitions_the_damaged_balance_and_moves_the_named_colliders() {
    let mut first = None;
    for parts in [1, 2, 4] {
        let (mut world, [_, wood]) = strike(parts);
        strike_and_separate(&mut world, wood);
        let whole = balance(&world, wood);
        assert!(whole.integrity() < 1.0 && !whole.failed());
        let piece = RegionPiece {
            id: region(2, 1),
            body: body(2),
            fraction: 0.25,
            section: Section::new(0.25, 1.0),
            colliders: (parts / 2..parts).map(|part| collider(2, part)).collect(),
        };
        let new = world
            .split_loading_region(wood, Section::new(0.75, 1.0), &piece)
            .unwrap();
        assert_eq!(new.id, piece.id);
        assert!(world.loading_region_is_live(wood) && world.loading_region_is_live(new));
        let [kept, part] = [wood, new].map(|t| balance(&world, t));
        assert_eq!([part.volume(), kept.volume()], [5.0, 15.0]);
        let held = part.remaining_precise() + kept.remaining_precise();
        assert!(held <= whole.remaining_precise());
        assert!(whole.remaining_precise() - held <= 1.0e-9 * f64::from(whole.capacity()));
        for r in [kept, part] {
            assert!((r.integrity() - whole.integrity()).abs() <= f32::EPSILON);
        }
        assert_eq!(
            [wood, new].map(|t| stored(&world, t).section()),
            [Section::new(0.75, 1.0), piece.section]
        );
        for part in 0..parts {
            let owner = if piece.colliders.contains(&collider(2, part)) {
                new
            } else {
                wood
            };
            assert_eq!(
                source(&world, collider(2, part)),
                Some(ContactSource {
                    region: owner,
                    patch: 0
                })
            );
        }
        assert_restores_and_replays(&world);
        // Equal collider partitions share one region, so they split alike.
        let balances = [kept, part].map(|r| r.remaining_precise());
        assert_eq!(*first.get_or_insert(balances), balances, "{parts} parts");
    }
}

#[test]
fn a_piece_can_become_a_fragment_body_and_leave_with_its_share() {
    let (mut world, [_, wood]) = strike(1);
    strike_and_separate(&mut world, wood);
    let whole = balance(&world, wood);
    insert_ball(&mut world, 3, 1, Vec2::new(1.0, 3.0), 0.0);
    let piece = RegionPiece {
        id: region(3, 0),
        body: body(3),
        fraction: 0.5,
        section: Section::new(0.5, 1.0),
        colliders: Vec::new(),
    };
    let fragment = world
        .split_loading_region(wood, Section::new(0.5, 1.0), &piece)
        .unwrap();
    world
        .bind_loading_collider(
            collider(3, 0),
            ContactSource {
                region: fragment,
                patch: 0,
            },
        )
        .unwrap();
    let parts = [wood, fragment].map(|t| balance(&world, t));
    assert_eq!(parts.map(|r| r.volume()), [10.0, 10.0]);
    assert!(parts.iter().map(Region::remaining_precise).sum::<f64>() <= whole.remaining_precise());
    assert_restores_and_replays(&world);
    assert!(world.remove_entity(PhysicsId::new(3)));
    assert!(!world.loading_region_is_live(fragment));
    assert_eq!(balance(&world, wood), parts[0]);
    assert_restores_and_replays(&world);
}

#[test]
fn a_rebuilt_body_keeps_the_balance_its_region_moved_with() {
    let (mut reference, [_, wood]) = strike(1);
    strike_and_separate(&mut reference, wood);
    let mut rebuilt = reference.clone();
    insert_ball(&mut rebuilt, 3, 1, Vec2::X, 0.0);
    let before = stored(&rebuilt, wood).clone();
    let moved = rebuilt
        .transfer_loading_region(wood, region(3, 0), body(3))
        .unwrap();
    assert!(!rebuilt.loading_region_is_live(wood));
    assert_eq!(source(&rebuilt, collider(2, 0)), None);
    assert_eq!(stored(&rebuilt, moved), &before);
    rebuilt
        .bind_loading_collider(
            collider(3, 0),
            ContactSource {
                region: moved,
                patch: 0,
            },
        )
        .unwrap();
    assert!(rebuilt.remove_entity(PhysicsId::new(2)));
    assert_eq!(stored(&rebuilt, moved), &before);
    assert_restores_and_replays(&rebuilt);
    // The same second strike charges the moved balance as it does the original.
    for (world, token) in [(&mut reference, wood), (&mut rebuilt, moved)] {
        world.set_pose(body(1), -Vec2::X, 0.0, true);
        world.set_velocity(body(1), Vec2::X * SPEED, 0.0, true);
        strike_and_separate(world, token);
    }
    assert!(balance(&reference, wood).remaining_precise() < before.remaining);
    assert_eq!(balance(&rebuilt, moved), balance(&reference, wood));
}

#[test]
fn contact_and_invalid_requests_refuse_without_changing_the_world() {
    let (mut world, [_, wood]) = strike(2);
    let kept = Section::new(0.5, 1.0);
    let piece = RegionPiece {
        id: region(2, 1),
        body: body(2),
        fraction: 0.5,
        section: Section::new(0.5, 1.0),
        colliders: vec![collider(2, 1)],
    };
    // While the strike is in contact, a changed region could be struck anew.
    for _ in 0..60 {
        if in_contact(&world, wood) {
            break;
        }
        world.step(DT);
    }
    assert!(in_contact(&world, wood));
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.split_loading_region(wood, kept, &piece),
        Err(LoadingError::RegionInContact)
    );
    assert_eq!(
        world.transfer_loading_region(wood, region(2, 1), body(2)),
        Err(LoadingError::RegionInContact)
    );
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);

    separate(&mut world, wood);
    let bytes = world.snapshot_bytes().unwrap();
    let with = |change: fn(&mut RegionPiece)| {
        let mut piece = piece.clone();
        change(&mut piece);
        piece
    };
    let refusals = [
        (with(|p| p.body = body(9)), LoadingError::UnknownBody),
        (with(|p| p.id = region(1, 1)), LoadingError::UnknownBody),
        (with(|p| p.id = region(2, 0)), LoadingError::RegionExists),
        (with(|p| p.fraction = 0.0), LoadingError::InvalidMaterial),
        (with(|p| p.fraction = 1.0), LoadingError::InvalidMaterial),
        (
            with(|p| p.fraction = f32::NAN),
            LoadingError::InvalidMaterial,
        ),
        (
            with(|p| p.section.depth = 0.0),
            LoadingError::InvalidMaterial,
        ),
        (
            with(|p| p.colliders = vec![collider(1, 0)]),
            LoadingError::InvalidBinding,
        ),
        (
            with(|p| p.colliders = vec![collider(7, 0)]),
            LoadingError::InvalidBinding,
        ),
        (
            with(|p| p.colliders = vec![collider(2, 1), collider(2, 1)]),
            LoadingError::InvalidBinding,
        ),
        (
            with(|p| {
                p.id = region(1, 1);
                p.body = body(1);
            }),
            LoadingError::InvalidBinding,
        ),
    ];
    for (bad, reason) in refusals {
        assert_eq!(
            world.split_loading_region(wood, kept, &bad),
            Err(reason),
            "{bad:?}"
        );
    }
    assert_eq!(
        world.split_loading_region(wood, Section::new(0.0, 1.0), &piece),
        Err(LoadingError::InvalidMaterial)
    );
    let stale = RegionToken {
        id: wood.id,
        incarnation: wood.incarnation + 7,
    };
    assert_eq!(
        world.split_loading_region(stale, kept, &piece),
        Err(LoadingError::StaleRegion)
    );
    for (to, onto, reason) in [
        (region(2, 0), body(2), LoadingError::RegionExists),
        (region(9, 0), body(9), LoadingError::UnknownBody),
        (region(1, 1), body(2), LoadingError::UnknownBody),
    ] {
        assert_eq!(world.transfer_loading_region(wood, to, onto), Err(reason));
    }
    assert_eq!(
        world.transfer_loading_region(stale, region(2, 1), body(2)),
        Err(LoadingError::StaleRegion)
    );
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);

    // A full registry refuses a split's new region but not a transfer, which
    // adds none.
    let mut full = world.clone();
    full.loading.as_mut().unwrap().config.max_regions = 2;
    assert_eq!(
        full.split_loading_region(wood, kept, &piece),
        Err(LoadingError::Capacity)
    );
    assert!(
        full.transfer_loading_region(wood, region(2, 1), body(2))
            .is_ok()
    );
    // Without a registered material there is no balance to split or move.
    let bare = world.create_loading_region(region(2, 2), body(2)).unwrap();
    assert_eq!(
        world.split_loading_region(bare, kept, &piece),
        Err(LoadingError::InvalidMaterial)
    );
    assert_eq!(
        world.transfer_loading_region(bare, region(2, 3), body(2)),
        Err(LoadingError::InvalidMaterial)
    );
    world.loading.as_mut().unwrap().response = None;
    assert_eq!(
        world.split_loading_region(wood, kept, &piece),
        Err(LoadingError::ResponseDisabled)
    );
}
