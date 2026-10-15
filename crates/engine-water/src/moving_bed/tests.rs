use super::*;
use crate::{PoolSpec, WaterConfig};
use engine_core::Vec2;

#[test]
fn gently_retracting_opposed_slopes_retain_connected_mixed_ribbons() {
    const N: usize = 32;
    const DT: f64 = 1.0 / 60.0;
    let mut w = WaterWorld::new(
        WaterConfig {
            exit_y: -80.0,
            max_parcels: 256,
            ..WaterConfig::default()
        },
        (0..2)
            .map(|side| PoolSpec {
                left: -80.0 + side as f64 * 80.0,
                column_width: 80.0 / N as f64,
                bed: vec![0.0; N],
                boundaries: [Boundary::Closed; 2],
            })
            .collect(),
    )
    .unwrap();
    for side in 0..2 {
        w.configure_sloped_bed(side, &[[0.0; 2]; N]).unwrap();
        for i in 0..N {
            w.add_to_pool(
                side,
                -80.0 + side as f64 * 80.0 + (i as f64 + 0.5) * 2.5,
                7.0 * 2.5,
            )
            .unwrap();
        }
    }
    for tick in 1..=120 {
        for side in 0..2 {
            let spec = w.pools[side].spec.clone();
            for i in 0..N {
                w.add_to_pool(
                    side,
                    spec.left + (i as f64 + 0.5) * spec.column_width,
                    spec.column_width * DT,
                )
                .unwrap();
            }
        }
        let opening = tick as f64 * 0.16 * DT;
        let drop = opening * 12.0;
        let gap = opening * 14.0;
        let left: [[f64; 2]; N] = std::array::from_fn(|i| {
            [
                -drop * i as f64 / N as f64,
                -drop * (i + 1) as f64 / N as f64,
            ]
        });
        let right: [[f64; 2]; N] = std::array::from_fn(|i| {
            [
                -drop + drop * i as f64 / N as f64,
                -drop + drop * (i + 1) as f64 / N as f64,
            ]
        });
        let outlet = Boundary::Spill { lip: -drop };
        w.move_sloped_pools(
            &[
                PoolGeometry {
                    pool: 0,
                    left: -80.0,
                    column_width: (80.0 - gap) / N as f64,
                    bed_edges: &left,
                    boundaries: [Boundary::Closed, outlet],
                },
                PoolGeometry {
                    pool: 1,
                    left: gap,
                    column_width: (80.0 - gap) / N as f64,
                    bed_edges: &right,
                    boundaries: [outlet, Boundary::Closed],
                },
            ],
            DT,
        )
        .unwrap();
        w.step(DT).unwrap();
    }
    for (p, s) in w.parcels.iter().zip(&w.spills) {
        if let Some(s) = s
            && matches!(s.source, crate::SpillSource::Junction { .. })
            && (-50.0..-15.0).contains(&p.position.y)
        {
            assert!(s.ribbon(p).is_some(), "{s:?} {p:?}");
        }
    }
}

#[test]
fn a_partial_wedge_releases_only_the_wet_part_of_a_retracted_strip() {
    let mut w = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: 0.0,
            column_width: 1.0,
            bed: vec![0.0],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    w.configure_sloped_bed(0, &[[0.0, 1.0]]).unwrap();
    w.add_to_pool(0, 0.1, 0.125).unwrap(); // triangular wet interval [0, .5]
    w.move_sloped_pools(
        &[PoolGeometry {
            pool: 0,
            left: 0.0,
            column_width: 0.75,
            bed_edges: &[[0.0, 0.75]],
            boundaries: [Boundary::Closed; 2],
        }],
        1.0 / 60.0,
    )
    .unwrap();
    assert_eq!(w.stats().in_flight, 0.0, "first strip is entirely dry");
    w.move_sloped_pools(
        &[PoolGeometry {
            pool: 0,
            left: 0.25,
            column_width: 0.5,
            bed_edges: &[[0.25, 0.75]],
            boundaries: [Boundary::Closed; 2],
        }],
        1.0 / 60.0,
    )
    .unwrap();
    assert!((w.stats().pooled - 0.03125).abs() < 1e-12);
    assert!((w.stats().in_flight - 0.09375).abs() < 1e-12);
    assert_eq!(w.stats().injected, 0.125);
    assert_eq!(w.stats().reclaimed, 0.0);
    assert_eq!(w.stats().drained, 0.0);
    assert!((w.pools()[0].columns().next().unwrap().surface - 0.5).abs() < 1e-12);
}

