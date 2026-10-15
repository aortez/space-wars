//! Integer diagnostic world: move to a target, then remain there for a dwell.
//! No ship code, physics solver, random state, or rendering state affects motion.

#![forbid(unsafe_code)]

use engine_common::{
    Action, Camera2, Observation, RenderCircle, RenderColor, RenderFrame, RenderPoint,
    RenderPrimitive, Scenario, StepResult, Stroke, TickModel,
};
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

pub const WORLD_VERSION: &str = "line_mover_v1";
pub const MIN_POSITION: i32 = -4;
pub const MAX_POSITION: i32 = 4;
pub const DISTANCE_SCALE: f32 = 8.0;
pub const MAX_EPISODE_TICKS: u32 = 256;
pub const DT: Duration = Duration::from_nanos(16_666_667);
const ACTION_KIND: u32 = 0x4c4d_0001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineConfig {
    pub start: i32,
    pub target: i32,
    pub dwell_ticks: u32,
    pub max_ticks: u32,
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            start: -2,
            target: 1,
            dwell_ticks: 2,
            max_ticks: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError;

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("line positions must be in [-4, 4] and 1 <= dwell <= max_ticks <= 256")
    }
}

impl std::error::Error for ConfigError {}

impl LineConfig {
    pub fn validate(self) -> Result<(), ConfigError> {
        if !(MIN_POSITION..=MAX_POSITION).contains(&self.start)
            || !(MIN_POSITION..=MAX_POSITION).contains(&self.target)
            || self.dwell_ticks == 0
            || self.dwell_ticks > self.max_ticks
            || self.max_ticks > MAX_EPISODE_TICKS
        {
            return Err(ConfigError);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineAction {
    Left,
    Stay,
    Right,
}

impl LineAction {
    pub fn encode(self) -> Action {
        Action::scenario(
            ACTION_KIND,
            vec![match self {
                Self::Left => 0,
                Self::Stay => 1,
                Self::Right => 2,
            }],
        )
    }

    pub fn decode(action: &Action) -> Option<Self> {
        match action {
            Action::Scenario { kind, payload } if *kind == ACTION_KIND => {
                match payload.as_slice() {
                    [0] => Some(Self::Left),
                    [1] => Some(Self::Stay),
                    [2] => Some(Self::Right),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineOutcome {
    Arrived,
    Timeout,
    InvalidAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSnapshot {
    pub tick: u32,
    pub position: i32,
    pub target: i32,
    pub dwell: u32,
    pub outcome: Option<LineOutcome>,
}

#[derive(Debug, Clone)]
pub struct LineState {
    config: LineConfig,
    snapshot: LineSnapshot,
}

impl LineState {
    pub fn snapshot(&self) -> LineSnapshot {
        self.snapshot
    }

    /// The encoder has fixed meaning regardless of the sampled initial distance.
    pub fn inputs(&self) -> [f32; 1] {
        [(self.snapshot.target - self.snapshot.position) as f32 / DISTANCE_SCALE]
    }
}

pub struct LineMoverScenario;

impl LineMoverScenario {
    pub fn try_init(config: LineConfig) -> Result<LineState, ConfigError> {
        config.validate()?;
        Ok(LineState {
            snapshot: LineSnapshot {
                tick: 0,
                position: config.start,
                target: config.target,
                dwell: 0,
                outcome: None,
            },
            config,
        })
    }
}

impl Scenario for LineMoverScenario {
    type State = LineState;
    type Config = LineConfig;

    fn init(config: Self::Config, _seed: u64) -> Self::State {
        Self::try_init(config).expect("invalid line-mover configuration")
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() || state.snapshot.outcome.is_some() {
            return StepResult {
                terminated: state.snapshot.outcome.is_some(),
            };
        }
        let action = match actions {
            [] => Some(LineAction::Stay),
            [action] => LineAction::decode(action),
            _ => None,
        };
        let s = &mut state.snapshot;
        s.tick += 1;
        if let Some(action) = action {
            let delta = match action {
                LineAction::Left => -1,
                LineAction::Stay => 0,
                LineAction::Right => 1,
            };
            s.position = (s.position + delta).clamp(MIN_POSITION, MAX_POSITION);
            // Count consecutive post-step states at the target; arrival wins an
            // exact-deadline tie. An initially coincident target still needs dwell.
            s.dwell = if s.position == s.target {
                s.dwell + 1
            } else {
                0
            };
            if s.dwell >= state.config.dwell_ticks {
                s.outcome = Some(LineOutcome::Arrived);
            } else if s.tick >= state.config.max_ticks {
                s.outcome = Some(LineOutcome::Timeout);
            }
        } else {
            s.outcome = Some(LineOutcome::InvalidAction);
        }
        StepResult {
            terminated: s.outcome.is_some(),
        }
    }

    fn observe(state: &Self::State) -> Observation {
        Observation {
            payload: serde_json::to_vec(&state.inputs()).expect("finite line observation"),
        }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, 0.0), 10.0));
        for (x, radius, color) in [
            (state.snapshot.target, 0.35, RenderColor::rgb(0.2, 0.9, 0.3)),
            (
                state.snapshot.position,
                0.18,
                RenderColor::rgb(0.3, 0.5, 1.0),
            ),
        ] {
            frame.push_primitive(
                0,
                RenderPrimitive::Circle(RenderCircle {
                    center: RenderPoint::new(x as f32, 0.0),
                    radius,
                    fill: None,
                    stroke: Some(Stroke::new(color, 0.06)),
                }),
            );
        }
        frame
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

#[cfg(test)]
mod tests;
