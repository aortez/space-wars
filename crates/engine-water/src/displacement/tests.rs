use super::*;
use crate::{PoolSpec, WaterConfig};

const DT: f64 = 1.0 / 60.0;

fn tank(columns: usize) -> WaterWorld {
    let mut w = WaterWorld::new(
        WaterConfig {
            gravity: 40.0,
            damping: 2.0,
            ..WaterConfig::default()
        },
        vec![PoolSpec {
            left: -50.0,
            column_width: 100.0 / columns as f64,
            bed: vec![0.0; columns],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for i in 0..columns {
        w.add_to_pool(
            0,
            -50.0 + (i as f64 + 0.5) * 100.0 / columns as f64,
            2000.0 / columns as f64,
        )
        .unwrap();
    }
    w
}

fn body(y: f32) -> DisplacementBox {
    DisplacementBox {
        center: Vec2::new(0.0, y),
        half_extents: Vec2::new(10.0, 5.0),
    }
}

fn conserved(w: &WaterWorld) {
    let s = w.stats();
    assert!(
        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-7,
        "{s:?}"
    );
    assert!(
        w.pools()[0]
            .columns()
            .all(|c| c.volume >= 0.0 && c.surface.is_finite() && c.displaced >= 0.0)
    );
    assert_eq!(s.parcels, 0);
    assert_eq!(s.drained, 0.0);
}

fn settle(w: &mut WaterWorld, expected: f64) {
    for _ in 0..3600 {
        w.step(DT).unwrap();
        conserved(w);
    }
    assert!(
        w.pools()[0]
            .columns()
            .all(|c| (c.surface - expected).abs() < 0.025),
        "expected={expected} {:?}",
        w.pools()[0]
            .columns()
            .map(|c| c.surface)
            .collect::<Vec<_>>()
    );
}

#[test]
fn fully_submerged_box_raises_level_by_its_area_and_removal_restores_it() {
    for columns in [32, 128] {
        let mut w = tank(columns);
        w.set_displacer(0, Some(body(10.0))).unwrap();
        assert!((w.stats().displaced - 200.0).abs() < 1e-8);
        assert!((w.stats().pooled - 2000.0).abs() < 1e-8);
        assert!(w.sample(Vec2::new(0.0, 10.0)).is_none());
        assert!(w.sample(Vec2::new(30.0, 10.0)).is_some());
        settle(&mut w, 22.0);
        w.set_displacer(0, None).unwrap();
        assert_eq!(w.stats().displaced, 0.0);
        settle(&mut w, 20.0);
    }
}

#[test]
fn partial_submersion_uses_free_tank_capacity_not_the_old_surface() {
    let mut w = tank(128);
    // bottom=18, width=20. 100*h - 20*(h-18) = 2000 => h=20.5.
    w.set_displacer(0, Some(body(23.0))).unwrap();
    assert!((w.stats().displaced - 50.0).abs() < 1e-7);
    settle(&mut w, 20.5);
    // Half the box is outside the basin; only the intersecting area counts.
    w.set_displacer(
        0,
        Some(DisplacementBox {
            center: Vec2::new(50.0, 10.0),
            ..body(10.0)
        }),
    )
    .unwrap();
    assert!((w.stats().displaced - 100.0).abs() < 1e-7);
    settle(&mut w, 21.0);
}

#[test]
fn repeated_entry_exit_and_translation_make_bounded_ripples_without_volume_drift() {
    for dt in [1.0 / 120.0, DT, 1.0 / 30.0] {
        let mut a = tank(128);
        let mut b = tank(128);
        let mut peak: f64 = 0.0;
        let count = (24.0 / dt) as usize;
        let ptr = a.pools[0].displaced.as_ptr();
        for tick in 0..count {
            let phase = std::f64::consts::TAU * tick as f64 * dt / 4.0;
            let mut hull = body((20.0 + 12.0 * phase.cos()) as f32);
            hull.center.x = (15.0 * phase.sin()) as f32;
            for w in [&mut a, &mut b] {
                w.set_displacer(0, Some(hull)).unwrap();
                w.step(dt).unwrap();
                conserved(w);
            }
            assert_eq!(a.pools, b.pools);
            assert_eq!(a.pools[0].displaced.as_ptr(), ptr);
            let c: Vec<_> = a.pools()[0].columns().collect();
            let min = c.iter().map(|c| c.surface).fold(f64::INFINITY, f64::min);
            let max = c.iter().map(|c| c.surface).fold(0.0, f64::max);
            peak = peak.max(max - min);
            assert!(max < 45.0, "unbounded surface {max} dt={dt}");
        }
        assert!(peak > 1.0, "no propagating disturbance");
        a.set_displacer(0, None).unwrap();
        settle(&mut a, 20.0);
    }
}

#[test]
fn sources_and_partial_reclaim_refresh_occupancy_without_readding_the_box() {
    let mut w = tank(128);
    w.set_displacer(0, Some(body(23.0))).unwrap();
    assert!((w.stats().displaced - 50.0).abs() < 1e-7);
    w.add_to_pool(0, -30.0, 800.0).unwrap();
    assert!((w.stats().displaced - 200.0).abs() < 1e-7);
    settle(&mut w, 30.0);
    w.reclaim_fraction(0.5).unwrap();
    assert_eq!(w.stats().displaced, 0.0);
    settle(&mut w, 14.0);
    assert!((w.stats().reclaimed - 1400.0).abs() < 1e-7);
}

#[test]
fn invalid_geometry_is_atomic_and_empty_or_reclaimed_water_has_no_ghost_height() {
    let mut w = tank(32);
    w.set_displacer(0, Some(body(10.0))).unwrap();
    let before = w.pools.clone();
    for invalid in [
        DisplacementBox {
            center: Vec2::new(f32::NAN, 1.0),
            ..body(10.0)
        },
        DisplacementBox {
            half_extents: Vec2::new(40.0, 1.0),
            ..body(10.0)
        },
        DisplacementBox {
            half_extents: Vec2::ZERO,
            ..body(10.0)
        },
    ] {
        assert!(w.set_displacer(0, Some(invalid)).is_err());
        assert_eq!(w.pools, before);
    }
    assert!(w.set_displacer(1, Some(body(10.0))).is_err());
    for spec in [
        PoolSpec {
            left: -50.0,
            column_width: 10.0,
            bed: vec![0.0; 10],
            boundaries: [Boundary::Spill { lip: 0.0 }; 2],
        },
        PoolSpec {
            left: -50.0,
            column_width: 50.0,
            bed: vec![0.0, 1.0],
            boundaries: [Boundary::Closed; 2],
        },
    ] {
        let mut invalid = WaterWorld::new(WaterConfig::default(), vec![spec]).unwrap();
        assert_eq!(
            invalid.set_displacer(0, Some(body(10.0))),
            Err(WaterError::InvalidGeometry)
        );
    }
    w.reclaim();
    w.step(DT).unwrap();
    assert_eq!(w.stats().displaced, 0.0);
    assert!(w.pools()[0].columns().all(|c| c.surface == c.bed));
    conserved(&w);
}
