//! Backend-neutral gamepad polling, seat assignment, and controller UI routing.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gamepad, GamepadId, Gilrs, Mapping};
use slint::{ComponentHandle, SharedString, Timer, TimerMode};
use spacewars_control::UiAction;

use crate::controller_controls::{Device, SharedControllers};
use crate::controller_profile::{self, RawState};
use crate::input::{self, GameKey, GamepadSeatInput, SharedGamepadInput, SharedInput};
use crate::{MainWindow, UserActivity};

#[cfg(unix)]
mod simulated;
#[cfg(unix)]
pub(crate) use simulated::SimulatedInput;
#[cfg(test)]
mod assignment_tests;

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
    controllers: SharedControllers,
) -> Option<Timer> {
    let gilrs = match Gilrs::new() {
        Ok(gilrs) => gilrs,
        Err(err) => {
            tracing::warn!(error = %err, "gamepad backend unavailable; keyboard input remains active.");
            return None;
        }
    };

    let mut pump = GamepadPump::new(gilrs, input, gamepads, controllers);
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
    input: SharedInput,
    gamepads: SharedGamepadInput,
    ui_driver: Option<usize>,
    ui_snapshot: GamepadSeatInput,
    ui_repeat: UiRepeat,
    mode_handoff: ModeHandoff,
    controllers: SharedControllers,
    controller_epoch: u64,
}

impl GamepadPump {
    fn new(
        gilrs: Gilrs,
        input: SharedInput,
        gamepads: SharedGamepadInput,
        controllers: SharedControllers,
    ) -> Self {
        Self {
            gilrs,
            input,
            gamepads,
            ui_driver: None,
            ui_snapshot: GamepadSeatInput::default(),
            ui_repeat: UiRepeat::default(),
            mode_handoff: ModeHandoff::default(),
            controllers,
            controller_epoch: 0,
        }
    }

    fn initialize(&mut self, window: &MainWindow) {
        let connected = self.gilrs.gamepads().map(|(id, _)| id).collect::<Vec<_>>();
        for id in connected {
            apply_controller_profile(&mut self.gilrs, id);
        }
        let devices = self
            .gilrs
            .gamepads()
            .map(|(id, _)| self.device(id))
            .collect();
        self.controllers.borrow_mut().connect_many(devices);
        self.observe_mode(window);
        self.sample_gamepads(window);
        self.refresh_connection_ui(window);
    }

