use super::*;

const DT: f64 = 1.0 / 60.0;

fn spec(left: f64, width: f64, bed: Vec<f64>, boundaries: [Boundary; 2]) -> PoolSpec {
    PoolSpec {
        left,
        column_width: width / bed.len() as f64,
        bed,
        boundaries,
    }
}

fn assert_accounting(world: &WaterWorld) {
    let stats = world.stats();
    let sum = stats.pooled + stats.in_flight + stats.drained + stats.reclaimed;
    assert!(
        (stats.injected - sum).abs() < stats.injected.max(1.0) * 1.0e-10,
        "{stats:?}"
    );
    assert!(
        world
            .pools
            .iter()
            .flat_map(|p| &p.volume)
            .all(|v| v.is_finite() && *v >= 0.0)
    );
    assert!(
        world
            .pools
            .iter()
            .flat_map(|p| &p.velocity)
            .all(|v| v.is_finite())
    );
    assert!(stats.parcels <= world.config.max_parcels);
    assert!(
        world
            .parcels
            .iter()
            .all(|p| p.volume > 0.0 && p.position.x.is_finite() && p.position.y.is_finite())
    );
}

#[test]
fn closed_pool_settles_without_losing_water_or_allocating_during_steps() {
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(-100.0, 200.0, vec![0.0; 64], [Boundary::Closed; 2])],
    )
    .unwrap();
    world.add_to_pool(0, -80.0, 1000.0).unwrap();
    let parcels_ptr = world.parcels.as_ptr();
    let columns_ptr = world.pools[0].volume.as_ptr();
    for _ in 0..6000 {
        world.step(DT).unwrap();
        assert_accounting(&world);
        assert_eq!(world.parcels.as_ptr(), parcels_ptr);
        assert_eq!(world.pools[0].volume.as_ptr(), columns_ptr);
    }
    let depths: Vec<_> = world.pools[0].columns().map(|c| c.surface).collect();
    assert!(depths.iter().all(|h| (*h - 5.0).abs() < 0.05), "{depths:?}");
    assert_eq!(world.stats().drained, 0.0);
    assert_eq!(world.stats().in_flight, 0.0);
    assert!(world.sample(Vec2::new(0.0, 2.5)).is_some());
    assert!(world.sample(Vec2::new(0.0, 5.1)).is_none());
}

#[test]
fn uneven_beds_preserve_a_level_surface_and_dry_steps_block_flow() {
    let bed: Vec<_> = (0..32).map(|i| if i < 16 { 0.0 } else { 10.0 }).collect();
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(0.0, 160.0, bed, [Boundary::Closed; 2])],
    )
    .unwrap();
    for i in 0..16 {
        world
            .add_to_pool(0, i as f64 * 5.0 + 2.5, 5.0 * 5.0)
            .unwrap();
    }
    for _ in 0..600 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert!(world.pools[0].volume[16..].iter().all(|v| *v == 0.0));
    for i in 0..32 {
        let volume = (15.0 - world.pools[0].surface(i)) * 5.0;
        world.add_to_pool(0, i as f64 * 5.0 + 2.5, volume).unwrap();
    }
    let before = world.pools[0].volume.clone();
    for _ in 0..600 {
        world.step(DT).unwrap();
    }
    assert_eq!(
        world.pools[0].volume, before,
        "equal surface elevations, not equal depths, are equilibrium"
    );
}

#[test]
fn spill_is_in_flight_before_collection_and_never_counted_as_drained_twice() {
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![
            spec(
                -100.0,
                80.0,
                vec![40.0; 32],
                [Boundary::Closed, Boundary::Spill { lip: 40.0 }],
            ),
            spec(-100.0, 600.0, vec![-100.0; 64], [Boundary::Closed; 2]),
        ],
    )
    .unwrap();
    world.add_to_pool(0, -21.0, 1200.0).unwrap();
    world.step(DT).unwrap();
    assert!(world.stats().in_flight > 0.0);
    assert_eq!(world.pools[1].volume.iter().sum::<f64>(), 0.0);
    assert_eq!(world.stats().drained, 0.0);
    let before = world.parcels[0];
    world.step(DT).unwrap();
    assert!(world.parcels[0].position.y < before.position.y);
    assert!(world.parcels[0].velocity.y < before.velocity.y);
    for _ in 0..1800 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert!(
        world.pools[1].volume.iter().sum::<f64>() > 1000.0,
        "{:?}",
        world.stats()
    );
    assert_eq!(world.stats().drained, 0.0);
    world.reclaim();
    assert_accounting(&world);
    assert_eq!(world.stats().parcels, 0);
    assert_eq!(world.stats().wet_columns, 0);
    world.reclaim();
    assert_accounting(&world);
}

