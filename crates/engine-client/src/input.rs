//! Client-side keyboard state and original Spacewars control mapping.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use engine_common::{Action, PointerPhase, RenderPoint};
use engine_nes::ControllerButtons;
use scenario_spacewars::{
    ControlSource, ShipIntent, ShipIntentEncoder, SpacewarsAction, SpacewarsScenario,
    SpacewarsState,
};
use slint::ComponentHandle;
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, PhysicalKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};

use crate::MainWindow;

pub(crate) type SharedInput = Rc<RefCell<ClientInput>>;
pub(crate) type SharedGamepadInput = Rc<RefCell<GamepadInput>>;
type SharedKeyboardState = Rc<RefCell<BTreeSet<GameKey>>>;

pub(crate) fn new_shared_input() -> (SharedInput, SharedGamepadInput) {
    let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
    let input = Rc::new(RefCell::new(ClientInput::new(Rc::clone(&gamepads))));
    (input, gamepads)
}

pub(crate) struct ClientInput {
    pressed: SharedKeyboardState,
    gamepads: SharedGamepadInput,
    spacewars_controls: SpacewarsControls,
    pointer_events: Vec<ScreenPointerEvent>,
    active_pointer: Option<RenderPoint>,
}

impl Default for ClientInput {
    fn default() -> Self {
        Self::new(Rc::new(RefCell::new(GamepadInput::default())))
    }
}

