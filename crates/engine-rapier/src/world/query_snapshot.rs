//! Owned queries from one completed physics step. No solver, contacts or step API.
use super::*;
use rapier2d::{parry::query::DefaultQueryDispatcher, prelude::QueryPipeline};

#[derive(Clone)]
pub struct QuerySnapshot {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    broad_phase: BroadPhaseBvh,
}

/// The same region expressed in the snapshot and current rigid-body frames.
/// Exclusions and groups must cover all queries used to build the plan.
pub struct QueryRegion<'a> {
    pub previous_position: Vec2,
    pub previous_angle: f32,
    pub current_position: Vec2,
    pub current_angle: f32,
    pub radius: f32,
    pub groups: CollisionGroups,
    pub excluded: &'a [PhysicsId],
}

impl PhysicsWorld {
    /// Copies collision poses, handles and the spatial index; SharedShape keeps
    /// immutable geometry shared. Call only after pending changes are flushed by
    /// the normal physics step. Construction is outside a planning work quota.
    pub fn query_snapshot(&self) -> QuerySnapshot {
        QuerySnapshot {
            bodies: self.raw.bodies.clone(),
            colliders: self.raw.colliders.clone(),
            broad_phase: self.raw.broad_phase.clone(),
        }
    }
}

impl QuerySnapshot {
    pub fn counts(&self) -> (usize, usize) {
        (self.bodies.len(), self.colliders.len())
    }

    fn pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        // PhysicsWorld uses Rapier's default dispatcher. Retain the original
        // collider handles/tree so equal-distance ray ties match that world.
        self.broad_phase.as_query_pipeline(
            &DefaultQueryDispatcher,
            &self.bodies,
            &self.colliders,
            filter,
        )
    }

    pub fn cast_ray(
        &self,
        origin: Vec2,
        direction: Vec2,
        options: RayCastOptions,
    ) -> Option<RayHit> {
        if !finite_vec2(origin)
            || !finite_vec2(direction)
            || direction.length_squared() <= f32::EPSILON
            || !options.max_distance.is_finite()
            || options.max_distance < 0.0
        {
            return None;
        }
        let direction = direction.normalized();
        let predicate = |_: ColliderHandle, collider: &Collider| {
            decode_collider(collider.user_data)
                .is_none_or(|id| Some(id.entity) != options.exclude_entity)
        };
        let filter = QueryFilter {
            flags: if options.include_sensors {
                QueryFilterFlags::empty()
            } else {
                QueryFilterFlags::EXCLUDE_SENSORS
            },
            groups: Some(options.collision_groups.to_rapier()),
            predicate: Some(&predicate),
            ..Default::default()
        };
        let (handle, intersection) = self.pipeline(filter).cast_ray_and_get_normal(
            &Ray::new(to_rapier(origin), to_rapier(direction)),
            options.max_distance,
            options.solid,
        )?;
        Some(RayHit {
            collider: decode_collider(self.colliders.get(handle)?.user_data)?,
            point: origin + direction * intersection.time_of_impact,
            normal: from_rapier(intersection.normal),
            distance: intersection.time_of_impact,
        })
    }

    /// Conservative local dependency check, including entering/leaving bodies,
    /// shape replacement, filters and sensor/enabled state. Rigid anchor motion
    /// alone is allowed. The 0.002-unit pose tolerance covers transform roundoff;
    /// callers must reserve a larger clearance margin and still revalidate action
    /// permissions. This is planning evidence, not a collision permission.
    pub fn matches_region(&self, world: &PhysicsWorld, region: QueryRegion<'_>) -> bool {
        if !finite_vec2(region.previous_position)
            || !finite_vec2(region.current_position)
            || !region.previous_angle.is_finite()
            || !region.current_angle.is_finite()
            || !region.radius.is_finite()
            || region.radius <= 0.0
        {
            return false;
        }
        let old_frame = Pose::new(to_rapier(region.previous_position), region.previous_angle);
        let new_frame = Pose::new(to_rapier(region.current_position), region.current_angle);
        let relevant = |collider: &Collider, center: Vec2| {
            let groups = collider.collision_groups();
            let id = decode_collider(collider.user_data);
            let bounds = collider.compute_aabb();
            collider.is_enabled()
                && !collider.is_sensor()
                && groups.test(region.groups.to_rapier())
                && id.is_none_or(|id| !region.excluded.contains(&id.entity))
                && bounds.mins.x <= center.x + region.radius
                && bounds.maxs.x >= center.x - region.radius
                && bounds.mins.y <= center.y + region.radius
                && bounds.maxs.y >= center.y - region.radius
                // A hollow boundary's AABB contains the whole arena. Check
                // distance to actual geometry before treating it as local.
                && (collider.shape().project_point(collider.position(), to_rapier(center), true).point
                    - to_rapier(center)).length() <= region.radius
        };
        let same = |a: &Collider, b: &Collider| {
            if a.user_data != b.user_data
                || a.collision_groups() != b.collision_groups()
                || a.is_sensor() != b.is_sensor()
                || a.is_enabled() != b.is_enabled()
            {
                return false;
            }
            // PhysicsWorld replaces generational collider handles when geometry
            // changes; it exposes no in-place shape mutation. Handle identity
            // also survives serialization/clone, unlike an Arc's address.
            let a = old_frame.inv_mul(a.position());
            let b = new_frame.inv_mul(b.position());
            let angle = a.rotation.angle() - b.rotation.angle();
            (a.translation - b.translation).length()
                + 2.0 * (angle * 0.5).sin().abs() * region.radius
                <= 0.002
        };
        self.colliders
            .iter()
            .filter(|(_, c)| relevant(c, region.previous_position))
            .all(|(h, a)| world.raw.colliders.get(h).is_some_and(|b| same(a, b)))
            && world
                .raw
                .colliders
                .iter()
                .filter(|(_, c)| relevant(c, region.current_position))
                .all(|(h, b)| self.colliders.get(h).is_some_and(|a| same(a, b)))
    }
}

