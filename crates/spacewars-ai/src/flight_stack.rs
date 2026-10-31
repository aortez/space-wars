//! Composed learned flight stack for gameplay: the navigation network chooses
//! a heading and speed from the planner's goal every six ticks, and the motor
//! network flies steering, thrust and brake at 60 Hz. The driver keeps the task
//! clock, dwell count and held command that the headless environment kept, so
//! both networks see the observations they were qualified on. There is no
//! native controller, fallback or rescue here: an unsupported ship state or a
//! refused command is an error the host decides about.
use crate::learned_flight::FlightPolicyFile;
use crate::learned_motor::{MotorPolicy, MotorPolicyFile};
use crate::learned_navigation::{NavigationOutput, NavigationPolicy};
use engine_common::Action;
use engine_core::Vec2;
use engine_rapier::world::polar::PolarSensor;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceFlightAction, SurfaceSortieAction, SurfaceSortieState, SurfaceWingAction,
        flight_environment::{
            ArrivalCriteria, ArrivalErrors, FlightAction, flight_sensor_config,
            motor::{self, MotorCommand},
        },
        flight_policy_observation::{FlightPolicyContext, FlightPolicyObservation},
    },
};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const STACK_ID: &str = "learned_flight_stack_v1";
pub const STACK_ID_V2: &str = "learned_flight_stack_v2";
/// Ticks each navigation command is held while the motor network flies it.
pub const HOLD_TICKS: u32 = 6;
pub const MAX_DEADLINE_TICKS: u32 = 1800;

const NAVIGATION_EXPORT: &[u8] =
    include_bytes!("../fixtures/learned-flight-stack-v1/navigation.json");
const MOTOR_EXPORT: &[u8] = include_bytes!("../fixtures/learned-flight-stack-v1/motor.json");
const NAVIGATION_EXPORT_V2: &[u8] =
    include_bytes!("../fixtures/learned-flight-stack-v2/navigation.json");
const MOTOR_EXPORT_V2: &[u8] = include_bytes!("../fixtures/learned-flight-stack-v2/motor.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlightStackError {
    /// An export does not carry the contract the stack expects.
    Load(String),
    Goal(&'static str),
    /// The ship is not in a state the stack was qualified to fly.
    Observation(&'static str),
    Inference(&'static str),
    /// The navigation network issued a command the contract refuses.
    Command(&'static str),
    /// Decisions come once per tick, in order, for the goal's attempt.
    Tick,
    /// The attempt has ended; a goal with a new id starts another.
    Ended(GoalOutcome),
}

impl fmt::Display for FlightStackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(reason) => write!(f, "flight stack export: {reason}"),
            Self::Goal(reason) | Self::Observation(reason) | Self::Inference(reason) => {
                f.write_str(reason)
            }
            Self::Command(reason) => write!(f, "navigation command refused: {reason}"),
            Self::Tick => f.write_str("flight stack decisions must advance one tick at a time"),
            Self::Ended(GoalOutcome::Arrived) => f.write_str("goal reached; issue a new goal"),
            Self::Ended(GoalOutcome::Deadline) => {
                f.write_str("goal deadline passed; issue a new goal")
            }
        }
    }
}

impl std::error::Error for FlightStackError {}

/// The two exported networks of the composed stack.
pub struct FlightStack {
    navigation: NavigationPolicy,
    motor: MotorPolicy,
}

impl FlightStack {
    pub fn from_files(
        navigation: FlightPolicyFile,
        motor: MotorPolicyFile,
    ) -> Result<Self, FlightStackError> {
        let navigation = NavigationPolicy::from_file(navigation)
            .map_err(|e| FlightStackError::Load(e.into()))?;
        let motor = MotorPolicy::from_file(motor).map_err(|e| FlightStackError::Load(e.into()))?;
        if navigation.contract() != motor.contract() {
            return Err(FlightStackError::Load(
                "navigation and motor command contracts differ".into(),
            ));
        }
        Ok(Self { navigation, motor })
    }

    /// The command contract both networks share.
    pub fn contract(&self) -> u32 {
        self.motor.contract()
    }

    /// The retained exports under `fixtures/learned-flight-stack-v1`: the
    /// LF-083 fitted navigation network over the LF-078 v4 fitted motor network.
    pub fn retained() -> Result<Self, FlightStackError> {
        Self::embedded(NAVIGATION_EXPORT, MOTOR_EXPORT)
    }

    /// The retained exports under `fixtures/learned-flight-stack-v2`: the
    /// LF-091 refitted navigation network over the LF-091 v6 fitted motor
    /// network, the match-envelope stack with goal-relative commands.
    pub fn retained_v2() -> Result<Self, FlightStackError> {
        Self::embedded(NAVIGATION_EXPORT_V2, MOTOR_EXPORT_V2)
    }

    fn embedded(navigation: &[u8], motor: &[u8]) -> Result<Self, FlightStackError> {
        let navigation = serde_json::from_slice(navigation)
            .map_err(|e| FlightStackError::Load(e.to_string()))?;
        let motor =
            serde_json::from_slice(motor).map_err(|e| FlightStackError::Load(e.to_string()))?;
        Self::from_files(navigation, motor)
    }

    pub fn navigation(&self) -> &NavigationPolicy {
        &self.navigation
    }

    pub fn motor(&self) -> &MotorPolicy {
        &self.motor
    }
}

/// The planner's request: a world-frame center-of-mass destination and
/// velocity, with the arrival rule and attempt deadline of the flight contract.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NavigationGoal {
    /// A new id starts a new attempt with a fresh clock, dwell and command.
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub arrival: ArrivalCriteria,
    pub deadline_ticks: u32,
}

