//! Stationary-frame motor-command diagnostic on the production flight physics.
//! The carrier's position-arrival rule cannot terminate before our deadline.
use super::*;
use serde_json::json;

pub const MOTOR_FEATURES: [&str; 18] = [
    "heading_error_sin",
    "heading_error_cos",
    "command_speed_over20",
    "com_velocity_local_x_over20",
    "com_velocity_local_y_over20",
    "origin_velocity_local_x_over20",
    "origin_velocity_local_y_over20",
    "spin_over2",
    "gravity_local_x_over45",
    "gravity_local_y_over45",
    "thrust_acceleration_over45",
    "turn_speed_over1_8",
    "turn_acceleration_over6",
    "brake_acceleration_over40",
    "brake_gain_over4",
    "previous_steering",
    "previous_thrust",
    "previous_brake",
];

pub fn schema() -> serde_json::Value {
    json!({"version":1,"kind":"stationary-frame-heading-speed-v1","features":MOTOR_FEATURES,
        "heading":"world radians; forward=(-sin(heading),cos(heading))",
        "speed":"nonnegative COM speed along command heading in stationary world frame",
        "action_contract":"continuous-flight-v2","hz":60})
}

/// The v1 stationary-frame contract: zero gravity, a stationary planet,
/// speeds to 20, and arrival that also requires the commanded heading.
pub const CONTRACT_V1: u32 = 1;
/// The v2 planet-frame contract: the command is relative to the pilot's
/// planet frame, the one the physical brake decelerates toward; gravity and
/// planet spin are allowed; speeds run to 60; arrival requires only relative
/// velocity and spin, since holding a point under gravity tilts the nose.
pub const CONTRACT_V2: u32 = 2;
pub const MAX_SPEED_V1: f32 = 20.0;
pub const MAX_SPEED_V2: f32 = 60.0;
pub const VELOCITY_SCALE_V2: f32 = 70.0;

pub const MOTOR_FEATURES_V2: [&str; 18] = [
    "heading_error_sin",
    "heading_error_cos",
    "command_speed_over70",
    "frame_com_velocity_local_x_over70",
    "frame_com_velocity_local_y_over70",
    "frame_origin_velocity_local_x_over70",
    "frame_origin_velocity_local_y_over70",
    "spin_over2",
    "gravity_local_x_over45",
    "gravity_local_y_over45",
    "thrust_acceleration_over45",
    "turn_speed_over1_8",
    "turn_acceleration_over6",
    "brake_acceleration_over40",
    "brake_gain_over4",
    "previous_steering",
    "previous_thrust",
    "previous_brake",
];

pub fn schema_v2() -> serde_json::Value {
    json!({"version":2,"kind":"planet-frame-heading-speed-v2","features":MOTOR_FEATURES_V2,
        "heading":"world radians; forward=(-sin(heading),cos(heading))",
        "speed":"nonnegative COM speed along the command heading relative to the pilot's planet frame at the ship origin, up to 60",
        "frame":"the planet frame the physical brake decelerates toward: its point velocity at the ship origin",
        "arrival":"relative velocity within 1 for thirty complete intervals, with spin within 0.2 at a zero-speed command and within 0.5 otherwise; no heading requirement; retention judges velocity only",
        "action_contract":"continuous-flight-v2","hz":60})
}

pub fn schema_for(contract: u32) -> Result<serde_json::Value, &'static str> {
    match contract {
        CONTRACT_V1 => Ok(schema()),
        CONTRACT_V2 => Ok(schema_v2()),
        _ => Err("unsupported motor contract"),
    }
}

fn default_contract() -> u32 {
    CONTRACT_V1
}

