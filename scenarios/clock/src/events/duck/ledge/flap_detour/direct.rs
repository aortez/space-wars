//! Opt-in survey of a closing flap. The survey is read-only and bounded;
//! ordinary movement commands and real contacts perform the entire stunt.
use super::*;
use engine_rapier::world::{BallCastOptions, BallCastStatus, CollisionGroups};

// Observe a settled ordinary spawn before surveying at most 60 candidates
// (four launch ticks × fifteen steering delays), each at most 58 free steps.
const SETTLE_TICKS: u64 = 12;
const LAST_LAUNCH_TICK: u64 = 16;
const LAST_LANDING_TICK: u64 = 58;
const LANDING_GRACE_TICKS: u64 = 16;
const STABLE_CONTACT_TICKS: u8 = 3;
const MAX_SAMPLES: usize = 64;

#[derive(Clone, Copy)]
pub(super) struct Plan {
    launch: u64,
    approach: u64,
    landing: u64,
    target_x: f32,
    pub perch_x: f32,
    contacts: u8,
}

struct Prediction {
    plan: Plan,
    points: [Vec2; MAX_SAMPLES],
    len: usize,
    perch: Vec2,
}

impl DuckEvent {
    pub(super) fn decide_direct_flap_approach(
        &mut self,
        water: Option<&WaterWorld>,
    ) -> Option<Command> {
        let still = || Command {
            gait: Gait::Still,
            direction: -1.0,
            jump: false,
        };
        if self.ledge.detour.stats.phase == DetourPhase::Pending {
            if self.ledge.detour.age > SETTLE_TICKS {
                self.finish_flap_detour(Outcome::Expired);
                return None;
            }
            if self.ledge.detour.age < SETTLE_TICKS || !self.grounded() {
                return Some(still());
            }
            let Some((plan, landing)) = self.plan_direct_flap(water) else {
                self.finish_flap_detour(Outcome::Unsafe);
                return None;
            };
            self.controller.interrupt_for_ledge();
            self.controller.direction = -1.0;
            self.ledge.detour.direct = Some(plan);
            self.ledge.detour.landing = Some(landing);
            self.ledge.detour.stats.attempts = 1;
            self.ledge.detour.stats.ticks = 1;
            self.ledge.detour.stats.phase = DetourPhase::Approaching;
        }
        let mut plan = self.ledge.detour.direct.unwrap();
        if self.ledge.detour.age > plan.landing + LANDING_GRACE_TICKS
            || self.door_panels()[0].is_none()
        {
            self.finish_flap_detour(Outcome::Expired);
            return None;
        }

        plan.contacts = if self.ledge.detour.launched && self.standing_on(flaps::body(0)) {
            plan.contacts + 1
        } else {
            0
        };
        self.ledge.detour.direct = Some(plan);
        if plan.contacts >= STABLE_CONTACT_TICKS {
            let motion = self.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
            plan.perch_x = motion.position.x;
            self.ledge.detour.direct = Some(plan);
            self.ledge.detour.stats.phase = DetourPhase::Perching;
            self.controller.direction = 1.0;
            return Some(self.direct_position_command(plan.perch_x, false));
        }
        let jump = self.ledge.detour.age == plan.launch;
        if jump {
            let landing = self.ledge.detour.landing.unwrap();
            if !self.grounded()
                || self
                    .predict_direct(plan)
                    .is_none_or(|p| !self.direct_route_clear(&p, landing))
            {
                self.finish_flap_detour(Outcome::Blocked);
                return None;
            }
            self.ledge.detour.launched = true;
        }
        let command = if self.ledge.detour.age < plan.approach {
            Command { jump, ..still() }
        } else {
            self.direct_position_command(plan.target_x, jump)
        };
        if self.ledge.detour.launched {
            let motion = self.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
            let mut velocity = motion.linear_velocity;
            let next = self.direct_flight_step(motion.position, &mut velocity, &command);
            if !self.direct_clear_segment(motion.position, next, self.ledge.detour.landing.unwrap())
            {
                self.finish_flap_detour(Outcome::Blocked);
                return None;
            }
        }
        Some(command)
    }

