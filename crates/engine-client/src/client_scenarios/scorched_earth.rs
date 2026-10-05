use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::{
    input::{ClientInput, GameKey},
    render::{FrameLayout, Viewport},
};
use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_scorched_earth::{
    Controls, ScorchedAction, ScorchedConfig, ScorchedScenario, ScorchedState,
};
use std::time::Duration;

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "scorched-earth",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        headless_benchmark: false,
        pointer_input: true,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Scorched Earth: Left/Right or A/D adjusts elevation; Up/Down or W/S adjusts power. Hold Space to fire. Tab selects the other tank. V toggles the scripted duel; T switches Round/Angular dirt and resets. R restarts with the same seed. The other tank returns fire automatically. On-screen buttons provide the same controls. Gamepad: d-pad or right stick aims and changes power, bottom face fires, left face selects a tank, top face switches dirt, right shoulder toggles Demo. Start/Esc opens the pause menu. Destroy the opponent or its footing. Material capacity rejections preserve the dirt; combat damage still applies.",
    create,
};

struct ScorchedClient {
    state: ScorchedState,
}
fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(ScorchedClient {
        state: ScorchedScenario::init(ScorchedConfig::default(), seed),
    }))
}
impl ClientScenario for ScorchedClient {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }
    fn tick_model(&self) -> TickModel {
        ScorchedScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        ScorchedScenario::step(&mut self.state, actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark: bool) -> Vec<Action> {
        vec![ScorchedAction::Controls(controls(input)).encode()]
    }
    fn render_frames(&self, _renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frame = ScorchedScenario::render_frame(&self.state);
        // Keep both tanks and every world-space button visible in narrow windows.
        frame.camera.height = frame
            .camera
            .height
            .max(100.0 * viewport.height / viewport.width.max(1.0));
        vec![frame]
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
fn controls(input: &ClientInput) -> Controls {
    let (horizontal, fire, collapse) = input.spaceling_gamepad_input(0);
    let (stick, trigger, vertical) = input.terrain_gamepad_mining();
    let tools = input.terrain_gamepad_tools();
    let pressed = |a, b| input.is_pressed(a) || input.is_pressed(b);
    let axis = |negative, positive, fallback| match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        (true, true) => 0.0,
        _ => fallback,
    };
    Controls {
        aim: axis(
            pressed(GameKey::P1TurnLeft, GameKey::NesLeft),
            pressed(GameKey::P1TurnRight, GameKey::NesRight),
            if stick.x != 0.0 { stick.x } else { horizontal },
        ),
        power: axis(
            pressed(GameKey::P1Brake, GameKey::NesDown),
            pressed(GameKey::P1Thrust, GameKey::NesUp),
            if stick.y != 0.0 { stick.y } else { vertical },
        ),
        fire: fire
            || (trigger && !input.scorched_gamepad_barrage())
            || input.is_pressed(GameKey::P1Laser),
        collapse: collapse || input.is_pressed(GameKey::P1Cannon),
        barrage: input.scorched_gamepad_barrage() || input.is_pressed(GameKey::P1Wing),
        select: tools.tunnel || input.is_pressed(GameKey::NesSelect),
        shape: tools.cycle_tool || input.is_pressed(GameKey::TerrainTool),
        demo: tools.cycle_view || input.is_pressed(GameKey::TerrainView),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GamepadInput, GamepadSeatInput};
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn controller_buttons_and_axes_release_after_disconnect() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let input = ClientInput::new(Rc::clone(&pads));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                south: true,
                east: true,
                left_bumper: true,
                west: true,
                north: true,
                right_bumper: true,
                right_stick_x: 0.8,
                right_stick_y: 0.7,
                ..Default::default()
            },
        );
        let c = controls(&input);
        assert!(c.fire && c.select && c.shape && c.demo && c.collapse && c.barrage);
        assert!(c.aim > 0.0 && c.power > 0.0);
        pads.borrow_mut().set_seat(0, GamepadSeatInput::default());
        assert_eq!(controls(&input), Controls::default());
    }

    #[test]
    fn keyboard_and_picade_dpad_use_the_same_controls() {
        let mut keyboard = ClientInput::default();
        for key in [GameKey::NesRight, GameKey::NesUp, GameKey::P1Laser] {
            keyboard.press(key);
        }
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let pad = ClientInput::new(Rc::clone(&pads));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                dpad_up: true,
                south: true,
                ..Default::default()
            },
        );
        assert_eq!(controls(&keyboard), controls(&pad));
        keyboard.clear();
        assert_eq!(controls(&keyboard), Controls::default());
    }

    #[test]
    fn tanks_and_controls_remain_visible_in_landscape_and_portrait() {
        use crate::raster::{RasterOptions, RasterRenderer};
        let scene = ScorchedClient {
            state: ScorchedScenario::init(ScorchedConfig::default(), 42),
        };
        for viewport in [Viewport::new(800.0, 480.0), Viewport::new(800.0, 1280.0)] {
            let frames = scene.render_frames(RenderBackend::Vector, viewport);
            assert_eq!(frames, scene.render_frames(RenderBackend::Raster, viewport));
            let overlay =
                crate::render::raster_text_overlay(&frames, viewport, scene.frame_layout());
            for label in ["YOU · 100", "CPU · 100", "Angle −", "Reset"] {
                let item = overlay.iter().find(|p| p.text == label).expect(label);
                assert!(item.x >= 0.0 && item.x < viewport.width, "{label}");
                assert!(item.y >= 0.0 && item.y < viewport.height, "{label}");
            }
            let image = RasterRenderer::new().image_from_frames_with_layout(
                &frames,
                viewport,
                scene.frame_layout(),
                RasterOptions::default(),
            );
            let pixels = image.to_rgb8().unwrap();
            for (r, g, b) in [(97_u8, 207_u8, 171_u8), (245, 135, 79)] {
                assert!(
                    pixels
                        .as_slice()
                        .iter()
                        .filter(|p| p.r.abs_diff(r) <= 1
                            && p.g.abs_diff(g) <= 1
                            && p.b.abs_diff(b) <= 1)
                        .count()
                        > 80,
                    "both physical tank hulls must be visible"
                );
            }
        }
    }
}