#[test]
fn raised_outlet_retains_water_below_its_lip() {
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(
            0.0,
            100.0,
            vec![0.0; 20],
            [Boundary::Closed, Boundary::Spill { lip: 10.0 }],
        )],
    )
    .unwrap();
    for i in 0..20 {
        world.add_to_pool(0, i as f64 * 5.0 + 1.0, 25.0).unwrap();
    }
    for _ in 0..600 {
        world.step(DT).unwrap();
    }
    assert_eq!(world.stats().in_flight, 0.0);
    assert_eq!(world.stats().drained, 0.0);
    assert_accounting(&world);
}

#[test]
fn capacity_backpressure_holds_water_upstream_and_sources_fail_without_mutation() {
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: 1,
            ..WaterConfig::default()
        },
        vec![spec(
            0.0,
            50.0,
            vec![0.0; 20],
            [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
        )],
    )
    .unwrap();
    world.add_to_pool(0, 49.0, 1000.0).unwrap();
    world.step(DT).unwrap();
    assert_eq!(world.parcels.len(), 1);
    let before = world.stats();
    assert_eq!(
        world.add_falling(Parcel {
            position: Vec2::Y * 100.0,
            velocity: Vec2::ZERO,
            volume: 50.0,
            duration: DT
        }),
        Err(WaterError::Capacity)
    );
    assert_eq!(world.stats(), before);
    world.step(DT).unwrap();
    assert!((world.stats().pooled - before.pooled).abs() < 1.0e-9);
    assert!(world.stats().capacity_limited_ticks > 0);
    for _ in 0..600 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert!(world.stats().drained > 0.0);
}

#[test]
fn swept_deposition_catches_a_thin_pool_crossed_within_one_tick() {
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(0.0, 1.0, vec![0.0], [Boundary::Closed; 2])],
    )
    .unwrap();
    world
        .add_falling(Parcel {
            position: Vec2::new(-4.5, 1.0),
            velocity: Vec2::new(600.0, -120.0),
            volume: 0.5,
            duration: DT,
        })
        .unwrap();
    world.step(DT).unwrap();
    assert_eq!(world.stats().pooled, 0.5);
    assert_eq!(world.stats().parcels, 0);
    assert_accounting(&world);
}

#[test]
fn both_exact_pool_edges_collect_vertical_parcels_symmetrically() {
    for x in [-10.0, 10.0] {
        let mut world = WaterWorld::new(
            WaterConfig::default(),
            vec![spec(-10.0, 20.0, vec![0.0; 20], [Boundary::Closed; 2])],
        )
        .unwrap();
        world
            .add_falling(Parcel {
                position: Vec2::new(x, 1.0),
                velocity: Vec2::new(0.0, -100.0),
                volume: 5.0,
                duration: DT,
            })
            .unwrap();
        world.step(DT).unwrap();
        assert_eq!(world.stats().pooled, 5.0);
        assert_eq!(world.stats().in_flight, 0.0);
        assert_accounting(&world);
    }
}

#[test]
fn maximum_geometry_stays_bounded_for_supported_fixed_timesteps() {
    let columns = MAX_COLUMNS / MAX_POOLS;
    let pools: Vec<_> = (0..MAX_POOLS)
        .map(|i| {
            spec(
                i as f64 * 200.0,
                100.0,
                vec![0.0; columns],
                [Boundary::Spill { lip: 0.0 }; 2],
            )
        })
        .collect();
    for dt in [1.0 / 120.0, DT, MAX_STEP] {
        let mut world = WaterWorld::new(
            WaterConfig {
                max_parcels: MAX_PARCELS,
                ..WaterConfig::default()
            },
            pools.clone(),
        )
        .unwrap();
        for i in 0..MAX_POOLS {
            world
                .add_to_pool(i, i as f64 * 200.0 + 50.0, 1000.0)
                .unwrap();
        }
        for _ in 0..240 {
            world.step(dt).unwrap();
            assert_accounting(&world);
        }
    }
    let too_many = vec![spec(
        0.0,
        100.0,
        vec![0.0; MAX_COLUMNS + 1],
        [Boundary::Closed; 2],
    )];
    assert!(WaterWorld::new(WaterConfig::default(), too_many).is_err());
}