impl ClientInput {
    pub(crate) fn new(gamepads: SharedGamepadInput) -> Self {
        let pressed = Rc::new(RefCell::new(BTreeSet::new()));
        Self {
            spacewars_controls: SpacewarsControls::new(Rc::clone(&pressed), Rc::clone(&gamepads)),
            pressed,
            gamepads,
            pointer_events: Vec::new(),
            active_pointer: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct GamepadSeatInput {
    pub connected: bool,
    pub name: String,
    pub left_stick_x: f32,
    pub left_stick_y: f32,
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub right_bumper: bool,
    pub south: bool,
    pub east: bool,
    pub west: bool,
    pub start: bool,
    pub select: bool,
}

#[derive(Debug, Default)]
pub(crate) struct GamepadInput {
    seats: [GamepadSeatInput; 2],
}

impl GamepadInput {
    pub(crate) fn seat(&self, player: usize) -> Option<&GamepadSeatInput> {
        self.seats.get(player)
    }

    pub(crate) fn set_seat(&mut self, player: usize, state: GamepadSeatInput) {
        if let Some(seat) = self.seats.get_mut(player) {
            *seat = state;
        }
    }

    pub(crate) fn disconnect_seat(&mut self, player: usize) {
        let Some(seat) = self.seats.get_mut(player) else {
            return;
        };
        let name = std::mem::take(&mut seat.name);
        *seat = GamepadSeatInput {
            name,
            ..GamepadSeatInput::default()
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenPointerEvent {
    pub position: RenderPoint,
    pub phase: PointerPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GameKey {
    Reset,
    Pause,
    ForcePause,
    Benchmark,
    Back,
    Controls,
    ReturnLauncher,
    NesUp,
    NesDown,
    NesLeft,
    NesRight,
    NesA,
    NesSelect,
    NesStart,
    P1Wing,
    P1Thrust,
    P1Brake,
    P1Reverse,
    P1TurnLeft,
    P1TurnRight,
    P1Laser,
    P1Cannon,
    P1ZoomIn,
    P1ZoomOut,
    P2Wing,
    P2Thrust,
    P2Brake,
    P2Reverse,
    P2TurnLeft,
    P2TurnRight,
    P2Laser,
    P2Cannon,
    P2ZoomIn,
    P2ZoomOut,
}

pub(crate) fn install_window_input(window: &MainWindow, input: SharedInput) {
    // `has_winit_window()` stays false until the native window is created during
    // `run()`. The event filter only needs the winit adapter, so install it
    // before the event loop starts.
    let keyboard_input = Rc::clone(&input);
    window.window().on_winit_window_event(move |_, event| {
        if matches!(event, WindowEvent::Focused(false)) {
            keyboard_input.borrow_mut().handle_focus_loss();
            return EventResult::Propagate;
        }

        let Some((key, state)) = mapped_key_event(event) else {
            return EventResult::Propagate;
        };

        match state {
            ElementState::Pressed => keyboard_input.borrow_mut().press(key),
            ElementState::Released => keyboard_input.borrow_mut().release(key),
        }

        EventResult::Propagate
    });

    window.on_scenario_pointer(move |x, y, phase| {
        let phase = match phase {
            0 => PointerPhase::Press,
            1 => PointerPhase::Drag,
            2 => PointerPhase::Release,
            3 => PointerPhase::Cancel,
            _ => return,
        };
        input.borrow_mut().push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(x, y),
            phase,
        });
    });
}

impl ClientInput {
    pub(crate) fn is_pressed(&self, key: GameKey) -> bool {
        self.pressed.borrow().contains(&key)
    }

    pub(crate) fn take_reset_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Reset)
    }

    pub(crate) fn take_pause_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Pause)
    }

    pub(crate) fn take_force_pause_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::ForcePause)
    }

    pub(crate) fn take_benchmark_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Benchmark)
    }

    pub(crate) fn take_back_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Back)
    }

    pub(crate) fn take_controls_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Controls)
    }

    pub(crate) fn take_return_launcher_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::ReturnLauncher)
    }

    pub(crate) fn actions_for_spacewars(
        &mut self,
        state: &SpacewarsState,
        benchmark_active: bool,
    ) -> Vec<Action> {
        let mut actions = self.spacewars_controls.actions(state, benchmark_active);
        let pressed = self.pressed.borrow();
        for controls in [P1_CONTROLS, P2_CONTROLS] {
            if pressed.contains(&controls.zoom_in) {
                actions.push(SpacewarsAction::zoom_in(controls.player));
            } else if pressed.contains(&controls.zoom_out) {
                actions.push(SpacewarsAction::zoom_out(controls.player));
            }
        }
        actions
    }

    pub(crate) fn rover_gamepad_input(&self) -> (f32, bool, bool) {
        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(0).filter(|gamepad| gamepad.connected) else {
            return (0.0, false, false);
        };
        let brake = gamepad.east || gamepad.dpad_down;
        let throttle = match (gamepad.dpad_left, gamepad.dpad_right) {
            (false, true) if !brake => 1.0,
            (true, false) if !brake => -1.0,
            _ => 0.0,
        };
        (throttle, brake, gamepad.south)
    }

    pub(crate) fn nes_controller_buttons(&self, player: usize) -> ControllerButtons {
        let mut buttons = ControllerButtons::NONE;
        let pressed = self.pressed.borrow();
        if player == 0 {
            if pressed.contains(&GameKey::NesUp) {
                buttons |= ControllerButtons::UP;
            }
            if pressed.contains(&GameKey::NesDown) {
                buttons |= ControllerButtons::DOWN;
            }
            if pressed.contains(&GameKey::NesLeft) {
                buttons |= ControllerButtons::LEFT;
            }
            if pressed.contains(&GameKey::NesRight) {
                buttons |= ControllerButtons::RIGHT;
            }
            if pressed.contains(&GameKey::NesA) || pressed.contains(&GameKey::P1Laser) {
                buttons |= ControllerButtons::A;
            }
            if pressed.contains(&GameKey::P1Reverse) {
                buttons |= ControllerButtons::B;
            }
            if pressed.contains(&GameKey::NesSelect) {
                buttons |= ControllerButtons::SELECT;
            }
            if pressed.contains(&GameKey::NesStart) {
                buttons |= ControllerButtons::START;
            }
        }
        drop(pressed);

        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(player).filter(|gamepad| gamepad.connected) else {
            return buttons;
        };
        if gamepad.dpad_up {
            buttons |= ControllerButtons::UP;
        }
        if gamepad.dpad_down {
            buttons |= ControllerButtons::DOWN;
        }
        if gamepad.dpad_left {
            buttons |= ControllerButtons::LEFT;
        }
        if gamepad.dpad_right {
            buttons |= ControllerButtons::RIGHT;
        }
        if gamepad.south {
            buttons |= ControllerButtons::A;
        }
        if gamepad.east {
            buttons |= ControllerButtons::B;
        }
        if gamepad.select {
            buttons |= ControllerButtons::SELECT;
        }
        if gamepad.start {
            buttons |= ControllerButtons::START;
        }
        buttons
    }

    pub(crate) fn reset_spacewars_controls(&mut self) {
        self.spacewars_controls.reset();
    }

    pub(crate) fn clear(&mut self) {
        self.clear_keyboard();
        self.pointer_events.clear();
        self.active_pointer = None;
    }

    fn clear_keyboard(&mut self) {
        self.pressed.borrow_mut().clear();
    }

    fn handle_focus_loss(&mut self) {
        self.cancel_pointer();
        self.clear_keyboard();
        self.press(GameKey::ForcePause);
    }

    pub(crate) fn take_pointer_events(&mut self) -> Vec<ScreenPointerEvent> {
        std::mem::take(&mut self.pointer_events)
    }

    pub(crate) fn has_pointer_cancellation(&self) -> bool {
        self.pointer_events
            .iter()
            .any(|event| event.phase == PointerPhase::Cancel)
    }

    pub(crate) fn cancel_pointer(&mut self) {
        let Some(position) = self.active_pointer.take() else {
            return;
        };
        self.pointer_events.push(ScreenPointerEvent {
            position,
            phase: PointerPhase::Cancel,
        });
    }

    pub(crate) fn discard_pointer_events(&mut self) {
        self.pointer_events.clear();
        self.active_pointer = None;
    }

    pub(crate) fn push_pointer_event(&mut self, event: ScreenPointerEvent) {
        match event.phase {
            PointerPhase::Press => {
                self.active_pointer = Some(event.position);
                self.pointer_events.push(event);
            }
            PointerPhase::Drag => {
                if self.active_pointer.is_some() {
                    self.active_pointer = Some(event.position);
                    self.pointer_events.push(event);
                }
            }
            PointerPhase::Release => {
                if self.active_pointer.take().is_some() {
                    self.pointer_events.push(event);
                }
            }
            PointerPhase::Cancel => {
                self.cancel_pointer();
            }
        }
    }

    pub(crate) fn press(&mut self, key: GameKey) {
        self.pressed.borrow_mut().insert(key);
    }

    pub(crate) fn release(&mut self, key: GameKey) {
        self.pressed.borrow_mut().remove(&key);
    }
}

