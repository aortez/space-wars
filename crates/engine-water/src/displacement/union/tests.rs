use super::*;
use crate::{
    Boundary, WaterConfig, WaterWorld,
    displacement::{DisplacementBox, MAX_DISPLACERS},
    immersion::HullShape,
};
use engine_core::Vec2;

fn tank(columns: usize) -> WaterWorld {
    super::super::tests::tank(columns)
}

fn square(x: f32, y: f32, half: f32, angle: f32) -> DisplacementBody {
    DisplacementBox {
        center: Vec2::new(x, y),
        half_extents: Vec2::new(half, half),
        angle,
    }
    .into()
}

fn assert_area(bodies: &[DisplacementBody], expected: f64) {
    for columns in [32, 128, 512] {
        let mut water = tank(columns);
        water.set_displacers(0, bodies).unwrap();
        let area = water.stats().displaced;
        assert!(
            (area - expected).abs() < 1e-6,
            "area={area} expected={expected} bodies={bodies:?}"
        );
        let mut reversed = bodies.to_vec();
        reversed.reverse();
        let before = water.pools()[0].displaced.clone();
        water.set_displacers(0, &reversed).unwrap();
        for (a, b) in before.iter().zip(&water.pools()[0].displaced) {
            assert!((a - b).abs() < 1e-8, "order-dependent columns: {a} {b}");
        }
    }
}

#[test]
fn union_counts_disjoint_touching_nested_duplicate_and_triple_overlap_once() {
    let a = square(0.0, 10.0, 5.0, 0.0);
    assert_area(&[a, a], 100.0);
    assert_area(&[a, a, a], 100.0);
    assert_area(&[a, square(0.0, 10.0, 2.0, 0.4)], 100.0);
    assert_area(&[a, square(15.0, 10.0, 5.0, 0.0)], 200.0);
    assert_area(&[a, square(10.0, 10.0, 5.0, 0.0)], 200.0); // vertical seam
    assert_area(
        &[square(0.0, 5.0, 5.0, 0.0), square(0.0, 15.0, 5.0, 0.0)],
        200.0,
    ); // horizontal seam
    assert_area(&[a, square(5.0, 10.0, 5.0, 0.0)], 150.0);
    assert_area(
        &[a, square(5.0, 10.0, 5.0, 0.0), square(2.0, 10.0, 5.0, 0.0)],
        150.0,
    );
    assert_area(
        &[a, square(0.0, 10.0, 5.0, std::f32::consts::FRAC_PI_4)],
        100.0 * (4.0 - 2.0 * 2.0_f64.sqrt()),
    );
    // Distinct rotations yield the same outline; shared boundary ownership is
    // geometric, not a hash/deduplication special case.
    assert_area(
        &[a, square(0.0, 10.0, 5.0, std::f32::consts::PI)],
        100.0 + 50.0 * (std::f32::consts::PI as f64 - std::f64::consts::PI).abs(),
    );
}

#[test]
fn general_path_matches_single_box_clipping_and_partial_reference_level() {
    for (x, y, angle) in [
        (0.0, 23.0, 0.0),
        (0.0, 20.5, 0.65),
        (50.0, 20.0, 0.5),
        (50.0, 0.0, 0.7),
        (80.0, 0.0, 0.7),
    ] {
        let mut single = tank(128);
        let mut multi = tank(128);
        let body = square(x, y, 5.0, angle);
        single.set_displacers(0, &[body]).unwrap();
        multi
            .set_displacers(0, &[body, square(-30.0, 100.0, 1.0, 0.0)])
            .unwrap();
        for (a, b) in single.pools()[0]
            .displaced
            .iter()
            .zip(&multi.pools()[0].displaced)
        {
            assert!((a - b).abs() < 1e-7, "x={x} y={y} angle={angle}: {a} {b}");
        }
    }
    let mut water = tank(128);
    // Two partly submerged boxes with a 5-wide overlap: union width=15,
    // bottom=18. Free capacity 100*h - 15*(h-18) = 2000.
    let bodies = [square(0.0, 23.0, 5.0, 0.0), square(5.0, 23.0, 5.0, 0.0)];
    water.set_displacers(0, &bodies).unwrap();
    let level = (2000.0 - 15.0 * 18.0) / 85.0;
    assert!((water.stats().displaced - (level - 18.0) * 15.0).abs() < 1e-8);
    super::super::tests::settle(&mut water, level);
}

