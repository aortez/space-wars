//! Small, deterministic controller. Decisions use observed motion, not the
//! actuator's configured top speed or jump impulse. No path search per frame.

use engine_common::{ClockDuckBehavior, ClockDuckJumpProfile};
use engine_core::Vec2;

use super::flow;
use super::planner::{self, Capabilities, Course, Plan};
use super::{DT, Obstacle};

#[derive(Clone, Copy)]
pub(super) struct Observation {
    pub position: Vec2,
    pub velocity: Vec2,
    pub grounded: bool,
    pub blocked: bool,
    pub support: Option<usize>,
}

#[derive(Clone, Copy)]
pub(super) enum Gait {
    Still,
    Walk,
    Run,
    Pace(f32),
}

pub(super) struct Command {
    pub gait: Gait,
    pub direction: f32,
    pub jump: bool,
}

pub(super) struct CourseContext<'a> {
    pub course: &'a Course,
    pub obstacles: &'a [Obstacle; 3],
    pub width: f32,
    pub radius: f32,
    pub floor: f32,
}

/// Actuator tuning lives separately from the controller's learned capabilities.
pub(super) struct Movement {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub gravity: f32,
    pub jump_height: f32,
}

impl Movement {
    pub fn new(width: f32, radius: f32) -> Self {
        Self {
            walk_speed: width / 7.0,
            run_speed: width / 5.0,
            gravity: radius * 50.0,
            jump_height: radius * 4.5,
        }
    }

    pub fn velocity_delta(&self, observed: Observation, command: &Command) -> Vec2 {
        let target = command.direction
            * match command.gait {
                Gait::Still => 0.0,
                Gait::Walk => self.walk_speed,
                Gait::Run => self.run_speed,
                Gait::Pace(fraction) => self.run_speed * fraction.clamp(0.0, 1.0),
            };
        Vec2::new(
            (target - observed.velocity.x)
                .clamp(-self.run_speed * DT * 6.0, self.run_speed * DT * 6.0),
            if command.jump && observed.grounded {
                (2.0 * self.gravity * self.jump_height).sqrt() - observed.velocity.y
            } else {
                0.0
            },
        )
    }
}

/// Fixed-size rolling median: a transient spike cannot become a permanent max.
#[derive(Default)]
pub(super) struct Samples {
    values: [f32; 9],
    pub count: usize,
    next: usize,
}

impl Samples {
    pub fn push(&mut self, value: f32) {
        if !value.is_finite() || value <= 0.0 {
            return;
        }
        self.values[self.next] = value;
        self.next = (self.next + 1) % self.values.len();
        self.count = (self.count + 1).min(self.values.len());
    }

    pub fn median(&self) -> Option<f32> {
        if self.count == 0 {
            return None;
        }
        let mut values = self.values;
        values[..self.count].sort_by(f32::total_cmp);
        // With two warm-up jumps, use the conservative lower observation.
        Some(values[(self.count - 1) / 2])
    }
}

struct JumpTrial {
    start: Vec2,
    peak: f32,
    ticks: u32,
    airborne: bool,
    clean: bool,
}

