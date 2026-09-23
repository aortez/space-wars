use super::*;
use crate::{PoolSpec, WaterConfig};

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