impl NavigationGoal {
    /// A stationary destination under the contract's default arrival rule
    /// and the full thirty-second deadline.
    pub fn stationary(id: u64, position: Vec2) -> Self {
        Self {
            id,
            position,
            velocity: Vec2::ZERO,
            arrival: ArrivalCriteria::default(),
            deadline_ticks: MAX_DEADLINE_TICKS,
        }
    }

    fn validate(&self, max_speed: f32) -> Result<(), FlightStackError> {
        let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        let arrival = self.arrival;
        if !finite(self.position)
            || !finite(self.velocity)
            || self.velocity.length() > max_speed
            || ![arrival.position, arrival.velocity, arrival.spin]
                .into_iter()
                .all(f32::is_finite)
            || arrival.position <= 0.0
            || arrival.velocity <= 0.0
            || arrival.spin < 0.0
            || arrival.dwell_ticks == 0
            || arrival.dwell_ticks > self.deadline_ticks
            || self.deadline_ticks > MAX_DEADLINE_TICKS
        {
            return Err(FlightStackError::Goal("invalid navigation goal"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalOutcome {
    Arrived,
    Deadline,
}

/// The attempt's clock and arrival state, judged the way the environment
/// judges a task: physical center-of-mass errors and consecutive dwell.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GoalProgress {
    pub elapsed_ticks: u32,
    pub dwell_ticks: u32,
    pub errors: ArrivalErrors,
    pub outcome: Option<GoalOutcome>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StackDecision {
    pub tick: u64,
    pub goal_id: u64,
    pub progress: GoalProgress,
    /// The absolute heading and speed the motor network is flying.
    pub command: MotorCommand,
    /// The navigation network's output on the ticks it issued the command.
    pub navigation: Option<NavigationOutput>,
    pub action: FlightAction,
}

struct Attempt {
    goal_id: u64,
    elapsed_ticks: u32,
    dwell_ticks: u32,
    qualified: bool,
    outcome: Option<GoalOutcome>,
    command: Option<MotorCommand>,
    command_ticks: u32,
}

/// Per-ship runtime state: the sensor, the attempt clock and the held command.
/// The host sets the ship's learned flight control mode, calls `decide` once
/// per tick before stepping, and applies the returned action.
pub struct FlightStackDriver {
    player: usize,
    sensor: PolarSensor,
    attempt: Option<Attempt>,
    last: Option<StackDecision>,
}

impl FlightStackDriver {
    pub fn new(player: usize) -> Result<Self, FlightStackError> {
        Ok(Self {
            player,
            sensor: PolarSensor::new(flight_sensor_config())
                .map_err(FlightStackError::Observation)?,
            attempt: None,
            last: None,
        })
    }

    pub fn player(&self) -> usize {
        self.player
    }

    pub fn last(&self) -> Option<&StackDecision> {
        self.last.as_ref()
    }

    /// The observation the navigation network would see for the goal on the
    /// current attempt's clock, for diagnostics; `decide` reads its own.
    pub fn observation(
        &self,
        state: &SurfaceSortieState,
        goal: NavigationGoal,
    ) -> Result<FlightPolicyObservation, FlightStackError> {
        let own = state
            .free_flight_observation(self.player)
            .ok_or(FlightStackError::Observation("flight state unavailable"))?;
        let attempt = self.attempt.as_ref().filter(|a| a.goal_id == goal.id);
        let context = FlightPolicyContext {
            target_position: goal.position,
            target_velocity: goal.velocity,
            arrival: goal.arrival,
            elapsed_ticks: attempt.map_or(0, |a| a.elapsed_ticks),
            dwell_ticks: attempt.map_or(0, |a| a.dwell_ticks),
            deadline_ticks: goal.deadline_ticks,
            previous_action: FlightAction {
                steering: own.applied[0],
                thrust: own.applied[1],
                brake: own.applied[2],
            },
        };
        state
            .flight_policy_observation(self.player, &self.sensor, context)
            .map_err(FlightStackError::Observation)
    }

    /// One decision for the current tick. Repeating a tick returns the same
    /// decision; a skipped tick is an error; a goal with a new id starts a
    /// fresh attempt immediately, including a new navigation command.
    pub fn decide(
        &mut self,
        stack: &FlightStack,
        state: &SurfaceSortieState,
        goal: NavigationGoal,
    ) -> Result<StackDecision, FlightStackError> {
        goal.validate(stack.navigation.max_speed())?;
        let frame = state
            .flight_frame(self.player, goal.position, goal.velocity)
            .map_err(FlightStackError::Observation)?;
        let tick = frame.world_tick;
        let continuing = self.attempt.as_ref().is_some_and(|a| a.goal_id == goal.id);
        if continuing {
            if let Some(last) = &self.last {
                if last.tick == tick {
                    return Ok(last.clone());
                }
                if last.tick + 1 != tick {
                    return Err(FlightStackError::Tick);
                }
            }
            if let Some(outcome) = self.attempt.as_ref().and_then(|a| a.outcome) {
                return Err(FlightStackError::Ended(outcome));
            }
        }
        let own = state
            .free_flight_observation(self.player)
            .ok_or(FlightStackError::Observation("flight state unavailable"))?;
        let errors = frame.errors;
        let qualifies = errors.qualifies(goal.arrival);
        if !continuing {
            self.attempt = Some(Attempt {
                goal_id: goal.id,
                elapsed_ticks: 0,
                dwell_ticks: 0,
                qualified: qualifies,
                outcome: None,
                command: None,
                command_ticks: 0,
            });
            self.last = None;
        }
        let attempt = self.attempt.as_mut().expect("attempt exists");
        if continuing {
            attempt.elapsed_ticks += 1;
            attempt.dwell_ticks = if attempt.qualified && qualifies {
                attempt.dwell_ticks + 1
            } else {
                0
            };
            attempt.qualified = qualifies;
            attempt.outcome = if attempt.dwell_ticks >= goal.arrival.dwell_ticks {
                Some(GoalOutcome::Arrived)
            } else if attempt.elapsed_ticks >= goal.deadline_ticks {
                Some(GoalOutcome::Deadline)
            } else {
                None
            };
        }
        // The controls the ship applied on the previous tick, as the
        // environment records its previous action.
        let previous_action = FlightAction {
            steering: own.applied[0],
            thrust: own.applied[1],
            brake: own.applied[2],
        };
        // The sensors sweep only on the ticks the navigation network decides;
        // the motor network's inputs come from the ship state alone.
        let navigation = if attempt.command.is_none() || attempt.command_ticks >= HOLD_TICKS {
            let context = FlightPolicyContext {
                target_position: goal.position,
                target_velocity: goal.velocity,
                arrival: goal.arrival,
                elapsed_ticks: attempt.elapsed_ticks,
                dwell_ticks: attempt.dwell_ticks,
                deadline_ticks: goal.deadline_ticks,
                previous_action,
            };
            let observation = state
                .flight_policy_observation(self.player, &self.sensor, context)
                .map_err(FlightStackError::Observation)?;
            let output = stack
                .navigation
                .infer(&observation.inputs)
                .map_err(FlightStackError::Inference)?;
            // A v2 command is relative to the goal; the goal's velocity in
            // the pilot's planet frame is added here, exactly.
            attempt.command = Some(
                output
                    .command
                    .absolute_under(
                        stack.navigation.contract(),
                        frame.angle,
                        goal.velocity - own.frame_velocity,
                        stack.navigation.max_speed(),
                    )
                    .map_err(FlightStackError::Command)?,
            );
            attempt.command_ticks = 0;
            Some(output)
        } else {
            None
        };
        let command = attempt.command.expect("command issued");
        let inputs = motor::command_inputs_for(
            stack.motor.contract(),
            state,
            self.player,
            previous_action,
            command,
        )
        .map_err(FlightStackError::Inference)?;
        let action = stack
            .motor
            .infer(&inputs)
            .map_err(FlightStackError::Inference)?
            .action;
        attempt.command_ticks += 1;
        let decision = StackDecision {
            tick,
            goal_id: goal.id,
            progress: GoalProgress {
                elapsed_ticks: attempt.elapsed_ticks,
                dwell_ticks: attempt.dwell_ticks,
                errors,
                outcome: attempt.outcome,
            },
            command,
            navigation,
            action,
        };
        self.last = Some(decision.clone());
        Ok(decision)
    }
}

/// The three scenario actions that apply a flight action to a ship exactly as
/// the headless environment applies one: analog steering, analog thrust and
/// brake, wings open.
pub fn flight_actions(action: FlightAction, owner: PlayerId) -> [Action; 3] {
    [
        SurfaceSortieAction {
            horizontal: action.steering,
            primary_held: false,
            brake_held: false,
            interact_held: false,
        }
        .encode(owner),
        SurfaceFlightAction {
            thrust: action.thrust,
            brake: action.brake,
        }
        .encode(owner),
        SurfaceWingAction { closed: false }.encode(owner),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{
        SurfaceSortieScenario,
        flight_environment::{FLIGHT_DT, FlightEnvironment, FlightOutcome, FlightTask},
    };

    fn goal_of(env: &FlightEnvironment, id: u64) -> NavigationGoal {
        let task = &env.trace().task;
        NavigationGoal {
            id,
            position: env.frame().target_position,
            velocity: task.goal_velocity,
            arrival: task.arrival,
            deadline_ticks: task.deadline_ticks,
        }
    }

    #[test]
    fn retained_exports_load_with_their_contracts() {
        let stack = FlightStack::retained().unwrap();
        assert_eq!(stack.contract(), 1);
        assert!(stack.motor().infer(&[0.0; 18]).is_ok());
        let stack = FlightStack::retained_v2().unwrap();
        assert_eq!(stack.contract(), 2);
        assert_eq!(stack.navigation().max_speed(), 60.0);
        assert!(stack.motor().infer(&[0.0; 18]).is_ok());
    }

    #[test]
    fn driver_keeps_the_environment_clock_and_arrives_on_the_default_task() {
        let stack = FlightStack::retained().unwrap();
        let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
        let mut driver = FlightStackDriver::new(0).unwrap();
        let goal = goal_of(&env, 1);
        let mut last = None;
        while env.progress().outcome.is_none() {
            let decision = driver.decide(&stack, env.state(), goal).unwrap();
            let progress = env.progress();
            assert_eq!(decision.progress.elapsed_ticks, progress.elapsed_ticks);
            assert_eq!(decision.progress.dwell_ticks, progress.dwell_ticks);
            assert_eq!(decision.progress.errors, env.frame().errors);
            assert_eq!(
                decision.navigation.is_some(),
                progress.elapsed_ticks.is_multiple_of(HOLD_TICKS)
            );
            assert_eq!(driver.decide(&stack, env.state(), goal).unwrap(), decision);
            env.step(decision.action).unwrap();
            last = Some(decision);
        }
        assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
        let decision = driver.decide(&stack, env.state(), goal).unwrap();
        assert_eq!(decision.progress.outcome, Some(GoalOutcome::Arrived));
        assert_eq!(decision.progress.dwell_ticks, 30);
        assert_eq!(
            decision.progress.elapsed_ticks,
            last.unwrap().progress.elapsed_ticks + 1
        );
        // The same tick repeats the decision; the ended attempt refuses the
        // next tick, which a neutral step of the ended world supplies.
        assert_eq!(driver.decide(&stack, env.state(), goal).unwrap(), decision);
        let mut state = env.state().clone();
        SurfaceSortieScenario::step(&mut state, &[], FLIGHT_DT);
        assert_eq!(
            driver.decide(&stack, &state, goal),
            Err(FlightStackError::Ended(GoalOutcome::Arrived))
        );
        let next = NavigationGoal { id: 2, ..goal };
        let again = driver.decide(&stack, &state, next).unwrap();
        assert_eq!(again.progress.elapsed_ticks, 0);
        assert_eq!(again.progress.dwell_ticks, 0);
        assert_eq!(again.progress.outcome, None);
        assert!(again.navigation.is_some());
        assert_eq!(
            again.progress.errors,
            state
                .flight_frame(0, goal.position, goal.velocity)
                .unwrap()
                .errors
        );
    }

    #[test]
    fn skipped_ticks_deadlines_and_bad_goals_are_explicit() {
        let stack = FlightStack::retained().unwrap();
        let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
        let mut driver = FlightStackDriver::new(0).unwrap();
        let mut goal = goal_of(&env, 1);
        goal.arrival.dwell_ticks = 5;
        goal.deadline_ticks = 12;
        for expected in 0..=12 {
            let decision = driver.decide(&stack, env.state(), goal).unwrap();
            assert_eq!(decision.progress.elapsed_ticks, expected);
            assert_eq!(
                decision.progress.outcome,
                (expected == 12).then_some(GoalOutcome::Deadline)
            );
            env.step(decision.action).unwrap();
        }
        assert_eq!(
            driver.decide(&stack, env.state(), goal),
            Err(FlightStackError::Ended(GoalOutcome::Deadline))
        );
        let goal = goal_of(&env, 2);
        driver.decide(&stack, env.state(), goal).unwrap();
        env.step(FlightAction::default()).unwrap();
        env.step(FlightAction::default()).unwrap();
        assert_eq!(
            driver.decide(&stack, env.state(), goal),
            Err(FlightStackError::Tick)
        );
        let fresh = driver
            .decide(&stack, env.state(), goal_of(&env, 3))
            .unwrap();
        assert_eq!(fresh.progress.elapsed_ticks, 0);
        for bad in [
            NavigationGoal {
                position: Vec2::new(f32::NAN, 0.0),
                ..goal
            },
            NavigationGoal {
                velocity: Vec2::new(21.0, 0.0),
                ..goal
            },
            NavigationGoal {
                deadline_ticks: 1801,
                ..goal
            },
            NavigationGoal {
                arrival: ArrivalCriteria {
                    dwell_ticks: 0,
                    ..goal.arrival
                },
                ..goal
            },
        ] {
            assert_eq!(
                driver.decide(&stack, env.state(), bad),
                Err(FlightStackError::Goal("invalid navigation goal"))
            );
        }
    }

    #[test]
    fn flight_actions_encode_the_environment_triple() {
        let action = FlightAction {
            steering: -0.25,
            thrust: 0.5,
            brake: 1.0,
        };
        let [controls, analog, wings] = flight_actions(action, PlayerId::PLAYER_2);
        assert_eq!(
            SurfaceSortieAction::decode(&controls),
            Some((
                PlayerId::PLAYER_2,
                SurfaceSortieAction {
                    horizontal: -0.25,
                    primary_held: false,
                    brake_held: false,
                    interact_held: false,
                }
            ))
        );
        assert_eq!(
            SurfaceFlightAction::decode(&analog),
            Some((
                PlayerId::PLAYER_2,
                SurfaceFlightAction {
                    thrust: 0.5,
                    brake: 1.0
                }
            ))
        );
        assert_eq!(
            SurfaceWingAction::decode(&wings),
            Some((PlayerId::PLAYER_2, SurfaceWingAction { closed: false }))
        );
    }
}
