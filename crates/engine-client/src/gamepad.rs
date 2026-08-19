//! Backend-neutral gamepad polling, seat assignment, and controller UI routing.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gamepad, GamepadId, Gilrs, Mapping};
use slint::{ComponentHandle, SharedString, Timer, TimerMode};

use crate::MainWindow;
use crate::input::{self, GameKey, GamepadSeatInput, SharedGamepadInput, SharedInput};
use crate::ui_navigation::UiAction;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const UI_REPEAT_DELAY: Duration = Duration::from_millis(350);
const UI_REPEAT_INTERVAL: Duration = Duration::from_millis(100);
const UI_STICK_THRESHOLD: f32 = 0.55;
const AXIS_DPAD_THRESHOLD: f32 = 0.5;

// Linux SDL GUID for USB 0079:0011, version 0110. Gilrs 0.11 parses
// this controller's signed half-axis D-pad mappings as one mapping per
// physical axis, leaving only Right and Up usable. Remap those axes to
// ordinary stick axes and synthesize all four D-pad directions below.
const RETRO_CONTROLLER_UUID: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x79, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x10, 0x01, 0x00, 0x00,
];

const REMAPPED_BUTTONS: [Button; 15] = [
    Button::South,
    Button::East,
    Button::North,
    Button::West,
    Button::C,
    Button::Z,
    Button::LeftTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::Mode,
    Button::LeftThumb,
    Button::RightThumb,
];

const REMAPPED_AXES: [Axis; 6] = [
    Axis::LeftStickX,
    Axis::LeftStickY,
    Axis::LeftZ,
    Axis::RightStickX,
    Axis::RightStickY,
    Axis::RightZ,
];

pub(crate) fn start_gamepad_pump(
    window: &MainWindow,
    input: SharedInput,
    gamepads: SharedGamepadInput,
) -> Option<Timer> {
    let gilrs = match Gilrs::new() {
        Ok(gilrs) => gilrs,
        Err(err) => {
            tracing::warn!(error = %err, "gamepad backend unavailable; keyboard input remains active.");
            return None;
        }
    };

    let mut pump = GamepadPump::new(gilrs, input, gamepads);
    pump.initialize(window);

    let timer = Timer::default();
    let weak_window = window.as_weak();
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        pump.tick(&window);
    });
    Some(timer)
}

struct GamepadPump {
    gilrs: Gilrs,
    assignments: SeatAssignments,
    input: SharedInput,
    gamepads: SharedGamepadInput,
    ui_driver: Option<usize>,
    ui_repeat: UiRepeat,
    mode_handoff: ModeHandoff,
}

impl GamepadPump {
    fn new(gilrs: Gilrs, input: SharedInput, gamepads: SharedGamepadInput) -> Self {
        Self {
            gilrs,
            assignments: SeatAssignments::default(),
            input,
            gamepads,
            ui_driver: None,
            ui_repeat: UiRepeat::default(),
            mode_handoff: ModeHandoff::default(),
        }
    }

    fn initialize(&mut self, window: &MainWindow) {
        let connected = self.gilrs.gamepads().map(|(id, _)| id).collect::<Vec<_>>();
        for id in connected {
            apply_controller_profile(&mut self.gilrs, id);
            let name = self.gilrs.gamepad(id).name().to_owned();
            let id = usize::from(id);
            if let Some(seat) = self.assignments.connect(id) {
                tracing::info!(
                    gamepad_id = id,
                    player = seat + 1,
                    gamepad = %name,
                    "assigned connected gamepad."
                );
            }
        }
        self.observe_mode(window);
        self.sample_gamepads();
        self.refresh_connection_ui(window);
    }

