//! Exercise the actual menu callbacks, shared sampling gates, and NES inputs
//! with deterministic physical snapshots; no display server or gilrs devices.

use super::*;
use crate::controller_controls;
use crate::settings_writer::SettingsWriter;
use engine_common::Settings;
use engine_nes::ControllerButtons;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

struct TestPlatform;
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
    }
}

fn device(id: usize, model: &str) -> Device {
    Device {
        id,
        key: format!("gilrs-v1:linux:{model}"),
        name: model.into(),
        seat: None,
    }
}

fn pad() -> GamepadSeatInput {
    GamepadSeatInput {
        connected: true,
        ..Default::default()
    }
}

#[test]
fn nes_face_button_layout_does_not_swap_host_menu_actions() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.set_launcher_scenario("nes".into());
    ui.set_scenario_captures_gamepad_start(true);
    ui.set_scenario_captures_gamepad_select(true);
    for launcher in [true, false] {
        ui.set_launcher_visible(launcher);
        ui.set_ingame_menu_visible(!launcher);
        assert!(matches!(
            button_route(&ui, Button::South, "Microsoft X-Box 360 pad", false, false),
            ButtonRoute::Menu(UiAction::Confirm)
        ));
        assert!(matches!(
            button_route(&ui, Button::East, "Microsoft X-Box 360 pad", false, false),
            ButtonRoute::Menu(UiAction::Back)
        ));
    }
}

#[test]
fn player_assignment_callbacks_clear_held_input_swap_nes_ports_and_survive_reload() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.set_launcher_visible(true);
    ui.set_sound_visible(true);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let settings = Arc::new(RwLock::new(Settings::default()));
    settings.write().unwrap().audio.master_volume = 0.05;
    let writer = SettingsWriter::new(path.clone()).unwrap();
    let (input, gamepads) = input::new_shared_input();
    let controllers =
        controller_controls::install(&ui, settings.clone(), writer.clone(), gamepads.clone());
    controllers
        .borrow_mut()
        .connect_many(vec![device(41, "Picade"), device(12, "USB pad")]);

    let mut handoff = ModeHandoff::default();
    let mut epoch = controllers.borrow().epoch;
    let mut sample = |a: GamepadSeatInput, b: GamepadSeatInput| {
        handoff.observe_epoch(&mut epoch, controllers.borrow().epoch);
        publish_gamepad_samples(
            &mut handoff,
            &mut gamepads.borrow_mut(),
            None,
            vec![
                (41, controllers.borrow().seat(41), a),
                (12, controllers.borrow().seat(12), b),
            ],
        );
    };
    sample(pad(), pad());
    let a = GamepadSeatInput {
        south: true,
        dpad_left: true,
        ..pad()
    };
    let b = GamepadSeatInput {
        east: true,
        dpad_right: true,
        ..pad()
    };
    sample(a.clone(), b.clone());
    assert_eq!(
        input.borrow().nes_controller_buttons(0),
        ControllerButtons::B | ControllerButtons::LEFT
    );
    assert_eq!(
        input.borrow().nes_controller_buttons(1),
        ControllerButtons::A | ControllerButtons::RIGHT
    );

    ui.invoke_controllers_open();
    ui.invoke_controllers_command("controllers.device.12".into());
    gamepads.borrow_mut().simulate(
        0,
        spacewars_control::InputButton::Start,
        Instant::now() + Duration::from_secs(1),
    );
    ui.invoke_controllers_command("controllers.assign-p1".into());
    assert!(!gamepads.borrow().has_simulated());
    // Must clear synchronously, before the next pump/host tick.
    assert_eq!(
        input.borrow().nes_controller_buttons(0),
        ControllerButtons::NONE
    );
    assert_eq!(
        input.borrow().nes_controller_buttons(1),
        ControllerButtons::NONE
    );
    assert_eq!(
        [controllers.borrow().seat(12), controllers.borrow().seat(41)],
        [Some(0), Some(1)]
    );
    assert!(ui.get_controllers_assignments().contains("P1: USB pad"));
    assert!(ui.get_controllers_assignments().contains("P2: Picade"));
    assert!(
        controller_controls::inventory(&ui)
            .iter()
            .any(|control| control.id == "controllers.assign-p2")
    );

    sample(a.clone(), b.clone());
    assert_eq!(
        input.borrow().nes_controller_buttons(0),
        ControllerButtons::NONE
    );
    assert_eq!(
        input.borrow().nes_controller_buttons(1),
        ControllerButtons::NONE
    );
    // Releasing only one controller does not unblock a held second controller.
    sample(pad(), b.clone());
    sample(a.clone(), b.clone());
    assert_eq!(
        input.borrow().nes_controller_buttons(0),
        ControllerButtons::NONE
    );
    assert_eq!(
        input.borrow().nes_controller_buttons(1),
        ControllerButtons::B | ControllerButtons::LEFT
    );
    sample(pad(), pad());
    sample(a, b);
    assert_eq!(
        input.borrow().nes_controller_buttons(0),
        ControllerButtons::A | ControllerButtons::RIGHT
    );
    assert_eq!(
        input.borrow().nes_controller_buttons(1),
        ControllerButtons::B | ControllerButtons::LEFT
    );

    writer
        .save_blocking(settings.read().unwrap().clone())
        .unwrap();
    let saved = crate::settings::load_settings(&path).unwrap().settings;
    assert_eq!(saved.audio.master_volume, 0.05);
    assert_eq!(
        saved.controls.player_1_device.as_deref(),
        Some("gilrs-v1:linux:USB pad")
    );
    assert_eq!(
        saved.controls.player_2_device.as_deref(),
        Some("gilrs-v1:linux:Picade")
    );
    // Restart-style initialization: different IDs and reversed arrival order.
    let reloaded =
        controller_controls::install(&ui, Arc::new(RwLock::new(saved)), writer, gamepads);
    reloaded
        .borrow_mut()
        .connect_many(vec![device(80, "USB pad"), device(70, "Picade")]);
    assert_eq!(
        [reloaded.borrow().seat(80), reloaded.borrow().seat(70)],
        [Some(0), Some(1)]
    );
}

