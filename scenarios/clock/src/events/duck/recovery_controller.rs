//! Recover actual footing after an impact, not the old jump's imaginary arc.
//! Bounded to the known course spans; no second physics world or debris graph.
use super::*;
use engine_common::ClockDuckRecoveryState;

#[derive(Default)]
pub(in crate::events::duck) struct Recovery {
    pub stats: ClockDuckRecoveryState,
    pub facing: Option<f32>,
    stable_ticks: u8,
    jumped: bool,
}

impl Recovery {
    pub fn clear(&mut self) {
        self.stats.active = false;
        self.stats.target_surface = None;
        self.facing = None;
        self.stable_ticks = 0;
        self.jumped = false;
    }
}

/// Displacement when accelerating from the observed speed toward a capped
/// speed. This includes braking/reversal, including externally imparted speed
/// above the duck's own maximum; it never clamps momentum instantaneously.
fn travel(velocity: f32, target: f32, time: f32, acceleration: f32) -> f32 {
    let ramp = ((target - velocity).abs() / acceleration).min(time);
    let end = velocity + (target - velocity).signum() * acceleration * ramp;
    (velocity + end) * 0.5 * ramp + end * (time - ramp)
}

fn landing_target(
    observed: Observation,
    context: CourseContext<'_>,
    caps: Capabilities,
    jump: bool,
) -> Option<(usize, f32)> {
    let gravity = 8.0 * caps.height / caps.flight.powi(2);
    let velocity = observed.velocity + observed.support_velocity;
    let vertical = if jump {
        4.0 * caps.height / caps.flight + observed.support_velocity.y
    } else {
        velocity.y
    };
    let acceleration = caps.acceleration * 0.85;
    context
        .course
        .surfaces
        .iter()
        .enumerate()
        .filter_map(|(index, surface)| {
            let (left, right) = surface.inside(context.radius)?;
            let rise = context.floor + surface.height + context.radius - observed.position.y;
            let discriminant = vertical * vertical - 2.0 * gravity * rise;
            if discriminant < 0.0 {
                return None;
            }
            let time = (vertical + discriminant.sqrt()) / gravity;
            if !(DT..=1.5).contains(&time) {
                return None;
            }
            let lo =
                observed.position.x + travel(velocity.x, -caps.speed * 0.9, time, acceleration);
            let hi = observed.position.x + travel(velocity.x, caps.speed * 0.9, time, acceleration);
            if hi < left || lo > right {
                return None;
            }
            let x = ((left + right) * 0.5).clamp(left.max(lo), right.min(hi));
            let stopped =
                observed.position.x + velocity.x * velocity.x.abs() / (2.0 * acceleration);
            Some((index, x, (x - stopped).abs()))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(index, x, _)| (index, x))
}

impl Controller {
    pub fn decide_recovery(
        &mut self,
        observed: Observation,
        context: CourseContext<'_>,
        caps: Capabilities,
    ) -> Option<Command> {
        if !self.recovery.stats.active {
            let in_flight = self.navigator.flight_tick.is_some();
            let wrong_landing = in_flight
                && self.navigator.airborne
                && observed.grounded
                && self
                    .navigator
                    .plan
                    .is_some_and(|p| observed.support != Some(p.target));
            if observed.grounded && observed.support.is_none() {
                self.recovery.stats.unstable_support += 1;
            } else if observed.blocked && in_flight {
                self.recovery.stats.interrupted_jumps += 1;
            } else if wrong_landing {
                self.recovery.stats.wrong_landings += 1;
            } else if !observed.grounded && !in_flight {
                self.recovery.stats.lost_support += 1;
            } else {
                return None;
            }
            if wrong_landing {
                self.navigator.record_miss(
                    observed.position.x,
                    self.navigator.plan.unwrap().landing.x,
                    self.direction,
                    context.radius,
                );
            }
            self.interrupt_route();
            self.recovery.stats.active = true;
        }
        self.behavior = ClockDuckBehavior::SeekingSupport;
        self.recovery.stats.ticks += 1;
        let radius = context.radius;
        let stable = observed.support.and_then(|index| {
            context.course.surfaces[index]
                .inside(radius)
                .map(|bounds| (index, bounds))
        });
        if let Some((_, (left, right))) = stable
            && !observed.blocked
            && observed.position.x >= left
            && observed.position.x <= right
            && observed.velocity.x.abs() < caps.speed * 0.15
            && observed.velocity.y.abs() < radius
        {
            self.recovery.stable_ticks += 1;
            if self.recovery.stable_ticks >= 3 {
                self.recovery.stats.recoveries += 1;
                self.recovery.clear();
                return None;
            }
        } else {
            self.recovery.stable_ticks = 0;
        }
        // A known fixed support is already a safe target. Otherwise use the
        // actual ballistic descent and capped horizontal reach, not the plan's
        // original launch velocity. Prefer a nearby landing over route progress.
        let mut target = stable
            .map(|(index, (left, right))| (index, (left + right) * 0.5))
            .or_else(|| landing_target(observed, context, caps, false));
        let mut jump = false;
        let sliding_off = stable.is_some_and(|(_, (left, right))| {
            let speed = observed.velocity.x;
            let stop = observed.position.x + speed * speed.abs() / (2.0 * caps.acceleration * 0.85);
            stop < left || stop > right
        });
        if (target.is_none() || sliding_off)
            && observed.grounded
            && !observed.blocked
            && !self.recovery.jumped
        {
            // Debris or a ledge we cannot brake on can be an emergency
            // foothold, once per recovery. No ceiling contact or midair jump.
            let escape = landing_target(observed, context, caps, true);
            if escape.is_some() {
                target = escape;
                jump = true;
                self.recovery.jumped = true;
                self.recovery.stats.escape_jumps += 1;
            }
        }
        self.recovery.stats.target_surface = target.map(|(index, _)| index);
        let Some((_, x)) = target else {
            self.recovery.stats.no_target_ticks += 1;
            return Some(Command {
                gait: Gait::Still,
                direction: self.direction,
                jump: false,
            });
        };
        let distance = x - observed.position.x;
        let speed = (2.0 * caps.acceleration * 0.65 * (distance.abs() - radius * 0.1).max(0.0))
            .sqrt()
            .min(caps.speed * 0.9);
        self.recovery.facing = Some(distance.signum());
        Some(Command {
            gait: Gait::Pace(speed / caps.speed),
            direction: distance.signum(),
            jump,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::ClockDuckCoursePattern;

    const OBSTACLES: [Obstacle; 3] = [Obstacle {
        start: 150.0,
        end: 180.0,
        height: 0.0,
    }; 3];

    fn course() -> Course {
        Course {
            surfaces: vec![
                planner::Surface {
                    start: -100.0,
                    end: 150.0,
                    height: 0.0,
                },
                planner::Surface {
                    start: 180.0,
                    end: 240.0,
                    height: 20.0,
                },
                planner::Surface {
                    start: 280.0,
                    end: 400.0,
                    height: 10.0,
                },
            ],
            pattern: ClockDuckCoursePattern::Platforms,
            attempts: 1,
            fallback: false,
        }
    }

    fn context(course: &Course) -> CourseContext<'_> {
        CourseContext {
            course,
            obstacles: &OBSTACLES,
            width: 400.0,
            radius: 8.0,
            floor: 0.0,
        }
    }

    fn calibrated() -> Controller {
        let mut controller = Controller::new();
        for _ in 0..2 {
            controller.heights.push(32.0);
            controller.flight_ticks.push(48.0);
        }
        for _ in 0..9 {
            controller.speeds.push(160.0);
            controller.accelerations.push(960.0);
        }
        controller
    }

    fn observation(x: f32, y: f32) -> Observation {
        Observation {
            position: Vec2::new(x, y),
            velocity: Vec2::ZERO,
            support_velocity: Vec2::ZERO,
            grounded: false,
            blocked: false,
            support: None,
        }
    }

    #[test]
    fn disrupted_running_jump_brakes_toward_real_support_then_resumes_personality() {
        for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
            let course = course();
            let context = context(&course);
            let mut controller = calibrated();
            controller.profile = profile;
            controller.direction = -1.0;
            let caps = controller.capabilities().unwrap();
            let plan = planner::plan(&course, 1, 0, context.radius, caps).unwrap();
            controller.navigator.plan = Some(Plan {
                running: true,
                ..plan
            });
            controller.navigator.flight_tick = Some(8);
            controller.navigator.airborne = true;
            let mut observed = observation(196.0, 55.0);
            observed.velocity = Vec2::new(-80.0, -180.0);
            // An undisturbed airborne observation leaves the original brain alone.
            assert!(
                controller
                    .decide_recovery(observed, context, caps)
                    .is_none()
            );
            observed.blocked = true;
            let command = controller.decide_recovery(observed, context, caps).unwrap();
            assert!(!command.jump);
            let delta = Movement::new(800.0, context.radius).velocity_delta(observed, &command);
            assert!(delta.x > 0.0, "brake toward the source, not the old target");
            assert!(delta.x <= caps.acceleration * DT);
            assert_eq!(controller.recovery.stats.target_surface, Some(1));
            assert_eq!(controller.recovery.stats.interrupted_jumps, 1);
            assert!(
                controller.navigator.plan.is_none() && controller.navigator.flight_tick.is_none()
            );
            assert_eq!(controller.behavior, ClockDuckBehavior::SeekingSupport);

            observed = observation(210.0, 28.0);
            observed.grounded = true;
            observed.support = Some(1);
            for _ in 0..2 {
                assert!(
                    !controller
                        .decide_recovery(observed, context, caps)
                        .unwrap()
                        .jump
                );
            }
            assert!(
                controller
                    .decide_recovery(observed, context, caps)
                    .is_none()
            );
            assert_eq!(controller.recovery.stats.recoveries, 1);
            assert!(!controller.recovery.stats.active);
            assert_eq!(controller.profile, profile);
            assert_eq!(controller.direction, -1.0);
            assert_eq!(controller.heights.median(), Some(32.0));
            assert_eq!(controller.speeds.median(), Some(160.0));
        }
    }

    #[test]
    fn loose_foothold_allows_one_escape_jump_but_never_a_midair_or_ceiling_jump() {
        let course = course();
        let context = context(&course);
        let mut controller = calibrated();
        let caps = controller.capabilities().unwrap();
        let mut observed = observation(164.0, 8.0);
        observed.grounded = true;
        observed.blocked = true;
        assert!(
            !controller
                .decide_recovery(observed, context, caps)
                .unwrap()
                .jump
        );
        observed.blocked = false;
        observed.grounded = false;
        assert!(
            !controller
                .decide_recovery(observed, context, caps)
                .unwrap()
                .jump
        );
        observed.grounded = true;
        assert!(
            controller
                .decide_recovery(observed, context, caps)
                .unwrap()
                .jump
        );
        for _ in 0..10 {
            assert!(
                !controller
                    .decide_recovery(observed, context, caps)
                    .unwrap()
                    .jump
            );
        }
        assert_eq!(controller.recovery.stats.escape_jumps, 1);
        assert_eq!(controller.recovery.stats.unstable_support, 1);
    }

    #[test]
    fn unreachable_support_is_not_a_midair_rescue_and_water_preempts_recovery() {
        let course = course();
        let context = context(&course);
        let mut controller = calibrated();
        let caps = controller.capabilities().unwrap();
        let mut observed = observation(165.0, -30.0);
        observed.velocity = Vec2::new(-250.0, -100.0);
        let command = controller.decide_recovery(observed, context, caps).unwrap();
        assert!(!command.jump && matches!(command.gait, Gait::Still));
        assert!(controller.recovery.stats.target_surface.is_none());
        assert_eq!(controller.recovery.stats.no_target_ticks, 1);
        assert!(
            !controller
                .decide_water(observed, 0.5, context)
                .unwrap()
                .jump
        );
        assert!(!controller.recovery.stats.active);
        assert!(controller.recovery.facing.is_none());
        assert_eq!(controller.behavior, ClockDuckBehavior::Paddling);
        assert_eq!(controller.recovery.stats.lost_support, 1);
    }

    #[test]
    fn reach_accounts_for_momentum_braking_and_moving_support() {
        assert!((travel(-200.0, 100.0, 0.1, 100.0) + 19.5).abs() < 0.001);
        assert!((travel(0.0, 100.0, 2.0, 100.0) - 150.0).abs() < 0.001);
        let course = course();
        let context = context(&course);
        let controller = calibrated();
        let caps = controller.capabilities().unwrap();
        let mut observed = observation(210.0, 60.0);
        observed.velocity = Vec2::new(100.0, -50.0);
        let airborne = landing_target(observed, context, caps, false);
        observed.support_velocity = observed.velocity;
        observed.velocity = Vec2::ZERO;
        assert_eq!(landing_target(observed, context, caps, false), airborne);
    }
}