#[derive(Debug, Clone, Copy)]
struct PlayerControlMap {
    player: usize,
    wing: GameKey,
    thrust: GameKey,
    brake: GameKey,
    reverse: GameKey,
    turn_left: GameKey,
    turn_right: GameKey,
    laser: GameKey,
    cannon: GameKey,
    zoom_in: GameKey,
    zoom_out: GameKey,
}

const P1_CONTROLS: PlayerControlMap = PlayerControlMap {
    player: 0,
    wing: GameKey::P1Wing,
    thrust: GameKey::P1Thrust,
    brake: GameKey::P1Brake,
    reverse: GameKey::P1Reverse,
    turn_left: GameKey::P1TurnLeft,
    turn_right: GameKey::P1TurnRight,
    laser: GameKey::P1Laser,
    cannon: GameKey::P1Cannon,
    zoom_in: GameKey::P1ZoomIn,
    zoom_out: GameKey::P1ZoomOut,
};

const P2_CONTROLS: PlayerControlMap = PlayerControlMap {
    player: 1,
    wing: GameKey::P2Wing,
    thrust: GameKey::P2Thrust,
    brake: GameKey::P2Brake,
    reverse: GameKey::P2Reverse,
    turn_left: GameKey::P2TurnLeft,
    turn_right: GameKey::P2TurnRight,
    laser: GameKey::P2Laser,
    cannon: GameKey::P2Cannon,
    zoom_in: GameKey::P2ZoomIn,
    zoom_out: GameKey::P2ZoomOut,
};

