//! A persistent departure goal, applied only after committed movement finishes.
use super::*;

// A nearly finished wall tag gets at most three quarters of a second. Use the
// visit clock so water, ledges and detours cannot extend or renew the allowance.
const WALL_GRACE_TICKS: u64 = 45;

#[derive(Default)]
pub(super) struct ExitIntent {
    pub visible: bool,
    tick: u64,
    wall_until: Option<u64>,
}

impl Controller {
    pub fn notice_exit(&mut self, tick: u64, near_opposite_wall: bool) {
        if !self.exit.visible {
            self.exit.visible = true;
            self.exit.wall_until =
                (near_opposite_wall && self.direction < 0.0).then_some(tick + WALL_GRACE_TICKS);
        }
        self.exit.tick = tick;
    }

    pub(super) fn prioritize_exit(&mut self, observed: Observation, radius: f32) {
        if !self.exit.visible
            || self.direction > 0.0
            || !observed.grounded
            // A takeoff can retain last solve's ground contact for one frame.
            || observed.velocity.y > radius * 0.1
            || self.trial.is_some()
            || self.navigator.flight_tick.is_some()
            || self
                .exit
                .wall_until
                .is_some_and(|until| self.exit.tick < until)
        {
            return;
        }
        // Discard only an uncommitted route. The course controller calls this
        // after recovery and landing confirmation, using the real support.
        self.interrupt_route();
        self.direction = 1.0;
        self.cleared = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::ClockDuckCoursePattern;

    const OBSTACLES: [Obstacle; 3] = [Obstacle {
        start: 230.0,
        end: 350.0,
        height: 16.0,
    }; 3];

    fn calibrated() -> Controller {
        let mut controller = Controller::new();
        for _ in 0..2 {
            controller.heights.push(35.6);
            controller.flight_ticks.push(52.0);
        }
        for _ in 0..9 {
            controller.speeds.push(160.0);
            controller.accelerations.push(960.0);
        }
        controller.direction = -1.0;
        controller
    }

    fn observed(x: f32, y: f32, support: Option<usize>) -> Observation {
        Observation {
            position: Vec2::new(x, y),
            velocity: Vec2::new(-60.0, 0.0),
            support_velocity: Vec2::ZERO,
            debris: None,
            grounded: support.is_some(),
            blocked: false,
            support,
        }
    }

    fn course() -> Course {
        Course {
            surfaces: vec![
                planner::Surface {
                    start: 0.0,
                    end: 200.0,
                    height: 0.0,
                },
                planner::Surface {
                    start: 230.0,
                    end: 350.0,
                    height: 16.0,
                },
                planner::Surface {
                    start: 380.0,
                    end: 800.0,
                    height: 0.0,
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
            width: 800.0,
            radius: 8.0,
            floor: 0.0,
        }
    }

    #[test]
    fn exit_brakes_a_grounded_runner_but_preserves_airborne_direction() {
        let mut controller = calibrated();
        controller.notice_exit(100, false);
        let mut launching = observed(710.0, 8.0, Some(0));
        launching.velocity.y = 150.0;
        let command = controller.decide(launching, &OBSTACLES, 800.0, 8.0);
        assert_eq!(
            command.direction, -1.0,
            "lingering takeoff contact is not safe footing"
        );
        assert!(!command.jump);
        let airborne = observed(700.0, 50.0, None);
        let command = controller.decide(airborne, &OBSTACLES, 800.0, 8.0);
        assert_eq!(command.direction, -1.0);
        assert!(!command.jump);
        let grounded = observed(680.0, 8.0, Some(0));
        let command = controller.decide(grounded, &OBSTACLES, 800.0, 8.0);
        assert_eq!(command.direction, 1.0);
        assert!(!command.jump);
        let delta = Movement::new(800.0, 8.0).velocity_delta(grounded, &command);
        assert!(delta.x > 0.0 && delta.x <= 16.0);
        assert!(grounded.velocity.x + delta.x < 0.0, "braking takes time");
        for tick in 101..160 {
            controller.notice_exit(tick, true);
            let command = controller.decide(observed(790.0, 8.0, Some(0)), &OBSTACLES, 800.0, 8.0);
            assert_eq!(command.direction, 1.0, "no new wall turn or renewed grace");
        }
    }

    #[test]
    fn course_replaces_only_uncommitted_routes_for_both_profiles() {
        for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
            let mut controller = calibrated();
            controller.profile = profile;
            let course = course();
            controller.navigator.plan = Some(
                planner::plan(&course, 1, 0, 8.0, controller.capabilities().unwrap()).unwrap(),
            );
            controller.notice_exit(100, false);
            controller.decide_course(observed(280.0, 24.0, Some(1)), context(&course));
            assert_eq!(controller.direction, 1.0);
            assert_eq!(controller.navigator.plan.unwrap().target, 2);
            controller.notice_exit(101, false);
            controller.decide_course(observed(280.0, 24.0, Some(1)), context(&course));
            assert_eq!(controller.navigator.plan.unwrap().target, 2);
            assert_eq!(controller.navigator.wrong_surface, 0);
        }
    }

    #[test]
    fn committed_course_flight_confirms_landing_before_reversing() {
        for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
            let mut controller = calibrated();
            controller.profile = profile;
            let course = course();
            let plan =
                planner::plan(&course, 1, 0, 8.0, controller.capabilities().unwrap()).unwrap();
            controller.navigator.plan = Some(plan);
            controller.navigator.flight_tick = Some(5);
            controller.notice_exit(100, false);
            let command = controller.decide_course(observed(220.0, 50.0, None), context(&course));
            assert!(!command.jump);
            assert_eq!(controller.direction, -1.0);
            assert_eq!(controller.navigator.plan.unwrap().target, 0);
            let landing = observed(plan.landing.x, 8.0, Some(0));
            let command = controller.decide_course(landing, context(&course));
            assert!(!command.jump);
            assert_eq!(controller.navigator.confirmed, 1);
            assert_eq!(controller.direction, -1.0);
            controller.decide_course(landing, context(&course));
            assert_eq!(controller.direction, 1.0);
            assert_eq!(controller.navigator.plan.unwrap().target, 1);
        }
    }

    #[test]
    fn wall_grace_ends_on_a_tag_or_timeout_and_cannot_be_renewed() {
        for tagged in [false, true] {
            for platforms in [false, true] {
                let mut controller = calibrated();
                let course = course();
                controller.notice_exit(100, true);
                for elapsed in 0..=WALL_GRACE_TICKS + 5 {
                    controller.notice_exit(100 + elapsed, true);
                    let mut observation = observed(
                        if tagged && elapsed >= 3 { 24.0 } else { 32.0 },
                        8.0,
                        Some(0),
                    );
                    observation.velocity = Vec2::ZERO;
                    if platforms {
                        controller.decide_course(observation, context(&course));
                    } else {
                        controller.decide(observation, &OBSTACLES, 800.0, 8.0);
                    }
                    let turned = elapsed >= if tagged { 3 } else { WALL_GRACE_TICKS };
                    assert_eq!(controller.direction, if turned { 1.0 } else { -1.0 });
                    assert_eq!(controller.wall_tags[0], u32::from(tagged && elapsed >= 3));
                }
            }
        }
    }

    #[test]
    fn water_and_recovery_keep_control_until_safe_support() {
        let course = course();
        let context = context(&course);
        let mut controller = calibrated();
        controller.notice_exit(100, true);
        let airborne = observed(32.0, 16.0, None);
        assert!(controller.decide_water(airborne, 0.5, context).is_some());
        controller.notice_exit(200, true);
        let dry = observed(80.0, 8.0, Some(0));
        for _ in 0..11 {
            assert!(controller.decide_water(dry, 0.0, context).is_some());
            assert_eq!(controller.direction, -1.0);
        }
        assert!(controller.decide_water(dry, 0.0, context).is_none());
        // An unplanned fall still belongs to recovery after the water clears.
        controller.decide_course(airborne, context);
        assert!(controller.recovery.stats.active);
        assert_eq!(controller.direction, -1.0);
        let mut settled = dry;
        settled.velocity = Vec2::ZERO;
        for _ in 0..3 {
            controller.decide_course(settled, context);
        }
        assert!(!controller.recovery.stats.active);
        assert_eq!(controller.direction, 1.0);
    }
}
