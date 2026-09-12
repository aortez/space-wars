use super::*;
use crate::{Boundary, PoolSpec, WaterConfig};

fn pool(columns: usize) -> WaterWorld {
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: -100.0,
            column_width: 200.0 / columns as f64,
            bed: vec![-100.0; columns],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for i in 0..columns {
        water
            .add_to_pool(
                0,
                -100.0 + (i as f64 + 0.5) * 200.0 / columns as f64,
                20_000.0 / columns as f64,
            )
            .unwrap();
    }
    water
}

#[test]
fn dry_half_and_full_boxes_have_exact_area_centroid_and_moments() {
    for columns in [1, 7, 128, 512] {
        let water = pool(columns);
        let hull = WaterHull::new(HullShape::Box {
            half_width: 4.0,
            half_height: 2.0,
        })
        .unwrap();
        let dry = hull
            .measure(&water, Vec2::new(0.0, 5.0), 0.0, Vec2::new(0.0, 5.0))
            .unwrap();
        assert_eq!(dry.area, 0.0);
        let half = hull.measure(&water, Vec2::ZERO, 0.0, Vec2::ZERO).unwrap();
        assert!((half.area - 16.0).abs() < 1e-8);
        assert!(half.first_moment[0].abs() < 1e-8);
        assert!((half.first_moment[1] + 16.0).abs() < 1e-8);
        assert!((half.polar_moment - 16.0 * (16.0 / 3.0 + 4.0 / 3.0)).abs() < 1e-8);
        let full = hull
            .measure(&water, Vec2::new(0.0, -10.0), 0.8, Vec2::new(0.0, -10.0))
            .unwrap();
        assert!((full.area - 32.0).abs() < 1e-8);
        assert!(full.first_moment.iter().all(|v| v.abs() < 1e-8));
        assert!((full.polar_moment - 32.0 * 20.0 / 3.0).abs() < 1e-8);
    }
}

#[test]
fn circle_volume_and_symmetry_are_normalized_and_angle_independent() {
    let water = pool(128);
    let hull = WaterHull::new(HullShape::Circle { radius: 3.0 }).unwrap();
    let m = hull.measure(&water, Vec2::ZERO, 0.0, Vec2::ZERO).unwrap();
    assert!((m.area - hull.area() * 0.5).abs() < 1e-8);
    assert!(m.first_moment[0].abs() < 1e-8);
    assert!(m.first_moment[1] < 0.0);
    assert_eq!(
        m,
        hull.measure(&water, Vec2::ZERO, 1.7, Vec2::ZERO).unwrap()
    );
    let position = Vec2::new(0.0, -10.0);
    assert!(
        (hull.measure(&water, position, 0.0, position).unwrap().area - hull.area()).abs() < 1e-8
    );
}

#[test]
fn tilted_boxes_have_mirrored_restoring_levers_and_com_offsets_are_respected() {
    let water = pool(128);
    let hull = WaterHull::new(HullShape::Box {
        half_width: 6.0,
        half_height: 2.0,
    })
    .unwrap();
    let left = hull.measure(&water, Vec2::ZERO, 0.3, Vec2::ZERO).unwrap();
    let right = hull.measure(&water, Vec2::ZERO, -0.3, Vec2::ZERO).unwrap();
    assert!(left.first_moment[0] < 0.0);
    assert!((left.first_moment[0] + right.first_moment[0]).abs() < 1e-8);
    let offset = hull
        .measure(&water, Vec2::ZERO, 0.3, Vec2::new(2.0, 0.0))
        .unwrap();
    assert!((offset.first_moment[0] - (left.first_moment[0] - 2.0 * left.area)).abs() < 1e-8);
}

#[test]
fn flow_integrals_include_current_and_its_lever_about_the_center() {
    let mut water = pool(128);
    water.pools[0].velocity.fill(3.0);
    let hull = WaterHull::new(HullShape::Box {
        half_width: 4.0,
        half_height: 2.0,
    })
    .unwrap();
    let m = hull.measure(&water, Vec2::ZERO, 0.0, Vec2::ZERO).unwrap();
    assert!((m.flow[0] - 3.0 * m.area).abs() < 1e-8);
    assert_eq!(m.flow[1], 0.0);
    assert!((m.flow_torque + 3.0 * m.first_moment[1]).abs() < 1e-8);
}

#[test]
fn geometry_inputs_are_validated_and_water_is_not_mutated() {
    let water = pool(128);
    let before = water.stats();
    for radius in [0.0, -1.0, f32::NAN, f32::INFINITY, 1001.0] {
        assert!(WaterHull::new(HullShape::Circle { radius }).is_err());
    }
    let hull = WaterHull::new(HullShape::Circle { radius: 3.0 }).unwrap();
    assert!(
        hull.measure(&water, Vec2::ZERO, f32::NAN, Vec2::ZERO)
            .is_err()
    );
    for angle in 0..100 {
        hull.measure(
            &water,
            Vec2::new(angle as f32, -10.0),
            angle as f32,
            Vec2::ZERO,
        )
        .unwrap();
    }
    assert_eq!(before, water.stats());
}
