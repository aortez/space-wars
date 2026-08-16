use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_rover_lab::{RoverLabAction, RoverLabConfig, RoverLabScenario, RoverLabState};

use super::{
    ClientScenario, RenderBackend, ScenarioCapabilities, ScenarioRegistration, ScenarioStartMode,
};
use crate::input::{ClientInput, GameKey};
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "rover-lab",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
    },
    controls_help: "Rover Lab: W drives forward, S brakes, X drives in reverse, and R resets the articulated rover.",
    create,
};

pub(crate) struct RoverLabClientScenario {
    pub(crate) state: RoverLabState,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
) -> Box<dyn ClientScenario> {
    Box::new(RoverLabClientScenario {
        state: RoverLabScenario::init(RoverLabConfig::default(), seed),
    })
}

impl ClientScenario for RoverLabClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        RoverLabScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        RoverLabScenario::step(&mut self.state, actions, dt)
    }

    fn map_keyboard_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        let brake = input.is_pressed(GameKey::P1Brake);
        let throttle = if brake {
            0.0
        } else if input.is_pressed(GameKey::P1Thrust) {
            1.0
        } else if input.is_pressed(GameKey::P1Reverse) {
            -1.0
        } else {
            0.0
        };
        vec![RoverLabAction::drive(throttle, brake)]
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![RoverLabScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
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