#[test]
fn circle_union_is_bounded_unweighted_and_orientation_independent() {
    let circle = DisplacementBody {
        center: Vec2::new(0.0, 10.0),
        angle: 0.0,
        shape: HullShape::Circle { radius: 5.0 },
    };
    let polygon_area = 16.0 * 25.0 * (std::f64::consts::TAU / 32.0).sin();
    assert_area(&[circle], polygon_area);
    assert_area(
        &[
            circle,
            DisplacementBody {
                angle: 1.2,
                ..circle
            },
        ],
        polygon_area,
    );
    assert_area(&[circle, square(0.0, 10.0, 2.0, 0.0)], polygon_area);
    assert_area(&[circle, square(0.0, 10.0, 5.0, 0.0)], 100.0);
    assert_area(
        &[DisplacementBody {
            center: Vec2::new(50.0, 0.0),
            ..circle
        }],
        polygon_area / 4.0,
    );
    let mut water = tank(128);
    water
        .set_displacers(
            0,
            &[
                circle,
                DisplacementBody {
                    center: Vec2::new(5.0, 10.0),
                    ..circle
                },
            ],
        )
        .unwrap();
    let true_union = 50.0 * std::f64::consts::PI - (50.0 * 0.5_f64.acos() - 2.5 * 75.0_f64.sqrt());
    assert!(water.stats().displaced <= true_union);
    assert!(true_union - water.stats().displaced < 2.0 * std::f64::consts::PI * 25.0 * 0.0065);
    assert!(water.sample(Vec2::new(4.9, 10.0)).is_none());
    assert!(water.sample(Vec2::new(12.0, 10.0)).is_some());
}

#[test]
fn union_preserves_an_unoccupied_hole_in_a_ring_of_overlapping_boxes() {
    let rectangle = |x, y, hx, hy| {
        DisplacementBox {
            center: Vec2::new(x, y),
            half_extents: Vec2::new(hx, hy),
            angle: 0.0,
        }
        .into()
    };
    let bodies = [
        rectangle(0.0, 6.0, 5.0, 1.0),
        rectangle(0.0, 14.0, 5.0, 1.0),
        rectangle(-4.0, 10.0, 1.0, 5.0),
        rectangle(4.0, 10.0, 1.0, 5.0),
    ];
    assert_area(&bodies, 100.0 - 36.0);
    let mut water = tank(128);
    water.set_displacers(0, &bodies).unwrap();
    assert!(water.sample(Vec2::new(0.0, 10.0)).is_some());
    assert!(water.sample(Vec2::new(4.0, 10.0)).is_none());
    // Only an occupancy hole: this is not a sealed vessel or trapped-air model.
}

#[test]
fn replacement_removal_invalid_batch_and_reclaim_are_atomic() {
    let mut water = tank(32);
    let initial = [square(0.0, 10.0, 5.0, 0.0), square(15.0, 10.0, 5.0, 0.0)];
    water.set_displacers(0, &initial).unwrap();
    let before = water.pools.clone();
    for invalid in [
        vec![
            initial[0],
            DisplacementBody {
                angle: f32::NAN,
                ..initial[1]
            },
        ],
        vec![
            initial[0],
            DisplacementBody {
                shape: HullShape::Circle {
                    radius: f32::INFINITY,
                },
                ..initial[1]
            },
        ],
        vec![square(0.0, 0.0, 1.0, 0.0); MAX_DISPLACERS + 1],
        vec![square(0.0, 0.0, 20.0, 0.0); 2], // individually valid, collectively too wide
    ] {
        assert!(water.set_displacers(0, &invalid).is_err());
        assert_eq!(water.pools, before);
    }
    water.set_displacers(0, &initial[..1]).unwrap();
    assert_eq!(water.stats().displaced, 100.0);
    assert!(water.sample(Vec2::new(15.0, 10.0)).is_some());
    water.set_displacers(0, &initial).unwrap();
    water.reclaim_fraction(0.5).unwrap();
    assert!(water.stats().displaced < 200.0);
    water.reclaim();
    assert_eq!(water.stats().displaced, 0.0);
    assert!(water.pools()[0].columns().all(|c| c.surface == c.bed));
    water.set_displacers(0, &[]).unwrap();
    assert_eq!(water.stats().injected, 2000.0);
    assert_eq!(water.stats().reclaimed, 2000.0);
}

#[test]
fn moving_mixed_bodies_replay_conserve_and_reuse_bounded_scratch() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        let mut a = tank(128);
        let mut b = tank(128);
        let mut storage = None;
        for tick in 0..(8.0 / dt) as usize {
            let phase = tick as f32 * dt as f32;
            let mut bodies = [square(0.0, 0.0, 1.0, 0.0); MAX_DISPLACERS];
            for (i, body) in bodies.iter_mut().enumerate() {
                *body = DisplacementBody {
                    center: Vec2::new(
                        (phase + i as f32).sin() * 12.0,
                        20.0 + (phase * 2.0 + i as f32).cos() * 14.0,
                    ),
                    angle: phase * 2.0,
                    shape: if i % 2 == 0 {
                        HullShape::Box {
                            half_width: 2.0,
                            half_height: 1.0,
                        }
                    } else {
                        HullShape::Circle { radius: 1.5 }
                    },
                };
            }
            // Removal/recreation exercises complete snapshots, including empty.
            let len = if tick % 80 < 10 { 0 } else { MAX_DISPLACERS };
            for water in [&mut a, &mut b] {
                water.set_displacers(0, &bodies[..len]).unwrap();
                water.step(dt).unwrap();
                super::super::tests::conserved(water);
            }
            assert_eq!(a.pools, b.pools);
            if len > 0 {
                let state = &a.pools[0].displacement;
                let pointers = (
                    state.bodies.as_ptr(),
                    state.outline.edges.as_ptr(),
                    state.outline.edges.capacity(),
                );
                assert_eq!(*storage.get_or_insert(pointers), pointers);
                assert!(state.outline.edges.len() <= MAX_SEGMENTS);
            }
        }
    }
}

