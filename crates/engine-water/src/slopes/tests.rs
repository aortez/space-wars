use super::*;
use crate::{
    Parcel, PoolSpec,
    immersion::{HullShape, WaterHull},
};

fn ramp(mirrored: bool, open: bool) -> WaterWorld {
    let edges: Vec<_> = (0..32)
        .map(|i| {
            let at = |j: usize| {
                if mirrored {
                    j as f64 * 0.25
                } else {
                    8.0 - j as f64 * 0.25
                }
            };
            [at(i), at(i + 1)]
        })
        .collect();
    let mut boundaries = [Boundary::Closed; 2];
    if open {
        boundaries[usize::from(!mirrored)] = Boundary::Spill { lip: 0.0 };
    }
    let mut w = WaterWorld::new(
        WaterConfig {
            max_parcels: 256,
            ..WaterConfig::default()
        },
        vec![PoolSpec {
            left: -40.0,
            column_width: 2.5,
            bed: vec![0.0; 32],
            boundaries,
        }],
    )
    .unwrap();
    w.configure_sloped_bed(0, &edges).unwrap();
    w
}

fn accounting(w: &WaterWorld) {
    let s = w.stats();
    assert!(
        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-8,
        "{s:?}"
    );
    assert!(
        w.pools()[0]
            .columns()
            .all(|c| c.volume >= 0.0 && c.surface.is_finite())
    );
}

#[test]
fn triangular_and_full_column_capacity_invert_exactly() {
    for bed in [[0.0, 10.0], [10.0, 0.0], [4.0; 2], [-100.0, -98.0]] {
        for v in [0.0, 1e-8, 0.02, 1.0, 12.5, 50.0, 1000.0] {
            let h = level(bed, 2.5, v);
            assert!((area(bed, 2.5, h) - v).abs() < 1e-10, "{bed:?} {v} {h}");
        }
    }
}

#[test]
fn horizontal_lake_stays_at_rest_on_a_partly_dry_slope() {
    for mirror in [false, true] {
        for height in [2.13, 5.0, 12.0] {
            let mut w = ramp(mirror, false);
            let cells: Vec<_> = w.pools()[0].columns().collect();
            for c in cells {
                let v = c.area_below(height);
                if v > 0.0 {
                    w.add_to_pool(0, c.left + c.width * 0.5, v).unwrap();
                }
            }
            let before = w.pools[0].volume.clone();
            for _ in 0..600 {
                w.step(1.0 / 60.0).unwrap();
                accounting(&w);
            }
            for (a, b) in before.iter().zip(&w.pools[0].volume) {
                assert!((a - b).abs() < 1e-9);
            }
            assert!(w.pools()[0].columns().all(|c| c.velocity.abs() < 1e-9));
        }
    }
}

#[test]
fn thin_and_deep_runoff_drain_symmetrically_without_suction() {
    for depth in [0.03, 0.2, 4.0] {
        let mut a = ramp(false, true);
        let mut b = ramp(true, true);
        for w in [&mut a, &mut b] {
            for i in 0..32 {
                w.add_to_pool(0, -40.0 + (i as f64 + 0.5) * 2.5, depth * 2.5)
                    .unwrap();
            }
        }
        for _ in 0..2400 {
            a.step(1.0 / 60.0).unwrap();
            b.step(1.0 / 60.0).unwrap();
            accounting(&a);
            accounting(&b);
            assert!((a.stats().pooled - b.stats().pooled).abs() < 1e-8);
        }
        assert!(
            a.stats().drained > a.stats().injected * 0.99,
            "depth={depth} {:?}",
            a.stats()
        );
    }
}