struct SpacewarsControls {
    seats: [ControlSeat; 2],
    benchmark_sources: [BenchmarkSource; 2],
    encoder: ShipIntentEncoder,
}

impl SpacewarsControls {
    fn new(pressed: SharedKeyboardState, gamepads: SharedGamepadInput) -> Self {
        let mut p1 =
            ControlSeat::with_source(KeyboardSource::new(Rc::clone(&pressed), P1_CONTROLS));
        p1.add_source(GamepadSource::new(Rc::clone(&gamepads), 0));
        let mut p2 = ControlSeat::with_source(KeyboardSource::new(pressed, P2_CONTROLS));
        p2.add_source(GamepadSource::new(gamepads, 1));
        Self {
            seats: [p1, p2],
            benchmark_sources: [BenchmarkSource, BenchmarkSource],
            encoder: ShipIntentEncoder::default(),
        }
    }

    fn actions(&mut self, state: &SpacewarsState, benchmark_active: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        for player in 0..self.seats.len() {
            let intent = if benchmark_active {
                self.benchmark_sources[player].intent(state, player)
            } else {
                self.seats[player].intent(state, player)
            };
            actions.extend(self.encoder.encode(player, intent));
        }
        actions
    }

    fn reset(&mut self) {
        self.encoder.reset();
    }
}

struct ControlSeat {
    sources: Vec<Box<dyn ControlSource>>,
}

impl ControlSeat {
    fn with_source(source: impl ControlSource + 'static) -> Self {
        Self {
            sources: vec![Box::new(source)],
        }
    }

    fn add_source(&mut self, source: impl ControlSource + 'static) {
        self.sources.push(Box::new(source));
    }

    fn intent(&mut self, state: &SpacewarsState, player: usize) -> ShipIntent {
        self.sources
            .iter_mut()
            .fold(ShipIntent::default(), |intent, source| {
                intent.merged_with(source.intent(state, player))
            })
    }
}

struct KeyboardSource {
    pressed: SharedKeyboardState,
    controls: PlayerControlMap,
}

impl KeyboardSource {
    fn new(pressed: SharedKeyboardState, controls: PlayerControlMap) -> Self {
        Self { pressed, controls }
    }
}

impl ControlSource for KeyboardSource {
    fn intent(&mut self, _state: &SpacewarsState, _player: usize) -> ShipIntent {
        let pressed = self.pressed.borrow();
        let thrust = if pressed.contains(&self.controls.thrust) {
            1.0
        } else if pressed.contains(&self.controls.reverse) {
            -1.0
        } else {
            0.0
        };
        let turn = if pressed.contains(&self.controls.turn_left) {
            -1.0
        } else if pressed.contains(&self.controls.turn_right) {
            1.0
        } else {
            0.0
        };
        ShipIntent {
            turn,
            thrust,
            brake: if pressed.contains(&self.controls.brake) {
                1.0
            } else {
                0.0
            },
            wings_closed: pressed.contains(&self.controls.wing),
            laser: pressed.contains(&self.controls.laser),
            cannon: pressed.contains(&self.controls.cannon),
        }
    }
}

struct GamepadSource {
    gamepads: SharedGamepadInput,
    seat: usize,
}

impl GamepadSource {
    fn new(gamepads: SharedGamepadInput, seat: usize) -> Self {
        Self { gamepads, seat }
    }
}

impl ControlSource for GamepadSource {
    fn intent(&mut self, _state: &SpacewarsState, _player: usize) -> ShipIntent {
        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(self.seat) else {
            return ShipIntent::default();
        };
        if !gamepad.connected {
            return ShipIntent::default();
        }

        let turn = if gamepad.dpad_left {
            -1.0
        } else if gamepad.dpad_right {
            1.0
        } else {
            shape_stick(gamepad.left_stick_x)
        };
        let forward_thrust = if gamepad.dpad_up {
            1.0
        } else {
            shape_trigger(gamepad.right_trigger)
        };
        let brake = if gamepad.dpad_down {
            1.0
        } else {
            shape_trigger(gamepad.left_trigger)
        };
        ShipIntent {
            turn,
            thrust: if gamepad.east { -1.0 } else { forward_thrust },
            brake,
            wings_closed: gamepad.right_bumper,
            laser: gamepad.south,
            cannon: gamepad.west,
        }
    }
}