    fn tick(&mut self, window: &MainWindow) {
        self.observe_mode(window);
        while let Some(event) = self.gilrs.next_event() {
            let gamepad_id = event.id;
            let id = usize::from(event.id);
            match event.event {
                EventType::Connected => {
                    self.mode_handoff.block(id);
                    apply_controller_profile(&mut self.gilrs, event.id);
                    let name = self.gilrs.gamepad(event.id).name().to_owned();
                    self.controllers
                        .borrow_mut()
                        .connect(self.device(gamepad_id));
                    if let Some(seat) = self.controllers.borrow().seat(id) {
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
                    let seat = self.controllers.borrow().seat(id);
                    self.controllers.borrow_mut().disconnect(id);
                    self.mode_handoff.forget(id);
                    if let Some(seat) = seat {
                        self.gamepads.borrow_mut().disconnect_seat(seat);
                        tracing::warn!(gamepad_id = id, player = seat + 1, "gamepad disconnected.");
                        if !window.get_launcher_visible() {
                            let was_playing =
                                is_game_mode(window) && !window.get_autostart_running();
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
                    let consumed = self.controllers.borrow().captures_input();
                    if window.get_controllers_visible() {
                        let pad = self.gilrs.gamepad(gamepad_id);
                        let raw = raw_state(&pad);
                        let mapped = self.mapped_snapshot(id, &pad, &raw);
                        let edge = match event_type {
                            EventType::ButtonPressed(..) => Some(true),
                            EventType::ButtonReleased(..) => Some(false),
                            _ => None,
                        };
                        self.controllers.borrow_mut().observe(
                            id,
                            &raw,
                            &mapped,
                            edge,
                            Instant::now(),
                        );
                    }
                    if consumed {
                        continue;
                    }
                    self.observe_mode(window);
                    if is_pad_activity(event_type) {
                        self.ui_driver = Some(id);
                    }
                    let deliberate = match event_type {
                        EventType::ButtonPressed(..) | EventType::ButtonRepeated(..) => true,
                        EventType::AxisChanged(_, value, _) => value.abs() >= UI_STICK_THRESHOLD,
                        EventType::ButtonChanged(_, value, _) => value >= 0.5,
                        _ => false,
                    };
                    if deliberate && window.global::<UserActivity>().invoke_notify() {
                        self.begin_handoff();
                        continue;
                    }
                    self.route_button_edge(window, gamepad_id, event_type);
                }
            }
            self.observe_mode(window);
        }

        self.observe_mode(window);
        self.sample_gamepads(window);
        self.observe_mode(window);
        if is_ui_mode(window) && !self.controllers.borrow().captures_input() {
            self.update_ui_navigation(window);
        } else {
            self.ui_repeat.reset();
        }
    }

    fn route_button_edge(&mut self, window: &MainWindow, gamepad_id: GamepadId, event: EventType) {
        let id = usize::from(gamepad_id);
        if window.get_launcher_busy() || !self.mode_handoff.accepts_input(id) {
            return;
        }
        let EventType::ButtonPressed(button, code) = event else {
            return;
        };

        let gamepad = self.gilrs.gamepad(gamepad_id);
        let controllers = self.controllers.borrow();
        let profile = controllers.profile(usize::from(gamepad_id));
        let (button, start, select) = if let Some(profile) = profile {
            let Some(control) = controller_profile::button_control(profile, code.into_u32()) else {
                return;
            };
            let raw = raw_state(&gamepad);
            let mapped =
                controller_profile::remap(snapshot(&gamepad), profile, &raw, stick_codes(&gamepad));
            (logical_button(control), mapped.start, mapped.select)
        } else {
            (
                button,
                gamepad.is_pressed(Button::Start),
                gamepad.is_pressed(Button::Select),
            )
        };
        drop(controllers);
        let route = button_route(window, button, gamepad.name(), start, select);
        let seat = self.controllers.borrow().seat(id);
        if !route_allowed_for_assignment(route, seat) {
            return;
        }
        if matches!(route, ButtonRoute::Menu(_) | ButtonRoute::Host(_)) {
            self.begin_handoff();
        }
        // Menu/host routes do not consume the player argument. Unassigned
        // controllers may recover via menus, but never send scenario actions.
        apply_button_route(window, &self.input, route, seat.unwrap_or(0) as u8 + 1);
    }

    fn sample_gamepads(&mut self, window: &MainWindow) {
        if window.get_launcher_busy() || self.controllers.borrow().captures_input() {
            self.mode_handoff.block_all();
        }
        let snapshots = self
            .gilrs
            .gamepads()
            .map(|(id, gamepad)| {
                let id = usize::from(id);
                let raw = if window.get_controllers_visible()
                    || self.controllers.borrow().profile(id).is_some()
                {
                    raw_state(&gamepad)
                } else {
                    RawState::default()
                };
                let snapshot = self.mapped_snapshot(id, &gamepad, &raw);
                if window.get_controllers_visible() {
                    self.controllers.borrow_mut().observe(
                        id,
                        &raw,
                        &snapshot,
                        None,
                        Instant::now(),
                    );
                }
                (id, self.controllers.borrow().seat(id), snapshot)
            })
            .collect::<Vec<_>>();
        let held = snapshots.iter().any(|(_, _, pad)| autostart_pad_held(pad));
        window.global::<UserActivity>().set_gamepad_held(held);
        self.ui_snapshot = publish_gamepad_samples(
            &mut self.mode_handoff,
            &mut self.gamepads.borrow_mut(),
            self.ui_driver,
            snapshots,
        );
    }

    fn observe_mode(&mut self, window: &MainWindow) {
        let epoch = self.controllers.borrow().epoch;
        if self
            .mode_handoff
            .observe_epoch(&mut self.controller_epoch, epoch)
        {
            self.ui_driver = None;
            self.ui_repeat.reset();
            self.refresh_connection_ui(window);
        }
        if self.mode_handoff.observe(InputMode::from_window(window)) {
            self.ui_driver = None;
            self.ui_repeat.reset();
        }
    }

    fn device(&self, id: GamepadId) -> Device {
        let pad = self.gilrs.gamepad(id);
        let uuid = pad
            .uuid()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Device {
            id: usize::from(id),
            key: format!(
                "gilrs-v1:{}:{uuid}:{:?}:{:?}:{}",
                std::env::consts::OS,
                pad.vendor_id(),
                pad.product_id(),
                pad.os_name()
            ),
            name: pad.name().to_owned(),
            seat: None,
        }
    }

    fn mapped_snapshot(&self, id: usize, pad: &Gamepad<'_>, raw: &RawState) -> GamepadSeatInput {
        let original = snapshot(pad);
        if let Some(profile) = self.controllers.borrow().profile(id) {
            controller_profile::remap(original, profile, raw, stick_codes(pad))
        } else {
            original
        }
    }

    fn begin_handoff(&mut self) {
        self.mode_handoff.block_all();
        self.ui_driver = None;
        self.ui_repeat.reset();
    }

    fn update_ui_navigation(&mut self, window: &MainWindow) {
        let Some(_) = self.ui_driver else {
            self.ui_repeat.reset();
            return;
        };
        let gamepad = &self.ui_snapshot;
        if !gamepad.connected {
            self.ui_repeat.reset();
            return;
        }

        if let Some(action) = self.ui_repeat.update(ui_direction(gamepad), Instant::now()) {
            window.invoke_ui_action(action.code());
        }
    }

    fn refresh_connection_ui(&self, window: &MainWindow) {
        let devices = self
            .gilrs
            .gamepads()
            .map(|(id, pad)| {
                let seat = self.controllers.borrow().seat(usize::from(id));
                format!(
                    "{} · {}",
                    pad.name(),
                    seat.map(|seat| format!("Player {}", seat + 1))
                        .unwrap_or_else(|| "Unassigned".into())
                )
            })
            .collect::<Vec<_>>();
        window.set_device_controllers(if devices.is_empty() {
            "No gamepads connected · Keyboard input available".into()
        } else {
            devices.join("\n").into()
        });
        let binding = |seat: usize| {
            let player = seat + 1;
            if self.controllers.borrow().connected_id(seat).is_some() {
                format!("P{player} PAD + KEY")
            } else {
                format!("P{player} KEY")
            }
        };
        window.set_p1_input_binding(SharedString::from(binding(0)));
        window.set_p2_input_binding(SharedString::from(binding(1)));
        if !self.controllers.borrow().has_disconnected_seat() {
            window.set_controller_disconnected_visible(false);
            window.set_controller_disconnected_text(SharedString::from(""));
        } else if window.get_controller_disconnected_visible() {
            window.set_controller_disconnected_text(format!(
                "{} controller disconnected — slot reserved. Use App Settings → Controllers to replace it; keyboard/touch still work.",
                self.controllers.borrow().disconnected_player_labels(),
            ).into());
        }
    }
}

/// Use physical-device gates even for unassigned controllers: they must still
/// navigate menus, without being folded into either player's gameplay state.
fn publish_gamepad_samples(
    handoff: &mut ModeHandoff,
    gamepads: &mut input::GamepadInput,
    ui_driver: Option<usize>,
    snapshots: Vec<(usize, Option<usize>, GamepadSeatInput)>,
) -> GamepadSeatInput {
    let mut seats = std::array::from_fn(|_| GamepadSeatInput::default());
    let mut ui = GamepadSeatInput::default();
    for (id, seat, pad) in snapshots {
        let pad = handoff.filter(id, pad);
        if ui_driver == Some(id) {
            ui = pad.clone();
        }
        if let Some(seat) = seat.and_then(|seat| seats.get_mut(seat)) {
            *seat = pad;
        }
    }
    gamepads.replace_seats(seats);
    ui
}

fn route_allowed_for_assignment(route: ButtonRoute, seat: Option<usize>) -> bool {
    seat.is_some() || matches!(route, ButtonRoute::Menu(_) | ButtonRoute::Host(_))
}

fn clock_next_event_button(gamepad_name: &str) -> Button {
    // sw-picade-2's upper-right blue button was captured as HAT Button 3,
    // Linux BTN_WEST (308). Keep ordinary controllers on their right shoulder;
    // don't remap this cabinet's buttons globally for NES or other scenarios.
    if gamepad_name == "Space-Wars Picade" {
        Button::West
    } else {
        Button::RightTrigger
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonRoute {
    Menu(UiAction),
    Host(GameKey),
    ClockNext,
    ClockDuck,
    Scenario,
}

// Physical and CLI controllers use the same edge routing. Continuous gameplay
// controls remain in the normal GamepadSeatInput mapping for each scenario.
fn button_route(
    window: &MainWindow,
    button: Button,
    name: &str,
    start: bool,
    select: bool,
) -> ButtonRoute {
    if is_ui_mode(window) {
        return match button {
            Button::South => ButtonRoute::Menu(UiAction::Confirm),
            Button::East => ButtonRoute::Menu(UiAction::Back),
            Button::Start => ButtonRoute::Menu(UiAction::Start),
            Button::Select => ButtonRoute::Menu(UiAction::Controls),
            _ => ButtonRoute::Scenario,
        };
    }
    if window.get_launcher_scenario() == "clock" && button == clock_next_event_button(name) {
        return ButtonRoute::ClockNext;
    }
    if window.get_launcher_scenario() == "clock" && button == Button::North {
        return ButtonRoute::ClockDuck;
    }
    let captures_start = window.get_scenario_captures_gamepad_start();
    let captures_select = window.get_scenario_captures_gamepad_select();
    if native_console_menu_chord(button, captures_start, captures_select, start, select) {
        return ButtonRoute::Host(GameKey::Controls);
    }
    match gameplay_button_route(button, captures_start, captures_select) {
        Some(GameplayButtonRoute::HostPause) => ButtonRoute::Host(GameKey::Pause),
        Some(GameplayButtonRoute::HostControls) => ButtonRoute::Host(GameKey::Controls),
        _ => ButtonRoute::Scenario,
    }
}

fn apply_button_route(window: &MainWindow, input: &SharedInput, route: ButtonRoute, player: u8) {
    match route {
        ButtonRoute::Menu(action) => window.invoke_ui_action(action.code()),
        ButtonRoute::Host(key) => input.borrow_mut().press(key),
        ButtonRoute::ClockNext => input.borrow_mut().request_clock_next_event(),
        ButtonRoute::ClockDuck => input.borrow_mut().request_clock_player_duck(player),
        ButtonRoute::Scenario => {}
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
    scenario_captures_select: bool,
) -> Option<GameplayButtonRoute> {
    match button {
        Button::Start if scenario_captures_start => Some(GameplayButtonRoute::Scenario),
        Button::Start => Some(GameplayButtonRoute::HostPause),
        Button::Select if scenario_captures_select => Some(GameplayButtonRoute::Scenario),
        Button::Select => Some(GameplayButtonRoute::HostControls),
        _ => None,
    }
}

fn native_console_menu_chord(
    button: Button,
    scenario_captures_start: bool,
    scenario_captures_select: bool,
    start_pressed: bool,
    select_pressed: bool,
) -> bool {
    scenario_captures_start
        && scenario_captures_select
        && start_pressed
        && select_pressed
        && matches!(button, Button::Start | Button::Select)
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
        right_stick_x: gamepad.value(Axis::RightStickX),
        right_stick_y: gamepad.value(Axis::RightStickY),
        left_trigger: button_value(gamepad, Button::LeftTrigger2),
        right_trigger: button_value(gamepad, Button::RightTrigger2),
        dpad_up: gamepad.is_pressed(Button::DPadUp) || axis_dpad.up,
        dpad_down: gamepad.is_pressed(Button::DPadDown) || axis_dpad.down,
        dpad_left: gamepad.is_pressed(Button::DPadLeft) || axis_dpad.left,
        dpad_right: gamepad.is_pressed(Button::DPadRight) || axis_dpad.right,
        left_bumper: gamepad.is_pressed(Button::LeftTrigger),
        right_bumper: gamepad.is_pressed(Button::RightTrigger),
        south: gamepad.is_pressed(Button::South),
        east: gamepad.is_pressed(Button::East),
        north: gamepad.is_pressed(Button::North),
        west: gamepad.is_pressed(Button::West),
        start: gamepad.is_pressed(Button::Start),
        select: gamepad.is_pressed(Button::Select),
    }
}

fn stick_codes(gamepad: &Gamepad<'_>) -> [Option<u32>; 4] {
    [
        Axis::LeftStickX,
        Axis::LeftStickY,
        Axis::RightStickX,
        Axis::RightStickY,
    ]
    .map(|axis| gamepad.axis_code(axis).map(|code| code.into_u32()))
}

fn raw_state(gamepad: &Gamepad<'_>) -> RawState {
    RawState {
        button_values: gamepad
            .state()
            .buttons()
            .map(|(code, data)| (code.into_u32(), data.value()))
            .collect(),
        buttons: gamepad
            .state()
            .buttons()
            .map(|(code, data)| {
                (
                    code.into_u32(),
                    if data.is_pressed() {
                        data.value().max(0.65)
                    } else {
                        0.0
                    },
                )
            })
            .collect(),
        // Hat axes are already converted into distinct D-pad buttons by
        // gilrs. Sampling them again would capture a duplicate input.
        axes: [
            Axis::LeftStickX,
            Axis::LeftStickY,
            Axis::RightStickX,
            Axis::RightStickY,
        ]
        .into_iter()
        .filter_map(|axis| {
            gamepad
                .axis_code(axis)
                .map(|code| (code.into_u32(), gamepad.value(axis)))
        })
        .collect(),
    }
}

fn logical_button(control: engine_common::ControllerControl) -> Button {
    use engine_common::ControllerControl as C;
    match control {
        C::Up => Button::DPadUp,
        C::Down => Button::DPadDown,
        C::Left => Button::DPadLeft,
        C::Right => Button::DPadRight,
        C::South => Button::South,
        C::East => Button::East,
        C::West => Button::West,
        C::North => Button::North,
        C::LeftBumper => Button::LeftTrigger,
        C::RightBumper => Button::RightTrigger,
        C::LeftTrigger => Button::LeftTrigger2,
        C::RightTrigger => Button::RightTrigger2,
        C::Select => Button::Select,
        C::Start => Button::Start,
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

fn autostart_pad_held(pad: &GamepadSeatInput) -> bool {
    pad.left_stick_x.abs() >= UI_STICK_THRESHOLD
        || pad.left_stick_y.abs() >= UI_STICK_THRESHOLD
        || pad.right_stick_x.abs() >= UI_STICK_THRESHOLD
        || pad.right_stick_y.abs() >= UI_STICK_THRESHOLD
        || pad.left_trigger >= 0.5
        || pad.right_trigger >= 0.5
        || pad.dpad_up
        || pad.dpad_down
        || pad.dpad_left
        || pad.dpad_right
        || pad.left_bumper
        || pad.right_bumper
        || pad.south
        || pad.east
        || pad.north
        || pad.west
        || pad.start
        || pad.select
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
    InputMode::from_window(window) == InputMode::Gameplay
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Busy,
    Ui,
    Gameplay,
}

impl InputMode {
    fn from_window(window: &MainWindow) -> Self {
        if window.get_launcher_busy() {
            Self::Busy
        } else if window.get_launcher_visible()
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
    awaiting_neutral: BTreeMap<usize, bool>,
}

impl ModeHandoff {
    fn observe_epoch(&mut self, observed: &mut u64, current: u64) -> bool {
        if *observed == current {
            return false;
        }
        *observed = current;
        self.block_all();
        true
    }

    fn observe(&mut self, mode: InputMode) -> bool {
        if self.mode == Some(mode) {
            return false;
        }
        self.mode = Some(mode);
        self.block_all();
        true
    }

    fn block_all(&mut self) {
        self.awaiting_neutral
            .values_mut()
            .for_each(|waiting| *waiting = true);
    }

    fn block(&mut self, device: usize) {
        self.awaiting_neutral.insert(device, true);
    }

    fn forget(&mut self, device: usize) {
        self.awaiting_neutral.remove(&device);
    }

    fn accepts_input(&self, device: usize) -> bool {
        self.awaiting_neutral
            .get(&device)
            .is_some_and(|awaiting_neutral| !awaiting_neutral)
    }

    fn filter(&mut self, device: usize, snapshot: GamepadSeatInput) -> GamepadSeatInput {
        if self.accepts_input(device) {
            return snapshot;
        }
        if is_neutral(&snapshot) {
            self.awaiting_neutral.insert(device, false);
            snapshot
        } else {
            neutral_snapshot(&snapshot)
        }
    }
}

fn is_neutral(gamepad: &GamepadSeatInput) -> bool {
    input::shape_stick(gamepad.left_stick_x) == 0.0
        && input::shape_stick(gamepad.left_stick_y) == 0.0
        && input::shape_stick(gamepad.right_stick_x) == 0.0
        && input::shape_stick(gamepad.right_stick_y) == 0.0
        && input::shape_trigger(gamepad.left_trigger) == 0.0
        && input::shape_trigger(gamepad.right_trigger) == 0.0
        && !gamepad.dpad_up
        && !gamepad.dpad_down
        && !gamepad.dpad_left
        && !gamepad.dpad_right
        && !gamepad.left_bumper
        && !gamepad.right_bumper
        && !gamepad.south
        && !gamepad.east
        && !gamepad.north
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_console_captures_start_and_select_without_losing_host_controls() {
        assert_eq!(
            gameplay_button_route(Button::Start, true, true),
            Some(GameplayButtonRoute::Scenario)
        );
        assert_eq!(
            gameplay_button_route(Button::Start, false, false),
            Some(GameplayButtonRoute::HostPause)
        );
        assert_eq!(
            gameplay_button_route(Button::Select, true, true),
            Some(GameplayButtonRoute::Scenario)
        );
        assert_eq!(
            gameplay_button_route(Button::Select, false, false),
            Some(GameplayButtonRoute::HostControls)
        );
        assert!(native_console_menu_chord(
            Button::Select,
            true,
            true,
            true,
            true
        ));
        assert!(!native_console_menu_chord(
            Button::Select,
            true,
            true,
            false,
            true
        ));
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
    fn held_next_event_button_cannot_leak_from_menu_to_clock() {
        let mut handoff = ModeHandoff::default();
        handoff.observe(InputMode::Ui);
        handoff.filter(0, GamepadSeatInput::default());
        let held = GamepadSeatInput {
            connected: true,
            right_bumper: true,
            west: true,
            ..Default::default()
        };
        handoff.observe(InputMode::Gameplay);
        for _ in 0..60 {
            assert!(!handoff.filter(0, held.clone()).right_bumper);
            assert!(!handoff.filter(0, held.clone()).west);
            assert!(!handoff.accepts_input(0));
        }
        handoff.filter(0, GamepadSeatInput::default());
        assert!(handoff.accepts_input(0));
        assert!(handoff.filter(0, held).right_bumper);
    }

    #[test]
    fn clock_binding_uses_the_captured_picade_button_without_changing_other_controllers() {
        assert_eq!(clock_next_event_button("Space-Wars Picade"), Button::West);
        assert_eq!(clock_next_event_button("USB Gamepad"), Button::RightTrigger);
        assert_eq!(
            clock_next_event_button("Xbox Controller"),
            Button::RightTrigger
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

    #[test]
    fn leaving_busy_requires_a_fresh_release_before_gameplay_or_menu_input() {
        for destination in [InputMode::Gameplay, InputMode::Ui] {
            let mut handoff = ModeHandoff::default();
            handoff.observe(InputMode::Busy);
            handoff.filter(0, GamepadSeatInput::default());
            let held = GamepadSeatInput {
                connected: true,
                south: true,
                dpad_down: true,
                ..GamepadSeatInput::default()
            };
            // Busy polling never forwards controls, even after a neutral sample.
            handoff.block_all();
            assert!(is_neutral(&handoff.filter(0, held.clone())));
            assert!(handoff.observe(destination));
            assert!(is_neutral(&handoff.filter(0, held.clone())));
            assert!(!handoff.accepts_input(0));
            handoff.filter(0, GamepadSeatInput::default());
            assert!(handoff.accepts_input(0));
            assert_eq!(handoff.filter(0, held.clone()), held);
        }
    }
}
