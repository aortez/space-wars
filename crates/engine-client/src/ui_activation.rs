//! Stable control-ID activation routed through the ordinary menu-action path.

use spacewars_control::UiAction;

use crate::{MainWindow, handle_ui_action};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationFocus {
    Launcher(i32),
    LauncherSettings(Option<i32>),
    LauncherControls(i32),
    TouchTest,
    PauseMain(i32),
    PauseControls,
    GameOver(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationTarget {
    focus: ActivationFocus,
    action: UiAction,
}

pub(crate) fn activate(window: &MainWindow, control_id: &str) -> bool {
    let Some(target) = activation_target(control_id, window.get_scenario_benchmark_available())
    else {
        return false;
    };

    match target.focus {
        ActivationFocus::Launcher(index) => window.set_launcher_focus_index(index),
        ActivationFocus::LauncherSettings(index) => {
            if let Some(index) = index {
                window.set_launcher_settings_focus_index(index);
            }
        }
        ActivationFocus::LauncherControls(index) => {
            window.set_launcher_controls_focus_index(index);
        }
        ActivationFocus::TouchTest | ActivationFocus::PauseControls => {}
        ActivationFocus::PauseMain(index) => window.set_ingame_menu_focus_index(index),
        ActivationFocus::GameOver(index) => window.set_game_over_focus_index(index),
    }
    handle_ui_action(window, target.action);
    true
}

#[cfg(test)]
pub(crate) fn supports(control_id: &str, benchmark_available: bool) -> bool {
    activation_target(control_id, benchmark_available).is_some()
}

fn activation_target(control_id: &str, benchmark_available: bool) -> Option<ActivationTarget> {
    let target = match control_id {
        "launcher.scenario.previous" => launcher(0, UiAction::Left),
        "launcher.scenario.next" => launcher(0, UiAction::Right),
        "launcher.start" => launcher(1, UiAction::Confirm),
        "launcher.settings" => launcher(2, UiAction::Confirm),
        "launcher.controls" => launcher(3, UiAction::Confirm),
        "launcher.quit" => launcher(4, UiAction::Confirm),
        "launcher.settings.back" => launcher_settings(None, UiAction::Back),
        "launcher.settings.start" => launcher_settings(None, UiAction::Start),
        "launcher.controls.back" => launcher_controls(0),
        "launcher.controls.touch-test" => launcher_controls(1),
        "launcher.controls.start" => launcher_controls(2),
        "launcher.touch-test.done" => ActivationTarget {
            focus: ActivationFocus::TouchTest,
            action: UiAction::Back,
        },
        "pause.resume" => pause_main(0),
        "pause.restart" => pause_main(1),
        "pause.benchmark" => pause_main(2),
        "pause.controls" => pause_main(2 + i32::from(benchmark_available)),
        "pause.return-to-launcher" => pause_main(3 + i32::from(benchmark_available)),
        "pause.controls.back" => ActivationTarget {
            focus: ActivationFocus::PauseControls,
            action: UiAction::Back,
        },
        "pause.controls.resume" => ActivationTarget {
            focus: ActivationFocus::PauseControls,
            action: UiAction::Start,
        },
        "game-over.play-again" => game_over(0),
        "game-over.return-to-launcher" => game_over(1),
        _ => return launcher_setting_target(control_id),
    };
    Some(target)
}

fn launcher_setting_target(control_id: &str) -> Option<ActivationTarget> {
    let (setting_id, action) = control_id
        .strip_suffix(".previous")
        .map(|id| (id, UiAction::Left))
        .or_else(|| {
            control_id
                .strip_suffix(".next")
                .map(|id| (id, UiAction::Right))
        })?;
    let focus_index = match setting_id {
        "launcher.settings.renderer" | "launcher.settings.nes.cartridge" => 0,
        "launcher.settings.raster-scale" => 1,
        "launcher.settings.spacewars.preset" | "launcher.settings.pizza.desired-balls" => 2,
        "launcher.settings.spacewars.planets" | "launcher.settings.pizza.spawn-rate" => 3,
        "launcher.settings.spacewars.asteroids" => 4,
        "launcher.settings.spacewars.player-health" => 5,
        "launcher.settings.spacewars.player-2" => 6,
        _ => return None,
    };
    Some(launcher_settings(Some(focus_index), action))
}

const fn launcher(index: i32, action: UiAction) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::Launcher(index),
        action,
    }
}

const fn launcher_settings(index: Option<i32>, action: UiAction) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::LauncherSettings(index),
        action,
    }
}

const fn launcher_controls(index: i32) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::LauncherControls(index),
        action: UiAction::Confirm,
    }
}

const fn pause_main(index: i32) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::PauseMain(index),
        action: UiAction::Confirm,
    }
}

const fn game_over(index: i32) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::GameOver(index),
        action: UiAction::Confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_targets_account_for_the_optional_benchmark() {
        assert_eq!(
            activation_target("pause.controls", true),
            Some(pause_main(3))
        );
        assert_eq!(
            activation_target("pause.controls", false),
            Some(pause_main(2))
        );
        assert_eq!(
            activation_target("pause.return-to-launcher", true),
            Some(pause_main(4))
        );
        assert_eq!(
            activation_target("pause.return-to-launcher", false),
            Some(pause_main(3))
        );
    }

    #[test]
    fn settings_targets_select_the_visible_row_and_direction() {
        assert_eq!(
            activation_target("launcher.settings.spacewars.player-health.previous", false),
            Some(launcher_settings(Some(5), UiAction::Left))
        );
        assert_eq!(
            activation_target("launcher.settings.pizza.spawn-rate.next", false),
            Some(launcher_settings(Some(3), UiAction::Right))
        );
        assert_eq!(
            activation_target("launcher.settings.unknown.next", false),
            None
        );
    }
}
