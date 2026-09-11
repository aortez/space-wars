//! Live Clock menu operations. Touch, keyboard, gamepad and guarded UI control
//! all enter here; the host applies changes at a simulation boundary.

use std::rc::Rc;
use std::sync::{Arc, RwLock};

use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, ClockTimeFormat, Settings};
use slint::ComponentHandle;
use spacewars_control::UiAction;

use crate::{MainWindow, host, ui_navigation};

pub(crate) fn publish_settings(window: &MainWindow, settings: ClockSettings) {
    window.set_launcher_clock_time_format(
        crate::clock_time_format_label(settings.time_format).into(),
    );
    window.set_launcher_clock_event_profile(
        crate::clock_event_profile_label(settings.event_profile).into(),
    );
    window.set_launcher_clock_falling_enabled(settings.events.falling);
    window.set_launcher_clock_color_cycle_enabled(settings.events.color_cycle);
    window.set_launcher_clock_meltdown_enabled(settings.events.meltdown);
    window.set_launcher_clock_duck_enabled(settings.events.duck);
    window.set_launcher_clock_marquee_enabled(settings.events.marquee);
    window.set_launcher_clock_digit_slide_enabled(settings.events.digit_slide);
    window.set_launcher_clock_marquee_preset(settings.marquee_preset.label().into());
    window.set_launcher_clock_marquee_message(settings.marquee_message.as_str().into());
}

pub(crate) fn install(
    window: &MainWindow,
    controls: host::SharedScenarioControls,
    settings: Arc<RwLock<Settings>>,
    writer: crate::settings_writer::SettingsWriter,
) {
    window.set_clock_event_labels(slint::ModelRc::new(slint::VecModel::from(
        ClockEventKind::ALL
            .into_iter()
            .map(|event| slint::SharedString::from(event.label()))
            .collect::<Vec<_>>(),
    )));
    let weak = window.as_weak();
    let open_controls = Rc::clone(&controls);
    window.on_ingame_clock_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_launcher_visible() || window.get_launcher_scenario() != "clock" {
            return;
        }
        open_controls.borrow_mut().request_pause();
        window.set_ingame_clock_focus_index(0);
        window.set_ingame_controls_visible(false);
        window.set_ingame_clock_visible(true);
    });

    let weak = window.as_weak();
    let adjust_controls = Rc::clone(&controls);
    window.on_ingame_clock_adjust(move |index, delta| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_ingame_clock_visible() || window.get_clock_controls_pending() {
            return;
        }
        window.set_ingame_clock_focus_index(index);
        if index == 4 {
            window.set_clock_preview_index(ui_navigation::moved_selection(
                window.get_clock_preview_index(),
                ClockEventKind::ALL.len() as i32,
                delta,
            ));
            return;
        }
        let mut controls = adjust_controls.borrow_mut();
        let Some(state) = controls.clock_state() else {
            return;
        };
        let Some(settings) = adjusted_settings(state.settings, index, delta) else {
            return;
        };
        if controls.request_clock_settings(settings) {
            window.set_clock_controls_pending(true);
        }
    });

    let weak = window.as_weak();
    window.on_ingame_clock_preview(move || {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_ingame_clock_visible() || window.get_clock_controls_pending() {
            return;
        }
        let Some(event) = usize::try_from(window.get_clock_preview_index())
            .ok()
            .and_then(|index| ClockEventKind::ALL.get(index))
            .copied()
        else {
            return;
        };
        if controls.borrow_mut().request_clock_preview(event) {
            window.set_clock_controls_pending(true);
        }
    });

    let weak = window.as_weak();
    window.on_clock_settings_applied(move || {
        let Some(window) = weak.upgrade() else { return };
        let Ok(clock) = crate::clock_setup_from_window(&window) else {
            return;
        };
        let snapshot = {
            let mut settings = settings.write().unwrap();
            settings.clock = clock;
            settings.clone()
        };
        writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
        window.set_clock_settings_error("".into());
    });
}

fn adjusted_settings(mut settings: ClockSettings, index: i32, delta: i32) -> Option<ClockSettings> {
    match index {
        0 => {
            settings.time_format = match settings.time_format {
                ClockTimeFormat::TwelveHour => ClockTimeFormat::TwentyFourHour,
                ClockTimeFormat::TwentyFourHour => ClockTimeFormat::TwelveHour,
            }
        }
        1 => {
            let profiles = [
                ClockEventProfile::Off,
                ClockEventProfile::Calm,
                ClockEventProfile::Demo,
            ];
            let current = profiles
                .iter()
                .position(|profile| *profile == settings.event_profile)?;
            settings.event_profile =
                profiles[ui_navigation::moved_selection(current as i32, 3, delta) as usize];
        }
        2 => settings.events.falling = !settings.events.falling,
        3 => settings.events.color_cycle = !settings.events.color_cycle,
        7 => settings.events.meltdown = !settings.events.meltdown,
        8 => settings.events.duck = !settings.events.duck,
        9 => settings.events.marquee = !settings.events.marquee,
        11 => settings.events.digit_slide = !settings.events.digit_slide,
        10 => {
            let presets = engine_common::ClockMarqueePreset::ALL;
            let index = presets
                .iter()
                .position(|preset| *preset == settings.marquee_preset)?;
            settings.marquee_preset =
                presets[ui_navigation::moved_selection(index as i32, presets.len() as i32, delta)
                    as usize];
        }
        _ => return None,
    }
    Some(settings)
}

pub(crate) fn open(window: &MainWindow) {
    if window.get_ingame_menu_visible() && window.get_launcher_scenario() == "clock" {
        window.set_ingame_controls_visible(false);
        window.set_ingame_clock_focus_index(0);
        window.set_ingame_clock_visible(true);
    }
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    if window.get_clock_controls_pending() {
        return;
    }
    let index = window.get_ingame_clock_focus_index();
    match action {
        UiAction::Up | UiAction::Down => {
            window.set_ingame_clock_focus_index(ui_navigation::moved_clock_selection(index, action))
        }
        UiAction::Left | UiAction::Right => {
            if matches!(index, 2 | 3 | 5 | 6 | 7 | 8 | 9 | 11) {
                window.set_ingame_clock_focus_index(ui_navigation::moved_clock_selection(
                    index, action,
                ));
            } else {
                window.invoke_ingame_clock_adjust(
                    index,
                    if action == UiAction::Left { -1 } else { 1 },
                );
            }
        }
        UiAction::Confirm if index <= 4 || matches!(index, 7..=11) => {
            window.invoke_ingame_clock_adjust(index, 1)
        }
        UiAction::Confirm if index == 6 => window.invoke_ingame_clock_preview(),
        UiAction::Confirm | UiAction::Back | UiAction::Controls => {
            window.set_ingame_clock_visible(false)
        }
        UiAction::Start => window.invoke_ingame_resume(),
    }
}