pub(super) struct Controller {
    pub profile: ClockDuckJumpProfile,
    pub behavior: ClockDuckBehavior,
    pub direction: f32,
    /// Entrance-relative left/right, transformed for public diagnostics.
    pub wall_tags: [u32; 2],
    pub target_obstacle: Option<usize>,
    pub cleared: usize,
    pub heights: Samples,
    pub flight_ticks: Samples,
    pub speeds: Samples,
    pub accelerations: Samples,
    pub navigator: Navigator,
    trial: Option<JumpTrial>,
    previous_speed: f32,
    jumped: u8,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            profile: ClockDuckJumpProfile::Careful,
            behavior: ClockDuckBehavior::WarmingUp,
            direction: 1.0,
            wall_tags: [0; 2],
            target_obstacle: None,
            cleared: 0,
            heights: Samples::default(),
            flight_ticks: Samples::default(),
            speeds: Samples::default(),
            accelerations: Samples::default(),
            navigator: Navigator::default(),
            trial: None,
            previous_speed: 0.0,
            jumped: 0,
        }
    }

    fn observe_jump(&mut self, observed: Observation, radius: f32) {
        let Some(trial) = &mut self.trial else { return };
        trial.ticks += 1;
        trial.peak = trial.peak.max(observed.position.y);
        trial.airborne |= !observed.grounded;
        trial.clean &=
            !observed.blocked && (observed.position.x - trial.start.x).abs() < radius * 0.25;
        if trial.airborne && observed.grounded && observed.velocity.y <= radius * 0.1 {
            if trial.clean && (observed.position.y - trial.start.y).abs() < radius * 0.25 {
                self.heights.push(trial.peak - trial.start.y);
                self.flight_ticks.push(trial.ticks as f32);
            }
            self.trial = None;
        }
    }

    pub fn decide(
        &mut self,
        observed: Observation,
        obstacles: &[Obstacle; 3],
        width: f32,
        radius: f32,
        exit_visible: bool,
    ) -> Command {
        self.observe_jump(observed, radius);
        let mut command = Command {
            gait: Gait::Still,
            direction: self.direction,
            jump: false,
        };
        if self.heights.count < 2 {
            self.behavior = ClockDuckBehavior::WarmingUp;
            if self.trial.is_none() && observed.grounded {
                self.trial = Some(JumpTrial {
                    start: observed.position,
                    peak: observed.position.y,
                    ticks: 0,
                    airborne: false,
                    clean: true,
                });
                command.jump = true;
            }
            return command;
        }

        // Sample sustained, unobstructed grounded travel, not acceleration,
        // braking, flight or wall impacts. Keep adapting once up to speed.
        let speed = observed.velocity.x.abs();
        if self.behavior == ClockDuckBehavior::MeasuringRun
            && observed.grounded
            && !observed.blocked
            && speed - self.previous_speed > radius * 0.05
        {
            self.accelerations.push((speed - self.previous_speed) / DT);
        }
        if matches!(
            self.behavior,
            ClockDuckBehavior::MeasuringRun | ClockDuckBehavior::Running
        ) && observed.grounded
            && !observed.blocked
            && speed > radius
            && (speed - self.previous_speed).abs() < speed * 0.01
            && observed.velocity.x * self.direction > 0.0
            && observed.position.x > width * 0.12
            && observed.position.x < width * 0.88
        {
            self.speeds.push(speed);
        }
        self.previous_speed = speed;
        self.behavior = if self.speeds.count < 9 {
            ClockDuckBehavior::MeasuringRun
        } else {
            ClockDuckBehavior::Running
        };
        command.gait = Gait::Run;

        // Tag a safe line just inside either edge. Reverse the requested motion
        // through the acceleration limiter, never reflect/teleport the body.
        let wall = if self.direction > 0.0 {
            width - radius * 3.0
        } else {
            radius * 3.0
        };
        let distance = (wall - observed.position.x) * self.direction;
        if !(exit_visible && self.direction > 0.0) {
            if distance <= 0.0 && observed.grounded {
                self.wall_tags[usize::from(self.direction > 0.0)] += 1;
                self.direction = -self.direction;
                self.jumped = 0;
                self.cleared = 0;
            } else if distance < width * 0.06 {
                command.gait = Gait::Walk;
            }
        }
        command.direction = self.direction;
        if observed.velocity.x * self.direction < 0.0 {
            self.behavior = ClockDuckBehavior::Turning;
        } else if exit_visible && self.direction > 0.0 {
            self.behavior = ClockDuckBehavior::Exiting;
        }

        self.target_obstacle = None;
        self.cleared = 0;
        for offset in 0..obstacles.len() {
            let index = if self.direction > 0.0 {
                offset
            } else {
                obstacles.len() - 1 - offset
            };
            let obstacle = obstacles[index];
            let (near, far) = if self.direction > 0.0 {
                (obstacle.start, obstacle.end)
            } else {
                (obstacle.end, obstacle.start)
            };
            if (observed.position.x - far) * self.direction > radius {
                self.cleared += 1;
                continue;
            }
            self.target_obstacle = Some(index);
            // Retained hurdle/pit baseline: use observed flight time to choose
            // a lead. Platform courses use this controller only for calibration
            // on the clear runway, then switch to their landing planner.
            if let (Some(run_speed), Some(flight)) =
                (self.speeds.median(), self.flight_ticks.median())
            {
                let lead = run_speed * flight * DT * 0.22 + radius;
                if observed.grounded
                    && observed.velocity.x * self.direction > 0.0
                    && self.jumped & (1 << index) == 0
                    && (near - observed.position.x) * self.direction <= lead
                {
                    command.jump = true;
                    self.jumped |= 1 << index;
                }
            }
            break;
        }
        command
    }
}

