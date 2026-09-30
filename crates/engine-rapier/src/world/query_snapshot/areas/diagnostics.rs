//! Bounded explanation of circular-region rejections, never an acceptance gate.
use super::*;

const MAX_COLLIDERS: usize = 8192;
const MAX_AREAS: usize = 8;
const MAX_CHANGES: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryColliderState {
    pub collider: Option<ColliderId>,
    /// All positions/bounds are relative to the corresponding anchor frame.
    pub position: Vec2,
    pub angle: f32,
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub memberships: u32,
    pub filter: u32,
    pub sensor: bool,
    pub enabled: bool,
}
impl QueryColliderState {
    fn read(collider: &Collider, frame: &Pose) -> Self {
        let pose = frame.inv_mul(collider.position());
        let bounds = collider.shape().compute_aabb(&pose);
        Self {
            collider: decode_collider(collider.user_data),
            position: from_rapier(pose.translation),
            angle: pose.rotation.angle(),
            minimum: from_rapier(bounds.mins),
            maximum: from_rapier(bounds.maxs),
            memberships: collider.collision_groups().memberships.bits(),
            filter: collider.collision_groups().filter.bits(),
            sensor: collider.is_sensor(),
            enabled: collider.is_enabled(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryChange {
    /// Handle replacement appears as removal plus addition, even with the same
    /// semantic collider ID. No shape identity is inferred from that ID.
    pub previous: Option<QueryColliderState>,
    pub current: Option<QueryColliderState>,
    pub motion_bound: Option<f32>,
    /// Indices of conservative areas touched by either relevant pose. Overlap
    /// is not proof that an individual query, route or action is blocked.
    pub areas: Vec<usize>,
    pub unsupported_tests: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RegionChanges {
    /// False for invalid/capacity-limited input. Empty details never certify it.
    pub complete: bool,
    pub unavailable: Option<&'static str>,
    pub changed_colliders: u64,
    pub region_changes: u64,
    pub area_tests: u64,
    /// Unsupported shape tests conservatively count as potential overlap.
    pub unsupported_tests: u64,
    /// Counts include omitted detail records. One collider can touch many areas.
    pub area_changes: Vec<u64>,
    pub omitted_changes: u64,
    pub changes: Vec<QueryChange>,
}

impl QuerySnapshot {
    /// Explain every changed collider intersecting the original circular gate,
    /// capped at 8,192 source/current colliders, eight areas and eight retained
    /// details. Scan totals stay complete when details are truncated. Geometry
    /// outside the circle is not audited. Synchronous work is reported separately
    /// from a planner's operation quota and the gate's early-exit counters.
    pub fn diagnose_region(
        &self,
        world: &PhysicsWorld,
        region: QueryRegion<'_>,
        areas: &[QueryArea],
    ) -> RegionChanges {
        let mut report = RegionChanges::default();
        if self.colliders.len() > MAX_COLLIDERS || world.raw.colliders.len() > MAX_COLLIDERS {
            report.unavailable = Some("collider capacity");
            return report;
        }
        if areas.len() > MAX_AREAS || region.excluded.len() > MAX_AREAS {
            report.unavailable = Some("area or exclusion capacity");
            return report;
        }
        if !finite_vec2(region.previous_position)
            || !finite_vec2(region.current_position)
            || !region.previous_angle.is_finite()
            || !region.current_angle.is_finite()
            || !region.radius.is_finite()
            || region.radius <= 0.0
            || areas.iter().any(|a| {
                !finite_vec2(a.minimum)
                    || !finite_vec2(a.maximum)
                    || a.minimum.x >= a.maximum.x
                    || a.minimum.y >= a.maximum.y
            })
        {
            report.unavailable = Some("invalid frame or bounds");
            return report;
        }
        report.area_changes = vec![0; areas.len()];
        let old_frame = Pose::new(to_rapier(region.previous_position), region.previous_angle);
        let new_frame = Pose::new(to_rapier(region.current_position), region.current_angle);
        let circle = Ball::new(region.radius);
        let eligible = |c: &Collider| {
            c.is_enabled()
                && !c.is_sensor()
                && decode_collider(c.user_data)
                    .is_none_or(|id| !region.excluded.contains(&id.entity))
        };
        let circle_hit = |c: &Collider, frame: &Pose, tests: &mut u64, errors: &mut u64| {
            if !eligible(c) || !c.collision_groups().test(region.groups.to_rapier()) {
                return false;
            }
            let pose = frame.inv_mul(c.position());
            let bounds = c.shape().compute_aabb(&pose);
            if bounds.maxs.x < -region.radius
                || bounds.mins.x > region.radius
                || bounds.maxs.y < -region.radius
                || bounds.mins.y > region.radius
            {
                return false;
            }
            *tests += 1;
            intersection_test(&pose, c.shape(), &Pose::IDENTITY, &circle).unwrap_or_else(|_| {
                *errors += 1;
                true
            })
        };
        let area_hit =
            |c: &Collider, frame: &Pose, a: &QueryArea, tests: &mut u64, errors: &mut u64| {
                if !eligible(c) || !c.collision_groups().test(a.groups.to_rapier()) {
                    return false;
                }
                let pose = frame.inv_mul(c.position());
                let bounds = c.shape().compute_aabb(&pose);
                if bounds.maxs.x < a.minimum.x
                    || bounds.mins.x > a.maximum.x
                    || bounds.maxs.y < a.minimum.y
                    || bounds.mins.y > a.maximum.y
                {
                    return false;
                }
                *tests += 1;
                let center = (a.minimum + a.maximum) * 0.5;
                let half = (a.maximum - a.minimum) * 0.5;
                intersection_test(
                    &pose,
                    c.shape(),
                    &Pose::new(to_rapier(center), 0.0),
                    &Cuboid::new(to_rapier(half)),
                )
                .unwrap_or_else(|_| {
                    *errors += 1;
                    true
                })
            };
        let mut record = |old: Option<&Collider>, new: Option<&Collider>| {
            report.changed_colliders += 1;
            let errors_before = report.unsupported_tests;
            if !old.is_some_and(|c| {
                circle_hit(
                    c,
                    &old_frame,
                    &mut report.area_tests,
                    &mut report.unsupported_tests,
                )
            }) && !new.is_some_and(|c| {
                circle_hit(
                    c,
                    &new_frame,
                    &mut report.area_tests,
                    &mut report.unsupported_tests,
                )
            }) {
                return;
            }
            report.region_changes += 1;
            let mut touched = Vec::new();
            for (index, area) in areas.iter().enumerate() {
                if old.is_some_and(|c| {
                    area_hit(
                        c,
                        &old_frame,
                        area,
                        &mut report.area_tests,
                        &mut report.unsupported_tests,
                    )
                }) || new.is_some_and(|c| {
                    area_hit(
                        c,
                        &new_frame,
                        area,
                        &mut report.area_tests,
                        &mut report.unsupported_tests,
                    )
                }) {
                    report.area_changes[index] += 1;
                    touched.push(index);
                }
            }
            if report.changes.len() == MAX_CHANGES {
                report.omitted_changes += 1;
                return;
            }
            report.changes.push(QueryChange {
                previous: old.map(|c| QueryColliderState::read(c, &old_frame)),
                current: new.map(|c| QueryColliderState::read(c, &new_frame)),
                motion_bound: old
                    .zip(new)
                    .map(|(a, b)| relative_motion_bound(a, b, &old_frame, &new_frame)),
                areas: touched,
                unsupported_tests: report.unsupported_tests - errors_before,
            });
        };
        for (handle, old) in self.colliders.iter() {
            let new = world.raw.colliders.get(handle);
            if new.is_none_or(|c| !same_geometry(old, c, &old_frame, &new_frame)) {
                record(Some(old), new);
            }
        }
        for (handle, new) in world.raw.colliders.iter() {
            if self.colliders.get(handle).is_none() {
                record(None, Some(new));
            }
        }
        report.complete = true;
        report
    }
}