const STICK_DEADZONE: f32 = 0.15;
const TRIGGER_DEADZONE: f32 = 0.05;

pub(crate) fn shape_stick(value: f32) -> f32 {
    shape_axis(value, STICK_DEADZONE, true)
}

pub(crate) fn shape_trigger(value: f32) -> f32 {
    shape_axis(value.clamp(0.0, 1.0), TRIGGER_DEADZONE, false)
}

fn shape_axis(value: f32, deadzone: f32, signed: bool) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = if signed {
        value.clamp(-1.0, 1.0)
    } else {
        value.clamp(0.0, 1.0)
    };
    let magnitude = value.abs();
    if magnitude <= deadzone {
        return 0.0;
    }

    let normalized = (magnitude - deadzone) / (1.0 - deadzone);
    normalized * normalized * value.signum()
}

struct BenchmarkSource;

impl ControlSource for BenchmarkSource {
    fn intent(&mut self, state: &SpacewarsState, player: usize) -> ShipIntent {
        SpacewarsScenario::benchmark_intent(state, player)
    }
}

fn mapped_key_event(event: &WindowEvent) -> Option<(GameKey, ElementState)> {
    let WindowEvent::KeyboardInput { event, .. } = event else {
        return None;
    };
    if event.repeat {
        return None;
    }

    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    game_key_from_key_code(code).map(|key| (key, event.state))
}

