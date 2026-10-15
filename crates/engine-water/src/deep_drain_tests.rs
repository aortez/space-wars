use super::*;
use crate::tests::assert_accounting;

fn deep_outfalls(depth: f64, gap: f64, sides: usize, config: WaterConfig) -> WaterWorld {
    let dx = (400.0 - gap) / 64.0;
    let mut water = WaterWorld::new(
        config,
        (0..sides)
            .map(|side| PoolSpec {
                left: if side == 0 { -400.0 } else { gap },
                column_width: dx,
                bed: vec![0.0; 64],
                boundaries: if side == 0 {
                    [Boundary::Closed, Boundary::Spill { lip: 0.0 }]
                } else {
                    [Boundary::Spill { lip: 0.0 }, Boundary::Closed]
                },
            })
            .collect(),
    )
    .unwrap();
    for side in 0..sides {
        for i in 0..64 {
            let x = water.pools[side].spec.left + (i as f64 + 0.5) * dx;
            water.add_to_pool(side, x, depth * dx).unwrap();
        }
    }
    water
}

#[test]
fn narrow_gap_mixes_at_birth_without_redepositing_a_deep_outfall() {
    // The narrowest gap is crossed during the half-frame birth advance. A
    // missed collision used to dump both outfalls into the opposite edge,
    // raising a 355-unit surface above 600 and preventing any drainage.
    for depth in [10.0, 355.0] {
        for gap in [0.2, 5.0, 40.0] {
            for hz in [30, 60, 120] {
                let config = WaterConfig {
                    exit_y: -150.0,
                    max_parcels: 512,
                    ..Default::default()
                };
                let mut single = deep_outfalls(depth, gap, 1, config);
                let mut opposed = deep_outfalls(depth, gap, 2, config);
                let storage = (opposed.parcels.as_ptr(), opposed.spills.as_ptr());
                for _ in 0..hz * 2 {
                    for water in [&mut single, &mut opposed] {
                        water.step(1.0 / f64::from(hz)).unwrap();
                        assert_accounting(water);
                        assert!(
                            water
                                .pools
                                .iter()
                                .flat_map(|p| p.columns())
                                .all(|c| c.surface <= depth + 1e-6)
                        );
                    }
                    assert_eq!(storage, (opposed.parcels.as_ptr(), opposed.spills.as_ptr()));
                    for (a, b) in single.pools[0].columns().zip(opposed.pools[0].columns()) {
                        assert!((a.volume - b.volume).abs() < 1e-8, "gap={gap}, {hz} Hz");
                    }
                }
                assert!(opposed.stats().spill_merges > 0);
                assert!(opposed.stats().drained > 0.0);
                assert_eq!(opposed.stats().capacity_limited_ticks, 0);
            }
        }
    }
}

#[test]
fn newborn_mixing_with_splash_and_capacity_pressure_preserves_the_ledger() {
    for max_parcels in [2, 6, 128] {
        let mut water = deep_outfalls(
            355.0,
            0.2,
            2,
            WaterConfig {
                exit_y: -150.0,
                max_parcels,
                splash: Some(SplashConfig::default()),
                ..Default::default()
            },
        );
        let storage = (water.parcels.as_ptr(), water.spills.as_ptr());
        for _ in 0..120 {
            water.step(1.0 / 60.0).unwrap();
            assert_accounting(&water);
            assert_eq!(storage, (water.parcels.as_ptr(), water.spills.as_ptr()));
            assert!(water.parcels.len() <= max_parcels);
        }
        assert!(water.stats().spill_merges > 0);
        if max_parcels == 128 {
            assert!(water.stats().splash_bursts > 0);
        } else {
            assert!(water.stats().capacity_limited_ticks > 0);
        }
    }
}
