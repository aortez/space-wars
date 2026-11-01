//! Composed navigation boundary: a relative heading and speed command drives the
//! exported motor network inside the flight environment for a held number of
//! ticks per decision. The navigation side sees the flight policy observation;
//! the motor network owns steering, thrust and brake at every tick. A loaded
//! navigation network can issue the commands itself.
use scenario_spacewars::surface_sortie::{
    flight_environment::{
        FlightAction, FlightEnvironment, FlightTask, FlightTrace, generation,
        generation::FlightTaskBatch,
        motor::{self, MotorCommand},
    },
    flight_policy_observation::{FlightPolicyInputs, schema},
};
use serde::Deserialize;
use serde_json::{Value, json};
use spacewars_ai::learned_flight::{ARCHITECTURE as NAVIGATION_ARCHITECTURE, FlightPolicyFile};
use spacewars_ai::learned_motor::{
    ARCHITECTURE, MOVEMENT_ARCHITECTURE, MotorPolicy, MotorPolicyFile, RESIDUAL_ARCHITECTURE,
};
use spacewars_ai::learned_navigation::{
    COMMAND_CONTRACT, COMMAND_CONTRACT_V2, FORMAT_VERSION, FORMAT_VERSION_V2, MAX_SPEED,
    MAX_SPEED_V2, NavigationCommand, NavigationPolicy,
};
use std::io::{self, BufRead, Write};

const MAX_HOLD: u32 = 60;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Schema,
    Generate {
        seed: u64,
        per_family: usize,
        max_rounds: u64,
    },
    GenerateTransfers {
        seed: u64,
        count: usize,
        max_attempts: u64,
        #[serde(default)]
        range: generation::transfer::TransferRange,
    },
    Reset {
        tasks: Vec<FlightTask>,
    },
    ResetAt {
        index: usize,
        task: FlightTask,
    },
    LoadMotorPolicy {
        path: String,
    },
    LoadNavigationPolicy {
        path: String,
    },
    InferNavigation {
        inputs: Vec<FlightPolicyInputs>,
    },
    StepCommand {
        commands: Vec<NavigationCommand>,
        hold: u32,
    },
    StepPolicy {
        hold: u32,
    },
    /// Holds every command as an absolute heading without stepping, so a
    /// scripted motor can fly the held command tick by tick.
    SetCommand {
        commands: Vec<NavigationCommand>,
    },
    /// Steps every open environment one tick with the given raw action under
    /// its held command, as the motor network does inside `step_command`.
    StepActions {
        actions: Vec<FlightAction>,
    },
    State,
    Restore {
        traces: Vec<FlightTrace>,
    },
    Close,
}

#[derive(Default)]
struct Bridge {
    envs: Vec<FlightEnvironment>,
    motor: Option<MotorPolicy>,
    navigation: Option<NavigationPolicy>,
    commands: Vec<Option<MotorCommand>>,
}