    pub(super) fn direct_position_command(&self, target: f32, jump: bool) -> Command {
        self.direct_command(
            self.world
                .as_ref()
                .unwrap()
                .motion(DUCK_BODY)
                .unwrap()
                .position,
            target,
            jump,
        )
    }

    fn direct_command(&self, position: Vec2, target: f32, jump: bool) -> Command {
        let distance = (target - position.x) * self.direction;
        Command {
            gait: Gait::Pace((distance.abs() * 16.0 / self.movement.run_speed).min(1.0)),
            direction: distance.signum(),
            jump,
        }
    }

    fn direct_support(&mut self, water: Option<&WaterWorld>) -> Option<Return> {
        let world = self.world.as_ref()?;
        let motion = world.motion(DUCK_BODY)?;
        let r = self.radius;
        let contact = world
            .surface_contacts(DUCK_COLLIDER)
            .find(|c| c.normal.y > 0.99 && c.separation <= r * 0.05)?;
        let body = world.collider_body(contact.collider)?;
        let pose = world.incoming_motion(body, DT)?;
        let (min, max) = world.body_solid_bounds(body)?;
        if !matches!(
            world.body_kind(body),
            Some(BodyKind::Fixed | BodyKind::KinematicPosition)
        ) || pose.angle.abs() > 0.001
            || motion.linear_velocity.length() > r * 0.1
            || !(5.7 * r..=6.3 * r).contains(&self.position()?.x)
        {
            return None;
        }
        let center = self.render_position(Vec2::new(r * 8.0, max.y + r));
        // Bounds alone do not prove that a compound floor has material here.
        self.ledge.detour.stats.clearance_queries += 1;
        let hit = world.cast_ball(
            center + Vec2::Y * (r * 3.0),
            r,
            -Vec2::Y,
            BallCastOptions {
                max_distance: r * 4.0,
                exclude_entity: Some(DUCK_ENTITY),
                ..Default::default()
            },
        )?;
        if world.collider_body(hit.collider) != Some(body)
            || hit.status != BallCastStatus::Converged
            || hit.normal.y < 0.99
            || (hit.distance - r * 3.0).abs() > r * 0.05
        {
            return None;
        }
        let landing = Return {
            center,
            body,
            pose: pose.position,
            angle: pose.angle,
        };
        (center.x > min.x + r * 2.0
            && center.x < max.x - r * 2.0
            && landing.valid(self)
            && landing.dry(water, r))
        .then_some(landing)
    }

    fn plan_direct_flap(&mut self, water: Option<&WaterWorld>) -> Option<(Plan, Return)> {
        let landing = self.direct_support(water)?;
        let panel = self.panel_at(0, 2.0 / 24.0);
        let half = panel.half_extents();
        // The upward cap near the hinge is reachable once the flap is almost
        // closed. Keep the center inside the cap rather than balancing on its tip.
        let local = Vec2::new(
            panel.inward * (half.x - self.radius * 0.25),
            half.y + self.radius,
        );
        let target_x = (panel.center() + local.rotate_radians(panel.angle)).x;
        for launch in SETTLE_TICKS + 1..=LAST_LAUNCH_TICK {
            for approach in 16..=30 {
                if launch <= self.ledge.detour.age {
                    continue;
                }
                let plan = Plan {
                    launch,
                    approach,
                    landing: 0,
                    target_x,
                    perch_x: target_x,
                    contacts: 0,
                };
                if let Some(prediction) = self.predict_direct(plan) {
                    return self
                        .direct_route_clear(&prediction, landing)
                        .then_some((prediction.plan, landing));
                }
            }
        }
        None
    }

