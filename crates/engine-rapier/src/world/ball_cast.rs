//! Read-only sphere sweeps, including the surface geometry at first contact.
use super::*;
use rapier2d::parry::{query::ShapeCastStatus, shape::Ball};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallCastOptions {
    pub max_distance: f32,
    pub include_sensors: bool,
    pub collision_groups: CollisionGroups,
    pub exclude_entity: Option<PhysicsId>,
}

impl Default for BallCastOptions {
    fn default() -> Self {
        Self {
            max_distance: f32::MAX,
            include_sensors: false,
            collision_groups: CollisionGroups::ALL,
            exclude_entity: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallCastStatus {
    Converged,
    Penetrating,
    OutOfIterations,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallCastHit {
    pub collider: ColliderId,
    /// Point on the struck collider, in world coordinates.
    pub point: Vec2,
    /// Outward normal of the struck collider, in world coordinates.
    pub normal: Vec2,
    /// Distance travelled by the ball's center along the normalized direction.
    pub distance: f32,
    /// Non-converged results must not be treated as exact contact predictions.
    pub status: BallCastStatus,
}

impl PhysicsWorld {
    /// Cast a virtual ball against the last completed step's query index.
    /// The world is unchanged. Invalid inputs and misses return None. Initial
    /// overlap stops the sweep at zero, even when moving out of the obstacle;
    /// its computed geometry is reported with Penetrating status.
    pub fn cast_ball(
        &self,
        origin: Vec2,
        radius: f32,
        direction: Vec2,
        options: BallCastOptions,
    ) -> Option<BallCastHit> {
        let length_squared = direction.length_squared();
        if !finite_vec2(origin)
            || !length_squared.is_finite()
            || length_squared <= f32::EPSILON
            || !radius.is_finite()
            || radius <= 0.0
            || !options.max_distance.is_finite()
            || options.max_distance < 0.0
        {
            return None;
        }
        let excluded = options.exclude_entity;
        let predicate = |_: ColliderHandle, collider: &Collider| {
            decode_collider(collider.user_data).is_none_or(|id| Some(id.entity) != excluded)
        };
        let (handle, hit) = self.raw.cast_shape(
            &Pose::new(to_rapier(origin), 0.0),
            to_rapier(direction.normalized()),
            &Ball::new(radius),
            rapier2d::parry::query::ShapeCastOptions {
                max_time_of_impact: options.max_distance,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
                ..Default::default()
            },
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
        // Rapier's composite query transforms witness1 and normal1 from the
        // struck collider (including compound children) into world space.
        Some(BallCastHit {
            collider: decode_collider(self.raw.colliders.get(handle)?.user_data)?,
            point: from_rapier(hit.witness1),
            normal: from_rapier(hit.normal1),
            distance: hit.time_of_impact,
            status: match hit.status {
                ShapeCastStatus::Converged => BallCastStatus::Converged,
                ShapeCastStatus::PenetratingOrWithinTargetDist => BallCastStatus::Penetrating,
                ShapeCastStatus::OutOfIterations => BallCastStatus::OutOfIterations,
                ShapeCastStatus::Failed => BallCastStatus::Failed,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(
        world: &mut PhysicsWorld,
        value: u64,
        position: Vec2,
        angle: f32,
        sensor: bool,
        groups: CollisionGroups,
        compound: bool,
    ) -> ColliderId {
        let entity = PhysicsId::new(value);
        let id = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        let shape = ColliderShape::Cuboid {
            half_width: 1.0,
            half_height: 1.0,
        };
        assert!(world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                angle,
                ..Default::default()
            },
            &[ColliderSpec {
                shape: if compound {
                    ColliderShape::Compound {
                        children: vec![CompoundChild {
                            position: Vec2::new(2.0, 0.0),
                            angle: 0.0,
                            shape,
                        }],
                    }
                } else {
                    shape
                },
                sensor,
                collision_groups: groups,
                ..ColliderSpec::ball(id, 1.0)
            }],
        ));
        id
    }

    #[test]
    fn ball_cast_corner_geometry_is_world_space_and_read_only() {
        for compound in [false, true] {
            for angle in [0.0, 0.7] {
                let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
                let position = Vec2::new(73.0, -41.0);
                let collider = insert(
                    &mut world,
                    1,
                    position,
                    angle,
                    false,
                    CollisionGroups::ALL,
                    compound,
                );
                world.step(1.0 / 60.0);
                let before = world.snapshot_bytes().unwrap();
                let offset = if compound {
                    Vec2::new(2.0, 0.0)
                } else {
                    Vec2::ZERO
                };
                let transform = |p: Vec2| position + (p + offset).rotate_radians(angle);
                // The center ray misses this square, but the round foot hits
                // its corner with normal (0.6, 0.8), not either face normal.
                let origin = transform(Vec2::new(1.3, 3.0));
                let down = -Vec2::Y.rotate_radians(angle);
                assert!(
                    world
                        .cast_ray(
                            origin,
                            down,
                            RayCastOptions {
                                max_distance: 4.0,
                                ..Default::default()
                            }
                        )
                        .is_none()
                );
                let hit = world
                    .cast_ball(
                        origin,
                        0.5,
                        down * 7.0,
                        BallCastOptions {
                            max_distance: 4.0,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(hit.collider, collider);
                assert_eq!(hit.status, BallCastStatus::Converged);
                assert!((hit.distance - 1.6).abs() < 0.001, "{hit:?}");
                assert!(
                    hit.point.distance_to(transform(Vec2::new(1.0, 1.0))) < 0.001,
                    "{hit:?}"
                );
                assert!(
                    hit.normal
                        .distance_to(Vec2::new(0.6, 0.8).rotate_radians(angle))
                        < 0.001,
                    "{hit:?}"
                );
                assert!(
                    (origin + down * hit.distance).distance_to(hit.point + hit.normal * 0.5)
                        < 0.001
                );
                assert_eq!(world.snapshot_bytes().unwrap(), before);
            }
        }
    }

    #[test]
    fn ball_cast_respects_groups_sensors_exclusions_and_distance() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let groups = CollisionGroups::new(1, 2);
        insert(
            &mut world,
            1,
            Vec2::ZERO,
            0.0,
            false,
            CollisionGroups::new(4, 4),
            false,
        );
        let sensor = insert(
            &mut world,
            2,
            Vec2::new(0.0, -3.0),
            0.0,
            true,
            CollisionGroups::new(2, 1),
            false,
        );
        let near = insert(
            &mut world,
            3,
            Vec2::new(0.0, -6.0),
            0.0,
            false,
            CollisionGroups::new(2, 1),
            false,
        );
        let far = insert(
            &mut world,
            4,
            Vec2::new(0.0, -9.0),
            0.0,
            false,
            CollisionGroups::new(2, 1),
            false,
        );
        world.step(1.0 / 60.0);
        let options = BallCastOptions {
            max_distance: 20.0,
            collision_groups: groups,
            ..Default::default()
        };
        let cast = |o| world.cast_ball(Vec2::new(0.0, 4.0), 0.5, -Vec2::Y, o);
        assert_eq!(cast(options).unwrap().collider, near);
        assert_eq!(
            cast(BallCastOptions {
                include_sensors: true,
                ..options
            })
            .unwrap()
            .collider,
            sensor
        );
        assert_eq!(
            cast(BallCastOptions {
                exclude_entity: Some(near.entity),
                ..options
            })
            .unwrap()
            .collider,
            far
        );
        assert!(
            cast(BallCastOptions {
                max_distance: 8.4,
                ..options
            })
            .is_none()
        );
    }

    #[test]
    fn ball_cast_reports_initial_overlap_and_rejects_invalid_input() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        insert(
            &mut world,
            1,
            Vec2::ZERO,
            0.0,
            false,
            CollisionGroups::ALL,
            false,
        );
        world.step(1.0 / 60.0);
        let options = BallCastOptions::default();
        let hit = world
            .cast_ball(Vec2::new(0.0, 1.2), 0.5, Vec2::Y, options)
            .unwrap();
        assert_eq!(hit.status, BallCastStatus::Penetrating);
        assert_eq!(hit.distance, 0.0);
        assert!(finite_vec2(hit.point) && finite_vec2(hit.normal));
        for radius in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(
                world
                    .cast_ball(Vec2::ZERO, radius, Vec2::Y, options)
                    .is_none()
            );
        }
        for direction in [
            Vec2::ZERO,
            Vec2::new(f32::NAN, 0.0),
            Vec2::new(f32::MAX, 0.0),
        ] {
            assert!(
                world
                    .cast_ball(Vec2::ZERO, 0.5, direction, options)
                    .is_none()
            );
        }
        for max_distance in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(
                world
                    .cast_ball(
                        Vec2::ZERO,
                        0.5,
                        Vec2::Y,
                        BallCastOptions {
                            max_distance,
                            ..options
                        }
                    )
                    .is_none()
            );
        }
        assert!(
            world
                .cast_ball(Vec2::new(f32::NAN, 0.0), 0.5, Vec2::Y, options)
                .is_none()
        );
    }
}
