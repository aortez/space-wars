use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_pizza::{PizzaBounds, PizzaConfig, PizzaScenario, PizzaState};

use super::{
    ClientScenario, RenderBackend, ScenarioCapabilities, ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "pizza",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: true,
        player_zoom: false,
        game_over: false,
    },
    controls_help: "Pizza: click empty space to create a ball, or click an existing ball to grab it. Drag for a rubbery pull and release to fling.",
    create,
};

pub(crate) struct PizzaClientScenario {
    pub(crate) state: PizzaState,
}

fn create(
    seed: u64,
    settings: &Settings,
    viewport: Viewport,
    _mode: ScenarioStartMode,
) -> Box<dyn ClientScenario> {
    let setup = settings.pizza.normalized();
    Box::new(PizzaClientScenario {
        state: PizzaScenario::init(
            PizzaConfig {
                desired_ball_count: setup.desired_balls as usize,
                ball_spawn_rate: setup.ball_spawn_rate,
                bounds: PizzaBounds::from_aspect_ratio(viewport.aspect_ratio()),
            },
            seed,
        ),
    })
}

impl ClientScenario for PizzaClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        PizzaScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        PizzaScenario::step(&mut self.state, actions, dt)
    }

    fn map_keyboard_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        Vec::new()
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![PizzaScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.state.set_aspect_ratio(viewport.aspect_ratio());
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
