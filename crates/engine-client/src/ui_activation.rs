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
    PauseSound,
    Sound(i32),
    Autostart(i32),
    DeviceInfo,
    PauseControls,
    PauseClock(i32),
    GameOver(i32),
    Gameplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationTarget {
    focus: ActivationFocus,
    action: UiAction,
}

pub(crate) fn activate(window: &MainWindow, control_id: &str) -> bool {
    let Some(target) = activation_target(
        control_id,
        window.get_scenario_benchmark_available(),
        window.get_launcher_scenario() == "spacewars",
    ) else {
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
        ActivationFocus::TouchTest
        | ActivationFocus::PauseControls
        | ActivationFocus::Gameplay
        | ActivationFocus::DeviceInfo => {}
        ActivationFocus::PauseMain(index) => window.set_ingame_menu_focus_index(index),
        ActivationFocus::PauseSound => window.set_ingame_menu_focus_index(
            4 + i32::from(
                window.get_scenario_benchmark_available()
                    || matches!(
                        window.get_launcher_scenario().as_str(),
                        "clock" | "spacewars"
                    ),
            ),
        ),
        ActivationFocus::Sound(index) => window.set_sound_focus_index(index),
        ActivationFocus::Autostart(index) => window.set_autostart_focus_index(index),
        ActivationFocus::PauseClock(index) => window.set_ingame_clock_focus_index(index),
        ActivationFocus::GameOver(index) => window.set_game_over_focus_index(index),
    }
    handle_ui_action(window, target.action);
    true
}

#[cfg(test)]
pub(crate) fn supports(control_id: &str, benchmark_available: bool) -> bool {
    activation_target(control_id, benchmark_available, false).is_some()
}