fn game_key_from_key_code(code: KeyCode) -> Option<GameKey> {
    match code {
        KeyCode::KeyR => Some(GameKey::Reset),
        KeyCode::KeyP => Some(GameKey::Pause),
        KeyCode::KeyB => Some(GameKey::Benchmark),
        KeyCode::Escape => Some(GameKey::Back),
        KeyCode::KeyC | KeyCode::F1 => Some(GameKey::Controls),
        KeyCode::KeyQ => Some(GameKey::ReturnLauncher),
        KeyCode::ArrowUp => Some(GameKey::NesUp),
        KeyCode::ArrowDown => Some(GameKey::NesDown),
        KeyCode::ArrowLeft => Some(GameKey::NesLeft),
        KeyCode::ArrowRight => Some(GameKey::NesRight),
        KeyCode::KeyZ => Some(GameKey::NesA),
        KeyCode::Tab => Some(GameKey::NesSelect),
        KeyCode::Enter => Some(GameKey::NesStart),
        KeyCode::KeyJ => Some(GameKey::P1Wing),
        KeyCode::KeyW => Some(GameKey::P1Thrust),
        KeyCode::KeyS => Some(GameKey::P1Brake),
        KeyCode::KeyX => Some(GameKey::P1Reverse),
        KeyCode::KeyA => Some(GameKey::P1TurnLeft),
        KeyCode::KeyD => Some(GameKey::P1TurnRight),
        KeyCode::Space => Some(GameKey::P1Laser),
        KeyCode::KeyK => Some(GameKey::P1Cannon),
        KeyCode::KeyU => Some(GameKey::P1ZoomIn),
        KeyCode::KeyI => Some(GameKey::P1ZoomOut),
        KeyCode::PageDown => Some(GameKey::P2Wing),
        KeyCode::Numpad8 => Some(GameKey::P2Thrust),
        KeyCode::Numpad5 => Some(GameKey::P2Brake),
        KeyCode::Numpad2 => Some(GameKey::P2Reverse),
        KeyCode::Numpad4 => Some(GameKey::P2TurnLeft),
        KeyCode::Numpad6 => Some(GameKey::P2TurnRight),
        KeyCode::Delete => Some(GameKey::P2Laser),
        KeyCode::End => Some(GameKey::P2Cannon),
        KeyCode::Insert => Some(GameKey::P2ZoomIn),
        KeyCode::Home => Some(GameKey::P2ZoomOut),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::SpacewarsConfig;
    use scenario_spacewars::{SpacewarsAction as ScenarioAction, SpacewarsActionKind};

    fn decoded(actions: &[Action]) -> Vec<ScenarioAction> {
        actions.iter().filter_map(ScenarioAction::decode).collect()
    }

    fn has_action(actions: &[ScenarioAction], player: usize, kind: SpacewarsActionKind) -> bool {
        actions
            .iter()
            .any(|action| action.player() == player && action.kind() == kind)
    }

    fn take_spacewars_actions(input: &mut ClientInput) -> Vec<Action> {
        let state = SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 0);
        input.actions_for_spacewars(&state, false)
    }

    #[test]
    fn pointer_cancel_uses_the_last_active_position_once() {
        let mut input = ClientInput::default();
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(30.0, 40.0),
            phase: PointerPhase::Drag,
        });
        input.take_pointer_events();

        input.cancel_pointer();
        input.cancel_pointer();

        assert_eq!(
            input.take_pointer_events(),
            vec![ScreenPointerEvent {
                position: RenderPoint::new(30.0, 40.0),
                phase: PointerPhase::Cancel,
            }]
        );
    }

    #[test]
    fn clearing_keyboard_state_preserves_pointer_cancellation() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Thrust);
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.take_pointer_events();
        input.cancel_pointer();

        input.clear_keyboard();

        assert!(!input.pressed.borrow().contains(&GameKey::P1Thrust));
        assert!(input.has_pointer_cancellation());
    }

    #[test]
    fn focus_loss_releases_controls_and_requests_a_host_pause() {
        let mut input = ClientInput::default();
        input.press(GameKey::NesA);
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.take_pointer_events();

        input.handle_focus_loss();

        assert_eq!(input.nes_controller_buttons(0), ControllerButtons::NONE);
        assert!(input.has_pointer_cancellation());
        assert!(input.take_force_pause_requested());
        assert!(!input.take_force_pause_requested());
    }

    #[test]
    fn keyboard_source_emits_changed_full_scale_setpoints() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Wing);
        input.press(GameKey::P1Thrust);

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));

        input.release(GameKey::P1Wing);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert_eq!(
            actions,
            vec![ScenarioAction::SetWings {
                player: 0,
                closed: false,
            }]
        );

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn release_emits_one_changed_zero_setpoint() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1TurnLeft);
        input.press(GameKey::P1Laser);
        take_spacewars_actions(&mut input);

        input.release(GameKey::P1TurnLeft);
        input.release(GameKey::P1Laser);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 0.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetLaser {
            player: 0,
            on: false,
        }));

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn brake_and_reverse_keys_use_distinct_keyboard_rows() {
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyS),
            Some(GameKey::P1Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyX),
            Some(GameKey::P1Reverse)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad5),
            Some(GameKey::P2Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad2),
            Some(GameKey::P2Reverse)
        );
    }

    #[test]
    fn nes_input_combines_keyboard_and_the_assigned_gamepad() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                south: true,
                start: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        input.press(GameKey::NesLeft);
        input.press(GameKey::P1Reverse);

        assert_eq!(
            input.nes_controller_buttons(0),
            ControllerButtons::LEFT
                | ControllerButtons::RIGHT
                | ControllerButtons::A
                | ControllerButtons::B
                | ControllerButtons::START
        );
        assert_eq!(input.nes_controller_buttons(1), ControllerButtons::NONE);
    }

    #[test]
    fn keyboard_source_reports_brake_and_propulsion_independently() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Wing);
        input.press(GameKey::P1Thrust);
        input.press(GameKey::P1Reverse);
        input.press(GameKey::P1Brake);

        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetBrake {
            player: 0,
            amount: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));
    }

    #[test]
    fn brake_release_emits_one_zero_setpoint() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Brake);
        take_spacewars_actions(&mut input);

        input.release(GameKey::P1Brake);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert_eq!(
            actions,
            vec![ScenarioAction::SetBrake {
                player: 0,
                amount: 0.0,
            }]
        );

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn p2_controls_use_numpad_and_navigation_keys() {
        assert_eq!(
            game_key_from_key_code(KeyCode::PageDown),
            Some(GameKey::P2Wing)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad8),
            Some(GameKey::P2Thrust)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad5),
            Some(GameKey::P2Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad2),
            Some(GameKey::P2Reverse)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad4),
            Some(GameKey::P2TurnLeft)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad6),
            Some(GameKey::P2TurnRight)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Delete),
            Some(GameKey::P2Laser)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::End),
            Some(GameKey::P2Cannon)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Insert),
            Some(GameKey::P2ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Home),
            Some(GameKey::P2ZoomOut)
        );
    }

    #[test]
    fn zoom_keys_emit_original_per_player_zoom_actions() {
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyU),
            Some(GameKey::P1ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyI),
            Some(GameKey::P1ZoomOut)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Insert),
            Some(GameKey::P2ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Home),
            Some(GameKey::P2ZoomOut)
        );

        let mut input = ClientInput::default();
        input.press(GameKey::P1ZoomIn);
        input.press(GameKey::P2ZoomOut);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(has_action(&actions, 0, SpacewarsActionKind::ZoomIn));
        assert!(has_action(&actions, 1, SpacewarsActionKind::ZoomOut));
    }

    #[test]
    fn reset_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Reset);

        assert!(input.take_reset_requested());
        assert!(!input.take_reset_requested());
    }

    #[test]
    fn pause_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Pause);

        assert!(input.take_pause_requested());
        assert!(!input.take_pause_requested());
    }

    #[test]
    fn benchmark_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Benchmark);

        assert!(input.take_benchmark_requested());
        assert!(!input.take_benchmark_requested());
    }

    #[test]
    fn escape_key_is_one_shot_back_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Back);

        assert!(input.take_back_requested());
        assert!(!input.take_back_requested());
        assert_eq!(game_key_from_key_code(KeyCode::Escape), Some(GameKey::Back));
    }

    #[test]
    fn controls_keys_are_one_shot_menu_controls() {
        let mut input = ClientInput::default();

        input.press(GameKey::Controls);

        assert!(input.take_controls_requested());
        assert!(!input.take_controls_requested());
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyC),
            Some(GameKey::Controls)
        );
        assert_eq!(game_key_from_key_code(KeyCode::F1), Some(GameKey::Controls));
    }

    #[test]
    fn q_key_is_one_shot_return_launcher_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::ReturnLauncher);

        assert!(input.take_return_launcher_requested());
        assert!(!input.take_return_launcher_requested());
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyQ),
            Some(GameKey::ReturnLauncher)
        );
    }

    #[test]
    fn gamepad_curves_apply_deadzones_and_reach_full_scale() {
        assert_eq!(shape_stick(0.14), 0.0);
        assert_eq!(shape_stick(-0.15), 0.0);
        assert!((shape_stick(0.575) - 0.25).abs() < 0.001);
        assert_eq!(shape_stick(-1.0), -1.0);
        assert_eq!(shape_trigger(0.04), 0.0);
        assert_eq!(shape_trigger(1.0), 1.0);
        assert_eq!(shape_stick(f32::NAN), 0.0);
    }

    #[test]
    fn gamepad_source_maps_ship_controls_and_reverse_wins() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                left_stick_x: 1.0,
                left_trigger: 0.5,
                right_trigger: 0.75,
                dpad_up: true,
                right_bumper: true,
                south: true,
                east: true,
                west: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: -1.0,
        }));
        assert!(actions.iter().any(|action| matches!(
            action,
            ScenarioAction::SetBrake {
                player: 0,
                amount,
            } if *amount > 0.0 && *amount < 0.5
        )));
        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetLaser {
            player: 0,
            on: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetCannon {
            player: 0,
            on: true,
        }));
    }

    #[test]
    fn dpad_controls_digital_flight_without_changing_zoom() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                dpad_up: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        input.press(GameKey::P1TurnLeft);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::ZoomIn));

        input.gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_down: true,
                ..GamepadSeatInput::default()
            },
        );
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetBrake {
            player: 0,
            amount: 1.0,
        }));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::ZoomOut));
    }
}
