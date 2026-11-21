//! Live Clock menu operations. Touch, keyboard, gamepad and guarded UI control
//! all enter here; the host applies changes at a simulation boundary.

use std::rc::Rc;
use std::sync::{Arc, RwLock};

use engine_common::{ClockSettings, Settings};
use slint::{ComponentHandle, Model};
use spacewars_control::UiAction;

use crate::{MainWindow, SettingsRowKind, clock_menu, clock_settings_table, host};

pub(crate) fn publish_settings(window: &MainWindow, settings: ClockSettings) {
    crate::clock_fonts::publish(window, settings.fonts);
    clock_menu::publish(window, &settings);
    window.set_launcher_clock_show_date(settings.show_date);
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
    window.set_launcher_clock_crow_enabled(settings.events.crow);
    window.set_launcher_clock_crow_water_tolerance(settings.crow_water_tolerance.label().into());
    window.set_launcher_clock_lamps(settings.lamps.label().into());
    window.set_launcher_clock_theme(settings.theme.label().into());
    window.set_launcher_clock_explosion_enabled(settings.events.explosion);
    window.set_launcher_clock_aquarium_enabled(settings.events.aquarium);
    window.set_launcher_clock_marquee_enabled(settings.events.marquee);
    window.set_launcher_clock_digit_slide_enabled(settings.events.digit_slide);
    window.set_launcher_clock_cell_swap_enabled(settings.events.cell_swap);
    window.set_launcher_clock_slot_machine_enabled(settings.events.slot_machine);
    window.set_launcher_clock_spider_enabled(settings.events.spider);
    window.set_launcher_clock_coins_enabled(settings.events.coins);
    window.set_launcher_clock_tanks_enabled(settings.events.tanks);
    window.set_launcher_clock_grass_enabled(settings.events.grass);
    window.set_launcher_clock_rain_enabled(settings.events.rain);
    window.set_launcher_clock_rain_amount(settings.rain_amount.label().into());
    window.set_launcher_clock_marquee_preset(settings.marquee_preset.label().into());
    window.set_launcher_clock_marquee_message(settings.marquee_message.as_str().into());
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
        if !takes_input(&window) {
            return;
        }
        window.set_ingame_clock_focus_index(index);
        let Some(setting) =
            clock_menu::row(&window, index).and_then(|row| clock_settings_table::find(&row.id))
        else {
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
        if !takes_input(&window) {
            return;
        }
        window.set_ingame_clock_focus_index(index);
        let Some(event) =
            clock_menu::row(&window, index).and_then(|row| clock_menu::event(&row.id))
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
        if takes_input(&window) {
            window.set_ingame_clock_focus_index(index);
            clock_menu::open_row(&window, index);
        }
    });

    let weak = window.as_weak();
    window.on_clock_menu_back(move || {
        let Some(window) = weak.upgrade() else { return };
        if takes_input(&window) {
            clock_menu::back(&window);
        }
    });

    let weak = window.as_weak();
    window.on_clock_settings_applied(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_install_fenced() {
            return;
        }
        let Ok(clock) = crate::clock_setup_from_window(&window) else {
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

/// Whether Clock Controls take a tap or a press now: showing, and not
/// waiting on the host to apply a change.
fn takes_input(window: &MainWindow) -> bool {
    !window.get_install_fenced()
        && window.get_ingame_clock_visible()
        && !window.get_clock_controls_pending()
}

/// Shows Clock Controls from the pause menu, on the page and row they were
/// left on.
pub(crate) fn open(window: &MainWindow) {
    if window.get_ingame_menu_visible() && window.get_launcher_scenario() == "clock" {
        window.set_ingame_controls_visible(false);
        window.set_ingame_clock_visible(true);
    }
}

/// A press on Clock Controls. Up and Down move between the rows and Back.
/// Left and Right change a row's setting, and Right opens a page. A / Enter
/// plays an event's row, opens a page, steps a setting on, or presses Back.
/// Back goes up a page, Controls closes them, and Start resumes.
pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    if window.get_clock_controls_pending() {
        return;
    }
    let index = window.get_ingame_clock_focus_index();
    let row = clock_menu::row(window, index);
    match action {
        UiAction::Up | UiAction::Down => {
            let rows = window.get_clock_menu_rows().iter().collect::<Vec<_>>();
            window.set_ingame_clock_focus_index(clock_menu::moved_focus(&rows, index, action));
        }
        UiAction::Left | UiAction::Right => match row.map(|row| row.kind) {
            Some(SettingsRowKind::Choice | SettingsRowKind::Switch) => window
                .invoke_clock_menu_adjust(index, if action == UiAction::Left { -1 } else { 1 }),
            Some(SettingsRowKind::Page) if action == UiAction::Right => {
                clock_menu::open_row(window, index)
            }
            _ => {}
        },
        UiAction::Confirm => match row {
            None => clock_menu::back(window),
            Some(row) if row.playable => window.invoke_clock_menu_play(index),
            Some(row) if row.kind == SettingsRowKind::Page => clock_menu::open_row(window, index),
            Some(_) => window.invoke_clock_menu_adjust(index, 1),
        },
        UiAction::Back => clock_menu::back(window),
        UiAction::Controls => window.set_ingame_clock_visible(false),
        UiAction::Start => window.invoke_ingame_resume(),
    }
}
