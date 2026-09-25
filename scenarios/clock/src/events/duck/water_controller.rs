//! A bounded interruption of dry navigation, not a second physics model.
//! Paddle toward the interior of a reachable bank, ride out the water, then
//! resume planning only after actual dry support. Do not calibrate in water.
use super::*;
use engine_common::ClockDuckWaterState;

#[derive(Default)]
pub(in crate::events::duck) struct WaterNavigation {
    active: bool,
    dry_ground_ticks: u8,
    facing: Option<f32>,
    pub stats: ClockDuckWaterState,
}

impl Controller {
    pub fn facing(&self) -> f32 {
        self.water
            .facing
            .or(self.recovery.facing)
            .unwrap_or(self.direction)
    }

    pub fn decide_water(
        &mut self,
        observed: Observation,
        submerged: f64,
        context: CourseContext<'_>,
    ) -> Option<Command> {
        let wet = submerged >= 0.15;
        if !self.water.active {
            if !wet {
                return None;
            }
            self.interrupt_route();
            self.recovery.clear();
            self.water.active = true;
            self.water.facing = Some(self.direction);
            self.water.stats.interruptions += 1;
        }
        // Hysteresis prevents ripples and splash contacts from alternately
        // launching a jump and cancelling it on consecutive simulation ticks.
        if submerged < 0.06 && observed.grounded {
            self.water.dry_ground_ticks += 1;
            if self.water.dry_ground_ticks >= 12 {
                self.water.active = false;
                self.water.facing = None;
                self.water.dry_ground_ticks = 0;
                self.water.stats.recoveries += 1;
                self.water.stats.target_surface = None;
                self.interrupt_route();
                self.behavior = ClockDuckBehavior::WarmingUp;
                return None;
            }
        } else {
            self.water.dry_ground_ticks = 0;
        }
        self.behavior = if wet {
            self.water.stats.paddling_ticks += 1;
            ClockDuckBehavior::Paddling
        } else {
            self.water.stats.recovering_ticks += 1;
            ClockDuckBehavior::Recovering
        };

        let CourseContext {
            course,
            radius,
            floor,
            ..
        } = context;
        // At most seven spans, no allocation or trajectory search. Do not try
        // to swim up an unreachable vertical bank or intentionally cross a dry
        // gap. Prefer the bank already beneath us, otherwise the nearest one.
        let target = course
            .surfaces
            .iter()
            .enumerate()
            .filter_map(|(index, surface)| {
                let (left, right) = surface.inside(radius)?;
                if floor + surface.height > observed.position.y - radius * 0.5 {
                    return None;
                }
                let distance = (observed.position.x.clamp(left, right) - observed.position.x).abs();
                Some((index, (left + right) * 0.5, distance))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        self.water.stats.target_surface = target.map(|(index, _, _)| index);
        let axis = target.map_or(0.0, |(_, x, _)| {
            let speed = ((x - observed.position.x) * 2.0).clamp(-radius * 4.0, radius * 4.0);
            ((speed - observed.velocity.x) / (radius * 4.0)).clamp(-0.8, 0.8)
        });
        if axis.abs() > 0.1 {
            self.water.facing = Some(axis.signum());
        }
        // The actuator applies the same capped paddle acceleration as player
        // input. Buoyancy and current still determine the actual trajectory.
        Some(Command {
            gait: Gait::Pace(axis.abs()),
            direction: axis.signum(),
            jump: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::ClockDuckCoursePattern;

    #[test]
    fn wet_interruption_keeps_clean_measurements_rejects_high_banks_and_waits_for_support() {
        let mut controller = Controller::new();
        for _ in 0..2 {
            controller.heights.push(10.0);
            controller.flight_ticks.push(40.0);
        }
        for _ in 0..9 {
            controller.speeds.push(40.0);
            controller.accelerations.push(200.0);
        }
        controller.navigator.flight_tick = Some(20);
        controller.navigator.airborne = true;
        controller.trial = Some(JumpTrial {
            start: Vec2::ZERO,
            peak: 10.0,
            ticks: 20,
            airborne: true,
            clean: true,
        });
        let course = Course {
            surfaces: vec![
                planner::Surface {
                    start: 0.0,
                    end: 100.0,
                    height: 0.0,
                },
                planner::Surface {
                    start: 110.0,
                    end: 200.0,
                    height: 100.0,
                },
            ],
            pattern: ClockDuckCoursePattern::Platforms,
            attempts: 1,
            fallback: false,
        };
        let obstacles = [Obstacle {
            start: 100.0,
            end: 110.0,
            height: 0.0,
        }; 3];
        let context = CourseContext {
            course: &course,
            obstacles: &obstacles,
            width: 200.0,
            radius: 2.0,
            floor: 0.0,
        };
        let mut observed = Observation {
            support_velocity: Vec2::ZERO,
            position: Vec2::new(115.0, 5.0),
            velocity: Vec2::new(50.0, 0.0),
            grounded: false,
            blocked: true,
            support: None,
        };
        let command = controller.decide_water(observed, 0.5, context).unwrap();
        assert!(!command.jump);
        assert!(
            command.direction < 0.0,
            "paddle toward the low bank, not into the high wall"
        );
        assert_eq!(controller.water.stats.target_surface, Some(0));
        assert!(controller.trial.is_none() && controller.navigator.flight_tick.is_none());
        assert!(controller.navigator.plan.is_none());
        // Intermittent immersion or loss of water while still airborne must
        // never restart the interrupted jump plan/calibration trial.
        for fraction in [0.0, 0.16, 0.08, 0.14, 0.0] {
            assert!(
                !controller
                    .decide_water(observed, fraction, context)
                    .unwrap()
                    .jump
            );
        }
        assert_eq!(controller.water.stats.interruptions, 1);
        assert_eq!(controller.water.stats.recoveries, 0);
        observed.grounded = true;
        observed.position = Vec2::new(50.0, 2.0);
        for _ in 0..11 {
            assert!(
                !controller
                    .decide_water(observed, 0.0, context)
                    .unwrap()
                    .jump
            );
        }
        assert!(controller.decide_water(observed, 0.0, context).is_none());
        assert_eq!(controller.water.stats.recoveries, 1);
        assert_eq!(controller.heights.median(), Some(10.0));
        assert_eq!(controller.flight_ticks.median(), Some(40.0));
        assert_eq!(controller.speeds.median(), Some(40.0));
        assert_eq!(controller.accelerations.median(), Some(200.0));
    }
}