#[derive(Default)]
pub(super) struct Navigator {
    pub running_jumps: u32,
    pub flowing_fallbacks: u32,
    pub moving_landings: u32,
    pub plan: Option<Plan>,
    pub flight_tick: Option<u32>,
    pub confirmed: u32,
    pub undershoots: u32,
    pub overshoots: u32,
    pub wrong_surface: u32,
    pub rejected: u32,
    pub reason: Option<planner::Rejection>,
    airborne: bool,
    retry: u32,
}

impl Controller {
    pub fn capabilities(&self) -> Option<Capabilities> {
        Some(Capabilities {
            height: self.heights.median()?,
            flight: self.flight_ticks.median()? * DT,
            speed: self.speeds.median()?,
            acceleration: self.accelerations.median()?,
        })
    }

    pub fn decide_course(
        &mut self,
        observed: Observation,
        context: CourseContext<'_>,
        exit_visible: bool,
    ) -> Command {
        let CourseContext {
            course,
            obstacles,
            width,
            radius,
            floor,
        } = context;
        if self.heights.count < 2 || self.speeds.count < 9 {
            return self.decide(observed, obstacles, width, radius, exit_visible);
        }
        let Some(capabilities) = self.capabilities() else {
            self.behavior = ClockDuckBehavior::Blocked;
            return Command {
                gait: Gait::Still,
                direction: self.direction,
                jump: false,
            };
        };
        let pace = |target: f32, maximum: f32| {
            let distance = target - observed.position.x;
            let speed =
                (2.0 * capabilities.acceleration * 0.65 * (distance.abs() - radius * 0.1).max(0.0))
                    .sqrt()
                    .min(maximum);
            Command {
                gait: Gait::Pace(speed / capabilities.speed),
                direction: distance.signum(),
                jump: false,
            }
        };
        if let (Some(plan), Some(tick)) = (self.navigator.plan, self.navigator.flight_tick) {
            self.behavior = ClockDuckBehavior::Jumping;
            self.navigator.flight_tick = Some(tick + 1);
            self.navigator.airborne |= !observed.grounded;
            if self.navigator.airborne && observed.grounded && observed.velocity.y <= radius * 0.1 {
                let target = course.surfaces[plan.target];
                let (left, right) = target.inside(radius).expect("planned landing interval");
                if observed.support == Some(plan.target)
                    && observed.position.x >= left - radius * 0.25
                    && observed.position.x <= right + radius * 0.25
                {
                    self.navigator.confirmed += 1;
                    if observed.velocity.x.abs() > capabilities.speed * 0.15 {
                        self.navigator.moving_landings += 1;
                    }
                    self.behavior = ClockDuckBehavior::Landing;
                } else {
                    let distance = (observed.position.x - plan.landing.x) * self.direction;
                    if distance < -radius {
                        self.navigator.undershoots += 1;
                    } else if distance > radius {
                        self.navigator.overshoots += 1;
                    } else {
                        self.navigator.wrong_surface += 1;
                    }
                }
                self.navigator.plan = None;
                self.navigator.flight_tick = None;
                self.navigator.airborne = false;
                // One grounded frame separates landing confirmation from the
                // next takeoff. The next decision uses the actual support.
                return if plan.running {
                    Command {
                        gait: Gait::Pace(plan.cruise / capabilities.speed),
                        direction: self.direction,
                        jump: false,
                    }
                } else {
                    pace(plan.landing.x, capabilities.speed * 0.4)
                };
            }
            return if plan.running {
                Command {
                    gait: Gait::Pace(plan.cruise / capabilities.speed),
                    direction: self.direction,
                    jump: false,
                }
            } else {
                pace(plan.landing.x, plan.cruise)
            };
        }
        let Some(source) = observed.support else {
            self.behavior = ClockDuckBehavior::Blocked;
            return Command {
                gait: Gait::Still,
                direction: self.direction,
                jump: false,
            };
        };
        let last = course.surfaces.len() - 1;
        self.cleared = if self.direction > 0.0 {
            source
        } else {
            last - source
        };
        if (self.direction > 0.0 && source == last) || (self.direction < 0.0 && source == 0) {
            let wall = if self.direction > 0.0 {
                width - radius * 3.0
            } else {
                radius * 3.0
            };
            if exit_visible && self.direction > 0.0 {
                self.behavior = ClockDuckBehavior::Exiting;
                return Command {
                    gait: Gait::Run,
                    direction: self.direction,
                    jump: false,
                };
            }
            if (observed.position.x - wall).abs() < radius * 0.3
                && observed.velocity.x.abs() < radius
            {
                self.wall_tags[usize::from(self.direction > 0.0)] += 1;
                self.direction = -self.direction;
                self.navigator.plan = None;
                self.cleared = 0;
            } else {
                self.behavior = ClockDuckBehavior::Turning;
                return pace(wall, capabilities.speed);
            }
        }
        if self.navigator.retry > 0 {
            self.navigator.retry -= 1;
            self.behavior = ClockDuckBehavior::Blocked;
            return Command {
                gait: Gait::Still,
                direction: self.direction,
                jump: false,
            };
        }
        if self.navigator.plan.is_none() {
            let target = if self.direction > 0.0 {
                source + 1
            } else {
                source - 1
            };
            self.target_obstacle = Some(target);
            let running = if self.profile == ClockDuckJumpProfile::Flowing {
                let plan = flow::plan(
                    course,
                    flow::Start {
                        surface: source,
                        x: observed.position.x,
                        velocity: observed.velocity.x,
                        direction: self.direction,
                    },
                    radius,
                    capabilities,
                );
                if plan.is_none() {
                    self.navigator.flowing_fallbacks += 1;
                }
                plan
            } else {
                None
            };
            match running
                .map(Ok)
                .unwrap_or_else(|| planner::plan(course, source, target, radius, capabilities))
            {
                Ok(plan) => {
                    self.navigator.plan = Some(plan);
                    self.navigator.reason = None;
                }
                Err(reason) => {
                    self.navigator.reason = Some(reason);
                    self.navigator.rejected += 1;
                    self.navigator.retry = 30;
                    self.behavior = ClockDuckBehavior::Blocked;
                    return Command {
                        gait: Gait::Still,
                        direction: self.direction,
                        jump: false,
                    };
                }
            }
        }
        let plan = self.navigator.plan.expect("accepted route");
        // A slip while approaching invalidates the source contract.
        if source != plan.source {
            self.navigator.plan = None;
            self.navigator.wrong_surface += 1;
            return Command {
                gait: Gait::Still,
                direction: self.direction,
                jump: false,
            };
        }
        self.behavior = ClockDuckBehavior::Approaching;
        if plan.running {
            let remaining = (plan.takeoff.x - observed.position.x) * self.direction;
            let ready = (observed.velocity.x * self.direction - plan.cruise).abs()
                <= capabilities.acceleration * DT * 0.85;
            if remaining <= plan.cruise * DT {
                if ready
                    && let Some(actual) =
                        flow::at_takeoff(course, plan, observed.position.x, radius, capabilities)
                {
                    self.navigator.plan = Some(actual);
                    self.navigator.flight_tick = Some(0);
                    self.navigator.airborne = false;
                    self.navigator.running_jumps += 1;
                    self.behavior = ClockDuckBehavior::Jumping;
                    return Command {
                        gait: Gait::Pace(plan.cruise / capabilities.speed),
                        direction: self.direction,
                        jump: true,
                    };
                }
                // The candidate reserved space to brake on the source too.
                // Abort before stepping off it; a careful plan starts next tick.
                self.navigator.flowing_fallbacks += 1;
                self.navigator.plan =
                    planner::plan(course, source, plan.target, radius, capabilities).ok();
                return Command {
                    gait: Gait::Still,
                    direction: self.direction,
                    jump: false,
                };
            }
            return Command {
                gait: Gait::Pace(plan.cruise / capabilities.speed),
                direction: self.direction,
                jump: false,
            };
        }
        let mut command = pace(plan.takeoff.x, capabilities.speed);
        if (observed.position.x - plan.takeoff.x).abs() < radius * 0.3
            && observed.velocity.x.abs() < radius * 0.5
            && (observed.position.y - radius - floor - plan.takeoff.y).abs() < radius * 0.2
        {
            self.navigator.flight_tick = Some(0);
            self.navigator.airborne = false;
            self.behavior = ClockDuckBehavior::Jumping;
            command = Command {
                gait: Gait::Pace(plan.cruise / capabilities.speed),
                direction: self.direction,
                jump: true,
            };
        }
        command
    }
}
