//! The line-mover adapter supplies facts, ordinary actions, and integer fitness.

use crate::evolution::{EvaluationTask, EvolutionError, TaskManifest};
use engine_common::Scenario;
use scenario_line_mover::{
    DT, LineAction, LineConfig, LineMoverScenario, LineOutcome, LineSnapshot, MAX_POSITION,
    MIN_POSITION, WORLD_VERSION,
};
use serde::{Deserialize, Serialize};
use spacewars_ai::model::{
    Activation, DenseGenome, DenseLayer, DenseModel, MODEL_VERSION, ModelContract, ModelMemory,
};

pub fn contract() -> ModelContract {
    ModelContract {
        organism: "diagnostic_line_mover".into(),
        role: "movement_v1".into(),
        observation: "signed_distance_over_8_v1".into(),
        action: "left_stay_right_logits_v1".into(),
        inputs: 1,
        outputs: 3,
        cadence_ticks: 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownPolicy {
    Oracle,
    Idle,
    Wrong,
}

pub fn known_genome(policy: KnownPolicy) -> DenseGenome {
    let weights = match policy {
        KnownPolicy::Oracle => vec![-1.0, 0.0, 1.0],
        KnownPolicy::Idle => vec![0.0; 3],
        KnownPolicy::Wrong => vec![1.0, 0.0, -1.0],
    };
    DenseGenome {
        version: MODEL_VERSION,
        contract: contract(),
        memory: ModelMemory::None,
        layers: vec![DenseLayer {
            outputs: 3,
            activation: Activation::Linear,
            weights,
            biases: vec![0.0, 0.0625, 0.0],
        }],
    }
}

/// Ties prefer Stay, then Left, then Right. All-zero outputs are neutral.
pub fn decode(outputs: &[f32]) -> Result<LineAction, EvolutionError> {
    if outputs.len() != 3 || outputs.iter().any(|v| !v.is_finite()) {
        return Err(EvolutionError("invalid line-mover outputs".into()));
    }
    let mut winner = 1;
    for index in [0, 2] {
        if outputs[index] > outputs[winner] {
            winner = index;
        }
    }
    Ok([LineAction::Left, LineAction::Stay, LineAction::Right][winner])
}

pub enum LineController<'a> {
    Rule(KnownPolicy),
    Model(&'a DenseModel),
}

impl LineController<'_> {
    fn id(&self) -> String {
        match self {
            Self::Rule(p) => format!("line_rule_{p:?}_v1").to_lowercase(),
            Self::Model(m) => m.id().into(),
        }
    }
    fn decide(&self, inputs: &[f32]) -> Result<(Option<Vec<f32>>, LineAction), EvolutionError> {
        match self {
            Self::Model(model) => {
                let raw = model
                    .infer(inputs)
                    .map_err(|e| EvolutionError(e.to_string()))?;
                let action = decode(&raw)?;
                Ok((Some(raw), action))
            }
            Self::Rule(policy) => {
                let direction = inputs[0].signum();
                let action = match policy {
                    KnownPolicy::Idle => LineAction::Stay,
                    _ if inputs[0] == 0.0 => LineAction::Stay,
                    KnownPolicy::Oracle if direction > 0.0 => LineAction::Right,
                    KnownPolicy::Wrong if direction < 0.0 => LineAction::Right,
                    _ => LineAction::Left,
                };
                Ok((None, action))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineFitness {
    // Field order is the lexicographic selection priority, not a weighted sum.
    pub completed: u32,
    pub proximity_points: u32,
    pub spare_ticks: u32,
}

impl LineFitness {
    fn add(&mut self, other: Self) {
        self.completed += other.completed;
        self.proximity_points += other.proximity_points;
        self.spare_ticks += other.spare_ticks;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOutcome {
    Arrived,
    Timeout,
    InvalidOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineTrace {
    pub before: LineSnapshot,
    pub inputs: [f32; 1],
    pub raw_outputs: Option<Vec<f32>>,
    pub action: Option<LineAction>,
    pub after: LineSnapshot,
    pub fitness: LineFitness,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineEpisode {
    pub version: u32,
    pub world: String,
    pub controller: String,
    pub config: LineConfig,
    pub final_state: LineSnapshot,
    pub outcome: DiagnosticOutcome,
    pub fitness: LineFitness,
    pub trace: Vec<LineTrace>,
}

fn fitness(state: LineSnapshot, config: LineConfig) -> LineFitness {
    let arrived = state.outcome == Some(LineOutcome::Arrived);
    LineFitness {
        completed: u32::from(arrived),
        proximity_points: (MAX_POSITION - MIN_POSITION) as u32
            - state.target.abs_diff(state.position),
        spare_ticks: if arrived {
            config.max_ticks - state.tick
        } else {
            0
        },
    }
}

pub fn run_episode(
    controller: LineController<'_>,
    config: LineConfig,
    retain_trace: bool,
) -> Result<LineEpisode, EvolutionError> {
    if let LineController::Model(model) = &controller {
        model
            .require_contract(&contract())
            .map_err(|e| EvolutionError(e.to_string()))?;
    }
    let mut state =
        LineMoverScenario::try_init(config).map_err(|e| EvolutionError(e.to_string()))?;
    let mut trace = Vec::new();
    let outcome = loop {
        let before = state.snapshot();
        let inputs: [f32; 1] = serde_json::from_slice(&LineMoverScenario::observe(&state).payload)
            .map_err(|e| EvolutionError(e.to_string()))?;
        match controller.decide(&inputs) {
            Ok((raw_outputs, action)) => {
                LineMoverScenario::step(&mut state, &[action.encode()], DT);
                if retain_trace {
                    trace.push(LineTrace {
                        before,
                        inputs,
                        raw_outputs,
                        action: Some(action),
                        after: state.snapshot(),
                        fitness: fitness(state.snapshot(), config),
                        error: None,
                    });
                }
            }
            Err(e) => {
                if retain_trace {
                    trace.push(LineTrace {
                        before,
                        inputs,
                        raw_outputs: None,
                        action: None,
                        after: before,
                        fitness: LineFitness::default(),
                        error: Some(e.to_string()),
                    });
                }
                break DiagnosticOutcome::InvalidOutput;
            }
        }
        match state.snapshot().outcome {
            Some(LineOutcome::Arrived) => break DiagnosticOutcome::Arrived,
            Some(LineOutcome::Timeout) => break DiagnosticOutcome::Timeout,
            Some(LineOutcome::InvalidAction) => {
                return Err(EvolutionError(
                    "adapter emitted an invalid scenario action".into(),
                ));
            }
            None => {}
        }
    };
    Ok(LineEpisode {
        version: 1,
        world: WORLD_VERSION.into(),
        controller: controller.id(),
        config,
        final_state: state.snapshot(),
        outcome,
        fitness: if outcome == DiagnosticOutcome::InvalidOutput {
            LineFitness::default()
        } else {
            fitness(state.snapshot(), config)
        },
        trace,
    })
}

#[derive(Debug, Clone)]
pub struct LineTask {
    cases: Vec<LineConfig>,
}

impl Default for LineTask {
    fn default() -> Self {
        Self {
            cases: (MIN_POSITION..=MAX_POSITION)
                .flat_map(|start| {
                    (MIN_POSITION..=MAX_POSITION).map(move |target| LineConfig {
                        start,
                        target,
                        ..LineConfig::default()
                    })
                })
                .collect(),
        }
    }
}

impl LineTask {
    pub fn cases(&self) -> &[LineConfig] {
        &self.cases
    }
}

impl EvaluationTask for LineTask {
    type Fitness = LineFitness;
    fn manifest(&self) -> TaskManifest {
        TaskManifest {
            id: "line_arrival_all_pairs_v1".into(),
            contract: contract(),
            configuration: serde_json::json!({"world": WORLD_VERSION, "cases": self.cases,
                "comparison": "completed_then_proximity_then_spare_ticks_v1"}),
        }
    }
    fn evaluate(&self, model: &DenseModel) -> Result<LineFitness, EvolutionError> {
        let mut total = LineFitness::default();
        for &config in &self.cases {
            total.add(run_episode(LineController::Model(model), config, false)?.fitness);
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests;
