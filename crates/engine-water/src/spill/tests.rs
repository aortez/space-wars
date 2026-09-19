use super::*;
use crate::{Boundary, PoolSpec, WaterWorld};

#[test]
fn compressed_junction_does_not_turn_into_a_wide_spike() {
    let tail = Section {
        position: Vec2::ZERO,
        velocity: Vec2::new(0.0, -60.0),
        flow: 120.0,
    };
    let spill = Spill {
        source: SpillSource::Junction { outlets: [1, 2] },
        tick: 0,
        tail,
        head: Section {
            position: Vec2::new(0.0, -0.001),
            ..tail
        },
    };
    let parcel = Parcel {
        position: Vec2::ZERO,
        velocity: tail.velocity,
        volume: 2.0,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    };
    assert!(
        spill.ribbon(&parcel).is_none(),
        "compressed slices use the compact volume-preserving fallback"
    );
}

#[test]
fn rising_stepped_source_does_not_fold_the_youngest_slice_through_the_lip() {
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: -40.0,
            column_width: 10.0,
            bed: vec![9.0, 6.0, 3.0, 0.0],
            boundaries: [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
        }],
    )
    .unwrap();
    for i in 0..4 {
        water
            .add_to_pool(0, -35.0 + i as f64 * 10.0, 150.0)
            .unwrap();
    }
    for _ in 0..60 {
        let missing = 150.0 - water.pools[0].volume[0];
        if missing > 0.0 {
            water.add_to_pool(0, -35.0, missing).unwrap();
        }
        water.step(1.0 / 60.0).unwrap();
    }
    let i = water.parcels.len() - 1;
    assert!(water.spill_ribbon(i).is_some(), "{:?}", water.spills[i]);
}

fn ledge(depth: f64, gravity: f64, edge: usize) -> WaterWorld {
    let mut boundaries = [Boundary::Closed; 2];
    boundaries[edge] = Boundary::Spill { lip: 0.0 };
    let mut water = WaterWorld::new(
        WaterConfig {
            gravity,
            exit_y: -10000.0,
            max_parcels: 256,
            ..WaterConfig::default()
        },
        vec![PoolSpec {
            left: -20.0,
            column_width: 40.0,
            bed: vec![0.0],
            boundaries,
        }],
    )
    .unwrap();
    water.add_to_pool(0, 0.0, depth * 40.0).unwrap();
    water
}

#[test]
fn connected_slices_share_faces_preserve_area_and_reuse_storage() {
    for hz in [30, 60, 120] {
        for gravity in [80.0, 400.0, 1000.0] {
            for edge in [0, 1] {
                let mut water = ledge(5.0, gravity, edge);
                let storage = (water.parcels.as_ptr(), water.spills.as_ptr());
                for _ in 0..hz / 2 {
                    let missing = 200.0 - water.pools[0].volume[0];
                    if missing > 0.0 {
                        water.add_to_pool(0, 0.0, missing).unwrap();
                    }
                    water.step(1.0 / hz as f64).unwrap();
                    assert_eq!(storage, (water.parcels.as_ptr(), water.spills.as_ptr()));
                }
                for i in 0..water.parcels.len() {
                    let ribbon = water
                        .spill_ribbon(i)
                        .expect("steady outfall stays connected");
                    assert!(ribbon.quads.iter().all(convex));
                    let rendered = ribbon.quads.iter().map(area).sum::<f64>();
                    let volume = water.parcels[i].volume;
                    assert!(
                        (rendered - volume).abs() < volume * 2e-4,
                        "{hz} {gravity} {edge}: {rendered} vs {volume}"
                    );
                    assert_eq!(ribbon.quads[0][1], ribbon.quads[1][0]);
                    assert_eq!(ribbon.quads[0][2], ribbon.quads[1][3]);
                    if i > 0 {
                        let older = water.spill_ribbon(i - 1).unwrap();
                        assert_eq!(ribbon.quads[1][1], older.quads[0][0]);
                        assert_eq!(ribbon.quads[1][2], older.quads[0][3]);
                    }
                }
                let newest = water.spill_ribbon(water.parcels.len() - 1).unwrap();
                let face = [newest.quads[0][0], newest.quads[0][3]];
                let surface = water.pools[0].surface(0) as f32;
                assert!(
                    face.iter()
                        .all(|p| (p.x - if edge == 0 { -20.0 } else { 20.0 }).abs() < 1e-5)
                );
                assert!(face.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).abs() < 1e-5);
                assert!(
                    (face.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) - surface).abs()
                        < 1e-5
                );
                water.reclaim_fraction(0.5).unwrap();
                for i in 0..water.parcels.len() {
                    let rendered = water
                        .spill_ribbon(i)
                        .unwrap()
                        .quads
                        .iter()
                        .map(area)
                        .sum::<f64>();
                    assert!(
                        (rendered - water.parcels[i].volume).abs() < water.parcels[i].volume * 2e-4
                    );
                }
                water.reclaim();
                assert!(water.parcels.is_empty() && water.spills.is_empty());
            }
        }
    }
}