fn activation_target(
    control_id: &str,
    benchmark_available: bool,
    material_match: bool,
) -> Option<ActivationTarget> {
    let target = match control_id {
        "gameplay.clock-controls" => ActivationTarget {
            focus: ActivationFocus::Gameplay,
            action: UiAction::Controls,
        },
        "launcher.scenario.previous" => launcher(0, UiAction::Left),
        "launcher.scenario.next" => launcher(0, UiAction::Right),
        "launcher.start" => launcher(1, UiAction::Confirm),
        "launcher.new-match" => launcher(6, UiAction::Confirm),
        "launcher.settings" => launcher(2, UiAction::Confirm),
        "launcher.controls" => launcher(3, UiAction::Confirm),
        "launcher.quit" => launcher(4, UiAction::Confirm),
        "launcher.sound" => launcher(5, UiAction::Confirm),
        "pause.sound" => ActivationTarget {
            focus: ActivationFocus::PauseSound,
            action: UiAction::Confirm,
        },
        "sound.volume.previous" => sound(0, UiAction::Left),
        "sound.volume.next" => sound(0, UiAction::Right),
        "sound.mute" => sound(1, UiAction::Confirm),
        "settings.fps-counter" => sound(2, UiAction::Confirm),
        "settings.device-info" => sound(3, UiAction::Confirm),
        "settings.autostart" => sound(4, UiAction::Confirm),
        "sound.back" => sound(5, UiAction::Confirm),
        "sound.retry" => sound(6, UiAction::Confirm),
        "autostart.activity.previous" => ActivationTarget {
            focus: ActivationFocus::Autostart(0),
            action: UiAction::Left,
        },
        "autostart.activity.next" => ActivationTarget {
            focus: ActivationFocus::Autostart(0),
            action: UiAction::Right,
        },
        "autostart.delay.previous" => ActivationTarget {
            focus: ActivationFocus::Autostart(1),
            action: UiAction::Left,
        },
        "autostart.delay.next" => ActivationTarget {
            focus: ActivationFocus::Autostart(1),
            action: UiAction::Right,
        },
        "autostart.start-now" => ActivationTarget {
            focus: ActivationFocus::Autostart(2),
            action: UiAction::Confirm,
        },
        "autostart.back" => ActivationTarget {
            focus: ActivationFocus::Autostart(3),
            action: UiAction::Confirm,
        },
        "info.back" => ActivationTarget {
            focus: ActivationFocus::DeviceInfo,
            action: UiAction::Back,
        },
        "info.scroll-up" => ActivationTarget {
            focus: ActivationFocus::DeviceInfo,
            action: UiAction::Up,
        },
        "info.scroll-down" => ActivationTarget {
            focus: ActivationFocus::DeviceInfo,
            action: UiAction::Down,
        },
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
        "pause.new-match" => pause_main(4),
        "pause.benchmark" => pause_main(2),
        "pause.controls" => pause_main(2 + i32::from(benchmark_available)),
        "pause.clock" => pause_main(4),
        "pause.clock.time-format.previous" => pause_clock(0, UiAction::Left),
        "pause.clock.time-format.next" => pause_clock(0, UiAction::Right),
        "pause.clock.event-profile.previous" => pause_clock(1, UiAction::Left),
        "pause.clock.event-profile.next" => pause_clock(1, UiAction::Right),
        "pause.clock.falling" => pause_clock(2, UiAction::Confirm),
        "pause.clock.color-cycle" => pause_clock(3, UiAction::Confirm),
        "pause.clock.meltdown" => pause_clock(7, UiAction::Confirm),
        "pause.clock.duck" => pause_clock(8, UiAction::Confirm),
        "pause.clock.marquee" => pause_clock(9, UiAction::Confirm),
        "pause.clock.digit-slide" => pause_clock(11, UiAction::Confirm),
        "pause.clock.marquee-preset.previous" => pause_clock(10, UiAction::Left),
        "pause.clock.marquee-preset.next" => pause_clock(10, UiAction::Right),
        "pause.clock.preview-event.previous" => pause_clock(4, UiAction::Left),
        "pause.clock.preview-event.next" => pause_clock(4, UiAction::Right),
        "pause.clock.back" => pause_clock(5, UiAction::Confirm),
        "pause.clock.preview" => pause_clock(6, UiAction::Confirm),
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
        "game-over.new-match" => game_over(1),
        "game-over.return-to-launcher" => game_over(if material_match { 2 } else { 1 }),
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
        "launcher.settings.spacewars.preset"
        | "launcher.settings.match.player-1"
        | "launcher.settings.travel.asteroid-interval"
        | "launcher.settings.combat.break-interval"
        | "launcher.settings.expedition.players"
        | "launcher.settings.pizza.desired-balls"
        | "launcher.settings.clock.time-format" => 2,
        "launcher.settings.spacewars.planets"
        | "launcher.settings.match.player-2"
        | "launcher.settings.travel.asteroid-strength"
        | "launcher.settings.combat.break-duration"
        | "launcher.settings.pizza.spawn-rate"
        | "launcher.settings.clock.digit-slide" => 3,
        "launcher.settings.spacewars.asteroids"
        | "launcher.settings.match.break-interval"
        | "launcher.settings.combat.mission"
        | "launcher.settings.clock.event-profile" => 4,
        "launcher.settings.spacewars.player-health"
        | "launcher.settings.match.break-duration"
        | "launcher.settings.combat.asteroid-interval"
        | "launcher.settings.clock.falling" => 5,
        "launcher.settings.spacewars.player-2"
        | "launcher.settings.combat.asteroid-strength"
        | "launcher.settings.match.asteroid-interval"
        | "launcher.settings.clock.color-cycle" => 6,
        "launcher.settings.match.asteroid-strength" | "launcher.settings.clock.meltdown" => 7,
        "launcher.settings.clock.duck" | "launcher.settings.match.length" => 8,
        "launcher.settings.clock.marquee" => 9,
        "launcher.settings.clock.marquee-preset" => 10,
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

const fn sound(index: i32, action: UiAction) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::Sound(index),
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

const fn pause_clock(index: i32, action: UiAction) -> ActivationTarget {
    ActivationTarget {
        focus: ActivationFocus::PauseClock(index),
        action,
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
            activation_target("pause.controls", true, false),
            Some(pause_main(3))
        );
        assert_eq!(
            activation_target("pause.controls", false, false),
            Some(pause_main(2))
        );
        assert_eq!(
            activation_target("pause.return-to-launcher", true, false),
            Some(pause_main(4))
        );
        assert_eq!(
            activation_target("pause.return-to-launcher", false, false),
            Some(pause_main(3))
        );
    }

    #[test]
    fn settings_targets_select_the_visible_row_and_direction() {
        assert_eq!(
            activation_target(
                "launcher.settings.spacewars.player-health.previous",
                false,
                false
            ),
            Some(launcher_settings(Some(5), UiAction::Left))
        );
        assert_eq!(
            activation_target("launcher.settings.pizza.spawn-rate.next", false, false),
            Some(launcher_settings(Some(3), UiAction::Right))
        );
        assert_eq!(
            activation_target("launcher.settings.unknown.next", false, false),
            None
        );
    }
}
