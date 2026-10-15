use super::*;
use crate::{Boundary, PoolSpec, WaterWorld};

mod junction;

fn has_boundary_edge(ribbon: &SpillRibbon, from: Vec2, to: Vec2) -> bool {
    ribbon
        .quads()
        .iter()
        .flat_map(|q| (0..4).map(move |i| (q[i], q[(i + 1) % 4])))
        .filter(|&(a, b)| (a == from && b == to) || (a == to && b == from))
        .count()
        == 1
}

#[test]
fn retained_sloping_outfalls_cover_the_vertical_lip_and_advect_the_same_face() {
    for hz in [30, 60, 120] {
        for edge in [0, 1] {
            for height in [10.0, 177.5, 355.0] {
                let mut w = ledge(0.0, 400.0, edge);
                w.config.retain_spill_shapes = true;
                let rise = 40.0 * 15.0_f64.to_radians().tan();
                w.configure_sloped_bed(0, &[if edge == 1 { [rise, 0.0] } else { [0.0, rise] }])
                    .unwrap();
                let volume = w.pools[0].column(0).area_below(height);
                w.add_to_pool(0, 0.0, volume).unwrap();
                for _ in 0..8 {
                    w.step(1.0 / hz as f64).unwrap();
                    let s = w.spills.last().unwrap().unwrap();
                    let p = w.parcels.last().unwrap();
                    let ribbon = s.ribbon(p).expect("sloped outlet remains a sheet");
                    let bottom = Vec2::new(if edge == 1 { 20.0 } else { -20.0 }, 0.0);
                    let top = Vec2::new(bottom.x, w.pools[0].surface(0) as f32);
                    assert!(has_boundary_edge(&ribbon, bottom, top));
                    assert!(s.tail.velocity.y < 0.0);
                    assert!(
                        (ribbon.quads().iter().map(area).sum::<f64>() - p.volume).abs()
                            < p.volume * 2e-4
                    );
                    let mut falling = s.tail;
                    falling.advance(0.2, w.config, None);
                    assert_eq!(
                        falling.offset(),
                        s.tail.offset(),
                        "gravity moves, rather than rotates, the face"
                    );
                    assert!(falling.position.y < s.tail.position.y);
                }
                let original = w.spills.last().unwrap().unwrap().tail.offset();
                w.reclaim_fraction(0.5).unwrap();
                assert_eq!(
                    w.spills.last().unwrap().unwrap().tail.offset(),
                    original * 0.5
                );
                for (i, p) in w.parcels.iter().enumerate() {
                    let rendered = w
                        .spill_ribbon(i)
                        .unwrap()
                        .quads()
                        .iter()
                        .map(area)
                        .sum::<f64>();
                    assert!((rendered - p.volume).abs() < p.volume * 2e-4);
                }
            }
        }
    }
}

#[test]
fn concave_slices_split_without_highlighting_the_internal_diagonal() {
    let concave = [
        Vec2::ZERO,
        Vec2::new(2.0, 0.0),
        Vec2::new(0.5, 0.5),
        Vec2::new(0.0, 2.0),
    ];
    for rotation in 0..4 {
        let quad = std::array::from_fn(|i| concave[(i + rotation) % 4]);
        for surface in [0, 1] {
            let mut ribbon = SpillRibbon::new(surface);
            ribbon.push_piece(quad).unwrap();
            assert_eq!(ribbon.quads().len(), 2);
            assert!(ribbon.quads().iter().all(convex));
            assert!((ribbon.quads().iter().map(area).sum::<f64>() - area(&quad)).abs() < 1e-9);
            assert_eq!((0..2).filter(|&i| ribbon.has_surface(i)).count(), 1);
            let expected = if surface == 1 {
                [quad[3], quad[2]]
            } else {
                [quad[0], quad[1]]
            };
            let piece = ribbon
                .quads()
                .iter()
                .enumerate()
                .find(|(i, _)| ribbon.has_surface(*i))
                .unwrap()
                .1;
            let actual = if surface == 1 {
                [piece[3], piece[2]]
            } else {
                [piece[0], piece[1]]
            };
            assert_eq!(actual, expected);
        }
    }
    let crossing = [
        Vec2::ZERO,
        Vec2::new(2.0, 2.0),
        Vec2::new(0.0, 2.0),
        Vec2::new(2.0, 0.0),
    ];
    assert!(SpillRibbon::new(1).push_piece(crossing).is_none());
}