#[test]
fn samples_and_immersion_clip_the_actual_wet_triangle() {
    let mut w = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: 0.0,
            column_width: 10.0,
            bed: vec![0.0],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    w.configure_sloped_bed(0, &[[0.0, 10.0]]).unwrap();
    w.add_to_pool(0, 2.0, 12.5).unwrap(); // level 5, area 5*5/2
    assert!(w.sample(Vec2::new(2.0, 3.0)).is_some());
    assert!(w.sample(Vec2::new(4.0, 3.0)).is_none());
    assert!(w.sample(Vec2::new(8.0, 5.0)).is_none());
    let hull = WaterHull::new(HullShape::Box {
        half_width: 10.0,
        half_height: 10.0,
    })
    .unwrap();
    let m = hull
        .measure(&w, Vec2::new(5.0, 5.0), 0.0, Vec2::ZERO)
        .unwrap();
    assert!((m.area - 12.5).abs() < 1e-10);
    let c = w.pools()[0].columns().next().unwrap();
    let center = c.water_centroid();
    assert!((center.x as f64 - 5.0 / 3.0).abs() < 1e-6);
    assert!((center.y as f64 - 10.0 / 3.0).abs() < 1e-6);
    assert!((m.first_moment[0] / m.area - center.x as f64).abs() < 1e-6);
    assert!((m.first_moment[1] / m.area - center.y as f64).abs() < 1e-6);
}

#[test]
fn falling_water_lands_on_dry_high_slope_without_underside_catches() {
    let mut w = ramp(false, false);
    assert_eq!(
        w.catch(Vec2::new(-37.0, 12.0), Vec2::new(-37.0, 2.0), None),
        Some((0, 1))
    );
    assert_eq!(
        w.catch(Vec2::new(-37.0, 5.0), Vec2::new(-37.0, 2.0), None),
        None
    );
    w.add_falling(Parcel {
        position: Vec2::new(-37.0, 12.0),
        velocity: Vec2::new(0.0, -200.0),
        volume: 3.0,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    })
    .unwrap();
    for _ in 0..5 {
        w.step(1.0 / 60.0).unwrap();
        accounting(&w);
    }
    assert_eq!(w.stats().parcels, 0);
    assert!((w.stats().pooled - 3.0).abs() < 1e-8);
}

#[test]
fn outfall_leaves_along_the_slope_and_raised_lips_retain_water() {
    for mirrored in [false, true] {
        let mut w = ramp(mirrored, true);
        let x = if mirrored { -39.0 } else { 39.0 };
        w.add_to_pool(0, x, 10.0).unwrap();
        w.step(1.0 / 60.0).unwrap();
        let ribbon = w.spill_ribbon(0).unwrap();
        assert!(
            ribbon
                .quads
                .iter()
                .flatten()
                .all(|p| p.x.is_finite() && p.y.is_finite())
        );
        let v = w.pools()[0].outlet_velocity(
            usize::from(!mirrored),
            if mirrored { -20.0 } else { 20.0 },
            1000.0,
        );
        assert!((v.y + 2.0).abs() < 1e-5);
    }
    let mut w = ramp(false, true);
    w.pools[0].spec.boundaries[1] = Boundary::Spill { lip: 3.0 };
    let cells: Vec<_> = w.pools()[0].columns().collect();
    let retained: f64 = cells.iter().map(|c| c.area_below(3.0)).sum();
    for c in cells {
        w.add_to_pool(0, c.left + 0.5 * c.width, c.area_below(12.0))
            .unwrap();
    }
    for _ in 0..2400 {
        w.step(1.0 / 60.0).unwrap();
        accounting(&w);
    }
    assert!(w.stats().pooled >= retained - 1e-8);
    assert!(
        w.stats().pooled < retained + 0.5,
        "{} {retained}",
        w.stats().pooled
    );
}

#[test]
fn configuration_rejects_moving_wet_beds_and_bad_endpoints_atomically() {
    let mut w = ramp(false, false);
    let before = w.pools[0].clone();
    assert_eq!(
        w.configure_sloped_bed(0, &[[0.0, f64::NAN]; 32]),
        Err(WaterError::InvalidGeometry)
    );
    assert_eq!(w.pools[0], before);
    w.add_to_pool(0, 0.0, 1.0).unwrap();
    let before = w.pools[0].clone();
    assert_eq!(
        w.configure_sloped_bed(0, &[[0.0; 2]; 32]),
        Err(WaterError::InvalidInput)
    );
    assert_eq!(w.pools[0], before);
    w.reclaim();
    w.step(1.0 / 60.0).unwrap();
    assert_eq!(
        w.configure_sloped_bed(0, &[[0.0; 2]; 32]),
        Err(WaterError::InvalidInput)
    );
}
