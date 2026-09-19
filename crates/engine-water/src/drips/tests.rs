use super::*;
use crate::{
    tests::{DT, assert_accounting, spec},
    *,
};

const DRIPS: DripConfig = DripConfig {
    target_volume: 3.0,
    max_delay: 0.4,
};

fn world(config: Option<DripConfig>, capacity: usize) -> WaterWorld {
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: capacity,
            ..WaterConfig::default()
        },
        vec![spec(
            0.0,
            10.0,
            vec![40.0],
            [Boundary::Closed, Boundary::Spill { lip: 40.0 }],
        )],
    )
    .unwrap();
    world.set_drip_config(0, config).unwrap();
    world
}

#[test]
fn credits_wait_for_volume_or_a_deadline_without_reserving_phantom_liquid() {
    let config = DripConfig {
        target_volume: 1.0,
        max_delay: 0.25,
    };
    let mut credit = Credit::default();
    for _ in 0..4 {
        assert_eq!(credit.request(config, 0.05, 0.2, 5.0), (0.0, false));
    }
    assert_eq!(credit.request(config, 0.05, 0.2, 5.0), (1.0, true));
    // Another face has spent the donor; an overdue request shrinks to match it.
    assert_eq!(credit.request(config, 0.05, 0.2, 0.1), (0.1, true));
    assert_eq!(credit.request(config, 0.05, 0.0, 0.0), (0.0, false));
    assert_eq!(credit, Credit::default());
    // Large instantaneous flows don't become isolated drips.
    assert_eq!(credit.request(config, 0.05, 2.0, 5.0), (2.0, false));
}

#[test]
fn waiting_water_remains_visible_sampleable_and_fully_accounted() {
    let mut world = world(Some(DRIPS), 32);
    world.add_to_pool(0, 5.0, 1.0).unwrap();
    world.step(DT).unwrap();
    assert_eq!(world.stats().pooled, 1.0);
    assert_eq!(world.stats().in_flight, 0.0);
    assert_eq!(world.pools[0].column(0).volume, 1.0);
    assert!(world.sample(Vec2::new(5.0, 40.05)).is_some());
    let mut first = None;
    for tick in 2..=26 {
        world.step(DT).unwrap();
        assert_accounting(&world);
        if world.stats().drip_parcels_emitted > 0 {
            first = Some(tick);
            break;
        }
    }
    assert!(matches!(first, Some(24..=25)), "{first:?}");
    assert_eq!(world.stats().drip_parcels_emitted, 1);
    assert_eq!(
        world.spill_source(0),
        Some(SpillSource::Drip { pool: 0, edge: 1 })
    );
    assert!(
        world.spill_ribbon(0).is_none(),
        "an isolated drop must not stretch back to its lip"
    );
    let credits = world.pools[0].outlet_credit;
    world.reclaim_fraction(0.0).unwrap();
    assert_eq!(world.pools[0].outlet_credit, credits);
}

#[test]
fn large_outfalls_match_continuous_flux_and_keep_their_ribbon() {
    let mut continuous = world(None, 32);
    let mut batched = world(Some(DRIPS), 32);
    continuous.add_to_pool(0, 5.0, 1000.0).unwrap();
    batched.add_to_pool(0, 5.0, 1000.0).unwrap();
    for _ in 0..3 {
        continuous.step(DT).unwrap();
        batched.step(DT).unwrap();
        assert_eq!(continuous.stats(), batched.stats());
        assert_eq!(continuous.parcels(), batched.parcels());
        assert_eq!(continuous.pools[0].volume, batched.pools[0].volume);
        assert_eq!(continuous.pools[0].velocity, batched.pools[0].velocity);
    }
    assert!(batched.spills.iter().all(|s| matches!(
        s,
        Some(Spill {
            source: SpillSource::Outlet { .. },
            ..
        })
    )));
    assert!(batched.spill_ribbon(batched.parcels.len() - 1).is_some());
}

#[test]
fn raised_lips_retain_liquid_when_two_batched_outlets_share_a_donor() {
    for lips in [[5.0, 5.0], [5.0, 8.0]] {
        let mut world = WaterWorld::new(
            WaterConfig::default(),
            vec![spec(
                0.0,
                1.0,
                vec![0.0],
                lips.map(|lip| Boundary::Spill { lip }),
            )],
        )
        .unwrap();
        world.set_drip_config(0, Some(DRIPS)).unwrap();
        world.add_to_pool(0, 0.5, 10.0).unwrap();
        for _ in 0..600 {
            world.step(DT).unwrap();
            assert_accounting(&world);
            assert!(world.stats().pooled >= 5.0 - 1e-12, "{:?}", world.stats());
        }
        assert!(
            world.stats().pooled < 5.01,
            "must drain down to its lowest lip"
        );
    }
}

#[test]
fn capacity_pressure_holds_real_water_and_resumes_without_unbounded_credit() {
    let mut world = world(Some(DRIPS), 1);
    world.add_to_pool(0, 5.0, 1.0).unwrap();
    world
        .add_falling(Parcel {
            position: Vec2::new(-50.0, 500.0),
            velocity: Vec2::ZERO,
            volume: 1.0,
            duration: DT,
            horizontal_bounds: None,
        })
        .unwrap();
    for tick in 0..60 {
        world.step(DT).unwrap();
        if tick < 20 {
            assert_eq!(
                world.stats().capacity_limited_ticks,
                0,
                "waiting is not capacity backpressure"
            );
        }
        assert_eq!(world.stats().pooled, 1.0);
        assert!(world.pools[0].outlet_credit[1].amount <= 1.0);
        assert!(world.pools[0].outlet_credit[1].age <= DRIPS.max_delay);
        assert_accounting(&world);
    }
    for _ in 0..120 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert!(world.stats().capacity_limited_ticks > 0);
    assert!(world.stats().drip_parcels_emitted > 0);
    assert!(world.stats().pooled < 1.0);
}