fn is_v1(contract: &u32) -> bool {
    *contract == CONTRACT_V1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotorTask {
    pub start: FlightStart,
    /// Relative to the actual completed neutral-setup heading, not spawn metadata.
    pub heading_delta: f32,
    pub speed: f32,
    pub deadline_ticks: u32,
    /// Continue after acquisition to train or evaluate uninterrupted retention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_horizon_ticks: Option<u32>,
    /// The command contract; omitted for the original stationary-frame tasks.
    #[serde(default = "default_contract", skip_serializing_if = "is_v1")]
    pub contract: u32,
    /// A v2 task flown above a planet of a generated match arena, for the
    /// arena's own gravity and spin; omitted for the practice world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferWorld>,
    /// A v2 command whose heading turns at this rate in radians per second
    /// from the tick the task starts, the way a route around a body turns a
    /// cruising ship's command; zero, the fixed command of every other task.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub heading_rate: f32,
    /// A v2 task on which surface contact does not end the task, for the
    /// starts that come down onto the ground or begin on it; omitted for
    /// the tasks on which any contact is a collision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contact_allowed: bool,
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl MotorTask {
    fn trace_version(&self) -> u32 {
        if self.contact_allowed {
            5
        } else if self.heading_rate != 0.0 {
            4
        } else if self.contract == CONTRACT_V2 {
            3
        } else if self.retention_horizon_ticks.is_some() {
            2
        } else {
            1
        }
    }

    pub fn max_speed(&self) -> f32 {
        if self.contract == CONTRACT_V2 {
            MAX_SPEED_V2
        } else {
            MAX_SPEED_V1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotorCommand {
    pub heading: f32,
    pub speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotorErrors {
    pub heading: f32,
    pub velocity: f32,
    pub spin: f32,
}

impl MotorErrors {
    /// Inside the acquisition tolerances. The v2 contract drops the heading
    /// requirement and loosens spin while moving: a ship holding its attitude
    /// against a planet's gravity while it cruises must turn as it goes around
    /// the planet, and the arrival dwell judges spin at the stop.
    fn qualifies(self, contract: u32, stopping: bool) -> bool {
        if contract == CONTRACT_V2 {
            self.velocity <= 1.0 && self.spin <= if stopping { 0.2 } else { 0.5 }
        } else {
            self.heading <= 5.0_f32.to_radians() && self.velocity <= 1.0 && self.spin <= 0.2
        }
    }

    /// Retention after acquisition. The v2 contract tracks velocity only:
    /// the nose may reorient against gravity or a rotating frame while the
    /// velocity holds.
    fn retains(self, contract: u32, stopping: bool) -> bool {
        if contract == CONTRACT_V2 {
            self.velocity <= 1.0
        } else {
            self.qualifies(contract, stopping)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorItem {
    pub inputs: Vec<f32>,
    pub frame: FlightFrame,
    pub errors: MotorErrors,
    pub progress: FlightProgress,
    pub evidence: FlightEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<MotorRetention>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MotorRetention {
    pub first_success_tick: Option<u32>,
    pub first_exit_tick: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorStep {
    pub action: FlightAction,
    pub item: MotorItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorTrace {
    pub version: u32,
    pub task: MotorTask,
    pub command: MotorCommand,
    pub initial: MotorItem,
    pub steps: Vec<MotorStep>,
}

pub struct MotorEnvironment {
    flight: FlightEnvironment,
    progress: FlightProgress,
    command: MotorCommand,
    retention: Option<MotorRetention>,
    trace: MotorTrace,
}

fn wrap(angle: f32) -> f32 {
    angle.sin().atan2(angle.cos())
}

/// The eighteen motor inputs for a flight environment under a heading/speed
/// command, exactly as the motor diagnostic observes them.
pub fn command_inputs(
    flight: &FlightEnvironment,
    command: MotorCommand,
) -> Result<Vec<f32>, &'static str> {
    command_inputs_at(&flight.state, 0, flight.previous_action, command)
}

/// The same eighteen inputs read from a live scenario state for the given
/// player's ship, with the action applied on the previous tick. The headless
/// environment and the composed gameplay runtime both derive the motor
/// network's inputs here, so training and gameplay see one definition.
/// The motor inputs of the given contract version for a flight environment.
pub fn command_inputs_contract(
    flight: &FlightEnvironment,
    contract: u32,
    command: MotorCommand,
) -> Result<Vec<f32>, &'static str> {
    command_inputs_for(contract, &flight.state, 0, flight.previous_action, command)
}

/// The motor inputs of the given contract version from a live state.
pub fn command_inputs_for(
    contract: u32,
    state: &SurfaceSortieState,
    player: usize,
    previous_action: FlightAction,
    command: MotorCommand,
) -> Result<Vec<f32>, &'static str> {
    match contract {
        CONTRACT_V1 => command_inputs_at(state, player, previous_action, command),
        CONTRACT_V2 => command_inputs_v2_at(state, player, previous_action, command),
        _ => Err("unsupported motor contract"),
    }
}

/// The v2 inputs: velocities relative to the pilot's planet frame at the ship
/// origin, the frame the physical brake decelerates toward, scaled by 70.
pub fn command_inputs_v2_at(
    state: &SurfaceSortieState,
    player: usize,
    previous_action: FlightAction,
    command: MotorCommand,
) -> Result<Vec<f32>, &'static str> {
    let own = state
        .free_flight_observation(player)
        .ok_or("missing motor ship")?;
    let body = state.world.physics.ship_body(own.vehicle.0);
    let velocity = state
        .world
        .physics
        .world
        .motion(body)
        .ok_or("missing motor ship motion")?
        .linear_velocity;
    let angle = own.ship.angle;
    let local = |v: Vec2| v.rotate_radians(-angle);
    let relative = local(velocity - own.frame_velocity);
    let origin_relative = local(own.ship.velocity - own.frame_velocity);
    let gravity = local(own.gravity);
    let limits = own.flight.limits;
    let delta = wrap(command.heading - angle);
    let action = previous_action;
    let inputs = vec![
        delta.sin(),
        delta.cos(),
        command.speed / VELOCITY_SCALE_V2,
        relative.x / VELOCITY_SCALE_V2,
        relative.y / VELOCITY_SCALE_V2,
        origin_relative.x / VELOCITY_SCALE_V2,
        origin_relative.y / VELOCITY_SCALE_V2,
        own.ship.spin / 2.0,
        gravity.x / 45.0,
        gravity.y / 45.0,
        limits.thrust_acceleration / 45.0,
        limits.turn_speed / 1.8,
        limits.turn_acceleration / 6.0,
        limits.brake_acceleration / 40.0,
        limits.brake_gain / 4.0,
        action.steering,
        action.thrust,
        action.brake,
    ];
    if !inputs.iter().all(|v| v.is_finite()) {
        return Err("non-finite motor inputs");
    }
    Ok(inputs)
}

pub fn command_inputs_at(
    state: &SurfaceSortieState,
    player: usize,
    previous_action: FlightAction,
    command: MotorCommand,
) -> Result<Vec<f32>, &'static str> {
    let own = state
        .free_flight_observation(player)
        .ok_or("missing motor ship")?;
    let body = state.world.physics.ship_body(own.vehicle.0);
    let velocity = state
        .world
        .physics
        .world
        .motion(body)
        .ok_or("missing motor ship motion")?
        .linear_velocity;
    let angle = own.ship.angle;
    let local = |v: Vec2| v.rotate_radians(-angle);
    let velocity = local(velocity);
    let origin_velocity = local(own.ship.velocity);
    let gravity = local(own.gravity);
    let limits = own.flight.limits;
    let delta = wrap(command.heading - angle);
    let action = previous_action;
    let inputs = vec![
        delta.sin(),
        delta.cos(),
        command.speed / 20.0,
        velocity.x / 20.0,
        velocity.y / 20.0,
        origin_velocity.x / 20.0,
        origin_velocity.y / 20.0,
        own.ship.spin / 2.0,
        gravity.x / 45.0,
        gravity.y / 45.0,
        limits.thrust_acceleration / 45.0,
        limits.turn_speed / 1.8,
        limits.turn_acceleration / 6.0,
        limits.brake_acceleration / 40.0,
        limits.brake_gain / 4.0,
        action.steering,
        action.thrust,
        action.brake,
    ];
    if !inputs.iter().all(|v| v.is_finite()) {
        return Err("non-finite motor inputs");
    }
    Ok(inputs)
}

impl MotorEnvironment {
    pub fn new(task: MotorTask) -> Result<Self, &'static str> {
        if task.contract != CONTRACT_V1 && task.contract != CONTRACT_V2 {
            return Err("unsupported motor contract");
        }
        let stationary = task.contract == CONTRACT_V1;
        if stationary && task.transfer.is_some() {
            return Err("unsupported motor task; stationary zero-gravity frame required");
        }
        if !task.heading_delta.is_finite()
            || task.heading_delta.abs() > std::f32::consts::PI
            || !task.speed.is_finite()
            || !(0.0..=task.max_speed()).contains(&task.speed)
            || !(30..=600).contains(&task.deadline_ticks)
            || (stationary && (task.start.gravity || task.start.planet_spin != 0.0))
            || task
                .retention_horizon_ticks
                .is_some_and(|h| h <= task.deadline_ticks || h > 1800)
            || !task.heading_rate.is_finite()
            || task.heading_rate.abs() > 1.0
            || (stationary && task.heading_rate != 0.0)
            || (stationary && task.contact_allowed)
        {
            return Err(if stationary {
                "unsupported motor task; stationary zero-gravity frame required"
            } else {
                "unsupported motor task"
            });
        }
        let horizon = task.retention_horizon_ticks.unwrap_or(task.deadline_ticks);
        // On an arena the flight task's goal rides the start planet; the motor
        // task ignores it, since its own command is the task.
        let flight = FlightEnvironment::new(FlightTask {
            start: task.start,
            goal_offset: task.transfer.map_or(Vec2::ZERO, |_| {
                Vec2::Y.rotate_radians(task.start.bearing) * (task.start.altitude + 100.0)
            }),
            goal_velocity: Vec2::ZERO,
            arrival: ArrivalCriteria {
                dwell_ticks: horizon,
                ..Default::default()
            },
            deadline_ticks: horizon,
            transfer: task.transfer.map(|t| TransferWorld {
                goal_planet: (t.start_planet + 1) % t.layout.planets.max(2),
                ..t
            }),
            contact_allowed: task.contact_allowed,
            ..Default::default()
        })?;
        let command = MotorCommand {
            heading: wrap(flight.frame.angle + task.heading_delta),
            speed: task.speed,
        };
        let progress = FlightProgress::default();
        let retention = task
            .retention_horizon_ticks
            .map(|_| MotorRetention::default());
        let initial = Self::observe(&flight, task.contract, command, progress, retention)?;
        Ok(Self {
            flight,
            progress,
            command,
            retention,
            trace: MotorTrace {
                version: task.trace_version(),
                task,
                command,
                initial,
                steps: Vec::new(),
            },
        })
    }

    fn observe(
        flight: &FlightEnvironment,
        contract: u32,
        command: MotorCommand,
        progress: FlightProgress,
        retention: Option<MotorRetention>,
    ) -> Result<MotorItem, &'static str> {
        let frame = flight.frame.clone();
        let inputs = command_inputs_contract(flight, contract, command)?;
        let delta = wrap(command.heading - frame.angle);
        let desired = Vec2::new(-command.heading.sin(), command.heading.cos()) * command.speed;
        // The v2 command is relative to the pilot's planet frame.
        let reference = if contract == CONTRACT_V2 {
            flight
                .state
                .free_flight_observation(0)
                .ok_or("missing motor ship")?
                .frame_velocity
        } else {
            Vec2::ZERO
        };
        Ok(MotorItem {
            errors: MotorErrors {
                heading: delta.abs(),
                velocity: (frame.velocity - reference - desired).length(),
                spin: frame.spin.abs(),
            },
            frame,
            inputs,
            progress,
            evidence: read_evidence(&flight.state),
            retention,
        })
    }

    pub fn item(&self) -> &MotorItem {
        self.trace
            .steps
            .last()
            .map(|s| &s.item)
            .unwrap_or(&self.trace.initial)
    }
    pub fn trace(&self) -> &MotorTrace {
        &self.trace
    }

    pub fn step(&mut self, action: FlightAction) -> Result<&MotorItem, &'static str> {
        if self.progress.outcome.is_some() || !action.valid() {
            return Err("invalid or terminal motor action");
        }
        let before = self.item().errors;
        let contract = self.trace.task.contract;
        let stopping = self.command.speed == 0.0;
        self.flight.step(action)?;
        // A turning command: its heading at the completed tick.
        if self.trace.task.heading_rate != 0.0 {
            self.command.heading = wrap(
                self.trace.command.heading
                    + self.trace.task.heading_rate * (self.progress.elapsed_ticks + 1) as f32
                        / FLIGHT_HZ as f32,
            );
        }
        let mut item = Self::observe(
            &self.flight,
            contract,
            self.command,
            self.progress,
            self.retention,
        )?;
        self.progress.elapsed_ticks += 1;
        self.progress.dwell_ticks =
            if before.qualifies(contract, stopping) && item.errors.qualifies(contract, stopping) {
                self.progress.dwell_ticks + 1
            } else {
                0
            };
        if let Some(retention) = &mut self.retention {
            if retention.first_success_tick.is_none() && self.progress.dwell_ticks >= 30 {
                retention.first_success_tick = Some(self.progress.elapsed_ticks);
            }
            if retention.first_success_tick.is_some()
                && !item.errors.retains(contract, stopping)
                && retention.first_exit_tick.is_none()
            {
                retention.first_exit_tick = Some(self.progress.elapsed_ticks);
            }
        }
        self.progress.outcome = match self.flight.progress.outcome {
            Some(FlightOutcome::Success | FlightOutcome::Deadline) | None => {
                if let Some(horizon) = self.trace.task.retention_horizon_ticks {
                    if self.progress.elapsed_ticks < horizon {
                        None
                    } else {
                        let retention = self.retention.unwrap();
                        if retention
                            .first_success_tick
                            .is_some_and(|tick| tick <= self.trace.task.deadline_ticks)
                            && retention.first_exit_tick.is_none()
                        {
                            Some(FlightOutcome::Success)
                        } else {
                            Some(FlightOutcome::Deadline)
                        }
                    }
                } else if self.progress.dwell_ticks >= 30 {
                    Some(FlightOutcome::Success)
                } else if self.progress.elapsed_ticks >= self.trace.task.deadline_ticks {
                    Some(FlightOutcome::Deadline)
                } else {
                    None
                }
            }
            physical_failure => physical_failure,
        };
        item.progress = self.progress;
        item.retention = self.retention;
        self.trace.steps.push(MotorStep { action, item });
        Ok(self.item())
    }

    pub fn replay(trace: &MotorTrace) -> Result<Self, &'static str> {
        if trace.version != trace.task.trace_version() {
            return Err("unsupported motor trace");
        }
        let mut env = Self::new(trace.task.clone())?;
        if env.command != trace.command || env.item() != &trace.initial {
            return Err("motor initial replay mismatch");
        }
        for expected in &trace.steps {
            if env.step(expected.action)? != &expected.item {
                return Err("motor replay mismatch");
            }
        }
        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(delta: f32) -> MotorTask {
        MotorTask {
            start: FlightStart::default(),
            heading_delta: delta,
            speed: 0.0,
            deadline_ticks: 60,
            retention_horizon_ticks: None,
            contract: CONTRACT_V1,
            transfer: None,
            heading_rate: 0.0,
            contact_allowed: false,
        }
    }

    fn task_v2(delta: f32, speed: f32) -> MotorTask {
        MotorTask {
            speed,
            contract: CONTRACT_V2,
            ..task(delta)
        }
    }

    #[test]
    fn command_uses_completed_heading_and_body_frame() {
        let env = MotorEnvironment::new(task(0.7)).unwrap();
        let item = env.item();
        assert!((item.errors.heading - 0.7).abs() < 1e-6);
        assert!((item.inputs[0] - 0.7_f32.sin()).abs() < 1e-6);
        assert_eq!(item.inputs.len(), 18);
        assert!(item.inputs.iter().all(|v| v.is_finite()));
        let mut moving = task(0.0);
        moving.speed = 4.0;
        let env = MotorEnvironment::new(moving).unwrap();
        assert!((env.item().errors.velocity - 4.0).abs() < 1e-6);
    }
    #[test]
    fn command_inputs_match_the_observed_item() {
        let mut moving = task(0.3);
        moving.speed = 5.0;
        let env = MotorEnvironment::new(moving).unwrap();
        assert_eq!(
            command_inputs(&env.flight, env.command).unwrap(),
            env.item().inputs
        );
    }
    #[test]
    fn physical_success_needs_complete_intervals_and_heading() {
        let mut env = MotorEnvironment::new(task(0.0)).unwrap();
        for _ in 0..29 {
            assert_eq!(
                env.step(FlightAction::default()).unwrap().progress.outcome,
                None
            );
        }
        assert_eq!(
            env.step(FlightAction::default()).unwrap().progress.outcome,
            Some(FlightOutcome::Success)
        );
        assert!(env.step(FlightAction::default()).is_err());
        let mut env = MotorEnvironment::new(task(0.7)).unwrap();
        for _ in 0..60 {
            env.step(FlightAction::default()).unwrap();
        }
        assert_eq!(env.item().progress.outcome, Some(FlightOutcome::Deadline));
        assert_eq!(env.item().progress.dwell_ticks, 0);
    }
    #[test]
    fn active_replay_and_invalid_action_preserve_state() {
        let mut env = MotorEnvironment::new(task(-1.2)).unwrap();
        for _ in 0..17 {
            env.step(FlightAction {
                steering: -0.4,
                brake: 0.2,
                thrust: 0.0,
            })
            .unwrap();
        }
        let trace = env.trace().clone();
        assert!(
            env.step(FlightAction {
                thrust: f32::NAN,
                ..Default::default()
            })
            .is_err()
        );
        assert_eq!(env.trace(), &trace);
        let mut replay = MotorEnvironment::replay(&trace).unwrap();
        assert_eq!(
            env.step(FlightAction::default()).unwrap(),
            replay.step(FlightAction::default()).unwrap()
        );
        let mut bad = trace;
        bad.steps[0].item.inputs[0] += 0.1;
        assert!(MotorEnvironment::replay(&bad).is_err());
    }
    #[test]
    fn scope_and_physical_tolerances_are_explicit() {
        for delta in [f32::NAN, 4.0] {
            assert!(MotorEnvironment::new(task(delta)).is_err());
        }
        let mut t = task(0.0);
        t.start.gravity = true;
        assert!(MotorEnvironment::new(t).is_err());
        let e = MotorErrors {
            heading: 5.0_f32.to_radians(),
            velocity: 1.0,
            spin: 0.2,
        };
        assert!(e.qualifies(CONTRACT_V1, true));
        assert!(!MotorErrors { spin: 0.20001, ..e }.qualifies(CONTRACT_V1, true));
        let tilted = MotorErrors { heading: 1.0, ..e };
        assert!(!tilted.qualifies(CONTRACT_V1, true));
        assert!(tilted.qualifies(CONTRACT_V2, true));
        assert!(
            !MotorErrors {
                velocity: 1.01,
                ..tilted
            }
            .qualifies(CONTRACT_V2, true)
        );
        let turning = MotorErrors {
            spin: 0.5,
            ..tilted
        };
        assert!(!turning.qualifies(CONTRACT_V2, true));
        assert!(turning.qualifies(CONTRACT_V2, false));
        assert!(
            !MotorErrors {
                spin: 0.51,
                ..tilted
            }
            .qualifies(CONTRACT_V2, false)
        );
        assert!(turning.retains(CONTRACT_V2, true));
        assert!(!MotorErrors { spin: 0.5, ..e }.retains(CONTRACT_V1, true));
    }

    #[test]
    fn v2_tasks_accept_gravity_and_spin_and_read_frame_relative_inputs() {
        let mut t = task_v2(0.3, 40.0);
        t.start.gravity = true;
        t.start.planet_spin = 0.015;
        let env = MotorEnvironment::new(t.clone()).unwrap();
        let item = env.item();
        assert_eq!(env.trace().version, 3);
        assert_eq!(item.inputs.len(), 18);
        assert!((item.inputs[2] - 40.0 / 70.0).abs() < 1e-6);
        // The ship starts at rest in the spinning frame while moving in the world.
        assert!(item.frame.velocity.length() > 1.0);
        assert!(item.inputs[3].abs() < 1e-3 && item.inputs[4].abs() < 1e-3);
        assert!(item.inputs[8].hypot(item.inputs[9]) > 0.01);
        assert!((item.errors.velocity - 40.0).abs() < 0.1);
        assert_eq!(
            command_inputs_for(
                CONTRACT_V2,
                env.flight.state(),
                0,
                FlightAction::default(),
                env.command
            )
            .unwrap(),
            item.inputs
        );
        assert!(MotorEnvironment::new(task_v2(0.0, 60.01)).is_err());
        assert!(
            MotorEnvironment::new(MotorTask {
                speed: 25.0,
                ..task(0.0)
            })
            .is_err()
        );
        assert!(
            MotorEnvironment::new(MotorTask {
                contract: 3,
                ..task(0.0)
            })
            .is_err()
        );
        let value = serde_json::to_value(&t).unwrap();
        assert_eq!(value["contract"], 2);
        assert!(
            serde_json::to_value(task(0.0))
                .unwrap()
                .get("contract")
                .is_none()
        );
        let legacy: MotorTask =
            serde_json::from_value(serde_json::to_value(task(0.0)).unwrap()).unwrap();
        assert_eq!(legacy.contract, CONTRACT_V1);
    }

    #[test]
    fn v2_tasks_fly_above_arena_planets_with_their_gravity() {
        let layout = mission::ArenaLayout::STANDARD;
        let mut t = task_v2(0.0, 0.0);
        t.start.gravity = true;
        t.start.altitude = 30.0;
        t.transfer = Some(TransferWorld {
            layout,
            start_planet: 1,
            goal_planet: 1,
        });
        let env = MotorEnvironment::new(t.clone()).unwrap();
        let item = env.item();
        assert!(
            item.inputs[8].hypot(item.inputs[9]) * 45.0 > 3.0,
            "arena gravity is felt"
        );
        assert!(
            item.errors.velocity < 0.5,
            "the start rests in the arena planet's frame"
        );
        assert!(
            MotorEnvironment::new(MotorTask {
                contract: CONTRACT_V1,
                ..t.clone()
            })
            .is_err()
        );
        t.transfer.as_mut().unwrap().goal_planet = 0;
        let same = MotorEnvironment::new(t).unwrap();
        assert_eq!(
            same.item().inputs,
            item.inputs,
            "the goal planet does not matter to a motor task"
        );
    }

    #[test]
    fn v2_arrival_needs_no_heading_and_replays() {
        let mut env = MotorEnvironment::new(task_v2(0.7, 0.0)).unwrap();
        for _ in 0..29 {
            assert_eq!(
                env.step(FlightAction::default()).unwrap().progress.outcome,
                None
            );
        }
        assert_eq!(
            env.step(FlightAction::default()).unwrap().progress.outcome,
            Some(FlightOutcome::Success)
        );
        assert!((env.item().errors.heading - 0.7).abs() < 1e-3);
        let replay = MotorEnvironment::replay(env.trace()).unwrap();
        assert_eq!(replay.item(), env.item());
        let mut bad = env.trace().clone();
        bad.version = 1;
        assert!(MotorEnvironment::replay(&bad).is_err());
    }

    #[test]
    fn a_contact_task_flies_on_after_touching_down_and_replays() {
        // The ship falls onto its side from the lowest start under a command
        // up; with the controls neutral it lands and lies there.
        let mut grounded = task_v2(-std::f32::consts::FRAC_PI_2, 20.0);
        grounded.start.gravity = true;
        grounded.start.altitude = 15.0;
        grounded.start.heading_offset = std::f32::consts::FRAC_PI_2;
        grounded.deadline_ticks = 300;
        let mut collision = MotorEnvironment::new(grounded.clone()).unwrap();
        while collision.item().progress.outcome.is_none() {
            collision.step(FlightAction::default()).unwrap();
        }
        assert_eq!(
            collision.item().progress.outcome,
            Some(FlightOutcome::Collision)
        );
        let touchdown = collision.item().progress.elapsed_ticks;
        grounded.contact_allowed = true;
        let mut env = MotorEnvironment::new(grounded.clone()).unwrap();
        assert_eq!(env.trace().version, 5);
        assert_eq!(
            env.flight.trace().version,
            FLIGHT_CONTACT_ENVIRONMENT_VERSION
        );
        let mut contact_ticks = 0;
        while env.item().progress.outcome.is_none() {
            contact_ticks += env
                .step(FlightAction::default())
                .unwrap()
                .evidence
                .collision as u32;
        }
        assert_eq!(env.item().progress.outcome, Some(FlightOutcome::Deadline));
        assert_eq!(env.item().progress.elapsed_ticks, 300);
        assert!(
            contact_ticks > 300 - touchdown - 30,
            "the ship lies on the ground: {contact_ticks} contact ticks after touching down at {touchdown}"
        );
        let replay = MotorEnvironment::replay(env.trace()).unwrap();
        assert_eq!(replay.item(), env.item());
        let value = serde_json::to_value(&grounded).unwrap();
        assert_eq!(value["contact_allowed"], true);
        assert!(
            serde_json::to_value(task_v2(0.0, 0.0))
                .unwrap()
                .get("contact_allowed")
                .is_none()
        );
        assert!(
            MotorEnvironment::new(MotorTask {
                contract: CONTRACT_V1,
                start: FlightStart::default(),
                speed: 0.0,
                ..grounded
            })
            .is_err(),
            "the stationary contract has no ground to touch"
        );
    }

    #[test]
    fn a_turning_command_rotates_with_the_clock_and_keeps_its_own_trace_version() {
        let mut task = MotorTask {
            start: FlightStart {
                altitude: 100.0,
                gravity: true,
                planet_spin: 0.015,
                ..FlightStart::default()
            },
            heading_delta: 0.0,
            speed: 40.0,
            deadline_ticks: 300,
            retention_horizon_ticks: Some(900),
            contract: CONTRACT_V2,
            transfer: None,
            heading_rate: 0.3,
            contact_allowed: false,
        };
        let mut env = MotorEnvironment::new(task.clone()).unwrap();
        let initial = env.trace().command.heading;
        for _ in 0..60 {
            env.step(FlightAction::default()).unwrap();
        }
        let item = env.item().clone();
        let delta = item.inputs[0].atan2(item.inputs[1]);
        let expected = wrap(initial + 0.3 - item.frame.angle);
        assert!(wrap(delta - expected).abs() < 1e-4, "{delta} {expected}");
        assert_eq!(env.trace().command.heading, initial);
        assert_eq!(env.trace().version, 4);
        assert!(
            serde_json::to_value(&task)
                .unwrap()
                .get("heading_rate")
                .is_some()
        );
        let replay = MotorEnvironment::replay(env.trace()).unwrap();
        assert_eq!(replay.item(), &item);
        task.heading_rate = 0.0;
        assert!(
            serde_json::to_value(&task)
                .unwrap()
                .get("heading_rate")
                .is_none()
        );
        assert_eq!(
            MotorEnvironment::new(task.clone()).unwrap().trace().version,
            3
        );
        task.heading_rate = 1.5;
        assert!(MotorEnvironment::new(task.clone()).is_err());
        task.heading_rate = 0.3;
        task.contract = CONTRACT_V1;
        task.start.gravity = false;
        task.start.planet_spin = 0.0;
        assert!(MotorEnvironment::new(task).is_err());
    }

    #[test]
    fn retention_continues_after_acquisition_and_replays() {
        let mut t = task(0.0);
        t.retention_horizon_ticks = Some(120);
        let mut env = MotorEnvironment::new(t).unwrap();
        for _ in 0..119 {
            assert_eq!(
                env.step(FlightAction::default()).unwrap().progress.outcome,
                None
            );
        }
        assert_eq!(env.item().retention.unwrap().first_success_tick, Some(30));
        let mut replay = MotorEnvironment::replay(env.trace()).unwrap();
        assert_eq!(
            env.step(FlightAction::default()).unwrap(),
            replay.step(FlightAction::default()).unwrap()
        );
        assert_eq!(env.item().progress.outcome, Some(FlightOutcome::Success));
        assert_eq!(env.trace().version, 2);
        let mut bad = env.trace().clone();
        bad.version = 1;
        assert!(MotorEnvironment::replay(&bad).is_err());
    }

    #[test]
    fn retention_remembers_a_breach_after_recovery() {
        let mut t = task(0.0);
        t.retention_horizon_ticks = Some(120);
        let mut env = MotorEnvironment::new(t).unwrap();
        for _ in 0..30 {
            env.step(FlightAction::default()).unwrap();
        }
        for _ in 0..5 {
            env.step(FlightAction {
                thrust: 1.0,
                ..Default::default()
            })
            .unwrap();
        }
        for _ in 35..120 {
            env.step(FlightAction {
                brake: 1.0,
                ..Default::default()
            })
            .unwrap();
        }
        assert!(env.item().errors.qualifies(CONTRACT_V1, true));
        assert!(env.item().retention.unwrap().first_exit_tick.is_some());
        assert_eq!(env.item().progress.outcome, Some(FlightOutcome::Deadline));
        assert!(MotorEnvironment::replay(env.trace()).is_ok());
    }

    #[test]
    fn retention_deadline_is_an_acquisition_limit_not_an_early_terminal() {
        let mut t = task(0.7);
        t.retention_horizon_ticks = Some(120);
        let mut env = MotorEnvironment::new(t).unwrap();
        for _ in 0..119 {
            assert_eq!(
                env.step(FlightAction::default()).unwrap().progress.outcome,
                None
            );
        }
        assert_eq!(
            env.step(FlightAction::default()).unwrap().progress.outcome,
            Some(FlightOutcome::Deadline)
        );
        assert_eq!(env.item().retention.unwrap().first_success_tick, None);
    }

    #[test]
    fn retention_validation_and_legacy_serialization_are_explicit() {
        for horizon in [0, 60, 1801] {
            let mut t = task(0.0);
            t.retention_horizon_ticks = Some(horizon);
            assert!(MotorEnvironment::new(t).is_err());
        }
        let env = MotorEnvironment::new(task(0.0)).unwrap();
        let value = serde_json::to_value(env.trace()).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value["task"].get("retention_horizon_ticks").is_none());
        assert!(value["initial"].get("retention").is_none());
    }
}
