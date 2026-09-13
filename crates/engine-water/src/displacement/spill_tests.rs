use super::*;
use crate::{Boundary, PoolSpec, WaterConfig};

fn basin(columns: usize, edge: usize, collector: bool, max_parcels: usize) -> WaterWorld {
    let mut boundaries = [Boundary::Closed; 2];
    boundaries[edge] = Boundary::Spill { lip: 20.0 };
    let mut specs = vec![PoolSpec {
        left: -50.0,
        column_width: 100.0 / columns as f64,
        bed: vec![0.0; columns],
        boundaries,
    }];
    if collector {
        specs.push(PoolSpec {
            left: -200.0,
            column_width: 400.0 / columns as f64,
            bed: vec![-80.0; columns],
            boundaries: [Boundary::Closed; 2],
        });
    }
    let mut water = WaterWorld::new(
        WaterConfig {
            gravity: 40.0,
            damping: 2.0,
            exit_y: -120.0,
            max_parcels,
            ..WaterConfig::default()
        },
        specs,
    )
    .unwrap();
    for i in 0..columns {
        water
            .add_to_pool(
                0,
                -50.0 + (i as f64 + 0.5) * 100.0 / columns as f64,
                2000.0 / columns as f64,
            )
            .unwrap();
    }
    water
}

fn box_at(y: f32) -> DisplacementBody {
    DisplacementBox {
        center: Vec2::new(0.0, y),
        half_extents: Vec2::new(10.0, 5.0),
        angle: 0.0,
    }
    .into()
}

fn volume(water: &WaterWorld, pool: usize) -> f64 {
    water.pools()[pool].columns().map(|c| c.volume).sum()
}

fn conserved(water: &WaterWorld) {
    let s = water.stats();
    assert!(
        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-7,
        "{s:?}"
    );
    assert!(
        water
            .pools()
            .iter()
            .flat_map(|p| p.columns())
            .all(|c| c.volume >= 0.0 && c.displaced >= 0.0 && c.surface.is_finite())
    );
}

#[test]
fn open_basin_recomputes_partial_occupancy_as_liquid_spills() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        let mut water = basin(1, 1, false, 512);
        water.set_displacers(0, &[box_at(23.0)]).unwrap();
        assert!((water.stats().displaced - 50.0).abs() < 1e-8);
        // A single column removes horizontal-flow transients: this is only
        // outflow and the reference capacity W*h - 20*(h-18) = liquid.
        for _ in 0..(600.0 / dt) as usize {
            water.step(dt).unwrap();
            let remaining = volume(&water, 0);
            let expected_height = (remaining - 360.0) / 80.0;
            let expected_displaced = 20.0 * (expected_height - 18.0);
            assert!(
                (water.stats().displaced - expected_displaced).abs() < 1e-8,
                "stale occupancy dt={dt}: {:?}, expected={expected_displaced}",
                water.stats()
            );
            conserved(&water);
        }
        // At the lip: remaining = 100*20 - 20*(20-18) = 1960.
        // Free outfall approaches equilibrium asymptotically.
        assert!((volume(&water, 0) - 1960.0).abs() < 0.25);
        let remaining = volume(&water, 0);
        water.set_displacers(0, &[]).unwrap();
        assert_eq!(volume(&water, 0), remaining);
        assert_eq!(water.stats().displaced, 0.0);
        for _ in 0..(5.0 / dt) as usize {
            water.step(dt).unwrap();
            conserved(&water);
        }
        assert_eq!(volume(&water, 0), remaining);
        assert!(
            (water.pools()[0].columns().next().unwrap().surface - remaining / 100.0).abs() < 1e-10
        );
        assert!((water.stats().drained - (2000.0 - remaining)).abs() < 1e-7);
    }
}

#[test]
fn a_slowly_inserted_box_spills_into_a_collector_and_withdrawal_cannot_refill_the_source() {
    for (columns, dt) in [(16, 1.0 / 30.0), (64, 1.0 / 60.0), (256, 1.0 / 120.0)] {
        for edge in [0, 1] {
            let mut water = basin(columns, edge, true, 512);
            let mut control = basin(columns, edge, true, 512);
            let mut saw_flight = false;
            let mut saw_collection = false;
            let mut last_source = 2000.0;
            // Free outfall decays with head^(3/2); it approaches the lip
            // asymptotically rather than finishing in the insertion interval.
            for tick in 0..(240.0 / dt) as usize {
                let time = tick as f64 * dt;
                let y = if time < 5.0 { 30.0 - 4.0 * time } else { 10.0 };
                water.set_displacers(0, &[box_at(y as f32)]).unwrap();
                water.step(dt).unwrap();
                control.step(dt).unwrap();
                conserved(&water);
                saw_flight |= water.stats().in_flight > 0.01;
                saw_collection |= volume(&water, 1) > 1.0;
                let source = volume(&water, 0);
                assert!(
                    source <= last_source + 1e-8,
                    "collector must not feed the higher basin"
                );
                last_source = source;
            }
            assert!(saw_flight && saw_collection);
            assert_eq!(water.stats().drained, 0.0);
            assert!((control.stats().pooled - 2000.0).abs() < 1e-8);
            assert_eq!(control.stats().displaced, 0.0);
            assert!(volume(&control, 1) < 1e-8);
            // Slow insertion limits slosh: final source capacity is about
            // 100*20 - 200 = 1800. Transient outflow can overshoot equilibrium.
            eprintln!("settled columns={columns} dt={dt} edge={edge} source={last_source}");
            assert!(
                (volume(&water, 0) - 1800.0).abs() < 5.0,
                "columns={columns} dt={dt} source={}",
                volume(&water, 0)
            );
            let retained = volume(&water, 0);
            water.set_displacers(0, &[]).unwrap();
            for _ in 0..(30.0 / dt) as usize {
                water.step(dt).unwrap();
                conserved(&water);
            }
            assert!(volume(&water, 0) <= retained + 1e-8);
            assert!(
                water.pools()[0]
                    .columns()
                    .all(|c| c.surface < 18.3 && c.surface > 17.7)
            );
            assert!((volume(&water, 1) - (2000.0 - volume(&water, 0))).abs() < 1e-7);
            assert_eq!(water.stats().displaced, 0.0);
        }
    }
}

