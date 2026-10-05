//! Query-only envelopes. Surface differences can be thinner than the convex
//! hull builder's tolerance, including segments after conversion back to f32.
use super::*;

pub(super) fn polygon_shape(vertices: &[Vector]) -> SharedShape {
    // Never discard a sliver or reject unrelated patches because it cannot
    // become a convex collider. A capsule enclosing all vertices also encloses
    // their convex hull, so it remains conservative for actor clearance.
    let mut ends = (vertices[0], vertices[0]);
    let mut length_squared = 0.0_f64;
    for (i, a) in vertices.iter().enumerate() {
        for b in &vertices[i + 1..] {
            let dx = f64::from(b.x) - f64::from(a.x);
            let dy = f64::from(b.y) - f64::from(a.y);
            let distance = dx * dx + dy * dy;
            if distance > length_squared {
                length_squared = distance;
                ends = (*a, *b);
            }
        }
    }
    let dx = f64::from(ends.1.x) - f64::from(ends.0.x);
    let dy = f64::from(ends.1.y) - f64::from(ends.0.y);
    let radius = vertices
        .iter()
        .map(|p| {
            let px = f64::from(p.x) - f64::from(ends.0.x);
            let py = f64::from(p.y) - f64::from(ends.0.y);
            let t = if length_squared > 0.0 {
                ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ((px - t * dx).powi(2) + (py - t * dy).powi(2)).sqrt()
        })
        .fold(0.0_f64, f64::max);
    // Round outward, allowing for f32 projection/support-map arithmetic. This
    // affects only a clearance query, never terrain mass or collider geometry.
    let padding = f64::from(f32::EPSILON) * length_squared.sqrt().max(1.0) * 8.0;
    // The hull implementation can panic on an entirely collapsed point cloud.
    // Only send it polygons with a resolvable thickness.
    if radius > padding
        && let Some(shape) = SharedShape::convex_hull(vertices)
    {
        return shape;
    }
    SharedShape::capsule(ends.0, ends.1, (radius + padding) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obstacle_reports_match_clearance_and_keep_all_current_solid_identities() {
        let mut world = PhysicsWorld::new(Default::default());
        for id in [9, 2, 4] {
            let id = PhysicsId::new(id);
            let mut collider =
                ColliderSpec::ball(ColliderId::new(id, ColliderRole::PRIMARY, 0), 0.2);
            collider.sensor = id.value() == 4;
            assert!(world.insert_body(
                BodyId::new(id, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    ..Default::default()
                },
                &[collider]
            ));
        }
        let patches = vec![vec![
            Vec2::new(-0.5, -0.5),
            Vec2::new(0.5, -0.5),
            Vec2::new(0.0, 0.5),
        ]];
        let before = world.snapshot_bytes().unwrap();
        let report =
            world.current_polygon_obstacles(Vec2::ZERO, 0.0, &patches, CollisionGroups::ALL, &[]);
        assert!(!report[0].0);
        assert_eq!(
            report[0]
                .1
                .iter()
                .map(|id| id.entity.value())
                .collect::<Vec<_>>(),
            [2, 9]
        );
        assert_eq!(
            world.current_polygons_clearance(Vec2::ZERO, 0.0, &patches, CollisionGroups::ALL, &[]),
            [false]
        );
        let clear = world.current_polygon_obstacles(
            Vec2::ZERO,
            0.0,
            &patches,
            CollisionGroups::ALL,
            &[PhysicsId::new(2), PhysicsId::new(9)],
        );
        assert_eq!(clear, [(true, Vec::new())]);
        assert_eq!(world.snapshot_bytes().unwrap(), before);
    }

    #[test]
    fn slivers_are_clear_when_empty_and_still_block_growth_through_actors() {
        // Captured from the three-planet seed-42 run; these fail convex_hull.
        let patches = vec![
            vec![
                Vec2::new(93.0, 21.544825),
                Vec2::new(93.0, 21.544825),
                Vec2::new(92.73768, 21.737677),
            ],
            vec![
                Vec2::new(117.651085, -83.651085),
                Vec2::new(117.5, -83.86386),
                Vec2::new(117.5, -83.86387),
            ],
            vec![
                Vec2::new(-42.0, -87.009995),
                Vec2::new(-42.000004, -87.009964),
                Vec2::new(-42.000523, -87.01509),
            ],
            vec![Vec2::new(10.0, 20.0); 3],
        ];
        let mut world = PhysicsWorld::new(Default::default());
        let position = Vec2::new(200.0, -300.0);
        let angle = 0.73;
        assert_eq!(
            world.current_polygons_clearance(position, angle, &patches, CollisionGroups::ALL, &[]),
            vec![true; patches.len()]
        );
        for (i, patch) in patches.iter().enumerate() {
            let entity = PhysicsId::new(i as u64 + 1);
            let point = patch[0];
            let body = BodyId::new(entity, BodyRole::PRIMARY);
            world.insert_body(
                body,
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: position + point.rotate_radians(angle),
                    ..Default::default()
                },
                &[ColliderSpec::ball(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    0.001,
                )],
            );
            // No solve/broadphase refresh: growth must see this insertion now.
            let clear = world.current_polygons_clearance(
                position,
                angle,
                &patches,
                CollisionGroups::ALL,
                &[],
            );
            assert!(!clear[i], "sliver {i} ignored its obstacle");
            assert!(clear.iter().enumerate().all(|(j, &clear)| j == i || clear));
            world.remove_entity(entity);
        }
    }
}