    fn tick(&mut self, window: &MainWindow) {
        self.observe_mode(window);
        while let Some(event) = self.gilrs.next_event() {
            let id = usize::from(event.id);
            match event.event {
                EventType::Connected => {
                    apply_controller_profile(&mut self.gilrs, event.id);
                    let name = self.gilrs.gamepad(event.id).name().to_owned();
                    if let Some(seat) = self.assignments.connect(id) {
                        self.mode_handoff.block(seat);
                        tracing::info!(
                            gamepad_id = id,
                            player = seat + 1,
                            gamepad = %name,
                            "gamepad connected."
                        );
                    } else {
                        tracing::warn!(
                            gamepad_id = id,
                            gamepad = %name,
                            "gamepad connected without an available player seat."
                        );
                    }
                    self.refresh_connection_ui(window);
                }
                EventType::Disconnected => {
                    if let Some(seat) = self.assignments.disconnect(id) {
                        self.mode_handoff.block(seat);
                        self.gamepads.borrow_mut().disconnect_seat(seat);
                        tracing::warn!(gamepad_id = id, player = seat + 1, "gamepad disconnected.");
                        if !window.get_launcher_visible() {
                            let was_playing = is_game_mode(window);
                            if was_playing {
                                self.input.borrow_mut().press(GameKey::ForcePause);
                            }
                            window.set_controller_disconnected_visible(true);
                            window.set_controller_disconnected_text(SharedString::from(format!(
                                "P{} controller disconnected — {}",
                                seat + 1,
                                if was_playing {
                                    "game paused; keyboard or touch remains available."
                                } else {
                                    "keyboard or touch remains available."
                                }
                            )));
                        }
                    }
                    self.refresh_connection_ui(window);
                }
                event_type => {
                    let Some(seat) = self.assignments.connected_seat(id) else {
                        continue;
                    };
                    if is_pad_activity(event_type) {
                        self.ui_driver = Some(seat);
                    }
                    self.route_button_edge(window, seat, event_type);
                }
            }
            self.observe_mode(window);
        }

        self.observe_mode(window);
        self.sample_gamepads();
        if is_ui_mode(window) {
            self.update_ui_navigation(window);
        } else {
            self.ui_repeat.reset();
        }
    }

    fn route_button_edge(&mut self, window: &MainWindow, seat: usize, event: EventType) {
        if !self.mode_handoff.accepts_input(seat) {
            return;
        }
        let EventType::ButtonPressed(button, _) = event else {
            return;
        };

        if is_ui_mode(window) {
            let action = match button {
                Button::South => Some(UiAction::Confirm),
                Button::East => Some(UiAction::Back),
                Button::Select => Some(UiAction::Controls),
                Button::Start => Some(UiAction::Start),
                _ => None,
            };
            if let Some(action) = action {
                self.begin_handoff();
                window.invoke_ui_action(action.code());
            }
            return;
        }

        match gameplay_button_route(button, window.get_scenario_captures_gamepad_start()) {
            Some(GameplayButtonRoute::HostPause) => {
                self.begin_handoff();
                self.input.borrow_mut().press(GameKey::Pause);
            }
            Some(GameplayButtonRoute::HostControls) => {
                self.begin_handoff();
                self.input.borrow_mut().press(GameKey::Controls);
            }
            // Native-console scenarios sample Start as part of their complete
            // controller snapshot. Other gameplay buttons are continuous too.
            Some(GameplayButtonRoute::Scenario) | None => {}
        }
    }

    fn sample_gamepads(&mut self) {
        let snapshots = self
            .gilrs
            .gamepads()
            .filter_map(|(id, gamepad)| {
                let seat = self.assignments.connected_seat(usize::from(id))?;
                Some((seat, snapshot(&gamepad)))
            })
            .collect::<Vec<_>>();
        let snapshots = snapshots
            .into_iter()
            .map(|(seat, snapshot)| (seat, self.mode_handoff.filter(seat, snapshot)))
            .collect::<Vec<_>>();
        let mut gamepads = self.gamepads.borrow_mut();
        for (seat, snapshot) in snapshots {
            gamepads.set_seat(seat, snapshot);
        }
    }

    fn observe_mode(&mut self, window: &MainWindow) {
        if self.mode_handoff.observe(InputMode::from_window(window)) {
            self.ui_driver = None;
            self.ui_repeat.reset();
        }
    }

    fn begin_handoff(&mut self) {
        self.mode_handoff.block_all();
        self.ui_driver = None;
        self.ui_repeat.reset();
    }

    fn update_ui_navigation(&mut self, window: &MainWindow) {
        let Some(seat) = self.ui_driver else {
            self.ui_repeat.reset();
            return;
        };
        let gamepad = self
            .gamepads
            .borrow()
            .seat(seat)
            .cloned()
            .unwrap_or_default();
        if !gamepad.connected {
            self.ui_repeat.reset();
            return;
        }

        if let Some(action) = self
            .ui_repeat
            .update(ui_direction(&gamepad), Instant::now())
        {
            window.invoke_ui_action(action.code());
        }
    }