#[test]
fn uncovered_water_keeps_its_wet_footprint_through_fall_and_reclamation() {
    for mirrored in [false, true] {
        for surface in [0.05, 10.0, 177.5, 355.0] {
            for hz in [30, 60, 120] {
                let bed = if mirrored { [1.0, 0.0] } else { [0.0, 1.0] };
                let mut w = WaterWorld::new(
                    WaterConfig {
                        retain_spill_shapes: true,
                        exit_y: -2000.0,
                        ..Default::default()
                    },
                    vec![PoolSpec {
                        left: 0.0,
                        column_width: 2.0,
                        bed: vec![0.0],
                        boundaries: [Boundary::Closed; 2],
                    }],
                )
                .unwrap();
                w.configure_sloped_bed(0, &[bed]).unwrap();
                w.add_to_pool(0, 1.0, slopes::area(bed, 2.0, surface))
                    .unwrap();
                let dt = 1.0 / f64::from(hz);
                let new_bed = if mirrored { [1.0, 0.125] } else { [0.125, 1.0] };
                w.move_sloped_pools(
                    &[PoolGeometry {
                        pool: 0,
                        left: if mirrored { 0.0 } else { 0.25 },
                        column_width: 1.75,
                        bed_edges: &[new_bed],
                        boundaries: [Boundary::Closed; 2],
                    }],
                    dt,
                )
                .unwrap();
                assert_eq!(w.parcels.len(), 1);
                assert_eq!(
                    w.spill_source(0),
                    Some(crate::SpillSource::Release { pool: 0 })
                );
                let origin = w.parcels[0].position;
                let original = w.spill_ribbon(0).unwrap().quads()[0];
                // A partially wet cut is a triangle; deeper cuts retain the
                // full surface height, even when velocity is initially zero.
                let top = original
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((f64::from(top) - surface).abs() < 1e-5);
                for tick in 0..=8 {
                    if tick > 0 {
                        w.step(dt).unwrap();
                    }
                    crate::tests::assert_accounting(&w);
                    assert_eq!(w.parcels.len(), 1);
                    let p = w.parcels[0];
                    let displacement = p.position - origin;
                    let q = w.spill_ribbon(0).unwrap().quads()[0];
                    for (a, b) in original.iter().zip(q) {
                        assert!((*a + displacement - b).length() < 4e-5);
                    }
                    let area = |q: [Vec2; 4]| -> f64 {
                        (0..4)
                            .map(|i| {
                                let a = q[i] - q[0];
                                let b = q[(i + 1) % 4] - q[0];
                                f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x)
                            })
                            .sum::<f64>()
                            * 0.5
                    };
                    assert!((area(q) - p.volume).abs() < p.volume * 5e-4);
                }
                let volume = w.parcels[0].volume;
                w.reclaim_fraction(0.75).unwrap();
                assert_eq!(w.parcels[0].volume, volume * 0.25);
                let now = w.spill_ribbon(0).unwrap().quads()[0];
                let then = original.map(|p| p + w.parcels[0].position - origin);
                assert!(
                    ((now[1] - now[0]).length() * 2.0 - (then[1] - then[0]).length()).abs() < 1e-5
                );
                w.reclaim();
                assert!(w.parcels().is_empty());
                assert!(w.spill_ribbon(0).is_none());
                crate::tests::assert_accounting(&w);
            }
        }
    }
}