#[test]
fn a_waiting_left_outlet_cannot_reserve_the_only_slot_a_due_right_outlet_needs() {
    let mut world = WaterWorld::new(
        WaterConfig {
            max_parcels: 1,
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
    world.set_drip_config(0, Some(DRIPS)).unwrap();
    world.add_to_pool(0, 5.0, 0.01).unwrap();
    world.add_to_pool(0, 15.0, 1.0).unwrap();
    // The right side has already waited its deadline; left has only just wetted.
    world.pools[0].outlet_credit[1] = Credit {
        amount: 0.2,
        age: DRIPS.max_delay,
    };
    world.step(DT).unwrap();
    assert_eq!(world.parcels.len(), 1);
    assert_eq!(
        world.spill_source(0),
        Some(SpillSource::Drip { pool: 0, edge: 1 })
    );
    assert_eq!(world.stats().capacity_limited_ticks, 0);
    assert_accounting(&world);
}

#[test]
fn removal_and_cleanup_clear_requests_without_releasing_water_twice() {
    let mut world = world(Some(DRIPS), 32);
    world.add_to_pool(0, 5.0, 1.0).unwrap();
    world.step(DT).unwrap();
    assert!(world.pools[0].outlet_credit[1].amount > 0.0);
    world.set_pool_supports(&[false]).unwrap();
    assert_eq!(world.pools[0].outlet_credit, [Credit::default(); 2]);
    assert_eq!(world.stats().in_flight, 1.0);
    // Let the released water pass below the absent support before restoring it.
    // Restoring immediately would legitimately catch that same falling water.
    for _ in 0..60 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    world.set_pool_supports(&[true]).unwrap();
    assert_eq!(world.stats().drip_parcels_emitted, 0);
    world.add_to_pool(0, 5.0, 1.0).unwrap();
    world.step(DT).unwrap();
    world.reclaim();
    assert_eq!(world.pools[0].outlet_credit, [Credit::default(); 2]);
    for _ in 0..60 {
        world.step(DT).unwrap();
        assert_accounting(&world);
    }
    assert_eq!(world.stats().parcels, 0);
    assert_eq!(world.stats().drip_parcels_emitted, 0);
}

#[test]
fn configuration_validation_is_atomic_and_reapplying_preserves_waiting_time() {
    let mut world = world(Some(DRIPS), 32);
    world.add_to_pool(0, 5.0, 1.0).unwrap();
    world.step(DT).unwrap();
    let before = world.pools[0].clone();
    for config in [
        DripConfig {
            target_volume: f64::NAN,
            ..DRIPS
        },
        DripConfig {
            target_volume: 0.0,
            ..DRIPS
        },
        DripConfig {
            max_delay: 0.0,
            ..DRIPS
        },
        DripConfig {
            max_delay: f64::INFINITY,
            ..DRIPS
        },
    ] {
        assert_eq!(
            world.set_drip_config(0, Some(config)),
            Err(WaterError::InvalidInput)
        );
    }
    assert_eq!(
        world.set_drip_config(1, None),
        Err(WaterError::InvalidInput)
    );
    world.set_drip_config(0, Some(DRIPS)).unwrap();
    assert_eq!(world.pools[0], before);
    world.set_drip_config(0, None).unwrap();
    assert_eq!(world.pools[0].outlet_credit, [Credit::default(); 2]);
    world.step(DT).unwrap();
    assert!(world.stats().in_flight > 0.0);
    assert_eq!(world.stats().drip_parcels_emitted, 0);
}

#[test]
fn substep_timing_and_storage_are_stable_at_supported_rates() {
    let mut totals = Vec::new();
    for hz in [30, 60, 120, 240] {
        let mut a = world(Some(DRIPS), 32);
        let mut b = world(Some(DRIPS), 32);
        a.add_to_pool(0, 5.0, 1.0).unwrap();
        b.add_to_pool(0, 5.0, 1.0).unwrap();
        let ptrs = (
            a.parcels.as_ptr(),
            a.spills.as_ptr(),
            a.pools.as_ptr(),
            a.pools[0].volume.as_ptr(),
        );
        for _ in 0..hz * 3 {
            a.step(1.0 / hz as f64).unwrap();
            b.step(1.0 / hz as f64).unwrap();
            assert_eq!(a.stats(), b.stats());
            assert_eq!(a.parcels(), b.parcels());
            assert_accounting(&a);
            assert_eq!(
                ptrs,
                (
                    a.parcels.as_ptr(),
                    a.spills.as_ptr(),
                    a.pools.as_ptr(),
                    a.pools[0].volume.as_ptr()
                )
            );
        }
        totals.push(a.stats().pooled);
    }
    assert!(
        totals.iter().all(|v| (v - totals[0]).abs() < 1e-10),
        "{totals:?}"
    );
}