    fn predict_direct(&self, mut plan: Plan) -> Option<Prediction> {
        let motion = self.world.as_ref()?.motion(DUCK_BODY)?;
        let mut position = motion.position;
        let mut velocity = motion.linear_velocity;
        let mut open = self.door_openness().0;
        let mut panel = self.door_panels()[0]?;
        let mut points = [Vec2::ZERO; MAX_SAMPLES];
        points[0] = position;
        let mut len = 1;
        for tick in self.ledge.detour.age..=LAST_LANDING_TICK {
            if tick > self.ledge.detour.age {
                let requested = (1.0 - tick.saturating_sub(20) as f32 / 24.0).max(0.0);
                let next_open = open + (requested - open).clamp(-1.0 / 24.0, 1.0 / 24.0);
                let next_open = if (next_open - requested).abs() < 1e-6 {
                    requested
                } else {
                    next_open
                };
                let next = self.panel_at(0, next_open);
                if !panel.sweep_touches(next, position, self.radius * 1.02) {
                    open = next_open;
                    panel = next;
                }
            }
            if open <= 0.0 {
                return None;
            }
            if tick < plan.launch {
                continue;
            }
            let command = if tick < plan.approach {
                Command {
                    gait: Gait::Still,
                    direction: -1.0,
                    jump: tick == plan.launch,
                }
            } else {
                self.direct_command(position, plan.target_x, tick == plan.launch)
            };
            position = self.direct_flight_step(position, &mut velocity, &command);
            points[len] = position;
            len += 1;
            let local = (position - panel.center()).rotate_radians(-panel.angle);
            let half = panel.half_extents();
            let nearest = Vec2::new(
                local.x.clamp(-half.x, half.x),
                local.y.clamp(-half.y, half.y),
            );
            let delta = local - nearest;
            if delta.length() <= self.radius * 1.005 {
                if delta.length() <= f32::EPSILON {
                    return None;
                }
                let normal = (delta / delta.length()).rotate_radians(panel.angle);
                // A slow, mostly upward contact is only a candidate. Real
                // contacts must settle before the perch timer starts; corner
                // glances or bounces cannot complete the detour.
                if local.y < half.y
                    || normal.y < 0.9
                    || velocity.dot(normal) >= 0.0
                    || velocity.length() > self.movement.run_speed * 0.85
                {
                    return None;
                }
                plan.landing = tick;
                plan.perch_x = position.x;
                return Some(Prediction {
                    plan,
                    points,
                    len,
                    perch: position,
                });
            }
        }
        None
    }

    fn direct_flight_step(&self, position: Vec2, velocity: &mut Vec2, command: &Command) -> Vec2 {
        // Rapier's ordinary four small steps integrate gravity before position.
        // Include that discrete fall rather than the continuous-flight arc;
        // the difference matters at the narrow closing-flap landing window.
        let steps = PhysicsWorldConfig::default().solver_iterations as f32;
        self.detour_flight_step(position, velocity, command)
            - Vec2::Y * (self.movement.gravity * DT * DT / (2.0 * steps))
    }

    fn direct_route_clear(&mut self, prediction: &Prediction, landing: Return) -> bool {
        let mut previous = prediction.points[0];
        for (i, point) in prediction.points[..prediction.len]
            .iter()
            .copied()
            .enumerate()
            .skip(1)
        {
            if i % 4 == 0 || i == prediction.len - 1 {
                if !self.direct_clear_segment(previous, point, landing) {
                    return false;
                }
                previous = point;
            }
        }
        let mut position = prediction.perch;
        let mut velocity = Vec2::ZERO;
        previous = position;
        for tick in 0..90 {
            let command = self.detour_return_command(position, landing, tick == 0);
            position = self.direct_flight_step(position, &mut velocity, &command);
            if tick % 4 == 3 || (velocity.y < 0.0 && position.y <= landing.center.y) {
                if !self.direct_clear_segment(previous, position, landing) {
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

    fn direct_clear_segment(&mut self, from: Vec2, to: Vec2, landing: Return) -> bool {
        let queries = &mut self.ledge.detour.stats.clearance_queries;
        if *queries >= geometry::MAX_QUERIES {
            return false;
        }
        *queries += 1;
        let delta = to - from;
        self.world
            .as_ref()
            .unwrap()
            .capsule_clearance_test_excluding(
                delta.length() * 0.5,
                self.radius * 1.03,
                CollisionGroups::ALL,
                vec![DUCK_ENTITY, flaps::body(0).entity, landing.body.entity],
            )(
            (from + to) * 0.5,
            delta.y.atan2(delta.x) - std::f32::consts::FRAC_PI_2,
        )
    }
}
