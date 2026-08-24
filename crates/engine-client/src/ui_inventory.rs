//! Stable control inventory for each externally visible UI screen.

use spacewars_control::{UiAction, UiControl, UiScreen};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScreenVisibility {
    pub(crate) launcher: bool,
    pub(crate) launcher_controls: bool,
    pub(crate) launcher_settings: bool,
    pub(crate) touch_test: bool,
    pub(crate) ingame_menu: bool,
    pub(crate) ingame_controls: bool,
    pub(crate) game_over: bool,
}

pub(crate) fn classify_screen(visibility: ScreenVisibility) -> UiScreen {
    if visibility.touch_test {
        UiScreen::LauncherTouchTest
    } else if visibility.launcher {
        if visibility.launcher_controls {
            UiScreen::LauncherControls
        } else if visibility.launcher_settings {
            UiScreen::LauncherSettings
        } else {
            UiScreen::LauncherMain
        }
    } else if visibility.game_over {
        UiScreen::GameOver
    } else if visibility.ingame_controls {
        UiScreen::PauseControls
    } else if visibility.ingame_menu {
        UiScreen::PauseMain
    } else {
        UiScreen::Gameplay
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UiInventoryContext {
    pub(crate) selected_scenario: String,
    pub(crate) launcher_focus_index: i32,
    pub(crate) launcher_settings_focus_index: i32,
    pub(crate) launcher_controls_focus_index: i32,
    pub(crate) ingame_menu_focus_index: i32,
    pub(crate) game_over_focus_index: i32,
    pub(crate) benchmark_available: bool,
    pub(crate) launch_available: bool,
    pub(crate) launcher_error: Option<String>,
    pub(crate) scenario_error: Option<String>,
    pub(crate) renderer: String,
    pub(crate) raster_scale: String,
    pub(crate) spacewars_preset: String,
    pub(crate) spacewars_planets: String,
    pub(crate) spacewars_asteroids: String,
    pub(crate) spacewars_player_health: String,
    pub(crate) spacewars_player_2: String,
    pub(crate) pizza_desired_balls: String,
    pub(crate) pizza_spawn_rate: String,
    pub(crate) nes_cartridge_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiInventory {
    pub(crate) selected_control: Option<String>,
    pub(crate) controls: Vec<UiControl>,
    pub(crate) actions: Vec<UiAction>,
    pub(crate) error: Option<String>,
}

pub(crate) fn inventory_for_screen(screen: UiScreen, context: &UiInventoryContext) -> UiInventory {
    match screen {
        UiScreen::LauncherMain => launcher_main_inventory(context),
        UiScreen::LauncherSettings => launcher_settings_inventory(context),
        UiScreen::LauncherControls => launcher_controls_inventory(context),
        UiScreen::LauncherTouchTest => UiInventory {
            selected_control: None,
            controls: vec![UiControl::new("launcher.touch-test.done", "Done", true)],
            actions: vec![UiAction::Back, UiAction::Controls],
            error: None,
        },
        UiScreen::Gameplay => UiInventory {
            selected_control: None,
            controls: Vec::new(),
            actions: Vec::new(),
            error: context.scenario_error.clone(),
        },
        UiScreen::PauseMain => pause_main_inventory(context),
        UiScreen::PauseControls => UiInventory {
            selected_control: Some("pause.controls.back".into()),
            controls: vec![
                UiControl::new("pause.controls.back", "Back", true),
                UiControl::new("pause.controls.resume", "Resume", true),
            ],
            actions: vec![
                UiAction::Confirm,
                UiAction::Back,
                UiAction::Start,
                UiAction::Controls,
            ],
            error: context.scenario_error.clone(),
        },
        UiScreen::GameOver => UiInventory {
            selected_control: selected_from_index(
                &["game-over.play-again", "game-over.return-to-launcher"],
                context.game_over_focus_index,
            ),
            controls: vec![
                UiControl::new("game-over.play-again", "Play Again", true),
                UiControl::new("game-over.return-to-launcher", "Launcher", true),
            ],
            actions: vec![
                UiAction::Up,
                UiAction::Down,
                UiAction::Left,
                UiAction::Right,
                UiAction::Confirm,
                UiAction::Back,
                UiAction::Start,
            ],
            error: context.scenario_error.clone(),
        },
    }
}

fn launcher_main_inventory(context: &UiInventoryContext) -> UiInventory {
    UiInventory {
        selected_control: selected_from_index(
            &[
                "launcher.scenario",
                "launcher.start",
                "launcher.settings",
                "launcher.controls",
                "launcher.quit",
            ],
            context.launcher_focus_index,
        ),
        controls: vec![
            UiControl::new("launcher.scenario.previous", "‹", true)
                .with_value(context.selected_scenario.clone()),
            UiControl::new("launcher.scenario.next", "›", true)
                .with_value(context.selected_scenario.clone()),
            UiControl::new("launcher.start", "Start Game", context.launch_available),
            UiControl::new("launcher.settings", "Settings", true),
            UiControl::new("launcher.controls", "Controls", true),
            UiControl::new("launcher.quit", "Quit", true),
        ],
        actions: vec![
            UiAction::Up,
            UiAction::Down,
            UiAction::Left,
            UiAction::Right,
            UiAction::Confirm,
            UiAction::Start,
            UiAction::Controls,
        ],
        error: context.launcher_error.clone(),
    }
}

fn launcher_settings_inventory(context: &UiInventoryContext) -> UiInventory {
    let mut controls = Vec::new();
    let selected_ids: &[&str] = match context.selected_scenario.as_str() {
        "spacewars" => {
            push_choice(
                &mut controls,
                "launcher.settings.renderer",
                &context.renderer,
            );
            push_choice(
                &mut controls,
                "launcher.settings.raster-scale",
                &format!("{}×", context.raster_scale),
            );
            push_choice(
                &mut controls,
                "launcher.settings.spacewars.preset",
                &context.spacewars_preset,
            );
            push_choice(
                &mut controls,
                "launcher.settings.spacewars.planets",
                &context.spacewars_planets,
            );
            push_choice(
                &mut controls,
                "launcher.settings.spacewars.asteroids",
                &context.spacewars_asteroids,
            );
            push_choice(
                &mut controls,
                "launcher.settings.spacewars.player-health",
                &format!("{}%", context.spacewars_player_health),
            );
            push_choice(
                &mut controls,
                "launcher.settings.spacewars.player-2",
                &context.spacewars_player_2,
            );
            &[
                "launcher.settings.renderer",
                "launcher.settings.raster-scale",
                "launcher.settings.spacewars.preset",
                "launcher.settings.spacewars.planets",
                "launcher.settings.spacewars.asteroids",
                "launcher.settings.spacewars.player-health",
                "launcher.settings.spacewars.player-2",
                "launcher.settings.back",
            ]
        }
        "pizza" => {
            push_choice(
                &mut controls,
                "launcher.settings.renderer",
                &context.renderer,
            );
            push_choice(
                &mut controls,
                "launcher.settings.raster-scale",
                &format!("{}×", context.raster_scale),
            );
            push_choice(
                &mut controls,
                "launcher.settings.pizza.desired-balls",
                &context.pizza_desired_balls,
            );
            push_choice(
                &mut controls,
                "launcher.settings.pizza.spawn-rate",
                &context.pizza_spawn_rate,
            );
            &[
                "launcher.settings.renderer",
                "launcher.settings.raster-scale",
                "launcher.settings.pizza.desired-balls",
                "launcher.settings.pizza.spawn-rate",
                "launcher.settings.back",
            ]
        }
        "falling" => &["launcher.settings.back"],
        "nes" => {
            push_choice(
                &mut controls,
                "launcher.settings.nes.cartridge",
                &context.nes_cartridge_name,
            );
            &["launcher.settings.nes.cartridge", "launcher.settings.back"]
        }
        _ => {
            push_choice(
                &mut controls,
                "launcher.settings.renderer",
                &context.renderer,
            );
            push_choice(
                &mut controls,
                "launcher.settings.raster-scale",
                &format!("{}×", context.raster_scale),
            );
            &[
                "launcher.settings.renderer",
                "launcher.settings.raster-scale",
                "launcher.settings.back",
            ]
        }
    };

    controls.push(UiControl::new("launcher.settings.back", "Back", true));
    controls.push(UiControl::new(
        "launcher.settings.start",
        "Start Game",
        context.launch_available,
    ));

    UiInventory {
        selected_control: selected_from_index(selected_ids, context.launcher_settings_focus_index),
        controls,
        actions: UiAction::ALL.to_vec(),
        error: context.launcher_error.clone(),
    }
}

fn launcher_controls_inventory(context: &UiInventoryContext) -> UiInventory {
    UiInventory {
        selected_control: selected_from_index(
            &[
                "launcher.controls.back",
                "launcher.controls.touch-test",
                "launcher.controls.start",
            ],
            context.launcher_controls_focus_index,
        ),
        controls: vec![
            UiControl::new("launcher.controls.back", "Back", true),
            UiControl::new("launcher.controls.touch-test", "Touch Test", true),
            UiControl::new(
                "launcher.controls.start",
                "Start Game",
                context.launch_available,
            ),
        ],
        actions: UiAction::ALL.to_vec(),
        error: context.launcher_error.clone(),
    }
}

fn pause_main_inventory(context: &UiInventoryContext) -> UiInventory {
    let mut ids = vec!["pause.resume", "pause.restart"];
    let mut controls = vec![
        UiControl::new("pause.resume", "Resume", true),
        UiControl::new("pause.restart", "Restart Round", true),
    ];
    if context.benchmark_available {
        ids.push("pause.benchmark");
        controls.push(UiControl::new("pause.benchmark", "Benchmark", true));
    }
    ids.extend(["pause.controls", "pause.return-to-launcher"]);
    controls.extend([
        UiControl::new("pause.controls", "Controls", true),
        UiControl::new("pause.return-to-launcher", "Return to Launcher", true),
    ]);

    UiInventory {
        selected_control: selected_from_index(&ids, context.ingame_menu_focus_index),
        controls,
        actions: UiAction::ALL.to_vec(),
        error: context.scenario_error.clone(),
    }
}

fn push_choice(controls: &mut Vec<UiControl>, id: &str, value: &str) {
    controls.push(UiControl::new(format!("{id}.previous"), "‹", true).with_value(value));
    controls.push(UiControl::new(format!("{id}.next"), "›", true).with_value(value));
}

fn selected_from_index(ids: &[&str], index: i32) -> Option<String> {
    usize::try_from(index)
        .ok()
        .and_then(|index| ids.get(index))
        .map(|id| (*id).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(scenario: &str) -> UiInventoryContext {
        UiInventoryContext {
            selected_scenario: scenario.into(),
            launch_available: true,
            renderer: "raster".into(),
            raster_scale: "2.0".into(),
            spacewars_preset: "Small Duel".into(),
            spacewars_planets: "on".into(),
            spacewars_asteroids: "off".into(),
            spacewars_player_health: "125".into(),
            spacewars_player_2: "rule bot".into(),
            pizza_desired_balls: "75".into(),
            pizza_spawn_rate: "0.10".into(),
            nes_cartridge_name: "Demo Cartridge".into(),
            ..Default::default()
        }
    }

    fn control_ids(inventory: &UiInventory) -> Vec<&str> {
        inventory
            .controls
            .iter()
            .map(|control| control.id.as_str())
            .collect()
    }

    #[test]
    fn classifies_all_ui_screens() {
        let cases = [
            (
                ScreenVisibility {
                    launcher: true,
                    ..Default::default()
                },
                UiScreen::LauncherMain,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    launcher_settings: true,
                    ..Default::default()
                },
                UiScreen::LauncherSettings,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    launcher_controls: true,
                    ..Default::default()
                },
                UiScreen::LauncherControls,
            ),
            (
                ScreenVisibility {
                    launcher: true,
                    touch_test: true,
                    ..Default::default()
                },
                UiScreen::LauncherTouchTest,
            ),
            (ScreenVisibility::default(), UiScreen::Gameplay),
            (
                ScreenVisibility {
                    ingame_menu: true,
                    ..Default::default()
                },
                UiScreen::PauseMain,
            ),
            (
                ScreenVisibility {
                    ingame_menu: true,
                    ingame_controls: true,
                    ..Default::default()
                },
                UiScreen::PauseControls,
            ),
            (
                ScreenVisibility {
                    game_over: true,
                    ..Default::default()
                },
                UiScreen::GameOver,
            ),
        ];

        for (visibility, expected) in cases {
            assert_eq!(classify_screen(visibility), expected);
        }
    }

    #[test]
    fn classification_uses_visible_layer_order() {
        assert_eq!(
            classify_screen(ScreenVisibility {
                launcher: true,
                launcher_controls: true,
                launcher_settings: true,
                touch_test: true,
                ingame_menu: true,
                ingame_controls: true,
                game_over: true,
            }),
            UiScreen::LauncherTouchTest
        );
        assert_eq!(
            classify_screen(ScreenVisibility {
                launcher: true,
                launcher_controls: true,
                launcher_settings: true,
                game_over: true,
                ..Default::default()
            }),
            UiScreen::LauncherControls
        );
        assert_eq!(
            classify_screen(ScreenVisibility {
                ingame_menu: true,
                ingame_controls: true,
                game_over: true,
                ..Default::default()
            }),
            UiScreen::GameOver
        );
    }

    #[test]
    fn launcher_main_reports_actions_selection_readiness_and_error() {
        let mut context = context("nes");
        context.launcher_focus_index = 0;
        context.launch_available = false;
        context.launcher_error = Some("No cartridge selected".into());

        let inventory = inventory_for_screen(UiScreen::LauncherMain, &context);

        assert_eq!(
            inventory.selected_control.as_deref(),
            Some("launcher.scenario")
        );
        assert_eq!(
            control_ids(&inventory),
            [
                "launcher.scenario.previous",
                "launcher.scenario.next",
                "launcher.start",
                "launcher.settings",
                "launcher.controls",
                "launcher.quit",
            ]
        );
        assert!(!inventory.controls[2].enabled);
        assert_eq!(inventory.controls[0].value.as_deref(), Some("nes"));
        assert_eq!(inventory.error.as_deref(), Some("No cartridge selected"));
    }

    #[test]
    fn settings_inventory_matches_each_scenario() {
        let cases = [
            ("spacewars", 16, "launcher.settings.spacewars.player-2"),
            ("pizza", 10, "launcher.settings.pizza.spawn-rate"),
            ("rover-lab", 6, "launcher.settings.raster-scale"),
            ("falling", 2, "launcher.settings.back"),
            ("nes", 4, "launcher.settings.nes.cartridge"),
        ];

        for (scenario, control_count, selected) in cases {
            let mut context = context(scenario);
            context.launcher_settings_focus_index = match scenario {
                "spacewars" => 6,
                "pizza" => 3,
                "rover-lab" => 1,
                "falling" => 0,
                "nes" => 0,
                _ => unreachable!(),
            };
            let inventory = inventory_for_screen(UiScreen::LauncherSettings, &context);

            assert_eq!(inventory.controls.len(), control_count);
            assert_eq!(inventory.selected_control.as_deref(), Some(selected));
            assert_eq!(
                control_ids(&inventory)[control_count - 2..],
                ["launcher.settings.back", "launcher.settings.start"]
            );
        }
    }

    #[test]
    fn settings_choices_expose_the_visible_value() {
        let inventory = inventory_for_screen(UiScreen::LauncherSettings, &context("spacewars"));
        let value = |id: &str| {
            inventory
                .controls
                .iter()
                .find(|control| control.id == id)
                .and_then(|control| control.value.as_deref())
        };

        assert_eq!(value("launcher.settings.renderer.previous"), Some("raster"));
        assert_eq!(value("launcher.settings.raster-scale.next"), Some("2.0×"));
        assert_eq!(
            value("launcher.settings.spacewars.player-health.next"),
            Some("125%")
        );
    }

    #[test]
    fn pause_inventory_tracks_dynamic_benchmark_and_selection() {
        let mut context = context("pizza");
        context.benchmark_available = true;
        context.ingame_menu_focus_index = 2;
        let with_benchmark = inventory_for_screen(UiScreen::PauseMain, &context);
        assert_eq!(
            with_benchmark.selected_control.as_deref(),
            Some("pause.benchmark")
        );
        assert!(control_ids(&with_benchmark).contains(&"pause.benchmark"));

        context.benchmark_available = false;
        let without_benchmark = inventory_for_screen(UiScreen::PauseMain, &context);
        assert_eq!(
            without_benchmark.selected_control.as_deref(),
            Some("pause.controls")
        );
        assert!(!control_ids(&without_benchmark).contains(&"pause.benchmark"));
    }

    #[test]
    fn static_screens_report_their_visible_controls() {
        let mut context = context("spacewars");
        context.launcher_controls_focus_index = 1;
        context.game_over_focus_index = 1;

        let cases = [
            (
                UiScreen::LauncherControls,
                Some("launcher.controls.touch-test"),
                vec![
                    "launcher.controls.back",
                    "launcher.controls.touch-test",
                    "launcher.controls.start",
                ],
            ),
            (
                UiScreen::LauncherTouchTest,
                None,
                vec!["launcher.touch-test.done"],
            ),
            (UiScreen::Gameplay, None, vec![]),
            (
                UiScreen::PauseControls,
                Some("pause.controls.back"),
                vec!["pause.controls.back", "pause.controls.resume"],
            ),
            (
                UiScreen::GameOver,
                Some("game-over.return-to-launcher"),
                vec!["game-over.play-again", "game-over.return-to-launcher"],
            ),
        ];

        for (screen, selected, ids) in cases {
            let inventory = inventory_for_screen(screen, &context);
            assert_eq!(inventory.selected_control.as_deref(), selected);
            assert_eq!(control_ids(&inventory), ids);
        }
    }

    #[test]
    fn reports_only_actions_handled_by_each_screen() {
        let context = context("spacewars");
        let cases = [
            (
                UiScreen::LauncherMain,
                vec![
                    UiAction::Up,
                    UiAction::Down,
                    UiAction::Left,
                    UiAction::Right,
                    UiAction::Confirm,
                    UiAction::Start,
                    UiAction::Controls,
                ],
            ),
            (UiScreen::LauncherSettings, UiAction::ALL.to_vec()),
            (UiScreen::LauncherControls, UiAction::ALL.to_vec()),
            (
                UiScreen::LauncherTouchTest,
                vec![UiAction::Back, UiAction::Controls],
            ),
            (UiScreen::Gameplay, vec![]),
            (UiScreen::PauseMain, UiAction::ALL.to_vec()),
            (
                UiScreen::PauseControls,
                vec![
                    UiAction::Confirm,
                    UiAction::Back,
                    UiAction::Start,
                    UiAction::Controls,
                ],
            ),
            (
                UiScreen::GameOver,
                vec![
                    UiAction::Up,
                    UiAction::Down,
                    UiAction::Left,
                    UiAction::Right,
                    UiAction::Confirm,
                    UiAction::Back,
                    UiAction::Start,
                ],
            ),
        ];

        for (screen, expected) in cases {
            assert_eq!(inventory_for_screen(screen, &context).actions, expected);
        }
    }

    #[test]
    fn every_enabled_control_has_a_semantic_activation_mapping() {
        for scenario in ["spacewars", "pizza", "rover-lab", "falling", "nes"] {
            let context = context(scenario);
            for screen in [
                UiScreen::LauncherMain,
                UiScreen::LauncherSettings,
                UiScreen::LauncherControls,
                UiScreen::LauncherTouchTest,
            ] {
                assert_inventory_is_activatable(screen, &context);
            }
        }

        for benchmark_available in [false, true] {
            let mut context = context("spacewars");
            context.benchmark_available = benchmark_available;
            for screen in [
                UiScreen::Gameplay,
                UiScreen::PauseMain,
                UiScreen::PauseControls,
                UiScreen::GameOver,
            ] {
                assert_inventory_is_activatable(screen, &context);
            }
        }
    }

    fn assert_inventory_is_activatable(screen: UiScreen, context: &UiInventoryContext) {
        for control in inventory_for_screen(screen, context)
            .controls
            .into_iter()
            .filter(|control| control.enabled)
        {
            assert!(
                crate::ui_activation::supports(&control.id, context.benchmark_available),
                "{} on {screen} has no semantic activation mapping",
                control.id
            );
        }
    }

    #[test]
    fn errors_follow_the_screen_that_displays_them() {
        let mut context = context("spacewars");
        context.launcher_error = Some("launcher failed".into());
        context.scenario_error = Some("scenario failed".into());

        assert_eq!(
            inventory_for_screen(UiScreen::LauncherSettings, &context)
                .error
                .as_deref(),
            Some("launcher failed")
        );
        assert_eq!(
            inventory_for_screen(UiScreen::Gameplay, &context)
                .error
                .as_deref(),
            Some("scenario failed")
        );
        assert_eq!(
            inventory_for_screen(UiScreen::LauncherTouchTest, &context).error,
            None
        );
    }
}
