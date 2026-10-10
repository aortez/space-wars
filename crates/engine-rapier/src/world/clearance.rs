//! Query-only envelopes. Surface differences can be thinner than the convex
//! hull builder's tolerance, including segments after conversion back to f32.
use super::*;

/// All blockers for one immutable query shape. Unknown identities and invalid
/// geometry always block; exclusions may remove only identified entities.
#[derive(Clone, Default)]
pub(crate) struct SolidObstacles {
    ids: Vec<ColliderId>,
    unknown: bool,
}

impl SolidObstacles {
    pub(crate) fn is_clear_excluding(&self, excluded: &[PhysicsId]) -> bool {
        !self.unknown && self.ids.iter().all(|id| excluded.contains(&id.entity))
    }

    pub(super) fn record(&mut self, id: Option<ColliderId>) {
        if let Some(id) = id {
            self.ids.push(id);
        } else {
            self.unknown = true;
        }
    }

    pub(super) fn into_ids(mut self) -> Vec<ColliderId> {
        self.ids.sort();
        self.ids.dedup();
        self.ids
    }
}

/// Exact current collision inputs. Generational handles identify immutable
/// shapes, including replacements with the same semantic ID or bounds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SolidDependency {
    handle: ColliderHandle,
    id: ColliderId,
    pose: Pose,
    groups: CollisionGroups,
}

impl PhysicsWorld {
    /// Batch identical-size cuboids against current poses, visiting the world
    /// once. Keep every blocker so an immutable caller can vary its exclusions
    /// without repeating collision queries. No completed-step index is used.
    pub(crate) fn current_cuboids_obstacles(
        &self,
        poses: &[(Vec2, f32)],
        half_extents: Vec2,
        groups: CollisionGroups,
        excluded: &[PhysicsId],
    ) -> Vec<SolidObstacles> {
        if !finite_vec2(half_extents)
            || half_extents.x <= 0.0
            || half_extents.y <= 0.0
            || poses
                .iter()
                .any(|(p, a)| !finite_vec2(*p) || !a.is_finite())
        {
            return vec![
                SolidObstacles {
                    ids: Vec::new(),
                    unknown: true
                };
                poses.len()
            ];
        }
        let shape = SharedShape::cuboid(half_extents.x, half_extents.y);
        let shapes: Vec<_> = poses
            .iter()
            .map(|&(position, angle)| (Pose::new(to_rapier(position), angle), shape.clone()))
            .collect();
        let mut obstacles = vec![SolidObstacles::default(); poses.len()];
        self.current_shapes_clearance(&shapes, groups, excluded, false, Some(&mut obstacles));
        obstacles
    }

