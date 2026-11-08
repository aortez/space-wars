//! Read-only nearest-boundary queries against the last completed step's index.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointProjectionOptions {
    pub max_distance: f32,
    pub include_sensors: bool,
    pub collision_groups: CollisionGroups,
    /// When set, only this entity's colliders are considered.
    pub entity: Option<PhysicsId>,
}

impl Default for PointProjectionOptions {
    fn default() -> Self {
        Self {
            max_distance: f32::MAX,
            include_sensors: false,
            collision_groups: CollisionGroups::ALL,
            entity: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointProjectionHit {
    pub collider: ColliderId,
    /// Nearest point on the collider's boundary, in world coordinates.
    pub point: Vec2,
    /// Distance from the query point to that boundary point.
    pub distance: f32,
    /// The query point lies inside the collider.
    pub inside: bool,
}

impl PhysicsWorld {
    /// Find the nearest collider boundary within `max_distance`. The world is
    /// unchanged. Invalid inputs and misses return None. A point inside a
    /// collider projects onto that collider's boundary.
    pub fn project_point(
        &self,
        point: Vec2,
        options: PointProjectionOptions,
    ) -> Option<PointProjectionHit> {
        if !finite_vec2(point) || !options.max_distance.is_finite() || options.max_distance < 0.0 {
            return None;
        }
        let entity = options.entity;
        let predicate = |_: ColliderHandle, collider: &Collider| {
            entity.is_none_or(|entity| {
                decode_collider(collider.user_data).is_some_and(|id| id.entity == entity)
            })
        };
        let (handle, projection) = self.raw.project_point(
            to_rapier(point),
            options.max_distance,
            false,
            QueryFilter {
                flags: if options.include_sensors {
                    QueryFilterFlags::empty()
                } else {
                    QueryFilterFlags::EXCLUDE_SENSORS
                },
                groups: Some(options.collision_groups.to_rapier()),
                predicate: Some(&predicate),
                ..Default::default()
            },
        )?;
        let nearest = from_rapier(projection.point);
        Some(PointProjectionHit {
            collider: decode_collider(self.raw.colliders.get(handle)?.user_data)?,
            point: nearest,
            distance: nearest.distance_to(point),
            inside: projection.is_inside,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(world: &mut PhysicsWorld, value: u64, position: Vec2, sensor: bool) -> ColliderId {
        let entity = PhysicsId::new(value);
        let id = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        assert!(world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                ..Default::default()
            },
            &[ColliderSpec {
                sensor,
                ..ColliderSpec::cuboid(id, 1.0, 1.0)
            }],
        ));
        id
    }

    #[test]
    fn projection_finds_nearest_boundary_and_is_read_only() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let near = insert(&mut world, 1, Vec2::ZERO, false);
        world.step(1.0 / 60.0);
        let before = world.snapshot_bytes().unwrap();
        let options = PointProjectionOptions::default();
        let outside = world.project_point(Vec2::new(3.0, 0.5), options).unwrap();
        assert_eq!(outside.collider, near);
        assert!(!outside.inside);
        assert!(outside.point.distance_to(Vec2::new(1.0, 0.5)) < 0.001);
        assert!((outside.distance - 2.0).abs() < 0.001, "{outside:?}");
        let inside = world.project_point(Vec2::new(0.6, 0.0), options).unwrap();
        assert!(inside.inside);
        assert!(inside.point.distance_to(Vec2::new(1.0, 0.0)) < 0.001);
        assert!((inside.distance - 0.4).abs() < 0.001, "{inside:?}");
        assert_eq!(world.snapshot_bytes().unwrap(), before);
    }

    #[test]
    fn projection_respects_entity_sensors_and_distance() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let near = insert(&mut world, 1, Vec2::ZERO, false);
        let sensor = insert(&mut world, 2, Vec2::new(5.0, 0.0), true);
        let far = insert(&mut world, 3, Vec2::new(9.0, 0.0), false);
        world.step(1.0 / 60.0);
        let options = PointProjectionOptions::default();
        let project = |o| world.project_point(Vec2::new(6.5, 0.0), o);
        assert_eq!(project(options).unwrap().collider, far);
        assert_eq!(
            project(PointProjectionOptions {
                include_sensors: true,
                ..options
            })
            .unwrap()
            .collider,
            sensor
        );
        let only = project(PointProjectionOptions {
            entity: Some(near.entity),
            ..options
        })
        .unwrap();
        assert_eq!(only.collider, near);
        assert!((only.distance - 5.5).abs() < 0.001, "{only:?}");
        assert!(
            project(PointProjectionOptions {
                max_distance: 1.4,
                ..options
            })
            .is_none()
        );
        for max_distance in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(
                project(PointProjectionOptions {
                    max_distance,
                    ..options
                })
                .is_none()
            );
        }
        assert!(
            world
                .project_point(Vec2::new(f32::NAN, 0.0), options)
                .is_none()
        );
    }
}