    fn refresh_connection_ui(&self, window: &MainWindow) {
        let connected = &self.assignments.connected;
        let binding = |seat: usize| {
            let player = seat + 1;
            if self.assignments.seats[seat].is_some_and(|id| connected.contains(&id)) {
                format!("P{player} PAD + KEY")
            } else {
                format!("P{player} KEY")
            }
        };
        window.set_p1_input_binding(SharedString::from(binding(0)));
        window.set_p2_input_binding(SharedString::from(binding(1)));
        if !self.assignments.has_disconnected_seat() {
            window.set_controller_disconnected_visible(false);
            window.set_controller_disconnected_text(SharedString::from(""));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplayButtonRoute {
    HostPause,
    HostControls,
    Scenario,
}

fn gameplay_button_route(
    button: Button,
    scenario_captures_start: bool,
) -> Option<GameplayButtonRoute> {
    match button {
        Button::Start if scenario_captures_start => Some(GameplayButtonRoute::Scenario),
        Button::Start => Some(GameplayButtonRoute::HostPause),
        Button::Select => Some(GameplayButtonRoute::HostControls),
        _ => None,
    }
}

fn apply_controller_profile(gilrs: &mut Gilrs, id: GamepadId) {
    let profile = {
        let gamepad = gilrs.gamepad(id);
        if !uses_retro_axis_dpad(gamepad.uuid()) {
            return;
        }

        let Some(x_axis) = gamepad
            .button_code(Button::DPadRight)
            .or_else(|| gamepad.button_code(Button::DPadLeft))
        else {
            tracing::warn!(
                gamepad_id = usize::from(id),
                "Retro Controller profile could not find its horizontal D-pad axis."
            );
            return;
        };
        let Some(y_axis) = gamepad
            .button_code(Button::DPadUp)
            .or_else(|| gamepad.button_code(Button::DPadDown))
        else {
            tracing::warn!(
                gamepad_id = usize::from(id),
                "Retro Controller profile could not find its vertical D-pad axis."
            );
            return;
        };

        let mut mapping = Mapping::new();
        for button in REMAPPED_BUTTONS {
            if let Some(code) = gamepad.button_code(button) {
                mapping.insert_btn(code, button);
            }
        }
        for axis in REMAPPED_AXES {
            if let Some(code) = gamepad.axis_code(axis) {
                if code != x_axis && code != y_axis {
                    mapping.insert_axis(code, axis);
                }
            }
        }
        mapping.insert_axis(x_axis, Axis::LeftStickX);
        mapping.insert_axis(y_axis, Axis::LeftStickY);
        mapping
    };

    match gilrs.set_mapping(
        usize::from(id),
        &profile,
        "Retro Controller (Space Wars axis D-pad)",
    ) {
        Ok(_) => tracing::info!(
            gamepad_id = usize::from(id),
            profile = "retro-axis-dpad",
            "applied controller input profile."
        ),
        Err(err) => tracing::warn!(
            gamepad_id = usize::from(id),
            error = %err,
            "failed to apply controller input profile."
        ),
    }
}

fn snapshot(gamepad: &Gamepad<'_>) -> GamepadSeatInput {
    let left_stick_x = gamepad.value(Axis::LeftStickX);
    let left_stick_y = gamepad.value(Axis::LeftStickY);
    let axis_dpad = if uses_retro_axis_dpad(gamepad.uuid()) {
        dpad_from_axes(left_stick_x, left_stick_y)
    } else {
        DpadInput::default()
    };
    GamepadSeatInput {
        connected: gamepad.is_connected(),
        name: gamepad.name().to_owned(),
        left_stick_x,
        left_stick_y,
        left_trigger: button_value(gamepad, Button::LeftTrigger2),
        right_trigger: button_value(gamepad, Button::RightTrigger2),
        dpad_up: gamepad.is_pressed(Button::DPadUp) || axis_dpad.up,
        dpad_down: gamepad.is_pressed(Button::DPadDown) || axis_dpad.down,
        dpad_left: gamepad.is_pressed(Button::DPadLeft) || axis_dpad.left,
        dpad_right: gamepad.is_pressed(Button::DPadRight) || axis_dpad.right,
        right_bumper: gamepad.is_pressed(Button::RightTrigger),
        south: gamepad.is_pressed(Button::South),
        east: gamepad.is_pressed(Button::East),
        west: gamepad.is_pressed(Button::West),
        start: gamepad.is_pressed(Button::Start),
        select: gamepad.is_pressed(Button::Select),
    }
}

fn uses_retro_axis_dpad(uuid: [u8; 16]) -> bool {
    uuid == RETRO_CONTROLLER_UUID
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DpadInput {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

fn dpad_from_axes(x: f32, y: f32) -> DpadInput {
    DpadInput {
        up: y >= AXIS_DPAD_THRESHOLD,
        down: y <= -AXIS_DPAD_THRESHOLD,
        left: x <= -AXIS_DPAD_THRESHOLD,
        right: x >= AXIS_DPAD_THRESHOLD,
    }
}

fn button_value(gamepad: &Gamepad<'_>, button: Button) -> f32 {
    gamepad
        .button_data(button)
        .map(|data| data.value())
        .unwrap_or_else(|| f32::from(gamepad.is_pressed(button)))
}

fn is_pad_activity(event: EventType) -> bool {
    match event {
        EventType::ButtonPressed(..)
        | EventType::ButtonReleased(..)
        | EventType::ButtonRepeated(..) => true,
        EventType::ButtonChanged(_, value, _) => value.abs() >= 0.05,
        EventType::AxisChanged(_, value, _) => value.abs() >= 0.1,
        _ => false,
    }
}

fn is_ui_mode(window: &MainWindow) -> bool {
    InputMode::from_window(window) == InputMode::Ui
}

fn is_game_mode(window: &MainWindow) -> bool {
    !is_ui_mode(window)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Ui,
    Gameplay,
}

impl InputMode {
    fn from_window(window: &MainWindow) -> Self {
        if window.get_launcher_visible()
            || window.get_ingame_menu_visible()
            || window.get_game_over_visible()
        {
            Self::Ui
        } else {
            Self::Gameplay
        }
    }
}

#[derive(Debug, Default)]
struct ModeHandoff {
    mode: Option<InputMode>,
    awaiting_neutral: [bool; 2],
}

impl ModeHandoff {
    fn observe(&mut self, mode: InputMode) -> bool {
        if self.mode == Some(mode) {
            return false;
        }
        self.mode = Some(mode);
        self.block_all();
        true
    }

    fn block_all(&mut self) {
        self.awaiting_neutral.fill(true);
    }

    fn block(&mut self, seat: usize) {
        if let Some(awaiting_neutral) = self.awaiting_neutral.get_mut(seat) {
            *awaiting_neutral = true;
        }
    }

    fn accepts_input(&self, seat: usize) -> bool {
        self.awaiting_neutral
            .get(seat)
            .is_some_and(|awaiting_neutral| !awaiting_neutral)
    }

    fn filter(&mut self, seat: usize, snapshot: GamepadSeatInput) -> GamepadSeatInput {
        if self.accepts_input(seat) {
            return snapshot;
        }
        if is_neutral(&snapshot) {
            if let Some(awaiting_neutral) = self.awaiting_neutral.get_mut(seat) {
                *awaiting_neutral = false;
            }
            snapshot
        } else {
            neutral_snapshot(&snapshot)
        }
    }
}

fn is_neutral(gamepad: &GamepadSeatInput) -> bool {
    input::shape_stick(gamepad.left_stick_x) == 0.0
        && input::shape_stick(gamepad.left_stick_y) == 0.0
        && input::shape_trigger(gamepad.left_trigger) == 0.0
        && input::shape_trigger(gamepad.right_trigger) == 0.0
        && !gamepad.dpad_up
        && !gamepad.dpad_down
        && !gamepad.dpad_left
        && !gamepad.dpad_right
        && !gamepad.right_bumper
        && !gamepad.south
        && !gamepad.east
        && !gamepad.west
        && !gamepad.start
        && !gamepad.select
}

fn neutral_snapshot(gamepad: &GamepadSeatInput) -> GamepadSeatInput {
    GamepadSeatInput {
        connected: gamepad.connected,
        name: gamepad.name.clone(),
        ..GamepadSeatInput::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiDirection {
    Up,
    Down,
    Left,
    Right,
}

impl UiDirection {
    const fn action(self) -> UiAction {
        match self {
            Self::Up => UiAction::Up,
            Self::Down => UiAction::Down,
            Self::Left => UiAction::Left,
            Self::Right => UiAction::Right,
        }
    }
}

fn ui_direction(gamepad: &GamepadSeatInput) -> Option<UiDirection> {
    if gamepad.dpad_up {
        return Some(UiDirection::Up);
    }
    if gamepad.dpad_down {
        return Some(UiDirection::Down);
    }
    if gamepad.dpad_left {
        return Some(UiDirection::Left);
    }
    if gamepad.dpad_right {
        return Some(UiDirection::Right);
    }

    let x = input::shape_stick(gamepad.left_stick_x);
    let y = input::shape_stick(gamepad.left_stick_y);
    if x.abs().max(y.abs()) < UI_STICK_THRESHOLD {
        return None;
    }
    if x.abs() > y.abs() {
        Some(if x < 0.0 {
            UiDirection::Left
        } else {
            UiDirection::Right
        })
    } else {
        Some(if y < 0.0 {
            UiDirection::Down
        } else {
            UiDirection::Up
        })
    }
}

#[derive(Debug, Default)]
struct UiRepeat {
    direction: Option<UiDirection>,
    next_repeat: Option<Instant>,
}

impl UiRepeat {
    fn update(&mut self, direction: Option<UiDirection>, now: Instant) -> Option<UiAction> {
        if direction != self.direction {
            self.direction = direction;
            self.next_repeat = direction.map(|_| now + UI_REPEAT_DELAY);
            return direction.map(UiDirection::action);
        }

        let direction = direction?;
        let next_repeat = self.next_repeat?;
        if now < next_repeat {
            return None;
        }
        self.next_repeat = Some(now + UI_REPEAT_INTERVAL);
        Some(direction.action())
    }

    fn reset(&mut self) {
        self.direction = None;
        self.next_repeat = None;
    }
}

#[derive(Debug, Default)]
struct SeatAssignments {
    seats: [Option<usize>; 2],
    connected: BTreeSet<usize>,
}

impl SeatAssignments {
    fn connect(&mut self, id: usize) -> Option<usize> {
        self.connected.insert(id);
        if let Some(seat) = self.seats.iter().position(|assigned| *assigned == Some(id)) {
            return Some(seat);
        }
        let seat = self.seats.iter().position(Option::is_none)?;
        self.seats[seat] = Some(id);
        Some(seat)
    }

    fn disconnect(&mut self, id: usize) -> Option<usize> {
        self.connected.remove(&id);
        self.seats.iter().position(|assigned| *assigned == Some(id))
    }

    fn connected_seat(&self, id: usize) -> Option<usize> {
        self.connected
            .contains(&id)
            .then(|| self.seats.iter().position(|assigned| *assigned == Some(id)))
            .flatten()
    }

    fn has_disconnected_seat(&self) -> bool {
        self.seats
            .iter()
            .flatten()
            .any(|id| !self.connected.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_can_capture_start_without_losing_the_host_controls_button() {
        assert_eq!(
            gameplay_button_route(Button::Start, true),
            Some(GameplayButtonRoute::Scenario)
        );
        assert_eq!(
            gameplay_button_route(Button::Start, false),
            Some(GameplayButtonRoute::HostPause)
        );
        assert_eq!(
            gameplay_button_route(Button::Select, true),
            Some(GameplayButtonRoute::HostControls)
        );
    }

    #[test]
    fn gamepads_take_the_first_two_seats_in_connection_order() {
        let mut assignments = SeatAssignments::default();

        assert_eq!(assignments.connect(41), Some(0));
        assert_eq!(assignments.connect(12), Some(1));
        assert_eq!(assignments.connect(99), None);
        assert_eq!(assignments.connected_seat(41), Some(0));
        assert_eq!(assignments.connected_seat(12), Some(1));
        assert_eq!(assignments.connected_seat(99), None);
    }

    #[test]
    fn reconnect_reclaims_the_reserved_seat() {
        let mut assignments = SeatAssignments::default();
        assignments.connect(41);
        assignments.connect(12);

        assert_eq!(assignments.disconnect(41), Some(0));
        assert!(assignments.has_disconnected_seat());
        assert_eq!(assignments.connected_seat(41), None);
        assert_eq!(assignments.connect(41), Some(0));
        assert_eq!(assignments.connected_seat(41), Some(0));
        assert!(!assignments.has_disconnected_seat());
    }

    #[test]
    fn ui_direction_prefers_dpad_then_uses_the_dominant_stick_axis() {
        let mut input = GamepadSeatInput {
            connected: true,
            left_stick_x: 1.0,
            left_stick_y: 0.7,
            ..GamepadSeatInput::default()
        };
        assert_eq!(ui_direction(&input), Some(UiDirection::Right));

        input.dpad_up = true;
        assert_eq!(ui_direction(&input), Some(UiDirection::Up));

        input.dpad_up = false;
        input.left_stick_x = 0.2;
        input.left_stick_y = 0.2;
        assert_eq!(ui_direction(&input), None);
    }

    #[test]
    fn retro_controller_profile_matches_only_the_exact_linux_uuid() {
        assert!(uses_retro_axis_dpad(RETRO_CONTROLLER_UUID));

        let mut other_version = RETRO_CONTROLLER_UUID;
        other_version[12] = 0x11;
        assert!(!uses_retro_axis_dpad(other_version));
    }

    #[test]
    fn retro_controller_axes_produce_all_four_dpad_directions() {
        assert_eq!(
            dpad_from_axes(1.0, 0.0),
            DpadInput {
                right: true,
                ..DpadInput::default()
            }
        );
        assert_eq!(
            dpad_from_axes(-1.0, 0.0),
            DpadInput {
                left: true,
                ..DpadInput::default()
            }
        );
        assert_eq!(
            dpad_from_axes(0.0, 1.0),
            DpadInput {
                up: true,
                ..DpadInput::default()
            }
        );
        assert_eq!(
            dpad_from_axes(0.0, -1.0),
            DpadInput {
                down: true,
                ..DpadInput::default()
            }
        );
        assert_eq!(dpad_from_axes(0.49, -0.49), DpadInput::default());
    }

    #[test]
    fn ui_repeat_emits_on_the_edge_then_after_the_repeat_delay() {
        let start = Instant::now();
        let mut repeat = UiRepeat::default();

        assert_eq!(
            repeat.update(Some(UiDirection::Down), start),
            Some(UiAction::Down)
        );
        assert_eq!(
            repeat.update(Some(UiDirection::Down), start + UI_REPEAT_DELAY / 2),
            None
        );
        assert_eq!(
            repeat.update(Some(UiDirection::Down), start + UI_REPEAT_DELAY),
            Some(UiAction::Down)
        );
        assert_eq!(repeat.update(None, start + UI_REPEAT_DELAY), None);
        assert_eq!(
            repeat.update(Some(UiDirection::Down), start + UI_REPEAT_DELAY),
            Some(UiAction::Down)
        );
    }

    #[test]
    fn held_steering_does_not_move_the_menu_after_a_mode_transition() {
        let mut handoff = ModeHandoff::default();
        handoff.observe(InputMode::Gameplay);
        handoff.filter(0, GamepadSeatInput::default());

        let steering = GamepadSeatInput {
            connected: true,
            left_stick_x: 1.0,
            ..GamepadSeatInput::default()
        };
        assert_eq!(handoff.filter(0, steering.clone()).left_stick_x, 1.0);

        assert!(handoff.observe(InputMode::Ui));
        let filtered = handoff.filter(0, steering);
        assert_eq!(ui_direction(&filtered), None);
        assert!(!handoff.accepts_input(0));

        handoff.filter(0, GamepadSeatInput::default());
        assert!(handoff.accepts_input(0));
        let next_press = GamepadSeatInput {
            connected: true,
            dpad_down: true,
            ..GamepadSeatInput::default()
        };
        assert_eq!(
            ui_direction(&handoff.filter(0, next_press)),
            Some(UiDirection::Down)
        );
    }

    #[test]
    fn held_confirm_does_not_become_laser_after_a_mode_transition() {
        let mut handoff = ModeHandoff::default();
        handoff.observe(InputMode::Ui);
        handoff.filter(0, GamepadSeatInput::default());

        let confirm = GamepadSeatInput {
            connected: true,
            south: true,
            ..GamepadSeatInput::default()
        };
        assert!(handoff.filter(0, confirm.clone()).south);

        // Resume is queued for the next host tick, so the input handoff must
        // begin before the visible mode catches up.
        handoff.block_all();
        assert!(!handoff.filter(0, confirm.clone()).south);
        assert!(!handoff.accepts_input(0));

        assert!(handoff.observe(InputMode::Gameplay));
        assert!(!handoff.filter(0, confirm).south);
        assert!(!handoff.accepts_input(0));

        handoff.filter(0, GamepadSeatInput::default());
        assert!(handoff.accepts_input(0));
        let next_press = GamepadSeatInput {
            connected: true,
            south: true,
            ..GamepadSeatInput::default()
        };
        assert!(handoff.filter(0, next_press).south);
    }
}
