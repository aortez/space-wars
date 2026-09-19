use crate::{
    tests::{DT, assert_accounting, spec},
    *,
};

fn world(capacity: usize) -> WaterWorld {
    WaterWorld::new(
        WaterConfig {
            max_parcels: capacity,
            ..WaterConfig::default()
        },
        vec![
            spec(-10.0, 20.0, vec![40.0; 2], [Boundary::Closed; 2]),
            spec(-100.0, 200.0, vec![0.0; 8], [Boundary::Closed; 2]),
        ],
    )
    .unwrap()
}

#[test]
fn removing_and_restoring_support_transfers_volume_without_injection_or_ghosts() {
    let mut world = world(32);
    world.add_to_pool(0, -5.0, 20.0).unwrap();
    world.add_to_pool(0, 5.0, 30.0).unwrap();
    let before = world.stats();
    world.set_pool_supports(&[false, true]).unwrap();
    let released = world.stats();
    assert_eq!(released.injected, before.injected);
    assert_eq!(released.reclaimed, 0.0);
    assert_eq!(released.drained, 0.0);
    assert_eq!(released.pooled, 0.0);
    assert_eq!(released.in_flight, 50.0);
    assert_eq!(released.parcels, 2);
    assert_eq!(world.pools()[0].columns().count(), 0);
    assert_eq!(world.pools()[0].columns_in_range(-20.0, 20.0).count(), 0);
    assert!(world.sample(Vec2::new(0.0, 40.5)).is_none());
    assert_eq!(
        world.add_to_pool(0, 0.0, 1.0),
        Err(WaterError::InvalidInput)
    );
    world.set_pool_supports(&[false, true]).unwrap();
    assert_eq!(world.stats(), released, "idempotent removal");
    for _ in 0..120 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert!((world.pools()[1].columns().map(|c| c.volume).sum::<f64>() - 50.0).abs() < 1e-9);
    world.set_pool_supports(&[true, true]).unwrap();
    assert!(
        world.pools()[0]
            .columns()
            .all(|c| c.volume == 0.0 && c.velocity == 0.0)
    );
    world
        .add_falling(Parcel {
            position: Vec2::new(0.0, 50.0),
            velocity: Vec2::new(0.0, -20.0),
            volume: 5.0,
            duration: DT,
            horizontal_bounds: None,
        })
        .unwrap();
    for _ in 0..30 {
        world.step(DT).unwrap();
    }
    assert_eq!(
        world.pools()[0].columns().map(|c| c.volume).sum::<f64>(),
        5.0
    );
    assert_accounting(&world);
}

#[test]
fn failed_batch_leaves_every_support_and_volume_unchanged() {
    let mut world = world(1);
    world.add_to_pool(0, -5.0, 20.0).unwrap();
    world.add_to_pool(1, 10.0, 30.0).unwrap();
    let before = world.stats();
    let pools = world.pools().to_vec();
    assert_eq!(
        world.set_pool_supports(&[false, false]),
        Err(WaterError::Capacity)
    );
    assert_eq!(world.set_pool_supports(&[]), Err(WaterError::InvalidInput));
    assert_eq!(world.pools(), pools);
    assert_eq!(world.stats(), before);
    assert!(world.parcels().is_empty());
    world.set_pool_supports(&[false, true]).unwrap();
    assert_accounting(&world);
}

#[test]
fn disabled_support_does_not_catch_rain_and_resets_displacement() {
    let mut world = world(32);
    world.add_to_pool(0, 0.0, 10.0).unwrap();
    world
        .set_displacer(
            0,
            Some(displacement::DisplacementBox {
                center: Vec2::new(0.0, 40.0),
                half_extents: Vec2::new(1.0, 1.0),
                angle: 0.0,
            }),
        )
        .unwrap();
    assert!(world.stats().displaced > 0.0);
    world.set_pool_supports(&[false, true]).unwrap();
    assert_eq!(world.stats().displaced, 0.0);
    assert_eq!(world.set_displacers(0, &[]), Err(WaterError::InvalidInput));
    world
        .add_falling(Parcel {
            position: Vec2::new(0.0, 60.0),
            velocity: Vec2::new(0.0, -20.0),
            volume: 5.0,
            duration: DT,
            horizontal_bounds: None,
        })
        .unwrap();
    for _ in 0..120 {
        world.step(DT).unwrap();
    }
    assert!((world.stats().pooled - 15.0).abs() < 1e-9);
    world.set_pool_supports(&[true, true]).unwrap();
    assert_eq!(world.stats().displaced, 0.0);
    assert_accounting(&world);
}