fn basin(parcels: usize) -> WaterWorld {
    let mut w = WaterWorld::new(
        WaterConfig {
            max_parcels: parcels,
            ..WaterConfig::default()
        },
        vec![PoolSpec {
            left: 0.0,
            column_width: 1.0,
            bed: vec![0.0; 16],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    w.configure_sloped_bed(0, &[[0.0; 2]; 16]).unwrap();
    for i in 0..16 {
        w.add_to_pool(0, i as f64 + 0.5, 4.0).unwrap();
    }
    w
}

#[test]
fn shrinking_floor_releases_only_uncovered_liquid_and_preserves_the_ledger() {
    let mut w = basin(128);
    let geometry = PoolGeometry {
        pool: 0,
        left: 0.0,
        column_width: 12.0 / 16.0,
        bed_edges: &[[0.0; 2]; 16],
        boundaries: [Boundary::Closed; 2],
    };
    w.move_sloped_pools(&[geometry], 1.0 / 60.0).unwrap();
    let s = w.stats();
    assert!((s.pooled - 48.0).abs() < 1e-10);
    assert!((s.in_flight - 16.0).abs() < 1e-10);
    assert_eq!(s.injected, 64.0);
    assert_eq!(s.drained, 0.0);
    assert_eq!(s.reclaimed, 0.0);
    assert!(w.parcels().iter().all(|p| p.position.x >= 12.0));
    // Re-extending does not invent replacement liquid.
    w.move_sloped_pools(
        &[PoolGeometry {
            column_width: 1.0,
            ..geometry
        }],
        1.0 / 60.0,
    )
    .unwrap();
    assert!((w.stats().pooled - 48.0).abs() < 1e-10);
}

#[test]
fn invalid_batch_capacity_and_fast_motion_leave_everything_unchanged() {
    let mut w = basin(1);
    let before = w.pools[0].clone();
    let shape = PoolGeometry {
        pool: 0,
        left: 0.0,
        column_width: 0.5,
        bed_edges: &[[0.0; 2]; 16],
        boundaries: [Boundary::Closed; 2],
    };
    assert_eq!(
        w.move_sloped_pools(&[shape], 1.0 / 60.0),
        Err(WaterError::Capacity)
    );
    assert_eq!(w.pools[0], before);
    assert!(w.parcels().is_empty());
    assert_eq!(
        w.move_sloped_pools(&[shape, PoolGeometry { pool: 8, ..shape }], 1.0 / 60.0),
        Err(WaterError::InvalidInput)
    );
    assert_eq!(w.pools[0], before);
    assert_eq!(
        w.move_sloped_pools(
            &[PoolGeometry {
                bed_edges: &[[5.0; 2]; 16],
                ..shape
            }],
            1.0 / 60.0
        ),
        Err(WaterError::InvalidInput)
    );
    assert_eq!(w.pools[0], before);
    assert_eq!(
        w.move_sloped_pools(&[shape], 0.0),
        Err(WaterError::InvalidInput)
    );
}

#[test]
fn repeated_slow_morphs_preserve_volume_and_reuse_geometry_storage() {
    let mut w = basin(128);
    let pointers = [
        w.pools[0].volume.as_ptr(),
        w.pools[0].geometry_scratch.as_ptr(),
    ];
    let beds = w.pools[0].slopes.as_ref().unwrap().as_ptr();
    for tick in 0..1200 {
        let drop = 0.4 * (tick as f64 * 0.01).sin();
        let edges: Vec<_> = (0..16)
            .map(|i| [drop * i as f64 / 16.0, drop * (i + 1) as f64 / 16.0])
            .collect();
        w.move_sloped_pools(
            &[PoolGeometry {
                pool: 0,
                left: 0.0,
                column_width: 1.0,
                bed_edges: &edges,
                boundaries: [Boundary::Closed; 2],
            }],
            1.0 / 60.0,
        )
        .unwrap();
        w.step(1.0 / 60.0).unwrap();
        assert!((w.stats().pooled - 64.0).abs() < 1e-9);
        assert_eq!(w.stats().injected, 64.0);
        assert!(pointers.contains(&w.pools[0].volume.as_ptr()));
        assert!(pointers.contains(&w.pools[0].geometry_scratch.as_ptr()));
        assert_eq!(beds, w.pools[0].slopes.as_ref().unwrap().as_ptr());
    }
}

#[test]
fn identical_snapshot_is_a_noop_and_mirrored_remaps_replay() {
    let mut w = basin(128);
    let before = w.pools[0].clone();
    w.move_sloped_pools(
        &[PoolGeometry {
            pool: 0,
            left: 0.0,
            column_width: 1.0,
            bed_edges: &[[0.0; 2]; 16],
            boundaries: [Boundary::Closed; 2],
        }],
        1.0 / 60.0,
    )
    .unwrap();
    assert_eq!(w.pools[0], before);
    let mut a = basin(128);
    let mut b = basin(128);
    for tick in 0..360 {
        let cut = 2.0 * (tick as f64 * 0.01).sin().abs();
        for (w, left) in [(&mut a, 0.0), (&mut b, cut)] {
            w.move_sloped_pools(
                &[PoolGeometry {
                    pool: 0,
                    left,
                    column_width: (16.0 - cut) / 16.0,
                    bed_edges: &[[0.0; 2]; 16],
                    boundaries: [Boundary::Closed; 2],
                }],
                1.0 / 60.0,
            )
            .unwrap();
            w.step(1.0 / 60.0).unwrap();
            let s = w.stats();
            assert!((s.injected - s.pooled - s.in_flight - s.drained).abs() < 1e-8);
        }
        assert!((a.stats().pooled - b.stats().pooled).abs() < 1e-8);
        for (va, vb) in a.pools[0].volume.iter().zip(b.pools[0].volume.iter().rev()) {
            assert!((va - vb).abs() < 1e-8);
        }
    }
}

#[test]
fn actuated_beds_accelerate_from_rest_and_conserve_water_at_steady_speed() {
    for hz in [60.0, 120.0] {
        let dt = 1.0 / hz;
        let mut water = basin(128);
        let mut height = 0.0;
        let mut speed: f64 = 0.0;
        for _ in 0..hz as usize {
            speed = (speed + 80.0 * dt).min(20.0);
            height -= speed * dt;
            water
                .actuate_sloped_pools(
                    &[PoolGeometry {
                        pool: 0,
                        left: 0.0,
                        column_width: 1.0,
                        bed_edges: &[[height; 2]; 16],
                        boundaries: [Boundary::Closed; 2],
                    }],
                    dt,
                )
                .unwrap();
            water.step(dt).unwrap();
            assert!((water.stats().pooled - 64.0).abs() < 1e-9);
            assert_eq!(water.stats().in_flight, 0.0);
            assert_eq!(water.stats().reclaimed, 0.0);
        }
        assert!(speed * dt > water.config.gravity * dt * dt * 0.5);
        let next = [[height - speed * dt; 2]; 16];
        let shape = PoolGeometry {
            pool: 0,
            left: 0.0,
            column_width: 1.0,
            bed_edges: &next,
            boundaries: [Boundary::Closed; 2],
        };
        let before = water.pools[0].clone();
        // The original API still rejects a fast move. A skipped actuation step
        // also invalidates velocity history rather than allowing a later jump.
        assert_eq!(
            water.move_sloped_pools(&[shape], dt),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(water.pools[0], before);
        water.step(dt).unwrap();
        assert_eq!(
            water.actuate_sloped_pools(&[shape], dt),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(water.pools[0], before);
    }
}

#[test]
fn actuated_beds_reject_jumps_duplicate_steps_and_capacity_failures_atomically() {
    let dt = 1.0 / 60.0;
    let mut water = basin(1);
    let before = water.pools[0].clone();
    let shape = PoolGeometry {
        pool: 0,
        left: 0.0,
        column_width: 1.0,
        bed_edges: &[[-1.0; 2]; 16],
        boundaries: [Boundary::Closed; 2],
    };
    assert_eq!(
        water.actuate_sloped_pools(&[shape], dt),
        Err(WaterError::InvalidInput)
    );
    assert_eq!(water.pools[0], before);
    assert_eq!(
        water.actuate_sloped_pools(
            &[PoolGeometry {
                column_width: 0.5,
                bed_edges: &[[-0.01; 2]; 16],
                ..shape
            }],
            dt
        ),
        Err(WaterError::Capacity)
    );
    assert_eq!(water.pools[0], before);
    assert!(water.parcels.is_empty());
    let gentle = PoolGeometry {
        bed_edges: &[[-0.01; 2]; 16],
        ..shape
    };
    water.actuate_sloped_pools(&[gentle], dt).unwrap();
    let after = water.pools[0].clone();
    assert_eq!(
        water.actuate_sloped_pools(&[gentle], dt),
        Err(WaterError::InvalidInput)
    );
    assert_eq!(water.pools[0], after);
    assert!((water.stats().pooled - 64.0).abs() < 1e-9);
}
