//! The Clock's settings pages in both menus: Clock Controls over the paused
//! Clock, where the host applies a change at a simulation boundary, and the
//! launcher's Scenario Settings, which keep their changes on the pages until
//! Start saves them. Touch, keyboard, gamepad and guarded UI control all
//! enter here.

use std::rc::Rc;
use std::sync::{Arc, RwLock};

use engine_common::{ClockSettings, Settings};
use slint::{ComponentHandle, Model};
use spacewars_control::UiAction;

use crate::clock_menu::{self, Menu};
use crate::{MainWindow, SettingsRowKind, clock_settings_table, host};

/// Shows `settings` on the pages of both menus, and in the fonts picker.
pub(crate) fn publish_settings(window: &MainWindow, settings: ClockSettings) {
    crate::clock_fonts::publish(window, settings.fonts);
    clock_menu::publish(window, &settings);
}

pub(crate) fn install(
    window: &MainWindow,
    controls: host::SharedScenarioControls,
    settings: Arc<RwLock<Settings>>,
    writer: crate::settings_writer::SettingsWriter,
) {
    crate::clock_fonts::install(window, Rc::clone(&controls));
    let weak = window.as_weak();
    let open_controls = Rc::clone(&controls);
    window.on_ingame_clock_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_install_fenced() {
            return;
        }
        if window.get_launcher_visible() || window.get_launcher_scenario() != "clock" {
            return;
        }
        open_controls.borrow_mut().request_pause();
        window.set_ingame_controls_visible(false);
        window.set_ingame_clock_visible(true);
    });

    let weak = window.as_weak();
    let adjust_controls = Rc::clone(&controls);
    window.on_clock_menu_adjust(move |index, delta| {
        let Some(window) = weak.upgrade() else { return };
        let Some(menu) = takes_input(&window) else {
            return;
        };
        menu.set_focus(&window, index);
        let Some(row) = clock_menu::row(&window, menu, index) else {
            return;
        };
        if menu == Menu::Launcher {
            adjust_launcher_row(&window, &row.id, delta);
            return;
        }
        let Some(setting) = clock_settings_table::find(&row.id) else {
            return;
        };
        let mut controls = adjust_controls.borrow_mut();
        let Some(state) = controls.clock_state() else {
            return;
        };
        let settings = clock_settings_table::stepped(setting, state.settings, delta);
        if controls.request_clock_settings(settings) {
            window.set_clock_controls_pending(true);
        }
    });

    let weak = window.as_weak();
    let play_controls = Rc::clone(&controls);
    window.on_clock_menu_play(move |index| {
        let Some(window) = weak.upgrade() else { return };
        if takes_input(&window) != Some(Menu::Pause) {
            return;
        }
        window.set_ingame_clock_focus_index(index);
        let Some(event) =
            clock_menu::row(&window, Menu::Pause, index).and_then(|row| clock_menu::event(&row.id))
        else {
            return;
        };
        if play_controls.borrow_mut().request_clock_preview(event) {
            window.set_clock_controls_pending(true);
        }
    });

    let weak = window.as_weak();
    window.on_clock_menu_activate(move |index| {
        let Some(window) = weak.upgrade() else { return };
        if let Some(menu) = takes_input(&window) {
            menu.set_focus(&window, index);
            clock_menu::open_row(&window, menu, index);
        }
    });

    let weak = window.as_weak();
    window.on_clock_menu_back(move || {
        let Some(window) = weak.upgrade() else { return };
        if let Some(menu) = takes_input(&window) {
            clock_menu::back(&window, menu);
        }
    });

    let weak = window.as_weak();
    window.on_clock_settings_applied(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_install_fenced() {
            return;
        }
        let Some(clock) = clock_menu::settings(&window) else {
            return;
        };
        let snapshot = {
            let mut settings = settings.write().unwrap();
            log_settings_change(settings.clock, clock, "live_clock_controls");
            settings.clock = clock;
            settings.clone()
        };
        writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
        window.set_clock_settings_error("".into());
    });
}

/// Changes the launcher's row named `id` by `delta`: a Clock setting, which
/// the pages keep until Start saves it, or the renderer or its scale.
fn adjust_launcher_row(window: &MainWindow, id: &str, delta: i32) {
    let Some(settings) = clock_menu::settings(window) else {
        return;
    };
    match id {
        "renderer" => crate::cycle_renderer(window, delta),
        "raster-scale" => crate::cycle_raster_scale(window, delta),
        _ => {
            if let Some(setting) = clock_settings_table::find(id) {
                publish_settings(
                    window,
                    clock_settings_table::stepped(setting, settings, delta),
                );
            }
            return;
        }
    }
    // The rows show the renderer's new value.
    clock_menu::publish(window, &settings);
}