#[test]
fn interrupted_flow_does_not_bridge_a_dry_interval_or_link_independent_drops() {
    let mut water = ledge(5.0, 400.0, 1);
    water.step(1.0 / 60.0).unwrap();
    // Stop ONLY the reservoir, leaving already emitted water in flight.
    water.reclaimed += water.pools[0].volume[0];
    water.pools[0].volume[0] = 0.0;
    for _ in 0..4 {
        water.step(1.0 / 60.0).unwrap();
    }
    let old_tail = water.spills[0].unwrap().tail.position;
    assert!(old_tail.x > 20.0 && old_tail.y < 2.5);
    water.add_to_pool(0, 0.0, 200.0).unwrap();
    water.step(1.0 / 60.0).unwrap();
    let restarted = water.spills.last().unwrap().unwrap();
    assert!(restarted.head.position.x < old_tail.x);
    water
        .add_falling(Parcel {
            position: Vec2::new(25.0, 10.0),
            velocity: Vec2::new(0.0, 20.0),
            volume: 2.0,
            duration: 1.0 / 60.0,
            horizontal_bounds: None,
        })
        .unwrap();
    assert!(water.spill_ribbon(water.parcels.len() - 1).is_none());
    water.step(1.0 / 60.0).unwrap();
    assert_eq!(water.spills.iter().filter(|s| s.is_none()).count(), 1);
}

#[test]
fn nearly_empty_outfalls_stay_speed_bounded_and_birth_motion_is_swept() {
    let source = ledge(0.0001, 400.0, 1);
    let mut water = WaterWorld::new(
        WaterConfig {
            max_speed: 10.0,
            ..source.config
        },
        vec![
            source.pools[0].spec.clone(),
            PoolSpec {
                left: 20.0,
                column_width: 1.0,
                bed: vec![-0.01],
                boundaries: [Boundary::Closed; 2],
            },
        ],
    )
    .unwrap();
    water.add_to_pool(0, 0.0, 0.0001 * 40.0).unwrap();
    water.step(1.0 / 30.0).unwrap();
    assert!(
        water.pools[1].volume[0] > 0.0,
        "collector crossed during the birth half-step"
    );
    assert!(water.parcels.is_empty());
    assert!(water.spills.is_empty());
    let stats = water.stats();
    assert!((stats.injected - stats.pooled).abs() < 1e-10);

    let mut water = ledge(1000.0, 400.0, 1);
    water.config.max_speed = 10.0;
    for _ in 0..60 {
        water.step(1.0 / 60.0).unwrap();
        assert!(
            water
                .parcels
                .iter()
                .all(|p| p.velocity.x.abs() <= 10.0 && p.velocity.y.abs() <= 10.0)
        );
    }
}