    /// Compact dependencies for memoizing a failed local clearance operation.
    /// Read parent poses directly so unflushed edits are included. Any motion
    /// invalidates this evidence; there is no tolerance or accepted-placement
    /// cache. Large regions simply opt out of memoization.
    pub(crate) fn current_solid_dependencies(
        &self,
        position: Vec2,
        angle: f32,
        area: QueryArea,
    ) -> Option<Vec<SolidDependency>> {
        if !finite_vec2(position)
            || !angle.is_finite()
            || !finite_vec2(area.minimum)
            || !finite_vec2(area.maximum)
            || area.minimum.x >= area.maximum.x
            || area.minimum.y >= area.maximum.y
        {
            return None;
        }
        let center = (area.minimum + area.maximum) * 0.5;
        let half = (area.maximum - area.minimum) * 0.5;
        let envelope = SharedShape::cuboid(half.x, half.y).compute_aabb(&Pose::new(
            to_rapier(position + center.rotate_radians(angle)),
            angle,
        ));
        let mut result = Vec::new();
        for (handle, collider) in self.raw.colliders.iter() {
            if !collider.is_enabled()
                || collider.is_sensor()
                || !collider.collision_groups().test(area.groups.to_rapier())
            {
                continue;
            }
            let pose = collider
                .parent()
                .and_then(|handle| self.raw.bodies.get(handle))
                .map_or(*collider.position(), |body| {
                    *body.position() * collider.position_wrt_parent().copied().unwrap_or_default()
                });
            let bounds = collider.shape().compute_aabb(&pose);
            if bounds.mins.x > envelope.maxs.x
                || bounds.maxs.x < envelope.mins.x
                || bounds.mins.y > envelope.maxs.y
                || bounds.maxs.y < envelope.mins.y
            {
                continue;
            }
            if result.len() == 256 {
                return None;
            }
            let groups = collider.collision_groups();
            result.push(SolidDependency {
                handle,
                id: decode_collider(collider.user_data)?,
                pose,
                groups: CollisionGroups::new(groups.memberships.bits(), groups.filter.bits()),
            });
        }
        result.sort_by_key(|d| d.id);
        Some(result)
    }
}

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
    fn batched_cuboids_match_live_clearance_for_every_excluded_subset() {
        let mut world = PhysicsWorld::new(Default::default());
        for value in 1..=5 {
            let id = PhysicsId::new(value);
            let mut spec =
                ColliderSpec::cuboid(ColliderId::new(id, ColliderRole::PRIMARY, 0), 0.8, 0.3);
            spec.local_position = Vec2::new(0.2, -0.1);
            spec.local_angle = 0.3;
            spec.sensor = value == 3;
            spec.collision_groups = CollisionGroups::new(1, 2);
            if value == 4 {
                spec.collision_groups = CollisionGroups::new(4, 4);
            }
            assert!(world.insert_body(
                BodyId::new(id, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: Vec2::new((value % 3) as f32 - 1.0, (value / 3) as f32),
                    angle: value as f32 * 0.4,
                    ..Default::default()
                },
                &[spec],
            ));
        }
        let disabled = ColliderId::new(PhysicsId::new(5), ColliderRole::PRIMARY, 0);
        let handle = world.colliders[*world.collider_indices.get(&disabled).unwrap()].handle;
        world.raw.colliders[handle].set_enabled(false);
        world.step(1.0 / 60.0);
        let poses: Vec<_> = (-4..=4)
            .flat_map(|y| {
                (-4..=4).flat_map(move |x| {
                    [0.0, 0.4, -1.2].map(|a| (Vec2::new(x as f32 * 0.5, y as f32 * 0.5), a))
                })
            })
            .collect();
        let half = Vec2::new(0.4, 0.6);
        for changed in [false, true] {
            if changed {
                // Both changes deliberately precede any world step.
                let id = PhysicsId::new(2);
                assert!(world.set_pose(
                    BodyId::new(id, BodyRole::PRIMARY),
                    Vec2::new(-1.8, 0.2),
                    0.9,
                    true,
                ));
                let id = PhysicsId::new(1);
                assert!(world.replace_colliders(
                    BodyId::new(id, BodyRole::PRIMARY),
                    ColliderRole::PRIMARY,
                    &[ColliderSpec::ball(
                        ColliderId::new(id, ColliderRole::PRIMARY, 0),
                        0.7
                    )],
                ));
            }
            let before = world.snapshot_bytes().unwrap();
            for groups in [
                CollisionGroups::ALL,
                CollisionGroups::new(2, 1),
                CollisionGroups::NONE,
            ] {
                let obstacles = world.current_cuboids_obstacles(&poses, half, groups, &[]);
                for mask in 0..32 {
                    let excluded: Vec<_> = (1..=5)
                        .filter(|i| mask & (1 << (i - 1)) != 0)
                        .map(PhysicsId::new)
                        .collect();
                    for (&(position, angle), obstacles) in poses.iter().zip(&obstacles) {
                        assert_eq!(
                            obstacles.is_clear_excluding(&excluded),
                            world.current_cuboid_is_clear(position, angle, half, groups, &excluded),
                            "changed={changed} pose={position:?}/{angle} excluded={excluded:?}",
                        );
                    }
                }
            }
            assert_eq!(world.snapshot_bytes().unwrap(), before);
        }
    }

    #[test]
    fn batched_cuboids_keep_unknown_blockers_and_reject_invalid_geometry() {
        let mut world = PhysicsWorld::new(Default::default());
        let id = PhysicsId::new(1);
        assert!(world.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[ColliderSpec::ball(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                0.5
            )],
        ));
        let half = Vec2::new(0.4, 0.4);
        let poses = [(Vec2::ZERO, 0.0), (Vec2::X * 10.0, 0.0)];
        let capture = |world: &PhysicsWorld| {
            world.current_cuboids_obstacles(&poses, half, CollisionGroups::ALL, &[])
        };
        assert!(!capture(&world)[0].is_clear_excluding(&[]));
        assert!(capture(&world)[0].is_clear_excluding(&[id]));
        // Unregistered raw geometry has no semantic ID and cannot be excluded.
        world
            .raw
            .colliders
            .insert(ColliderBuilder::ball(0.3).build());
        assert!(!capture(&world)[0].is_clear_excluding(&[id]));
        assert!(capture(&world)[1].is_clear_excluding(&[]));
        for (pose, half) in [
            ((Vec2::new(f32::NAN, 0.0), 0.0), half),
            ((Vec2::ZERO, f32::INFINITY), half),
            ((Vec2::ZERO, 0.0), Vec2::new(-0.5, 0.5)),
            ((Vec2::ZERO, 0.0), Vec2::new(0.5, f32::NAN)),
        ] {
            assert!(
                !world.current_cuboids_obstacles(&[pose], half, CollisionGroups::ALL, &[id])[0]
                    .is_clear_excluding(&[id])
            );
        }
    }

    #[test]
    fn local_dependencies_detect_current_motion_and_shape_replacement() {
        let mut world = PhysicsWorld::new(Default::default());
        let near = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
        let far = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);
        let collider = ColliderId::new(near.entity, ColliderRole::PRIMARY, 0);
        for (body, position) in [(near, Vec2::ZERO), (far, Vec2::X * 20.0)] {
            assert!(world.insert_body(
                body,
                BodySpec {
                    kind: BodyKind::Fixed,
                    position,
                    ..Default::default()
                },
                &[ColliderSpec::ball(
                    ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
                    1.0
                )],
            ));
        }
        let capture = |world: &PhysicsWorld| {
            world
                .current_solid_dependencies(
                    Vec2::ZERO,
                    0.0,
                    QueryArea {
                        minimum: Vec2::new(-2.0, -2.0),
                        maximum: Vec2::new(2.0, 2.0),
                        groups: CollisionGroups::ALL,
                    },
                )
                .unwrap()
        };
        let before = world.snapshot_bytes().unwrap();
        let original = capture(&world);
        assert_eq!(original.len(), 1);
        assert_eq!(world.snapshot_bytes().unwrap(), before);
        assert_eq!(original, capture(&world.clone()));
        world.set_pose(far, Vec2::X * 10.0, 0.0, true);
        assert_eq!(original, capture(&world));
        world.set_pose(far, Vec2::X, 0.0, true);
        assert_ne!(original, capture(&world));
        world.remove_entity(far.entity);
        assert_eq!(original, capture(&world));
        world.set_pose(near, Vec2::X * 0.00001, 0.0, true);
        assert_ne!(
            original,
            capture(&world),
            "even small unflushed motion invalidates"
        );
        world.set_pose(near, Vec2::ZERO, 0.0, true);
        assert_eq!(original, capture(&world));
        assert!(world.replace_colliders(
            near,
            ColliderRole::PRIMARY,
            &[ColliderSpec::cuboid(collider, 1.0, 1.0)]
        ));
        assert_ne!(
            original,
            capture(&world),
            "same ID and bounds are not shape identity"
        );
        let mut sensor = ColliderSpec::ball(collider, 1.0);
        sensor.sensor = true;
        assert!(world.replace_colliders(near, ColliderRole::PRIMARY, &[sensor]));
        assert!(capture(&world).is_empty());
    }

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
