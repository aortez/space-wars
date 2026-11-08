//! Synchronous JSON-lines training boundary. Each action is one real 60 Hz tick.
use scenario_spacewars::surface_sortie::{
    flight_environment::{
        FlightAction, FlightEnvironment, FlightTask, FlightTrace,
        generation::{FlightCandidate, FlightTaskBatch},
    },
    flight_policy_observation::{FlightPolicyInputs, schema},
};
use serde::Deserialize;
use serde_json::{Value, json};
use spacewars_ai::learned_flight::{
    ARCHITECTURE, FlightPolicy, FlightPolicyFile, GOAL_RESIDUAL_ARCHITECTURE,
};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Schema,
    Generate {
        seed: u64,
        per_family: usize,
        max_rounds: u64,
    },
    Inspect {
        candidate: FlightCandidate,
    },
    Reset {
        tasks: Vec<FlightTask>,
    },
    ResetAt {
        index: usize,
        task: FlightTask,
    },
    Step {
        actions: Vec<FlightAction>,
    },
    State,
    Restore {
        traces: Vec<FlightTrace>,
    },
    LoadPolicy {
        path: String,
    },
    Infer {
        inputs: Vec<FlightPolicyInputs>,
    },
    StepPolicy,
    Close,
}

#[derive(Default)]
struct Bridge {
    envs: Vec<FlightEnvironment>,
    policy: Option<FlightPolicy>,
}
impl Bridge {
    fn observations(&self) -> Result<Value, String> {
        let items=self.envs.iter().map(|env| {
            let inputs=match env.policy_observation() {
                Ok(o)=>Some(o.inputs),
                Err(_) if env.progress().outcome.is_some()=>None,
                Err(e)=>return Err(e.to_string()),
            };
            Ok(json!({"inputs":inputs,"frame":env.frame(),"progress":env.progress(),"evidence":env.trace().steps.last().map(|s|s.evidence)}))
        }).collect::<Result<Vec<_>,String>>()?;
        Ok(json!({"items":items}))
    }
    fn request(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::Schema => Ok(
                json!({"protocol":2,"schema":schema(),"architecture":ARCHITECTURE,
                    "architectures":[ARCHITECTURE,GOAL_RESIDUAL_ARCHITECTURE]}),
            ),
            Request::Generate {
                seed,
                per_family,
                max_rounds,
            } => {
                let batch = FlightTaskBatch::generate(seed, per_family, max_rounds)?;
                let attempts:Vec<_>=batch.attempts.into_iter().map(|r|json!({"candidate":r.candidate,"admitted":r.admitted,"rejection":r.rejection})).collect();
                Ok(json!({"complete":batch.complete,"attempts":attempts}))
            }
            Request::Inspect { candidate } => {
                let report = if candidate.recipe.starts_with("aligned-approach-v1/") {
                    candidate.inspect_aligned_approach()
                } else if candidate.recipe.starts_with("settle-recover-v1/") {
                    candidate.inspect_settle_recover()
                } else if candidate.recipe.starts_with("hold-robustness-v1/") {
                    candidate.inspect_hold_robustness()
                } else if candidate.recipe.starts_with("hold-first-exit-v1/") {
                    candidate.inspect_hold_first_exit()
                } else if candidate.recipe.starts_with("hold-position-v1/") {
                    candidate.inspect_hold_position()
                } else {
                    candidate.inspect_clear_control()
                };
                Ok(
                    json!({"candidate":report.candidate,"admitted":report.admitted,
                    "rejection":report.rejection,"evidence":report.evidence}),
                )
            }
            Request::Reset { tasks } => {
                if tasks.is_empty() || tasks.len() > 16 {
                    return Err("invalid environment count".into());
                }
                self.envs = tasks
                    .into_iter()
                    .map(FlightEnvironment::new)
                    .collect::<Result<_, _>>()?;
                self.observations()
            }
            Request::ResetAt { index, task } => {
                let env = self
                    .envs
                    .get_mut(index)
                    .ok_or("invalid environment index")?;
                env.reset(task)?;
                self.observations()
            }
            Request::Step { actions } => {
                if actions.len() != self.envs.len()
                    || self.envs.is_empty()
                    || self.envs.iter().any(|e| e.progress().outcome.is_some())
                {
                    return Err("invalid batch or terminal environment".into());
                }
                for (env, action) in self.envs.iter_mut().zip(actions) {
                    env.step(action)?;
                }
                self.observations()
            }
            Request::State => {
                Ok(json!({"traces":self.envs.iter().map(|e|e.trace()).collect::<Vec<_>>()}))
            }
            Request::Restore { traces } => {
                if traces.is_empty() || traces.len() > 16 {
                    return Err("invalid trace count".into());
                }
                let envs = traces
                    .iter()
                    .map(FlightEnvironment::replay)
                    .collect::<Result<_, _>>()?;
                self.envs = envs;
                self.observations()
            }
            Request::LoadPolicy { path } => {
                let file: FlightPolicyFile =
                    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                self.policy = Some(FlightPolicy::from_file(file)?);
                Ok(json!({"loaded":true}))
            }
            Request::Infer { inputs } => {
                let policy = self.policy.as_ref().ok_or("policy not loaded")?;
                let values = inputs
                    .iter()
                    .map(|x| policy.infer(x))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({"outputs":values}))
            }
            Request::StepPolicy => {
                let policy = self.policy.as_ref().ok_or("policy not loaded")?;
                let actions = self
                    .envs
                    .iter()
                    .map(|env| {
                        policy
                            .infer(&env.policy_observation()?.inputs)
                            .map(|o| o.action)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
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
        let result = request
            .map_err(|e| e.to_string())
            .and_then(|r| bridge.request(r));
        match result {
            Ok(value) => serde_json::to_writer(&mut out, &json!({"ok":true,"result":value}))?,
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
