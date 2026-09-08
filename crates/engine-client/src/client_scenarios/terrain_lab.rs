use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_terrain_lab::{
    MiningControls, TerrainLabAction, TerrainLabConfig, TerrainLabScenario, TerrainLabState,
    ToolControls,
};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::{ClientInput, GameKey};
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "terrain-lab",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        pointer_input: true,
        benchmark: false,
        player_zoom: true,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Terrain Lab: hold mouse/touch to aim and mine. E mines; W/S or up/down rotate aim; A/D or left/right walk; Space jumps or gets up when prone. T cycles Precision Laser / Drill / Excavator; V cycles Overview / Mining / Detail views. The highlighted cells show the next cut. Cut a section completely free and it becomes a moving, collidable fragment that you can keep mining. Hard fragment impacts damage both pieces of terrain; gentle contact is harmless. Impact destruction recovers nothing. Detached ore stays inside the rock until mined. Rock breaks sooner than ore; recovered amounts measure material area. Gamepad: left stick walks, right stick aims, d-pad up/down rotates aim, left shoulder or right trigger mines, bottom face button jumps or gets up, top face button changes tool, right shoulder changes view. Switch Pro: B jumps or gets up, X changes tool, R changes view, L or ZR mines, + pauses. Hold J or the left trigger (ZL on Switch) for debug mode, then press X/right face for a crater or K/left face for a tunnel. Debug cuts recover nothing and require a release before repeating. R on the keyboard restarts; Start or Esc pauses.",
    create,
};

struct TerrainLabClientScenario {
    state: TerrainLabState,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(TerrainLabClientScenario {
        state: TerrainLabScenario::init(TerrainLabConfig::default(), seed),
    }))
}

impl ClientScenario for TerrainLabClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }
    fn tick_model(&self) -> TickModel {
        TerrainLabScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        TerrainLabScenario::step(&mut self.state, actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        vec![
            controls(input),
            mining_controls(input),
            tool_controls(input),
        ]
    }
    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![TerrainLabScenario::render_frame(&self.state)]
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }
    fn zoom_player_in(&mut self, player: usize) {
        if player == 0 {
            self.state.zoom_in();
        }
    }
    fn zoom_player_out(&mut self, player: usize) {
        if player == 0 {
            self.state.zoom_out();
        }
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

fn controls(input: &ClientInput) -> Action {
    let (walk, jump, crater) = input.spaceling_gamepad_input(0);
    let tools = input.terrain_gamepad_tools();
    let left = input.is_pressed(GameKey::P1TurnLeft) || input.is_pressed(GameKey::NesLeft);
    let right = input.is_pressed(GameKey::P1TurnRight) || input.is_pressed(GameKey::NesRight);
    TerrainLabAction::controls(
        match (left, right) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            (true, true) => 0.0,
            _ => walk,
        },
        jump || input.is_pressed(GameKey::P1Laser),
        crater || input.is_pressed(GameKey::P1Reverse),
        tools.tunnel || input.is_pressed(GameKey::P1Cannon),
        tools.debug || input.is_pressed(GameKey::P1Wing),
    )
}

fn tool_controls(input: &ClientInput) -> Action {
    let tools = input.terrain_gamepad_tools();
    TerrainLabAction::Tools(ToolControls {
        cycle_tool: tools.cycle_tool || input.is_pressed(GameKey::TerrainTool),
        cycle_view: tools.cycle_view || input.is_pressed(GameKey::TerrainView),
    })
    .encode()
}

fn mining_controls(input: &ClientInput) -> Action {
    let (aim, held, gamepad_turn) = input.terrain_gamepad_mining();
    let increase = input.is_pressed(GameKey::P1Thrust) || input.is_pressed(GameKey::NesUp);
    let decrease = input.is_pressed(GameKey::P1Brake) || input.is_pressed(GameKey::NesDown);
    let turn = match (increase, decrease) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        (true, true) => 0.0,
        _ => gamepad_turn,
    };
    TerrainLabAction::Mining(MiningControls {
        held: held || input.is_pressed(GameKey::TerrainDrill),
        turn,
        aim: if increase || decrease || turn != 0.0 {
            engine_core::Vec2::ZERO
        } else {
            aim
        },
    })
    .encode()
}

#[cfg(test)]
mod tests;
