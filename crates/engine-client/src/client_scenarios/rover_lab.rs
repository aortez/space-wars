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
    controls_help: "Rover Lab: d-pad right drives forward, d-pad left drives in reverse, B or d-pad down brakes, and holding then releasing A jumps. The pause menu can restart the lab. Keyboard: W forward, S brake, X reverse, hold/release Space to jump, R reset.",
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

    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        let (throttle, brake, jump_held) = rover_controls(input);
        vec![RoverLabAction::drive(throttle, brake, jump_held)]
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

fn rover_controls(input: &ClientInput) -> (f32, bool, bool) {
    let (gamepad_throttle, gamepad_brake, gamepad_jump) = input.rover_gamepad_input();
    let brake = input.is_pressed(GameKey::P1Brake) || gamepad_brake;
    let throttle = if brake {
        0.0
    } else if input.is_pressed(GameKey::P1Thrust) {
        1.0
    } else if input.is_pressed(GameKey::P1Reverse) {
        -1.0
    } else {
        gamepad_throttle
    };
    let jump_held = input.is_pressed(GameKey::P1Laser) || gamepad_jump;
    (throttle, brake, jump_held)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::input::{GamepadInput, GamepadSeatInput};

    fn client_input_with_gamepad(
        state: GamepadSeatInput,
    ) -> (ClientInput, Rc<RefCell<GamepadInput>>) {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(0, state);
        (ClientInput::new(Rc::clone(&gamepads)), gamepads)
    }

    #[test]
    fn nes_dpad_drives_in_both_directions() {
        let (forward, _) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            dpad_right: true,
            ..GamepadSeatInput::default()
        });
        assert_eq!(rover_controls(&forward), (1.0, false, false));

        let (reverse, _) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            dpad_left: true,
            ..GamepadSeatInput::default()
        });
        assert_eq!(rover_controls(&reverse), (-1.0, false, false));
    }

    #[test]
    fn releasing_the_dpad_returns_to_neutral() {
        let (input, gamepads) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            dpad_right: true,
            ..GamepadSeatInput::default()
        });
        assert_eq!(rover_controls(&input), (1.0, false, false));

        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                ..GamepadSeatInput::default()
            },
        );
        assert_eq!(rover_controls(&input), (0.0, false, false));
    }

    #[test]
    fn nes_brake_controls_override_gamepad_and_keyboard_throttle() {
        let (mut input, _) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            dpad_right: true,
            east: true,
            ..GamepadSeatInput::default()
        });
        input.press(GameKey::P1Thrust);

        assert_eq!(rover_controls(&input), (0.0, true, false));

        let (input, _) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            dpad_left: true,
            dpad_down: true,
            ..GamepadSeatInput::default()
        });
        assert_eq!(rover_controls(&input), (0.0, true, false));
    }

    #[test]
    fn nes_a_and_keyboard_space_hold_jump() {
        let (input, _) = client_input_with_gamepad(GamepadSeatInput {
            connected: true,
            south: true,
            ..GamepadSeatInput::default()
        });
        assert_eq!(rover_controls(&input), (0.0, false, true));

        let (mut keyboard, _) = client_input_with_gamepad(GamepadSeatInput::default());
        keyboard.press(GameKey::P1Laser);
        assert_eq!(rover_controls(&keyboard), (0.0, false, true));
    }
}
