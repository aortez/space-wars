use super::*;
use engine_common::{PolygonGeometry, PolygonGeometryHint, RenderPolygon};
use std::sync::Arc;

#[test]
fn prepared_camera_preserves_point_bits_and_error_boundaries() {
    // Include cancellation, overflow, subnormals, signed zero and non-finite
    // inputs, not just well-conditioned origin-centered camera examples.
    let mut values = vec![
        0.,
        -0.,
        1.,
        -1.,
        0.1,
        -0.1,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
        -f32::MAX,
        f32::NAN,
        f32::INFINITY,
    ];
    let mut seed = 93_u32;
    for _ in 0..256 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        values.push(f32::from_bits(seed));
    }
    for viewport in [
        Viewport::new(1024., 768.),
        Viewport::new(361., 641.),
        Viewport {
            x: 17.25,
            y: -0.5,
            width: 0.125,
            height: 8191.,
        },
    ] {
        for center in [
            RenderPoint::ZERO,
            RenderPoint::new(-12345.67, 321.125),
            RenderPoint::new(1.0e20, -1.0e20),
            RenderPoint::new(f32::MAX, f32::MAX),
        ] {
            for height in [44., 640., 0.001, f32::MIN_POSITIVE, f32::MAX] {
                let camera = Camera2::new(center, height);
                let reference = Projection::new(&camera, viewport).unwrap();
                let mut prepared = Projection::new(&camera, viewport).unwrap();
                prepared.prepared = Some(PreparedCamera::new(camera, viewport));
                for &x in &values {
                    let point = RenderPoint::new(x, -x * 0.37);
                    let bits =
                        |p: Result<RenderPoint, String>| p.map(|p| (p.x.to_bits(), p.y.to_bits()));
                    assert_eq!(
                        bits(reference.point(point)),
                        bits(prepared.point(point)),
                        "{camera:?} {viewport:?} {point:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn prepared_rotations_keep_exact_hint_validation() {
    let geometry = Arc::new(
        PolygonGeometry::new(vec![RenderPolygon::filled(
            vec![
                RenderPoint::new(-13.125, 0.01),
                RenderPoint::new(2.57, -4.),
                RenderPoint::new(0.001, 19.),
            ],
            RenderColor::RED,
        )])
        .unwrap(),
    );
    for angle in [
        0.,
        -0.,
        0.31,
        -2.98,
        std::f32::consts::TAU,
        1234567.,
        f32::MAX,
    ] {
        for position in [RenderPoint::ZERO, RenderPoint::new(321.5, -17.75)] {
            let mut world = geometry.polygons()[0].clone();
            for p in &mut world.points {
                let v =
                    Vec2::new(position.x, position.y) + Vec2::new(p.x, p.y).rotate_radians(angle);
                *p = RenderPoint::new(v.x, v.y);
            }
            let hint = PolygonGeometryHint {
                layer: 0,
                first: 0,
                geometry: geometry.clone(),
                position,
                angle,
            };
            for mode in [PlanMode::Scratch, PlanMode::Prepared] {
                assert!(hint_matches(
                    &hint,
                    &[RenderPrimitive::Polygon(world.clone())],
                    mode
                ));
                let mut changed = world.clone();
                changed.points[0].x = f32::from_bits(changed.points[0].x.to_bits() ^ 1);
                assert!(!hint_matches(
                    &hint,
                    &[RenderPrimitive::Polygon(changed)],
                    mode
                ));
                let mut changed = world.clone();
                changed.fill = Some(Fill::new(RenderColor::GREEN));
                assert!(!hint_matches(
                    &hint,
                    &[RenderPrimitive::Polygon(changed)],
                    mode
                ));
                let mut changed = world.clone();
                changed.points.pop();
                assert!(!hint_matches(
                    &hint,
                    &[RenderPrimitive::Polygon(changed)],
                    mode
                ));
                let mut invalid = hint.clone();
                invalid.angle = f32::NAN;
                assert!(!hint_matches(
                    &invalid,
                    &[RenderPrimitive::Polygon(world.clone())],
                    mode
                ));
            }
        }
    }
}
