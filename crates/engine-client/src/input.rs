//! Client-side keyboard state and original Spacewars control mapping.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use engine_common::Action;
use scenario_spacewars::SpacewarsAction;
use slint::ComponentHandle;
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, PhysicalKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};

use crate::MainWindow;

pub(crate) type SharedInput = Rc<RefCell<ClientInput>>;

#[derive(Debug, Default)]
pub(crate) struct ClientInput {
    pressed: BTreeSet<GameKey>,
    released: BTreeSet<GameKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GameKey {
    P1Wing,
    P1Thrust,
    P1Reverse,
    P1TurnLeft,
    P1TurnRight,
    P1Laser,
    P1Cannon,
    P2Wing,
    P2Thrust,
    P2Reverse,
    P2TurnLeft,
    P2TurnRight,
    P2Laser,
    P2Cannon,
}

pub(crate) fn install_window_input(window: &MainWindow, input: SharedInput) {
    // `has_winit_window()` stays false until the native window is created during
    // `run()`. The event filter only needs the winit adapter, so install it
    // before the event loop starts.
    window.window().on_winit_window_event(move |_, event| {
        if matches!(event, WindowEvent::Focused(false)) {
            input.borrow_mut().clear();
            return EventResult::Propagate;
        }

        let Some((key, state)) = mapped_key_event(event) else {
            return EventResult::Propagate;
        };

        match state {
            ElementState::Pressed => input.borrow_mut().press(key),
            ElementState::Released => input.borrow_mut().release(key),
        }

        EventResult::Propagate
    });
}

impl ClientInput {
    pub(crate) fn actions_for_spacewars(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();

        self.append_player_actions(
            PlayerControlMap {
                player: 0,
                wing: GameKey::P1Wing,
                thrust: GameKey::P1Thrust,
                reverse: GameKey::P1Reverse,
                turn_left: GameKey::P1TurnLeft,
                turn_right: GameKey::P1TurnRight,
                laser: GameKey::P1Laser,
                cannon: GameKey::P1Cannon,
            },
            &mut actions,
        );
        self.append_player_actions(
            PlayerControlMap {
                player: 1,
                wing: GameKey::P2Wing,
                thrust: GameKey::P2Thrust,
                reverse: GameKey::P2Reverse,
                turn_left: GameKey::P2TurnLeft,
                turn_right: GameKey::P2TurnRight,
                laser: GameKey::P2Laser,
                cannon: GameKey::P2Cannon,
            },
            &mut actions,
        );

        self.released.clear();
        actions
    }

    pub(crate) fn clear(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }

    pub(crate) fn press(&mut self, key: GameKey) {
        self.pressed.insert(key);
        self.released.remove(&key);
    }

    pub(crate) fn release(&mut self, key: GameKey) {
        if self.pressed.remove(&key) {
            self.released.insert(key);
        }
    }

    fn append_player_actions(&self, controls: PlayerControlMap, actions: &mut Vec<Action>) {
        if self.pressed.contains(&controls.wing) {
            actions.push(SpacewarsAction::close_wings(controls.player));
        } else if self.released.contains(&controls.wing) {
            actions.push(SpacewarsAction::open_wings(controls.player));
        } else if self.pressed.contains(&controls.thrust) {
            actions.push(SpacewarsAction::thrust(controls.player));
        } else if self.pressed.contains(&controls.reverse) {
            actions.push(SpacewarsAction::reverse(controls.player));
        } else if self.released.contains(&controls.thrust)
            || self.released.contains(&controls.reverse)
        {
            actions.push(SpacewarsAction::thrust_halt(controls.player));
        }

        if self.pressed.contains(&controls.turn_left) {
            actions.push(SpacewarsAction::turn_left(controls.player));
        } else if self.pressed.contains(&controls.turn_right) {
            actions.push(SpacewarsAction::turn_right(controls.player));
        } else if self.released.contains(&controls.turn_left)
            || self.released.contains(&controls.turn_right)
        {
            actions.push(SpacewarsAction::turn_halt(controls.player));
        }

        if self.pressed.contains(&controls.laser) {
            actions.push(SpacewarsAction::fire_laser(controls.player));
        } else if self.released.contains(&controls.laser) {
            actions.push(SpacewarsAction::fire_laser_halt(controls.player));
        }

        if self.pressed.contains(&controls.cannon) {
            actions.push(SpacewarsAction::fire_cannon(controls.player));
        } else {
            actions.push(SpacewarsAction::fire_cannon_halt(controls.player));
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PlayerControlMap {
    player: usize,
    wing: GameKey,
    thrust: GameKey,
    reverse: GameKey,
    turn_left: GameKey,
    turn_right: GameKey,
    laser: GameKey,
    cannon: GameKey,
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
        KeyCode::KeyJ => Some(GameKey::P1Wing),
        KeyCode::KeyW => Some(GameKey::P1Thrust),
        KeyCode::KeyS => Some(GameKey::P1Reverse),
        KeyCode::KeyA => Some(GameKey::P1TurnLeft),
        KeyCode::KeyD => Some(GameKey::P1TurnRight),
        KeyCode::Space => Some(GameKey::P1Laser),
        KeyCode::KeyK => Some(GameKey::P1Cannon),
        KeyCode::PageDown => Some(GameKey::P2Wing),
        KeyCode::Numpad8 => Some(GameKey::P2Thrust),
        KeyCode::Numpad5 => Some(GameKey::P2Reverse),
        KeyCode::Numpad4 => Some(GameKey::P2TurnLeft),
        KeyCode::Numpad6 => Some(GameKey::P2TurnRight),
        KeyCode::Delete => Some(GameKey::P2Laser),
        KeyCode::End => Some(GameKey::P2Cannon),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenario_spacewars::{SpacewarsAction as ScenarioAction, SpacewarsActionKind};

    fn decoded(actions: &[Action]) -> Vec<ScenarioAction> {
        actions.iter().filter_map(ScenarioAction::decode).collect()
    }

    fn has_action(actions: &[ScenarioAction], player: usize, kind: SpacewarsActionKind) -> bool {
        actions
            .iter()
            .any(|action| action.player == player && action.kind == kind)
    }

    #[test]
    fn original_p1_wing_priority_suppresses_thrust_until_release_is_consumed() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Wing);
        input.press(GameKey::P1Thrust);

        let actions = decoded(&input.actions_for_spacewars());
        assert!(has_action(&actions, 0, SpacewarsActionKind::CloseWings));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::Thrust));

        input.release(GameKey::P1Wing);
        let actions = decoded(&input.actions_for_spacewars());
        assert!(has_action(&actions, 0, SpacewarsActionKind::OpenWings));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::Thrust));

        let actions = decoded(&input.actions_for_spacewars());
        assert!(has_action(&actions, 0, SpacewarsActionKind::Thrust));
    }

    #[test]
    fn release_emits_one_tick_halt_for_continuous_controls() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1TurnLeft);
        input.press(GameKey::P1Laser);
        input.actions_for_spacewars();

        input.release(GameKey::P1TurnLeft);
        input.release(GameKey::P1Laser);
        let actions = decoded(&input.actions_for_spacewars());
        assert!(has_action(&actions, 0, SpacewarsActionKind::TurnHalt));
        assert!(has_action(&actions, 0, SpacewarsActionKind::FireLaserHalt));

        let actions = decoded(&input.actions_for_spacewars());
        assert!(!has_action(&actions, 0, SpacewarsActionKind::TurnHalt));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::FireLaserHalt));
    }

    #[test]
    fn p2_controls_use_original_numpad_and_navigation_keys() {
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
    }
}
