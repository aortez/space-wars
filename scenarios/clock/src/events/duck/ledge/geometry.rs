//! Only authored course edges and fully open flap tips can become anchors.
use super::*;
use engine_rapier::world::{BallCastOptions, BallCastStatus};

pub const MAX_EDGES: usize = planner::MAX_SURFACES * 2 + 2;
pub const MAX_QUERIES: u32 = 64;

#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub body: BodyId,
    pub local_anchor: Vec2,
    pub outward: f32,
    pub flap: Option<(usize, flaps::FlapHandle)>,
    angle: f32,
}

impl Edge {
    pub fn in_reach(self, duck: &DuckEvent) -> bool {
        let world = duck.world.as_ref().unwrap();
        let motion = world.motion(DUCK_BODY).unwrap();
        let offset = motion.position - self.anchor(world).unwrap();
        let relative = motion.linear_velocity - self.velocity(world).unwrap();
        let r = duck.radius;
        offset.length() <= r * REACH_RADII
            && (0.3 * r..=2.0 * r).contains(&(offset.x * self.outward))
            && (-2.0 * r..=-0.1 * r).contains(&offset.y)
            && relative.x * self.outward <= r * 2.0
            && relative.length() <= r * DISRUPTION_SPEED_RADII
    }

    pub fn anchor(self, world: &PhysicsWorld) -> Option<Vec2> {
        let motion = world.motion(self.body)?;
        Some(motion.position + self.local_anchor.rotate_radians(motion.angle))
    }

    pub fn velocity(self, world: &PhysicsWorld) -> Option<Vec2> {
        let motion = world.incoming_motion(self.body, DT)?;
        let offset = self.local_anchor.rotate_radians(motion.angle);
        Some(motion.linear_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity)
    }

    pub fn valid(self, duck: &DuckEvent) -> bool {
        edges(duck).into_iter().flatten().any(|edge| {
            edge.body == self.body
                && edge.flap == self.flap
                && edge.outward == self.outward
                && (edge.local_anchor - self.local_anchor).length() < duck.radius * 0.01
                && (edge.angle - self.angle).abs() < 0.001
        })
    }
}

pub(super) fn edges(duck: &DuckEvent) -> [Option<Edge>; MAX_EDGES] {
    let mut result = [None; MAX_EDGES];
    let Some(world) = &duck.world else {
        return result;
    };
    let mut count = 0;
    if duck.responsive_floor.is_none()
        && duck.drain_floor.is_none()
        && let Some(course) = &duck.course
    {
        for index in 0..course.surfaces.len().min(planner::MAX_SURFACES) {
            let body = BodyId::new(PhysicsId::new(1000 + index as u64), BodyRole::PRIMARY);
            let Some(motion) = world.motion(body) else {
                continue;
            };
            let Some((min, max)) = world.body_solid_bounds(body) else {
                continue;
            };
            if world.body_kind(body) != Some(BodyKind::Fixed)
                || motion.angle.abs() > 0.001
                || max.x - min.x < duck.radius * 3.0
            {
                continue;
            }
            for outward in [-1.0, 1.0] {
                let anchor = Vec2::new(if outward < 0.0 { min.x } else { max.x }, max.y);
                result[count] = Some(Edge {
                    body,
                    local_anchor: anchor - motion.position,
                    outward,
                    flap: None,
                    angle: motion.angle,
                });
                count += 1;
            }
        }
    }
    for (side, panel) in duck.door_panels().into_iter().enumerate() {
        let Some(panel) = panel else { continue };
        let body = flaps::body(side);
        let Some(motion) = world.incoming_motion(body, DT) else {
            continue;
        };
        if world.body_kind(body) != Some(BodyKind::KinematicPosition)
            || (panel.angle.abs() - std::f32::consts::FRAC_PI_2).abs() > 0.001
            || motion.angular_velocity.abs() > 0.001
        {
            continue;
        }
        let anchor = panel.point(Vec2::new(
            panel.hinge.x + panel.inward * panel.width,
            panel.bottom,
        ));
        result[count] = Some(Edge {
            body,
            local_anchor: (anchor - panel.center()).rotate_radians(-panel.angle),
            outward: panel.inward,
            flap: Some((side, duck.flap_handle(side).unwrap())),
            angle: panel.angle,
        });
        count += 1;
    }
    result
}

pub(super) fn clear_path(
    world: &PhysicsWorld,
    from: Vec2,
    to: Vec2,
    radius: f32,
    landing: Option<BodyId>,
    queries: &mut u32,
) -> bool {
    if *queries >= MAX_QUERIES {
        return false;
    }
    *queries += 1;
    let delta = to - from;
    let hit = world.cast_ball(
        from,
        radius,
        if delta.length_squared() > f32::EPSILON {
            delta
        } else {
            Vec2::Y
        },
        BallCastOptions {
            // Rapier's sweep traversal excludes hits at a zero upper bound,
            // even initial overlaps. Keep stationary occupancy probes nonzero.
            max_distance: delta.length().max(radius * 0.001),
            exclude_entity: Some(DUCK_ENTITY),
            ..Default::default()
        },
    );
    hit.is_none_or(|hit| {
        landing.is_some_and(|body| hit.collider.entity == body.entity)
            && hit.normal.y > 0.7
            && hit.status == BallCastStatus::Converged
    })
}

pub(super) fn hand_clear(
    world: &PhysicsWorld,
    from: Vec2,
    anchor: Vec2,
    body: BodyId,
    radius: f32,
    queries: &mut u32,
) -> bool {
    if *queries >= MAX_QUERIES {
        return false;
    }
    *queries += 1;
    let delta = anchor - from;
    // A narrow hand sweep tolerates the corner's floating-point rounding while
    // still rejecting any intervening wall. An exact tangent ray can miss wood.
    world
        .cast_ball(
            from,
            radius * 0.03,
            delta,
            BallCastOptions {
                max_distance: delta.length(),
                exclude_entity: Some(DUCK_ENTITY),
                ..Default::default()
            },
        )
        .is_some_and(|hit| {
            hit.collider.entity == body.entity
                && hit.status == BallCastStatus::Converged
                && (hit.point - anchor).length() < radius * 0.08
        })
}
