use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::{
    input::{ClientInput, GameKey},
    render::{FrameLayout, Viewport},
};
use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use engine_core::Vec2;
use scenario_terrain_lab::granular_lab::{
    GranularControls, GranularLabAction, GranularLabConfig, GranularLabScenario, GranularLabState,
};
use std::time::Duration;

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "terrain-grains",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        headless_benchmark: false,
        pointer_input: true,
        benchmark: false,
        player_zoom: true,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Terrain Lab loose dirt: click/tap terrain to blast. WASD/arrows move the aim; Space fires, K drops a test box above the current surface. T cycles round, higher-friction and angular grains. V changes flat/slope/moving-planet ground. X changes the loose-body limit. These changes reset the experiment. On-screen buttons also change grain size, pause, step, run at quarter speed, or reset with the current choices. Wheel zooms. Gamepad: right stick or d-pad aims, bottom face or left shoulder fires, left face drops the box, top face changes grains, right shoulder changes ground, right face changes the limit. A rejected blast leaves material and motion unchanged. The box and dirt interact through ordinary physics. R restarts with defaults; Start/Esc opens the host pause menu.",
    create,
};

struct GranularClient {
    state: GranularLabState,
}
fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(GranularClient {
        state: GranularLabScenario::init(GranularLabConfig::default(), seed),
    }))
}
impl ClientScenario for GranularClient {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }
    fn tick_model(&self) -> TickModel {
        GranularLabScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        GranularLabScenario::step(&mut self.state, actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark: bool) -> Vec<Action> {
        vec![GranularLabAction::Controls(controls(input)).encode()]
    }
    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![GranularLabScenario::render_frame(&self.state)]
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }
    fn zoom_player_in(&mut self, player: usize) {
        if player == 0 {
            self.state.zoom_in()
        }
    }
    fn zoom_player_out(&mut self, player: usize) {
        if player == 0 {
            self.state.zoom_out()
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

fn controls(input: &ClientInput) -> GranularControls {
    let (walk, fire, budget) = input.spaceling_gamepad_input(0);
    let (stick, trigger, vertical) = input.terrain_gamepad_mining();
    let tools = input.terrain_gamepad_tools();
    let pressed = |a, b| input.is_pressed(a) || input.is_pressed(b);
    let axis = |negative, positive, fallback| match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        (true, true) => 0.0,
        _ => fallback,
    };
    GranularControls {
        aim: Vec2::new(
            axis(
                pressed(GameKey::P1TurnLeft, GameKey::NesLeft),
                pressed(GameKey::P1TurnRight, GameKey::NesRight),
                if stick.x != 0.0 { stick.x } else { walk },
            ),
            axis(
                pressed(GameKey::P1Brake, GameKey::NesDown),
                pressed(GameKey::P1Thrust, GameKey::NesUp),
                if stick.y != 0.0 { stick.y } else { vertical },
            ),
        ),
        fire: fire || trigger || input.is_pressed(GameKey::P1Laser),
        drop: tools.tunnel || input.is_pressed(GameKey::P1Cannon),
        preset: tools.cycle_tool || input.is_pressed(GameKey::TerrainTool),
        fixture: tools.cycle_view || input.is_pressed(GameKey::TerrainView),
        budget: budget || input.is_pressed(GameKey::P1Reverse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GamepadInput, GamepadSeatInput};
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn controller_releases_all_lab_actions() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let input = ClientInput::new(Rc::clone(&pads));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                south: true,
                west: true,
                north: true,
                east: true,
                right_bumper: true,
                right_stick_x: 0.8,
                ..Default::default()
            },
        );
        let c = controls(&input);
        assert!(c.fire && c.drop && c.preset && c.fixture && c.budget && c.aim.x > 0.0);
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                ..Default::default()
            },
        );
        assert_eq!(controls(&input), GranularControls::default());
    }
}
