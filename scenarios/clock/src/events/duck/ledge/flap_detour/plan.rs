//! Bounded read-only sweeps in the existing world. Before warm-up, this one
//! authored stunt predicts ordinary actuator inputs; it never supplies learned
//! samples to the course navigator or creates a second physics world.
use super::*;
use engine_rapier::world::{BallCastOptions, BallCastStatus};

#[derive(Clone, Copy)]
pub(super) struct Return {
    pub center: Vec2,
    pub body: BodyId,
    pose: Vec2,
    angle: f32,
}

impl Return {
    pub fn valid(self, duck: &DuckEvent) -> bool {
        let world = duck.world.as_ref().unwrap();
        world.incoming_motion(self.body, DT).is_some_and(|m| {
            (m.position - self.pose).length() < duck.radius * 0.01
                && (m.angle - self.angle).abs() < 0.001
                && m.linear_velocity.length() < duck.radius * 0.01
                && m.angular_velocity.abs() < 0.001
        })
    }

    pub fn dry(self, water: Option<&WaterWorld>, radius: f32) -> bool {
        water.is_none_or(|water| {
            water.pools().iter().all(|pool| {
                pool.columns_in_range(
                    f64::from(self.center.x - radius),
                    f64::from(self.center.x + radius),
                )
                .all(|c| c.surface <= f64::from(self.center.y - radius * 0.95))
            })
        })
    }
}

impl DuckEvent {
    pub(super) fn plan_flap_detour(
        &mut self,
        water: Option<&WaterWorld>,
    ) -> Option<(Edge, Return)> {
        let edge = edges(self)
            .into_iter()
            .flatten()
            .find(|e| e.flap.is_some_and(|(side, _)| side == 0))?;
        let world = self.world.as_ref()?;
        let motion = world.motion(DUCK_BODY)?;
        let r = self.radius;
        let anchor = edge.anchor(world)?;
        // A normal spawn on stationary, level support near the tip. Existing
        // shared debris, sloping/moving panels and interrupted entries can skip.
        let support = world.surface_contacts(DUCK_COLLIDER).find(|c| {
            c.normal.y > 0.99 && c.separation <= r * 0.05 && c.velocity.length() < r * 0.01
        })?;
        let body = world.collider_body(support.collider)?;
        if !matches!(
            world.body_kind(body),
            Some(BodyKind::Fixed | BodyKind::KinematicPosition)
        ) || motion.linear_velocity.length() > r
            || !(5.7 * r..=6.3 * r).contains(&self.position()?.x)
        {
            return None;
        }
        let source = world.incoming_motion(body, DT)?;
        if source.linear_velocity.length() > r * 0.01 || source.angular_velocity.abs() > 0.001 {
            return None;
        }
        let origin = anchor + Vec2::new(edge.outward * r * 4.0, r * 2.0);
        self.ledge.detour.stats.clearance_queries += 1;
        let hit = world.cast_ball(
            origin,
            r,
            -Vec2::Y,
            BallCastOptions {
                max_distance: r * 15.0,
                exclude_entity: Some(DUCK_ENTITY),
                ..Default::default()
            },
        )?;
        let body = world.collider_body(hit.collider)?;
        let pose = world.motion(body)?;
        let (min, max) = world.body_solid_bounds(body)?;
        if hit.status != BallCastStatus::Converged
            || hit.normal.y < 0.99
            || !matches!(
                world.body_kind(body),
                Some(BodyKind::Fixed | BodyKind::KinematicPosition)
            )
            || hit.point.x < min.x + r * 2.0
            || hit.point.x > max.x - r * 2.0
        {
            return None;
        }
        let landing = Return {
            center: Vec2::new(origin.x, origin.y - hit.distance),
            body,
            pose: pose.position,
            angle: pose.angle,
        };
        if !landing.valid(self) || !landing.dry(water, r) {
            return None;
        }
        let hang = anchor + Vec2::new(edge.outward * r * 1.12, -r * 0.9);
        let up = anchor + Vec2::new(edge.outward * r * 1.12, r * 1.15);
        let across = anchor + Vec2::new(-edge.outward * r * 1.5, r * 1.15);
        let perch = anchor + Vec2::new(-edge.outward * r * 1.5, r * 1.01);
        let queries = &mut self.ledge.detour.stats.clearance_queries;
        if !clear_path(world, hang, up, r, None, queries)
            || !clear_path(world, up, across, r, None, queries)
            || !clear_path(world, across, perch, r, Some(edge.body), queries)
        {
            return None;
        }
        // Predict only until the existing entrance deadline, including the
        // same acceleration and launch impulse used by real movement commands.
        let mut position = motion.position;
        let mut velocity = motion.linear_velocity;
        let mut reachable = false;
        for tick in self.phase_tick..20 {
            let local = Vec2::new(position.x * self.direction + self.width * 0.5, position.y);
            let command = self.detour_approach(local, tick == self.phase_tick);
            let next = self.detour_flight_step(position, &mut velocity, &command);
            if !clear_path(
                world,
                position,
                next,
                r * 0.97,
                None,
                &mut self.ledge.detour.stats.clearance_queries,
            ) {
                return None;
            }
            position = next;
            if edge.in_reach_from(world, position, velocity, r) {
                reachable = true;
                break;
            }
        }
        (reachable && self.detour_return_clear(perch, Vec2::ZERO, landing))
            .then_some((edge, landing))
    }

    pub(super) fn detour_return_clear(
        &mut self,
        from: Vec2,
        mut velocity: Vec2,
        landing: Return,
    ) -> bool {
        let mut position = from;
        let mut previous = from;
        for tick in 0..90 {
            let command = self.detour_return_command(position, landing, tick == 0);
            position = self.detour_flight_step(position, &mut velocity, &command);
            if tick % 4 == 3 || (velocity.y < 0.0 && position.y <= landing.center.y) {
                if !clear_path(
                    self.world.as_ref().unwrap(),
                    previous,
                    position,
                    self.radius * 0.97,
                    Some(landing.body),
                    &mut self.ledge.detour.stats.clearance_queries,
                ) {
                    return false;
                }
                previous = position;
            }
            if velocity.y < 0.0 && position.y <= landing.center.y {
                return (position.x - landing.center.x).abs() < self.radius * 0.5;
            }
        }
        false
    }

    fn detour_flight_step(&self, position: Vec2, velocity: &mut Vec2, command: &Command) -> Vec2 {
        let observation = Observation {
            position,
            velocity: Vec2::new(velocity.x * self.direction, velocity.y),
            support_velocity: Vec2::ZERO,
            grounded: command.jump,
            blocked: false,
            support: None,
            debris: None,
        };
        let mut delta = self.movement.velocity_delta(observation, command);
        delta.x *= self.direction;
        *velocity += delta;
        let next = position + *velocity * DT - Vec2::Y * (0.5 * self.movement.gravity * DT * DT);
        velocity.y -= self.movement.gravity * DT;
        next
    }
}