impl Bridge {
    fn observations(&self) -> Result<Value, String> {
        let items = self
            .envs
            .iter()
            .zip(&self.commands)
            .map(|(env, command)| {
                let inputs = match env.policy_observation() {
                    Ok(o) => Some(o.inputs),
                    Err(_) if env.progress().outcome.is_some() => None,
                    Err(e) => return Err(e.to_string()),
                };
                let motor_inputs = match (command, &self.motor) {
                    (Some(command), Some(motor)) if env.progress().outcome.is_none() => Some(
                        motor::command_inputs_contract(env, motor.contract(), *command)?,
                    ),
                    _ => None,
                };
                Ok(
                    json!({"inputs":inputs,"frame":env.frame(),"progress":env.progress(),
                    "evidence":env.trace().steps.last().map(|s|s.evidence),
                    "command":command,"motor_inputs":motor_inputs,
                    "previous_action":env.trace().steps.last().and_then(|s|s.action)}),
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(json!({"items":items}))
    }

    /// Holds every command as an absolute heading while the motor network
    /// flies the given number of ticks, stopping early at a physical outcome.
    /// Resolves every command to an absolute heading under the loaded
    /// contract and holds it for its environment.
    fn hold_commands(&mut self, commands: Vec<NavigationCommand>) -> Result<(), String> {
        let motor = self.motor.as_ref().ok_or("motor policy not loaded")?;
        if self.envs.is_empty()
            || commands.len() != self.envs.len()
            || self.envs.iter().any(|e| e.progress().outcome.is_some())
        {
            return Err("invalid navigation step batch".into());
        }
        // Without a navigation network the cap and the command's frame follow
        // the motor's contract.
        let contract = match &self.navigation {
            Some(navigation) => navigation.contract(),
            None => motor.contract(),
        };
        let max_speed = spacewars_ai::learned_navigation::max_speed(contract)?;
        for ((env, slot), command) in self.envs.iter().zip(&mut self.commands).zip(commands) {
            let own = env
                .state()
                .free_flight_observation(0)
                .ok_or("flight state unavailable")?;
            let goal_velocity = env.frame().target_velocity - own.frame_velocity;
            *slot = Some(command.absolute_under(
                contract,
                env.frame().angle,
                goal_velocity,
                max_speed,
            )?);
        }
        Ok(())
    }

    fn drive(&mut self, commands: Vec<NavigationCommand>, hold: u32) -> Result<Value, String> {
        if !(1..=MAX_HOLD).contains(&hold) {
            return Err("invalid navigation step batch".into());
        }
        self.hold_commands(commands)?;
        let motor = self.motor.as_ref().ok_or("motor policy not loaded")?;
        let mut ticks = Vec::with_capacity(self.envs.len());
        for (env, slot) in self.envs.iter_mut().zip(&self.commands) {
            let absolute = slot.ok_or("command not held")?;
            let mut taken = 0;
            while taken < hold && env.progress().outcome.is_none() {
                let inputs = motor::command_inputs_contract(env, motor.contract(), absolute)?;
                let action = motor.infer(&inputs)?.action;
                env.step(action)?;
                taken += 1;
            }
            ticks.push(taken);
        }
        let mut result = self.observations()?;
        result["ticks"] = json!(ticks);
        Ok(result)
    }

    fn request(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::Schema => Ok(json!({"protocol":2,"schema":schema(),
                "command_contract":COMMAND_CONTRACT,"max_hold":MAX_HOLD,"max_speed":MAX_SPEED,
                "command_contract_v2":COMMAND_CONTRACT_V2,"max_speed_v2":MAX_SPEED_V2,
                "navigation_architecture":NAVIGATION_ARCHITECTURE,"navigation_format_version":FORMAT_VERSION,
                "navigation_format_version_v2":FORMAT_VERSION_V2,
                "motor_schema":motor::schema(),"motor_schema_v2":motor::schema_v2(),
                "motor_architectures":[ARCHITECTURE,RESIDUAL_ARCHITECTURE,MOVEMENT_ARCHITECTURE]})),
            Request::Generate {
                seed,
                per_family,
                max_rounds,
            } => {
                let batch = FlightTaskBatch::generate(seed, per_family, max_rounds)?;
                let attempts: Vec<_> = batch
                    .attempts
                    .into_iter()
                    .map(|r| json!({"candidate":r.candidate,"admitted":r.admitted,"rejection":r.rejection}))
                    .collect();
                Ok(json!({"complete":batch.complete,"attempts":attempts}))
            }
            Request::GenerateTransfers {
                seed,
                count,
                max_attempts,
                range,
            } => {
                let attempts =
                    generation::transfer::generate_transfers(seed, count, max_attempts, range)?;
                Ok(json!({"attempts":attempts}))
            }
            Request::Reset { tasks } => {
                if tasks.is_empty() || tasks.len() > 16 {
                    return Err("invalid environment count".into());
                }
                self.envs = tasks
                    .into_iter()
                    .map(FlightEnvironment::new)
                    .collect::<Result<_, _>>()?;
                self.commands = vec![None; self.envs.len()];
                self.observations()
            }
            Request::ResetAt { index, task } => {
                let env = self
                    .envs
                    .get_mut(index)
                    .ok_or("invalid environment index")?;
                env.reset(task)?;
                self.commands[index] = None;
                self.observations()
            }
            Request::LoadMotorPolicy { path } => {
                let file: MotorPolicyFile =
                    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                self.motor = Some(MotorPolicy::from_file(file)?);
                Ok(json!({"loaded":true}))
            }
            Request::LoadNavigationPolicy { path } => {
                let file: FlightPolicyFile =
                    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                let navigation = NavigationPolicy::from_file(file)?;
                if let Some(motor) = &self.motor
                    && motor.contract() != navigation.contract()
                {
                    return Err("navigation and motor contracts differ".into());
                }
                let contract = navigation.contract();
                self.navigation = Some(navigation);
                Ok(json!({"loaded":true,"contract":contract}))
            }
            Request::InferNavigation { inputs } => {
                let navigation = self
                    .navigation
                    .as_ref()
                    .ok_or("navigation policy not loaded")?;
                let outputs = inputs
                    .iter()
                    .map(|x| navigation.infer(x))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({"outputs":outputs}))
            }
            Request::StepCommand { commands, hold } => self.drive(commands, hold),
            Request::SetCommand { commands } => {
                self.hold_commands(commands)?;
                self.observations()
            }
            Request::StepActions { actions } => {
                if self.envs.is_empty()
                    || actions.len() != self.envs.len()
                    || self.commands.iter().any(Option::is_none)
                {
                    return Err("invalid action step batch".into());
                }
                for (env, action) in self.envs.iter_mut().zip(actions) {
                    if env.progress().outcome.is_none() {
                        env.step(action)?;
                    }
                }
                self.observations()
            }
            Request::StepPolicy { hold } => {
                let navigation = self
                    .navigation
                    .as_ref()
                    .ok_or("navigation policy not loaded")?;
                let commands = self
                    .envs
                    .iter()
                    .map(|env| Ok(navigation.infer(&env.policy_observation()?.inputs)?.command))
                    .collect::<Result<Vec<_>, &'static str>>()?;
                let mut result = self.drive(commands.clone(), hold)?;
                result["commands"] = json!(commands);
                Ok(result)
            }
            Request::State => {
                Ok(json!({"traces":self.envs.iter().map(|e|e.trace()).collect::<Vec<_>>()}))
            }
            Request::Restore { traces } => {
                if traces.is_empty() || traces.len() > 16 {
                    return Err("invalid restore batch".into());
                }
                self.envs = traces
                    .iter()
                    .map(FlightEnvironment::replay)
                    .collect::<Result<_, _>>()?;
                self.commands = vec![None; self.envs.len()];
                self.observations()
            }
            Request::Close => Ok(json!({"closed":true})),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bridge = Bridge::default();
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request: Result<Request, _> = serde_json::from_str(&line?);
        let close = matches!(request, Ok(Request::Close));
        match request
            .map_err(|e| e.to_string())
            .and_then(|r| bridge.request(r))
        {
            Ok(result) => serde_json::to_writer(&mut out, &json!({"ok":true,"result":result}))?,
            Err(error) => serde_json::to_writer(&mut out, &json!({"ok":false,"error":error}))?,
        }
        writeln!(out)?;
        out.flush()?;
        if close {
            break;
        }
    }
    Ok(())
}