#[test]
fn partially_submerged_union_refreshes_when_only_liquid_volume_changes() {
    // Two separate boxes exercise the cached union path. Their combined
    // width is 20, so the independent capacity equation is the same as above.
    let bodies = [-15.0, 15.0].map(|x| DisplacementBody {
        center: Vec2::new(x, 23.0),
        angle: 0.0,
        shape: HullShape::Box {
            half_width: 5.0,
            half_height: 5.0,
        },
    });
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        let mut water = basin(1, 1, false, 512);
        water.set_displacers(0, &bodies).unwrap();
        for _ in 0..(10.0 / dt) as usize {
            water.step(dt).unwrap();
            let expected = 20.0 * ((volume(&water, 0) - 360.0) / 80.0 - 18.0);
            assert!((water.stats().displaced - expected).abs() < 1e-7);
            conserved(&water);
        }
        assert!(volume(&water, 0) < 1995.0);
    }
}

#[test]
fn overlapping_moving_displacers_replay_spill_and_reclaim_without_changing_the_ledger() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        let mut a = basin(64, 1, true, 512);
        let mut b = basin(64, 1, true, 512);
        let mut saw_spill = false;
        for tick in 0..(12.0 / dt) as usize {
            let t = tick as f32 * dt as f32;
            let input = [
                DisplacementBody {
                    center: Vec2::new(t.sin() * 10.0, 20.0 + (t * 2.0).cos() * 6.0),
                    angle: t,
                    ..box_at(20.0)
                },
                DisplacementBody {
                    center: Vec2::new(t.sin() * 10.0 + 5.0, 20.0),
                    angle: 0.0,
                    shape: HullShape::Circle { radius: 5.0 },
                },
            ];
            for w in [&mut a, &mut b] {
                w.set_displacers(0, &input).unwrap();
                // The same authoritative batch can be clipped by another
                // flat basin. Dry/outside bodies must not make phantom water.
                w.set_displacers(1, &input).unwrap();
                w.step(dt).unwrap();
                if tick == 100 {
                    w.reclaim_fraction(0.2).unwrap();
                }
                conserved(w);
            }
            assert_eq!(a.pools(), b.pools());
            assert_eq!(a.parcels(), b.parcels());
            assert_eq!(a.stats(), b.stats());
            saw_spill |= a.stats().in_flight > 0.1;
            assert_eq!(
                a.pools()[1].columns().map(|c| c.displaced).sum::<f64>(),
                0.0
            );
        }
        assert!(saw_spill);
        assert!(volume(&a, 1) > 0.0);
        a.reclaim();
        assert_eq!(
            (a.stats().pooled, a.stats().displaced, a.stats().in_flight),
            (0.0, 0.0, 0.0)
        );
        conserved(&a);
        a.step(dt).unwrap();
        assert!(
            a.pools()
                .iter()
                .flat_map(|p| p.columns())
                .all(|c| c.surface == c.bed)
        );
    }
}

#[test]
fn parcel_backpressure_holds_displaced_water_until_capacity_becomes_available() {
    let mut water = basin(1, 1, false, 1);
    water.set_displacers(0, &[box_at(23.0)]).unwrap();
    // One unrelated, valid parcel occupies the entire parcel budget.
    water
        .add_falling(crate::Parcel {
            position: Vec2::new(200.0, 20.0),
            velocity: Vec2::ZERO,
            volume: 1.0,
            duration: 1.0 / 60.0,
            horizontal_bounds: None,
        })
        .unwrap();
    let old = volume(&water, 0);
    water.step(1.0 / 60.0).unwrap();
    assert_eq!(volume(&water, 0), old);
    assert!((water.stats().displaced - 50.0).abs() < 1e-8);
    assert_eq!(water.stats().capacity_limited_ticks, 1);
    for _ in 0..600 {
        water.step(1.0 / 60.0).unwrap();
        conserved(&water);
    }
    assert!(volume(&water, 0) < old - 0.1);
    assert!(water.stats().drained > 1.0);
}
