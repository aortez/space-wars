use super::*;
use rapier2d::parry::{
    query::intersection_test,
    shape::{Ball, Cuboid},
};

#[cfg(test)]
mod tests;

/// Conservative query bounds in the anchor's local frame. Callers must cover
/// every physical query supporting the result, including edge interiors.
#[derive(Debug, Clone, Copy)]
pub struct QueryArea {
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub groups: CollisionGroups,
}

#[derive(Clone, Copy)]
pub struct QueryFrame<'a> {
    pub previous_position: Vec2,
    pub previous_angle: f32,
    pub current_position: Vec2,
    pub current_angle: f32,
    pub excluded: &'a [PhysicsId],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AreaValidation {
    pub valid: bool,
    pub changed_colliders: u64,
    pub area_tests: u64,
    pub unrelated_changes: u64,
}

impl QuerySnapshot {
    /// Validate selected query areas, including geometry entering or leaving
    /// them. This is synchronous dependency work, outside a planner's dispatch
    /// allowance; report both its time and the intersection-test count.
    pub fn validate_areas(
        &self,
        world: &PhysicsWorld,
        region: QueryFrame<'_>,
        areas: &[QueryArea],
    ) -> AreaValidation {
        if areas.is_empty()
            || areas.iter().any(|a| {
                !finite_vec2(a.minimum)
                    || !finite_vec2(a.maximum)
                    || a.minimum.x >= a.maximum.x
                    || a.minimum.y >= a.maximum.y
            })
        {
            return AreaValidation::default();
        }
        self.validate_changes(world, region, |collider, pose, tests| {
            let bounds = collider.shape().compute_aabb(pose);
            areas.iter().any(|area| {
                if !collider.collision_groups().test(area.groups.to_rapier())
                    || bounds.maxs.x < area.minimum.x
                    || bounds.mins.x > area.maximum.x
                    || bounds.maxs.y < area.minimum.y
                    || bounds.mins.y > area.maximum.y
                {
                    return false;
                }
                *tests += 1;
                let center = (area.minimum + area.maximum) * 0.5;
                let half = (area.maximum - area.minimum) * 0.5;
                // Actual shape intersection avoids treating a hollow arena
                // boundary's enclosing AABB as an obstruction everywhere.
                intersection_test(
                    pose,
                    collider.shape(),
                    &Pose::new(to_rapier(center), 0.0),
                    &Cuboid::new(to_rapier(half)),
                )
                .unwrap_or(true)
            })
        })
    }

    /// Certify a complete circular query region with the same collider-extent
    /// motion bound as local areas. Retained profiles can still use their
    /// original `matches_region` contract.
    pub fn validate_region(&self, world: &PhysicsWorld, region: QueryRegion<'_>) -> AreaValidation {
        if !region.radius.is_finite() || region.radius <= 0.0 {
            return AreaValidation::default();
        }
        let circle = Ball::new(region.radius);
        self.validate_changes(
            world,
            QueryFrame {
                previous_position: region.previous_position,
                previous_angle: region.previous_angle,
                current_position: region.current_position,
                current_angle: region.current_angle,
                excluded: region.excluded,
            },
            |collider, pose, tests| {
                let bounds = collider.shape().compute_aabb(pose);
                if !collider.collision_groups().test(region.groups.to_rapier())
                    || bounds.maxs.x < -region.radius
                    || bounds.mins.x > region.radius
                    || bounds.maxs.y < -region.radius
                    || bounds.mins.y > region.radius
                {
                    return false;
                }
                *tests += 1;
                intersection_test(pose, collider.shape(), &Pose::IDENTITY, &circle).unwrap_or(true)
            },
        )
    }

    fn validate_changes(
        &self,
        world: &PhysicsWorld,
        region: QueryFrame<'_>,
        intersects: impl Fn(&Collider, &Pose, &mut u64) -> bool,
    ) -> AreaValidation {
        let mut report = AreaValidation::default();
        if !finite_vec2(region.previous_position)
            || !finite_vec2(region.current_position)
            || !region.previous_angle.is_finite()
            || !region.current_angle.is_finite()
        {
            return report;
        }
        let old_frame = Pose::new(to_rapier(region.previous_position), region.previous_angle);
        let new_frame = Pose::new(to_rapier(region.current_position), region.current_angle);
        let same = |a: &Collider, b: &Collider| {
            if a.user_data != b.user_data
                || a.collision_groups() != b.collision_groups()
                || a.is_sensor() != b.is_sensor()
                || a.is_enabled() != b.is_enabled()
            {
                return false;
            }
            // Generational handle equality establishes immutable shape identity.
            // Bound angular displacement using the collider's own extent, even
            // when its origin lies well outside the selected route.
            let bounds = a.shape().compute_local_aabb();
            let extent = bounds.mins.abs().max(bounds.maxs.abs()).length();
            let a = old_frame.inv_mul(a.position());
            let b = new_frame.inv_mul(b.position());
            (a.translation - b.translation).length()
                + 2.0
                    * ((a.rotation.angle() - b.rotation.angle()) * 0.5)
                        .sin()
                        .abs()
                    * extent
                <= 0.002
        };
        let touches = |collider: &Collider, frame: &Pose, tests: &mut u64| {
            if !collider.is_enabled()
                || collider.is_sensor()
                || decode_collider(collider.user_data)
                    .is_some_and(|id| region.excluded.contains(&id.entity))
            {
                return false;
            }
            intersects(collider, &frame.inv_mul(collider.position()), tests)
        };
        for (handle, old) in self.colliders.iter() {
            let current = world.raw.colliders.get(handle);
            if current.is_some_and(|new| same(old, new)) {
                continue;
            }
            report.changed_colliders += 1;
            if touches(old, &old_frame, &mut report.area_tests)
                || current.is_some_and(|new| touches(new, &new_frame, &mut report.area_tests))
            {
                return report;
            }
            report.unrelated_changes += 1;
        }
        for (handle, current) in world.raw.colliders.iter() {
            if self.colliders.get(handle).is_some() {
                continue;
            }
            report.changed_colliders += 1;
            if touches(current, &new_frame, &mut report.area_tests) {
                return report;
            }
            report.unrelated_changes += 1;
        }
        report.valid = true;
        report
    }
}