#[test]
fn moving_outlet_slices_keep_the_exposed_surface_smooth_and_preserve_area() {
    for edge in [0, 1] {
        for volume in [200.0, 600.0, 800.0, 1000.0, 1400.0] {
            let sign = if edge == 1 { 1.0 } else { -1.0 };
            let tail = Section {
                position: Vec2::ZERO,
                velocity: Vec2::new(sign * 200.0, -10.0),
                flow: 40_000.0,
                material_offset: None,
            };
            let head = Section {
                position: Vec2::new(sign * 5.0, -1.0),
                velocity: Vec2::new(sign * 200.0, -12.0),
                flow: 40_000.0,
                material_offset: None,
            };
            let spill = Spill {
                sheet: None,
                source: SpillSource::Outlet { pool: 0, edge },
                tick: 1,
                upstream_end: None,
                tail,
                head,
            };
            let parcel = Parcel {
                position: (tail.position + head.position) * 0.5,
                velocity: (tail.velocity + head.velocity) * 0.5,
                volume,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            };
            let ribbon = spill.ribbon(&parcel).expect("a positive-area outfall");
            assert!(ribbon.quads().len() <= 6);
            assert!(ribbon.quads().iter().all(convex));
            let rendered = ribbon.quads().iter().map(area).sum::<f64>();
            assert!((rendered - volume).abs() < volume * 2e-4);
            let quads = ribbon.quads();
            let young = tail.position + tail.offset() * sign;
            let old = head.position + head.offset() * sign;
            let chord = old - young;
            assert!((0..quads.len()).any(|i| ribbon.has_surface(i)));
            for (_, q) in quads
                .iter()
                .enumerate()
                .filter(|(i, _)| ribbon.has_surface(*i))
            {
                for p in if edge == 1 {
                    [q[3], q[2]]
                } else {
                    [q[0], q[1]]
                } {
                    let offset = p - young;
                    assert!((offset.x * chord.y - offset.y * chord.x).abs() < 1e-3);
                    assert!((0.0..=1.001).contains(&(offset.dot(chord) / chord.length_squared())));
                }
            }
        }
    }
}

#[test]
fn converging_outlet_faces_do_not_extrude_a_long_underside_spike() {
    let tail = Section {
        position: Vec2::ZERO,
        velocity: Vec2::new(200.0, -10.0),
        flow: 40_000.0,
        material_offset: None,
    };
    let spill = Spill {
        sheet: None,
        source: SpillSource::Outlet { pool: 0, edge: 1 },
        tick: 1,
        upstream_end: None,
        tail,
        head: Section {
            position: Vec2::new(5.0, -1.0),
            velocity: Vec2::new(200.0, -20.0),
            ..tail
        },
    };
    let parcel = Parcel {
        position: Vec2::new(2.5, -0.5),
        velocity: tail.velocity,
        volume: 1400.0,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    };
    // The source is about 200 units deep. Its almost coincident lower corners
    // must not turn the extra area into a multi-thousand-unit needle.
    assert!(
        spill.ribbon(&parcel).is_none(),
        "use the compact, volume-preserving fallback for this collapsed strip"
    );
}

#[test]
fn compressed_junction_does_not_turn_into_a_wide_spike() {
    let tail = Section {
        position: Vec2::ZERO,
        velocity: Vec2::new(0.0, -60.0),
        flow: 120.0,
        material_offset: None,
    };
    let spill = Spill {
        sheet: None,
        upstream_end: None,
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
    if depth > 0.0 {
        water.add_to_pool(0, 0.0, depth * 40.0).unwrap();
    }
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
                    assert!(ribbon.quads().iter().all(convex));
                    let rendered = ribbon.quads().iter().map(area).sum::<f64>();
                    let volume = water.parcels[i].volume;
                    assert!(
                        (rendered - volume).abs() < volume * 2e-4,
                        "{hz} {gravity} {edge}: {rendered} vs {volume}"
                    );
                    let spill = water.spills[i].unwrap();
                    for section in [spill.tail, spill.head] {
                        assert!(has_boundary_edge(
                            &ribbon,
                            section.position - section.offset(),
                            section.position + section.offset()
                        ));
                    }
                    if i > 0 {
                        let older = water.spill_ribbon(i - 1).unwrap();
                        assert!(has_boundary_edge(
                            &older,
                            spill.head.position - spill.head.offset(),
                            spill.head.position + spill.head.offset()
                        ));
                    }
                }
                let newest = water.spill_ribbon(water.parcels.len() - 1).unwrap();
                let face = [newest.quads()[0][0], newest.quads()[0][3]];
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
                        .quads()
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