pub(crate) fn log_settings_change(previous: ClockSettings, next: ClockSettings, source: &str) {
    if previous == next {
        return;
    }
    // Only settings changes, never ticks. Do not include user-supplied text.
    tracing::info!(source,
        old_fonts = ?previous.fonts, new_fonts = ?next.fonts,
        old_profile = ?previous.event_profile, new_profile = ?next.event_profile,
        old_events = ?previous.events, new_events = ?next.events,
        old_rain = ?previous.rain_amount, new_rain = ?next.rain_amount,
        old_crow_water = ?previous.crow_water_tolerance, new_crow_water = ?next.crow_water_tolerance,
        old_lamps = ?previous.lamps, new_lamps = ?next.lamps,
        old_theme = ?previous.theme, new_theme = ?next.theme,
        old_format = ?previous.time_format, new_format = ?next.time_format,
        old_show_date = previous.show_date, new_show_date = next.show_date,
        old_marquee = ?previous.marquee_preset, new_marquee = ?next.marquee_preset,
        message_changed = previous.marquee_message != next.marquee_message,
        "Clock settings changed.");
}

/// The menu showing the pages, if it takes a tap or a press now: not while
/// the host applies a change to Clock Controls.
fn takes_input(window: &MainWindow) -> Option<Menu> {
    if window.get_install_fenced() || window.get_clock_controls_pending() {
        return None;
    }
    Menu::showing(window)
}

/// Shows Clock Controls from the pause menu, on the page and row they were
/// left on.
pub(crate) fn open(window: &MainWindow) {
    if window.get_ingame_menu_visible() && window.get_launcher_scenario() == "clock" {
        window.set_ingame_controls_visible(false);
        window.set_ingame_clock_visible(true);
    }
}

/// A press on the pages in `menu`. Up and Down move between the rows and
/// Back. Left and Right change a row's setting, and Right opens a page. A /
/// Enter plays an event's row in Clock Controls, opens a page, steps a setting
/// on, or presses Back. Back goes up a page. Controls closes Clock Controls,
/// or turns Scenario Settings to the launcher's Controls. Start resumes the
/// Clock, or starts it.
pub(crate) fn handle_action(window: &MainWindow, menu: Menu, action: UiAction) {
    if window.get_clock_controls_pending() {
        return;
    }
    let index = menu.focus(window);
    let row = clock_menu::row(window, menu, index);
    match action {
        UiAction::Up | UiAction::Down => {
            let rows = clock_menu::page_rows(window, menu)
                .iter()
                .collect::<Vec<_>>();
            menu.set_focus(window, clock_menu::moved_focus(&rows, index, action));
        }
        UiAction::Left | UiAction::Right => match row.map(|row| row.kind) {
            Some(SettingsRowKind::Choice | SettingsRowKind::Switch) => window
                .invoke_clock_menu_adjust(index, if action == UiAction::Left { -1 } else { 1 }),
            Some(SettingsRowKind::Page) if action == UiAction::Right => {
                clock_menu::open_row(window, menu, index)
            }
            _ => {}
        },
        UiAction::Confirm => match row {
            None => clock_menu::back(window, menu),
            Some(row) if row.playable => window.invoke_clock_menu_play(index),
            Some(row) if row.kind == SettingsRowKind::Page => {
                clock_menu::open_row(window, menu, index)
            }
            Some(_) => window.invoke_clock_menu_adjust(index, 1),
        },
        UiAction::Back => clock_menu::back(window, menu),
        UiAction::Controls => match menu {
            Menu::Pause => window.set_ingame_clock_visible(false),
            Menu::Launcher => {
                window.set_launcher_settings_visible(false);
                window.set_launcher_controls_visible(true);
            }
        },
        UiAction::Start => match menu {
            Menu::Pause => window.invoke_ingame_resume(),
            Menu::Launcher => window.invoke_launcher_start_game(),
        },
    }
}

#[cfg(test)]
mod tests {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    use super::*;

    struct TestPlatform;

    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
        }
    }

    /// In the launcher, a row's ‹ and › step its setting as the settings table
    /// does, and the pages keep it for Start to save. Renderer and Raster
    /// Scale step the launcher's own choices, which their rows then show.
    #[test]
    fn launcher_rows_step_their_settings_and_keep_them_on_the_pages() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        for setting in clock_settings_table::SETTINGS {
            for start in clock_settings_table::starting_points() {
                for delta in [1, -1] {
                    publish_settings(&window, start);
                    adjust_launcher_row(&window, setting.id(), delta);
                    assert_eq!(
                        clock_menu::settings(&window),
                        Some(clock_settings_table::stepped(setting, start, delta)),
                        "{} by {delta}",
                        setting.id()
                    );
                }
            }
        }
        window.set_launcher_renderer("raster".into());
        window.set_launcher_raster_scale_text("1.0".into());
        publish_settings(&window, ClockSettings::default());
        adjust_launcher_row(&window, "renderer", 1);
        adjust_launcher_row(&window, "raster-scale", -1);
        let shown = window
            .get_clock_menu_launcher()
            .top
            .iter()
            .map(|row| (row.id.to_string(), row.value.to_string()))
            .collect::<Vec<_>>();
        assert!(
            shown.contains(&("renderer".into(), "vector".into())),
            "{shown:?}"
        );
        assert!(
            shown.contains(&("raster-scale".into(), "3.0×".into())),
            "{shown:?}"
        );
        assert_eq!(
            clock_menu::settings(&window),
            Some(ClockSettings::default())
        );
    }
}
