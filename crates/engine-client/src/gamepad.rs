//! Backend-neutral gamepad polling, seat assignment, and controller UI routing.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gamepad, Gilrs};
use slint::{ComponentHandle, SharedString, Timer, TimerMode};

use crate::MainWindow;
use crate::input::{self, GameKey, GamepadSeatInput, SharedGamepadInput, SharedInput};
use crate::ui_navigation::UiAction;

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const UI_REPEAT_DELAY: Duration = Duration::from_millis(350);
const UI_REPEAT_INTERVAL: Duration = Duration::from_millis(100);
const UI_STICK_THRESHOLD: f32 = 0.55;

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
        }
    }

    fn initialize(&mut self, window: &MainWindow) {
        let connected = self
            .gilrs
            .gamepads()
            .map(|(id, gamepad)| (usize::from(id), gamepad.name().to_owned()))
            .collect::<Vec<_>>();
        for (id, name) in connected {
            if let Some(seat) = self.assignments.connect(id) {
                tracing::info!(
                    gamepad_id = id,
                    player = seat + 1,
                    gamepad = %name,
                    "assigned connected gamepad."
                );
            }
        }
        self.sample_gamepads();
        self.refresh_connection_ui(window);
    }

    fn tick(&mut self, window: &MainWindow) {
        while let Some(event) = self.gilrs.next_event() {
            let id = usize::from(event.id);
            match event.event {
                EventType::Connected => {
                    let name = self.gilrs.gamepad(event.id).name().to_owned();
                    if let Some(seat) = self.assignments.connect(id) {
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
                    self.route_button_edge(window, event_type);
                }
            }
        }

        self.sample_gamepads();
        if is_ui_mode(window) {
            self.update_ui_navigation(window);
        } else {
            self.ui_repeat.reset();
        }
    }

    fn route_button_edge(&mut self, window: &MainWindow, event: EventType) {
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
                window.invoke_ui_action(action.code());
            }
            return;
        }

        match button {
            Button::Start => self.input.borrow_mut().press(GameKey::Pause),
            Button::Select => self.input.borrow_mut().press(GameKey::Controls),
            _ => {}
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
        let mut gamepads = self.gamepads.borrow_mut();
        for (seat, snapshot) in snapshots {
            gamepads.set_seat(seat, snapshot);
        }
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

fn snapshot(gamepad: &Gamepad<'_>) -> GamepadSeatInput {
    GamepadSeatInput {
        connected: gamepad.is_connected(),
        name: gamepad.name().to_owned(),
        left_stick_x: gamepad.value(Axis::LeftStickX),
        left_stick_y: gamepad.value(Axis::LeftStickY),
        left_trigger: button_value(gamepad, Button::LeftTrigger2),
        right_trigger: button_value(gamepad, Button::RightTrigger2),
        dpad_up: gamepad.is_pressed(Button::DPadUp),
        dpad_down: gamepad.is_pressed(Button::DPadDown),
        dpad_left: gamepad.is_pressed(Button::DPadLeft),
        dpad_right: gamepad.is_pressed(Button::DPadRight),
        right_bumper: gamepad.is_pressed(Button::RightTrigger),
        south: gamepad.is_pressed(Button::South),
        east: gamepad.is_pressed(Button::East),
        west: gamepad.is_pressed(Button::West),
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
    window.get_launcher_visible()
        || window.get_ingame_menu_visible()
        || window.get_game_over_visible()
}

fn is_game_mode(window: &MainWindow) -> bool {
    !is_ui_mode(window)
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
}
