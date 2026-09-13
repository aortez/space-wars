use super::*;
use crate::{Boundary, PoolSpec, WaterConfig};

const DT: f64 = 1.0 / 60.0;

pub(super) fn tank(columns: usize) -> WaterWorld {
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
        angle: 0.0,
    }
}

pub(super) fn conserved(w: &WaterWorld) {
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

pub(super) fn settle(w: &mut WaterWorld, expected: f64) {
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
            angle: f32::NAN,
            ..body(10.0)
        },
        DisplacementBox {
            angle: f32::INFINITY,
            ..body(10.0)
        },
        // Must fit at every angle, not only at the submitted orientation.
        DisplacementBox {
            half_extents: Vec2::new(1.0, 40.0),
            ..body(10.0)
        },
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
            column_width: 50.0,
            bed: vec![0.0, 1.0],
            boundaries: [Boundary::Closed; 2],
        },
        PoolSpec {
            left: -50.0,
            column_width: 50.0,
            bed: vec![0.0, 1.0],
            boundaries: [Boundary::Spill { lip: 1.0 }; 2],
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

#[test]
fn rotated_box_area_reference_level_and_samples_match_the_actual_hull() {
    for columns in [32, 128, 512] {
        let mut w = tank(columns);
        let diamond = DisplacementBox {
            center: Vec2::new(0.0, 20.5),
            half_extents: Vec2::new(5.0, 5.0),
            angle: std::f32::consts::FRAC_PI_4,
        };
        w.set_displacer(0, Some(diamond)).unwrap();
        // A centrally symmetric square is half submerged when its reference
        // waterline passes through its center: 2000 + 50 = 100 * 20.5.
        assert!((w.stats().displaced - 50.0).abs() < 1e-8);
        settle(&mut w, 20.5);
        assert!(w.sample(Vec2::new(6.0, 20.0)).is_none()); // inside, outside old AABB
        assert!(w.sample(Vec2::new(4.5, 16.0)).is_some()); // outside, inside old AABB
        assert!(w.pools()[0].columns().all(|c| c.displaced <= c.width * 8.0));
        w.reclaim();
        assert_eq!(w.stats().displaced, 0.0);
        conserved(&w);
    }
}

#[test]
fn rotated_clipping_handles_bed_walls_dry_boxes_and_near_axis_angles() {
    let mut w = tank(128);
    for (x, y, area) in [
        (0.0, 10.0, 100.0),
        (50.0, 10.0, 50.0),
        (0.0, 0.0, 50.0),
        (50.0, 0.0, 25.0),
        (70.0, 10.0, 0.0),
        (0.0, 40.0, 0.0),
    ] {
        w.set_displacer(
            0,
            Some(DisplacementBox {
                center: Vec2::new(x, y),
                half_extents: Vec2::new(5.0, 5.0),
                angle: std::f32::consts::FRAC_PI_4,
            }),
        )
        .unwrap();
        assert!((w.stats().displaced - area).abs() < 1e-7, "x={x} y={y}");
        conserved(&w);
    }
    w.set_displacer(0, Some(body(23.0))).unwrap();
    let aligned = w.pools()[0].displaced.clone();
    for angle in [1e-8, -1e-8, std::f32::consts::PI, std::f32::consts::TAU] {
        w.set_displacer(
            0,
            Some(DisplacementBox {
                angle,
                ..body(23.0)
            }),
        )
        .unwrap();
        assert!(
            w.pools()[0]
                .displaced
                .iter()
                .zip(&aligned)
                .all(|(a, b)| (a - b).abs() < 1e-5)
        );
    }
}

#[test]
fn rotating_entry_exit_replays_conserves_and_reuses_storage() {
    for dt in [1.0 / 30.0, DT, 1.0 / 120.0] {
        let mut a = tank(128);
        let mut b = tank(128);
        let ptr = a.pools[0].displaced.as_ptr();
        for tick in 0..(12.0 / dt) as usize {
            let phase = tick as f64 * dt * std::f64::consts::TAU / 4.0;
            let pose = DisplacementBox {
                center: Vec2::new(
                    (15.0 * phase.sin()) as f32,
                    (20.0 + 16.0 * phase.cos()) as f32,
                ),
                angle: phase as f32,
                ..body(0.0)
            };
            for w in [&mut a, &mut b] {
                w.set_displacer(0, Some(pose)).unwrap();
                w.step(dt).unwrap();
                conserved(w);
                assert!(w.pools()[0].columns().all(|c| c.surface < 45.0));
            }
            assert_eq!(a.pools, b.pools);
            assert_eq!(a.pools[0].displaced.as_ptr(), ptr);
        }
        a.set_displacer(0, None).unwrap();
        settle(&mut a, 20.0);
    }
}

#[test]
fn rotated_occupancy_is_translation_invariant_and_refreshes_on_volume_changes() {
    let mut local = tank(128);
    let mut spec = local.pools()[0].spec().clone();
    spec.left += 500_000.0;
    spec.bed.fill(500_000.0);
    let mut translated = WaterWorld::new(WaterConfig::default(), vec![spec]).unwrap();
    for i in 0..128 {
        translated
            .add_to_pool(
                0,
                500_000.0 - 50.0 + (i as f64 + 0.5) * 100.0 / 128.0,
                2000.0 / 128.0,
            )
            .unwrap();
    }
    let box_local = DisplacementBox {
        angle: 0.63,
        ..body(23.0)
    };
    local.set_displacer(0, Some(box_local)).unwrap();
    translated
        .set_displacer(
            0,
            Some(DisplacementBox {
                center: box_local.center + Vec2::new(500_000.0, 500_000.0),
                ..box_local
            }),
        )
        .unwrap();
    assert_eq!(local.pools()[0].displaced, translated.pools()[0].displaced);
    assert!(local.stats().displaced > 0.0 && local.stats().displaced < 200.0);
    local.add_to_pool(0, -30.0, 1500.0).unwrap();
    assert!((local.stats().displaced - 200.0).abs() < 1e-8);
    local.reclaim_fraction(0.75).unwrap();
    assert_eq!(local.stats().displaced, 0.0);
    conserved(&local);
}