#[test]
fn unassigned_physical_controllers_can_navigate_but_never_feed_a_player() {
    let mut handoff = ModeHandoff::default();
    let mut gamepads = input::GamepadInput::default();
    let ui = publish_gamepad_samples(
        &mut handoff,
        &mut gamepads,
        Some(99),
        vec![(99, None, pad())],
    );
    assert!(ui.connected);
    let ui = publish_gamepad_samples(
        &mut handoff,
        &mut gamepads,
        Some(99),
        vec![(
            99,
            None,
            GamepadSeatInput {
                dpad_down: true,
                south: true,
                ..pad()
            },
        )],
    );
    assert_eq!(ui_direction(&ui), Some(UiDirection::Down));
    assert!(gamepads.seat(0).is_some_and(|pad| !pad.connected));
    assert!(gamepads.seat(1).is_some_and(|pad| !pad.connected));
    assert!(route_allowed_for_assignment(
        ButtonRoute::Menu(UiAction::Confirm),
        None
    ));
    assert!(route_allowed_for_assignment(
        ButtonRoute::Host(GameKey::Pause),
        None
    ));
    for route in [
        ButtonRoute::ClockDuck,
        ButtonRoute::ClockNext,
        ButtonRoute::Scenario,
    ] {
        assert!(!route_allowed_for_assignment(route, None));
        assert!(route_allowed_for_assignment(route, Some(0)));
    }
    // Transition gates also apply to an unassigned UI driver.
    handoff.block_all();
    let ui = publish_gamepad_samples(
        &mut handoff,
        &mut gamepads,
        Some(99),
        vec![(
            99,
            None,
            GamepadSeatInput {
                dpad_down: true,
                ..pad()
            },
        )],
    );
    assert_eq!(ui_direction(&ui), None);
}

#[test]
fn player_assignment_reset_and_identical_model_policy_use_real_menu_callbacks() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.set_launcher_visible(true);
    ui.set_sound_visible(true);
    let dir = tempfile::tempdir().unwrap();
    let settings = Arc::new(RwLock::new(Settings::default()));
    let profile = engine_common::ControllerProfile {
        device_key: device(1, "Same pad").key,
        name: "Same pad".into(),
        bindings: engine_common::ControllerControl::ALL
            .into_iter()
            .enumerate()
            .map(|(code, control)| engine_common::ControllerBinding {
                control,
                source: engine_common::ControllerSource::Button { code: code as u32 },
            })
            .collect(),
    };
    settings
        .write()
        .unwrap()
        .controls
        .controller_profiles
        .push(profile.clone());
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    let controllers = controller_controls::install(
        &ui,
        settings.clone(),
        writer.clone(),
        input::new_shared_input().1,
    );
    controllers
        .borrow_mut()
        .connect_many(vec![device(1, "Same pad"), device(2, "Same pad")]);
    ui.invoke_controllers_open();
    ui.invoke_controllers_command("controllers.device.2".into());
    ui.invoke_controllers_command("controllers.assign-p1".into());
    assert_eq!(controllers.borrow().seat(2), Some(0));
    assert!(ui.get_controllers_status().contains("session only"));
    assert_eq!(settings.read().unwrap().controls.player_1_device, None);
    assert_eq!(settings.read().unwrap().controls.player_2_device, None);
    controllers.borrow_mut().disconnect(2);
    // A stale device-page command cannot assign a now disconnected controller.
    ui.invoke_controllers_command("controllers.assign-p1".into());
    assert!(controllers.borrow().connected_id(0).is_none());
    controllers.borrow_mut().connect(device(2, "Same pad"));
    assert!(controllers.borrow().seat(2).is_none());
    ui.invoke_controllers_command("controllers.reset-players".into());
    assert_eq!(controllers.borrow().seat(1), Some(0));
    assert_eq!(controllers.borrow().seat(2), Some(1));
    writer
        .save_blocking(settings.read().unwrap().clone())
        .unwrap();
    let saved = crate::settings::load_settings(&dir.path().join("settings.toml"))
        .unwrap()
        .settings;
    assert_eq!(saved.controls.player_1_device, None);
    assert_eq!(saved.controls.player_2_device, None);
    assert_eq!(saved.controls.controller_profiles, vec![profile]);
}
