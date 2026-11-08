//! Separate JSON-lines motor boundary; the legacy flight protocol is unchanged.
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightAction,
    motor::{self, MotorEnvironment, MotorTask, MotorTrace},
};
use serde::Deserialize;
use serde_json::{Value, json};
use spacewars_ai::learned_motor::{
    ARCHITECTURE, MOVEMENT_ARCHITECTURE, MotorPolicy, MotorPolicyFile, RESIDUAL_ARCHITECTURE,
};
use std::io::{self, BufRead, Write};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Schema,
    Reset { tasks: Vec<MotorTask> },
    ResetAt { index: usize, task: MotorTask },
    Step { actions: Vec<FlightAction> },
    State,
    Restore { traces: Vec<MotorTrace> },
    LoadPolicy { path: String },
    Infer { inputs: Vec<Vec<f32>> },
    StepPolicy,
    Close,
}
#[derive(Default)]
struct Bridge {
    envs: Vec<MotorEnvironment>,
    policy: Option<MotorPolicy>,
}
impl Bridge {
    fn observations(&self) -> Value {
        json!({"items":self.envs.iter().map(|e|e.item()).collect::<Vec<_>>()})
    }
    fn request(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::Schema => Ok(
                json!({"protocol":2,"schema":motor::schema(),"schema_v2":motor::schema_v2(),
                    "schema_v3":motor::schema_v3(),"command_hold_ticks":motor::COMMAND_HOLD_TICKS,
                    "max_speed_v2":motor::MAX_SPEED_V2,"architecture":ARCHITECTURE,
                    "architectures":[ARCHITECTURE,RESIDUAL_ARCHITECTURE,MOVEMENT_ARCHITECTURE]}),
            ),
            Request::Reset { tasks } => {
                if tasks.is_empty() || tasks.len() > 16 {
                    return Err("invalid motor batch".into());
                }
                self.envs = tasks
                    .into_iter()
                    .map(MotorEnvironment::new)
                    .collect::<Result<_, _>>()?;
                Ok(self.observations())
            }
            Request::ResetAt { index, task } => {
                let env = self.envs.get_mut(index).ok_or("invalid index")?;
                *env = MotorEnvironment::new(task)?;
                Ok(self.observations())
            }
            Request::Step { actions } => {
                if self.envs.is_empty()
                    || actions.len() != self.envs.len()
                    || self
                        .envs
                        .iter()
                        .any(|e| e.item().progress.outcome.is_some())
                    || actions.iter().any(|a| {
                        !a.steering.is_finite()
                            || !(-1.0..=1.0).contains(&a.steering)
                            || !a.thrust.is_finite()
                            || !(0.0..=1.0).contains(&a.thrust)
                            || !a.brake.is_finite()
                            || !(0.0..=1.0).contains(&a.brake)
                    })
                {
                    return Err("invalid motor step batch".into());
                }
                for (env, action) in self.envs.iter_mut().zip(actions) {
                    env.step(action)?;
                }
                Ok(self.observations())
            }
            Request::State => {
                Ok(json!({"traces":self.envs.iter().map(|e|e.trace()).collect::<Vec<_>>()}))
            }
            Request::Restore { traces } => {
                if traces.is_empty() || traces.len() > 16 {
                    return Err("invalid motor restore batch".into());
                }
                self.envs = traces
                    .iter()
                    .map(MotorEnvironment::replay)
                    .collect::<Result<_, _>>()?;
                Ok(self.observations())
            }
            Request::LoadPolicy { path } => {
                let file: MotorPolicyFile =
                    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                self.policy = Some(MotorPolicy::from_file(file)?);
                Ok(json!({"loaded":true}))
            }
            Request::Infer { inputs } => {
                let policy = self.policy.as_ref().ok_or("motor policy not loaded")?;
                let outputs = inputs
                    .iter()
                    .map(|x| policy.infer(x))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({"outputs":outputs}))
            }
            Request::StepPolicy => {
                let policy = self.policy.as_ref().ok_or("motor policy not loaded")?;
                let actions = self
                    .envs
                    .iter()
                    .map(|e| policy.infer(&e.item().inputs).map(|o| o.action))
                    .collect::<Result<_, _>>()?;
                self.request(Request::Step { actions })
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