#[test]
fn translated_union_preserves_area_and_per_column_occupancy() {
    let bodies = [square(-3.0, 10.0, 5.0, 0.7), square(3.0, 10.0, 5.0, -0.2)];
    let mut reference = tank(128);
    reference.set_displacers(0, &bodies).unwrap();
    for offset in [100_000.0, -100_000.0] {
        let mut translated = WaterWorld::new(
            WaterConfig {
                exit_y: offset - 240.0,
                ..WaterConfig::default()
            },
            vec![PoolSpec {
                left: -50.0 + offset,
                column_width: 100.0 / 128.0,
                bed: vec![offset; 128],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        for i in 0..128 {
            translated
                .add_to_pool(
                    0,
                    offset - 50.0 + (i as f64 + 0.5) * 100.0 / 128.0,
                    2000.0 / 128.0,
                )
                .unwrap();
        }
        let shifted = bodies.map(|b| DisplacementBody {
            center: b.center + Vec2::new(offset as f32, offset as f32),
            ..b
        });
        translated.set_displacers(0, &shifted).unwrap();
        for (a, b) in reference.pools()[0]
            .displaced
            .iter()
            .zip(&translated.pools()[0].displaced)
        {
            assert!((a - b).abs() < 1e-8, "{a} {b}");
        }
    }
}

/// Independent test oracle: intersect convex polygons and use inclusion/
/// exclusion for four bodies only. Deliberately NOT the production algorithm.
fn intersection(subject: &[Point], clipping: &[Point]) -> Vec<Point> {
    let mut result = subject.to_vec();
    for i in 0..clipping.len() {
        if result.is_empty() {
            break;
        }
        let a = clipping[i];
        let direction = sub(clipping[(i + 1) % clipping.len()], a);
        let input = std::mem::take(&mut result);
        let mut previous = *input.last().unwrap();
        let mut prev_side = cross(direction, sub(previous, a));
        for current in input {
            let side = cross(direction, sub(current, a));
            if (side >= 0.0) != (prev_side >= 0.0) {
                let t = prev_side / (prev_side - side);
                result.push([
                    previous[0] + t * (current[0] - previous[0]),
                    previous[1] + t * (current[1] - previous[1]),
                ]);
            }
            if side >= 0.0 {
                result.push(current);
            }
            previous = current;
            prev_side = side;
        }
    }
    if clipping.len() < 3 {
        result.clear();
    }
    result
}

#[test]
fn seeded_mixed_overlaps_match_an_independent_intersection_oracle_per_column() {
    let mut seed = 234567_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f32 / u32::MAX as f32
    };
    for case in 0..80 {
        let mut water = tank(32);
        let bodies: [_; 4] = std::array::from_fn(|i| DisplacementBody {
            center: Vec2::new(
                if case % 3 == 0 { 47.0 } else { 0.0 } + (random() - 0.5) * 12.0,
                if case % 4 == 0 { 1.0 } else { 20.0 } + (random() - 0.5) * 10.0,
            ),
            angle: random() * 6.0,
            shape: if i % 2 == 0 {
                HullShape::Circle {
                    radius: 2.0 + random() * 2.0,
                }
            } else {
                HullShape::Box {
                    half_width: 2.0 + random() * 3.0,
                    half_height: 2.0 + random() * 3.0,
                }
            },
        });
        water.set_displacers(0, &bodies).unwrap();
        let spec = water.pools()[0].spec();
        let polygons = bodies.map(|b| Polygon::new(b, [spec.left, 0.0], 100.0));
        let height = 20.0 + water.stats().displaced / 100.0;
        for (column, actual) in water.pools()[0].displaced.iter().enumerate() {
            let left = column as f64 * spec.column_width;
            let right = left + spec.column_width;
            let mut expected = 0.0;
            for mask in 1_u32..16 {
                let mut clipped = vec![[left, 0.0], [right, 0.0], [right, height], [left, height]];
                for (i, p) in polygons.iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        clipped = intersection(&clipped, &p.points[..p.len]);
                    }
                }
                let area = super::super::polygon_area(&clipped);
                expected += if mask.count_ones() % 2 == 1 {
                    area
                } else {
                    -area
                };
            }
            assert!(
                (expected - actual).abs() < 1e-7,
                "case={case} column={column} actual={actual} expected={expected} bodies={bodies:?}"
            );
        }
    }
}