#[test]
fn channel_walls_stop_horizontal_motion_without_losing_water() {
    for direction in [-1.0, 1.0] {
        let config = WaterConfig {
            spill_channel: Some([-10.0, 10.0]),
            ..WaterConfig::default()
        };
        let pool = spec(-100.0, 20.0, vec![0.0; 10], [Boundary::Closed; 2]);
        let mut world = WaterWorld::new(config, vec![pool.clone()]).unwrap();
        assert_eq!(
            world.add_falling(Parcel {
                position: Vec2::new(11.0, 20.0),
                velocity: Vec2::ZERO,
                volume: 1.0,
                duration: DT,
            }),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(world.stats().injected, 0.0);
        world
            .add_falling(Parcel {
                position: Vec2::new(9.0 * direction, 20.0),
                velocity: Vec2::new(200.0 * direction, 0.0),
                volume: 20.0,
                duration: DT,
            })
            .unwrap();
        world.step(DT).unwrap();
        assert_eq!(world.parcels[0].position.x, 10.0 * direction);
        assert_eq!(world.parcels[0].velocity.x, 0.0);
        assert!(world.parcels[0].velocity.y < 0.0);
        for _ in 0..120 {
            world.step(DT).unwrap();
            assert_accounting(&world);
        }
        assert_eq!(world.stats().drained, 20.0);
        for bounds in [[10.0, -10.0], [f64::NAN, 1.0], [0.0, 0.0]] {
            assert!(
                WaterWorld::new(
                    WaterConfig {
                        spill_channel: Some(bounds),
                        ..config
                    },
                    vec![pool.clone()]
                )
                .is_err()
            );
        }
        let outside = spec(
            -100.0,
            20.0,
            vec![0.0; 10],
            [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
        );
        assert!(WaterWorld::new(config, vec![outside]).is_err());
    }
}

#[test]
fn sideways_entry_below_a_pool_surface_is_collected() {
    for direction in [-1.0, 1.0] {
        let mut world = WaterWorld::new(
            WaterConfig::default(),
            vec![spec(-1.0, 2.0, vec![0.0], [Boundary::Closed; 2])],
        )
        .unwrap();
        world.add_to_pool(0, 0.0, 20.0).unwrap();
        world
            .add_falling(Parcel {
                position: Vec2::new(-5.0 * direction, 5.0),
                velocity: Vec2::new(600.0 * direction, -60.0),
                volume: 1.0,
                duration: DT,
            })
            .unwrap();
        world.step(DT).unwrap();
        assert_eq!(world.stats().pooled, 21.0);
        assert_eq!(world.stats().parcels, 0);
        assert_accounting(&world);
    }
}

#[test]
fn partial_reclamation_is_explicit_conservative_and_validated() {
    let mut world = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(0.0, 100.0, vec![0.0; 20], [Boundary::Closed; 2])],
    )
    .unwrap();
    world.add_to_pool(0, 1.0, 100.0).unwrap();
    world
        .add_falling(Parcel {
            position: Vec2::new(200.0, 50.0),
            velocity: Vec2::ZERO,
            volume: 20.0,
            duration: DT,
        })
        .unwrap();
    let before = world.stats();
    for fraction in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            world.reclaim_fraction(fraction),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(world.stats(), before);
    }
    world.reclaim_fraction(0.0).unwrap();
    assert_eq!(world.stats(), before);
    world.reclaim_fraction(0.5).unwrap();
    assert_eq!(world.stats().pooled, 50.0);
    assert_eq!(world.stats().in_flight, 10.0);
    assert_eq!(world.stats().reclaimed, 60.0);
    assert_eq!(world.stats().drained, 0.0);
    assert_accounting(&world);
    world.reclaim();
    assert_eq!(world.stats().reclaimed, 120.0);
    assert_accounting(&world);
}

#[test]
fn invalid_inputs_and_steps_are_rejected_without_mutating_water() {
    let pool = spec(0.0, 100.0, vec![0.0; 20], [Boundary::Closed; 2]);
    assert!(WaterWorld::new(WaterConfig::default(), vec![]).is_err());
    assert!(
        WaterWorld::new(
            WaterConfig {
                max_parcels: MAX_PARCELS + 1,
                ..WaterConfig::default()
            },
            vec![pool.clone()]
        )
        .is_err()
    );
    let mut world = WaterWorld::new(WaterConfig::default(), vec![pool]).unwrap();
    let before = world.stats();
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, MAX_AMOUNT * 2.0] {
        assert!(world.add_to_pool(0, 1.0, value).is_err());
        assert!(world.step(value).is_err());
    }
    assert!(world.add_to_pool(1, 1.0, 1.0).is_err());
    assert!(world.add_to_pool(0, f64::NAN, 1.0).is_err());
    assert!(world.step(MAX_STEP + 0.001).is_err());
    assert_eq!(world.stats(), before);
}

#[test]
fn deterministic_mirrored_flows_and_drain_accounting() {
    let make = || {
        WaterWorld::new(
            WaterConfig::default(),
            vec![spec(
                -100.0,
                200.0,
                vec![0.0; 64],
                [Boundary::Spill { lip: 0.0 }; 2],
            )],
        )
        .unwrap()
    };
    let mut a = make();
    let mut b = make();
    for world in [&mut a, &mut b] {
        world.add_to_pool(0, -80.0, 1000.0).unwrap();
        world.add_to_pool(0, 80.0, 1000.0).unwrap();
    }
    for _ in 0..1800 {
        a.step(DT).unwrap();
        b.step(DT).unwrap();
        assert_eq!(a.stats(), b.stats());
        assert_eq!(a.parcels(), b.parcels());
        assert_eq!(a.pools(), b.pools());
        assert_accounting(&a);
        for i in 0..32 {
            assert!((a.pools[0].volume[i] - a.pools[0].volume[63 - i]).abs() < 1.0e-9);
        }
    }
    assert!(a.stats().drained > 1800.0, "{:?}", a.stats());
}