/// Prepared capsule reused across individual charged queries.
#[derive(Clone)]
pub struct CapsuleQuery {
    capsule: Option<Collider>,
    groups: CollisionGroups,
    excluded: Vec<PhysicsId>,
}
impl CapsuleQuery {
    pub fn new(
        half_segment: f32,
        radius: f32,
        groups: CollisionGroups,
        excluded: Vec<PhysicsId>,
    ) -> Self {
        Self {
            capsule: (half_segment.is_finite()
                && half_segment >= 0.0
                && radius.is_finite()
                && radius > 0.0)
                .then(|| ColliderBuilder::capsule_y(half_segment, radius).build()),
            groups,
            excluded,
        }
    }
    pub fn is_clear(&self, snapshot: &QuerySnapshot, position: Vec2, angle: f32) -> bool {
        let Some(capsule) = &self.capsule else {
            return false;
        };
        if !finite_vec2(position) || !angle.is_finite() {
            return false;
        }
        let predicate = |_: ColliderHandle, collider: &Collider| {
            decode_collider(collider.user_data).is_none_or(|id| !self.excluded.contains(&id.entity))
        };
        snapshot
            .pipeline(QueryFilter {
                flags: QueryFilterFlags::EXCLUDE_SENSORS,
                groups: Some(self.groups.to_rapier()),
                predicate: Some(&predicate),
                ..Default::default()
            })
            .intersect_shape(Pose::new(to_rapier(position), angle), capsule.shape())
            .next()
            .is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;
    fn body(world: &mut PhysicsWorld, n: u64, position: Vec2) -> (BodyId, ColliderId) {
        let entity = PhysicsId::new(n);
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                ..Default::default()
            },
            &[ColliderSpec::ball(collider, 1.0)],
        ));
        (body, collider)
    }
    fn region() -> QueryRegion<'static> {
        QueryRegion {
            previous_position: Vec2::ZERO,
            previous_angle: 0.0,
            current_position: Vec2::ZERO,
            current_angle: 0.0,
            radius: 10.0,
            groups: CollisionGroups::ALL,
            excluded: &[],
        }
    }

    #[test]
    fn queries_retain_old_poses_and_validate_cloned_rigid_frames() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let (id, _) = body(&mut world, 1, Vec2::Y * 5.0);
        world.step(DT);
        let snapshot = world.query_snapshot();
        let options = RayCastOptions {
            max_distance: 10.0,
            ..Default::default()
        };
        let hit = world.cast_ray(Vec2::ZERO, Vec2::Y, options);
        assert!(hit.is_some());
        assert_eq!(snapshot.cast_ray(Vec2::ZERO, Vec2::Y, options), hit);
        let capsule = CapsuleQuery::new(0.6, 0.3, CollisionGroups::ALL, vec![]);
        assert!(!capsule.is_clear(&snapshot, Vec2::Y * 5.0, 0.0));
        let mut clone = world.clone();
        assert!(snapshot.matches_region(&clone, region()));
        let position = Vec2::new(100.0, -20.0);
        let angle = 0.3;
        clone.set_pose(
            id,
            position + (Vec2::Y * 5.0).rotate_radians(angle),
            angle,
            true,
        );
        clone.step(DT);
        assert!(!snapshot.matches_region(&clone, region()));
        assert!(snapshot.matches_region(
            &clone,
            QueryRegion {
                current_position: position,
                current_angle: angle,
                ..region()
            }
        ));
        assert_eq!(snapshot.cast_ray(Vec2::ZERO, Vec2::Y, options), hit);
        assert!(clone.cast_ray(Vec2::ZERO, Vec2::Y, options).is_none());
    }

    #[test]
    fn entering_leaving_replacement_and_filter_changes_revoke_evidence() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let (near, collider) = body(&mut world, 1, Vec2::Y * 5.0);
        let (far, _) = body(&mut world, 2, Vec2::X * 50.0);
        world.step(DT);
        let snapshot = world.query_snapshot();
        let mut clone = world.clone();
        clone.set_pose(far, Vec2::X * 40.0, 0.0, true);
        clone.step(DT);
        assert!(
            snapshot.matches_region(&clone, region()),
            "distant motion is irrelevant"
        );
        clone.set_pose(far, Vec2::X * 5.0, 0.0, true);
        clone.step(DT);
        assert!(
            !snapshot.matches_region(&clone, region()),
            "an obstacle entered"
        );
        let mut clone = world.clone();
        clone.remove_entity(near.entity);
        clone.step(DT);
        assert!(
            !snapshot.matches_region(&clone, region()),
            "an obstacle disappeared"
        );
        for (radius, sensor) in [(2.0, false), (1.0, true)] {
            let mut shape = ColliderSpec::ball(collider, radius);
            shape.sensor = sensor;
            let mut clone = world.clone();
            assert!(clone.replace_colliders(near, collider.role, &[shape]));
            clone.step(DT);
            assert!(!snapshot.matches_region(&clone, region()));
        }
    }
}
