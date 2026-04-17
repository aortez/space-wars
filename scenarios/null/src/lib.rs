//! Null scenario: minimal [`Scenario`] impl for smoke-testing the workspace.
//!
//! No state, no entities. Exists so the host binaries have something to
//! compile against until real scenarios land.

use std::time::Duration;

use engine_common::{Action, Observation, RenderFrame, Scenario, StepResult, TickModel};

pub struct NullScenario;

pub struct NullState;

#[derive(Default)]
pub struct NullConfig;

impl Scenario for NullScenario {
    type State = NullState;
    type Config = NullConfig;

    fn init(_config: Self::Config, _seed: u64) -> Self::State {
        NullState
    }

    fn step(_state: &mut Self::State, _actions: &[Action], _dt: Duration) -> StepResult {
        StepResult::default()
    }

    fn observe(_state: &Self::State) -> Observation {
        Observation { payload: Vec::new() }
    }

    fn render_frame(_state: &Self::State) -> RenderFrame {
        RenderFrame::default()
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}