#[test]
fn many_small_supports_share_existing_budgets_and_reuse_storage() {
    let specs: Vec<_> = (0..MAX_POOLS)
        .map(|i| spec(i as f64 * 2.0, 1.0, vec![0.0; 4], [Boundary::Closed; 2]))
        .collect();
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: MAX_PARCELS,
            ..WaterConfig::default()
        },
        specs.clone(),
    )
    .unwrap();
    let ptrs = (
        world.heads.as_ptr(),
        world.origins.as_ptr(),
        world.parcels.as_ptr(),
        world.spills.as_ptr(),
        world.pools.as_ptr(),
    );
    for pool in 0..MAX_POOLS {
        world
            .add_to_pool(pool, pool as f64 * 2.0 + 0.5, 1.0)
            .unwrap();
    }
    for on in [false, true, false, true] {
        world.set_pool_supports(&[on; MAX_POOLS]).unwrap();
        world.step(DT).unwrap();
        assert_accounting(&world);
        assert_eq!(
            ptrs,
            (
                world.heads.as_ptr(),
                world.origins.as_ptr(),
                world.parcels.as_ptr(),
                world.spills.as_ptr(),
                world.pools.as_ptr()
            )
        );
    }
    let mut over = specs;
    over.push(spec(-2.0, 1.0, vec![0.0], [Boundary::Closed; 2]));
    assert!(matches!(
        WaterWorld::new(WaterConfig::default(), over),
        Err(WaterError::InvalidGeometry)
    ));
}

#[test]
fn support_cycle_cannot_reconnect_to_an_old_outfall() {
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: 32,
            ..WaterConfig::default()
        },
        vec![spec(
            0.0,
            20.0,
            vec![40.0],
            [Boundary::Spill { lip: 40.0 }; 2],
        )],
    )
    .unwrap();
    world.add_to_pool(0, 10.0, 20.0).unwrap();
    world.step(DT).unwrap();
    assert!(world.spill_source(0).is_some());
    let count = world.parcels.len();
    world.set_pool_supports(&[false]).unwrap();
    world.set_pool_supports(&[true]).unwrap();
    assert!((0..count).all(|i| world.spill_source(i).is_none()));
    assert_accounting(&world);
}

#[test]
fn reserved_slots_allow_support_removal_under_outlet_backpressure() {
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: 3,
            reserved_release_parcels: 2,
            ..WaterConfig::default()
        },
        vec![spec(
            0.0,
            20.0,
            vec![40.0; 2],
            [Boundary::Spill { lip: 40.0 }; 2],
        )],
    )
    .unwrap();
    world.add_to_pool(0, 5.0, 20.0).unwrap();
    world.add_to_pool(0, 15.0, 20.0).unwrap();
    world.step(DT).unwrap();
    assert_eq!(world.parcels().len(), 1);
    assert!(world.stats().capacity_limited_ticks > 0);
    assert_eq!(
        world.add_falling(Parcel {
            position: Vec2::new(0.0, 80.0),
            velocity: Vec2::ZERO,
            volume: 1.0,
            duration: DT,
            horizontal_bounds: None
        }),
        Err(WaterError::Capacity)
    );
    world.set_pool_supports(&[false]).unwrap();
    assert_eq!(world.parcels().len(), 3);
    world.set_pool_supports(&[true]).unwrap();
    world.step(DT).unwrap(); // above normal budget, but below the hard ceiling
    assert_accounting(&world);
    assert!(
        WaterWorld::new(
            WaterConfig {
                max_parcels: 3,
                reserved_release_parcels: 3,
                ..WaterConfig::default()
            },
            vec![spec(0.0, 1.0, vec![0.0], [Boundary::Closed; 2])]
        )
        .is_err()
    );
}
