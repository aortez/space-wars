//! Manual install panel. Socket I/O never runs on the UI thread.
use crate::MainWindow;
use slint::ComponentHandle;
use spacewars_control::{UiAction, UiControl};

pub(crate) fn startup_required() -> bool {
    cfg!(all(target_os = "linux", feature = "pi-kiosk"))
}

/// Keep initial scenario/settings work behind the same fence as manual and
/// automatic launches. The UI event loop and health endpoint remain responsive.
pub(crate) fn after_startup(
    window: &MainWindow,
    action: impl FnOnce(&MainWindow) + 'static,
) -> Option<std::rc::Rc<slint::Timer>> {
    if !window.get_install_fenced() {
        action(window);
        return None;
    }
    let weak = window.as_weak();
    let mut action = Some(action);
    let timer = std::rc::Rc::new(slint::Timer::default());
    let control = std::rc::Rc::downgrade(&timer);
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(20),
        move || {
            if let Some(window) = weak.upgrade()
                && !window.get_install_fenced()
                && let Some(action) = action.take()
            {
                if let Some(timer) = control.upgrade() {
                    timer.stop();
                }
                action(&window);
            }
        },
    );
    Some(timer)
}

#[cfg(target_os = "linux")]
#[path = "updates_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::install;

#[cfg(not(target_os = "linux"))]
pub(crate) fn install(
    window: &MainWindow,
    _: std::sync::Arc<std::sync::RwLock<engine_common::Settings>>,
    _: crate::settings_writer::SettingsWriter,
    _: crate::input::SharedInput,
) {
    let weak = window.as_weak();
    window.on_updates_open(move || {
        let Some(window) = weak.upgrade() else { return };
        window.set_updates_visible(true);
        window.set_updates_message("Installation is unavailable on this platform.".into());
    });
    let weak = window.as_weak();
    window.on_updates_cancel(move || {
        if let Some(w) = weak.upgrade() {
            w.set_updates_visible(false);
        }
    });
}

/// Install is focus 0, Back (or Cancel) is focus 1 and Automatic install, left
/// of Back, is focus 2. The cursor reaches Install only while an offer stands,
/// and never by itself: a button that turned live under a resting cursor could
/// otherwise take the next press.
pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    if window.get_updates_automatic_visible() {
        return automatic_action(window, action);
    }
    let can_confirm = window.get_updates_can_confirm();
    let focus = window.get_updates_focus();
    match action {
        UiAction::Up if can_confirm => window.set_updates_focus(0),
        UiAction::Down if focus == 0 => window.set_updates_focus(1),
        UiAction::Left if focus != 0 && window.get_updates_automatic_available() => {
            window.set_updates_focus(2)
        }
        UiAction::Right if focus == 2 => window.set_updates_focus(1),
        UiAction::Confirm if focus == 0 => {
            // A press on a disabled Install button does nothing; it must not
            // fall through to Back.
            if can_confirm {
                window.invoke_updates_confirm();
            }
        }
        UiAction::Confirm if focus == 2 => {
            if window.get_updates_automatic_available() {
                window.invoke_updates_automatic_open();
            }
        }
        UiAction::Confirm | UiAction::Back if window.get_updates_can_cancel() => {
            window.invoke_updates_cancel()
        }
        _ => {}
    }
}

/// On the Automatic install page, Mode is focus 0, the window 1 and Back 2.
/// Left and Right change the row under the cursor, and each change saves.
fn automatic_action(window: &MainWindow, action: UiAction) {
    let focus = window.get_updates_automatic_focus();
    match action {
        UiAction::Up | UiAction::Down => {
            window.set_updates_automatic_focus(crate::ui_navigation::moved_selection(
                focus,
                3,
                if action == UiAction::Up { -1 } else { 1 },
            ))
        }
        UiAction::Left | UiAction::Right if focus <= 1 => window
            .invoke_updates_automatic_adjust(focus, if action == UiAction::Left { -1 } else { 1 }),
        UiAction::Confirm if focus == 0 => window.invoke_updates_automatic_adjust(0, 1),
        UiAction::Confirm if focus == 2 => close_automatic(window),
        UiAction::Back => close_automatic(window),
        _ => {}
    }
}

/// Returns from the Automatic install page to its button on Updates.
pub(crate) fn close_automatic(window: &MainWindow) {
    window.set_updates_automatic_visible(false);
    window.set_updates_focus(2);
}

/// Returns the cursor to Back once the button under it is disabled, whatever
/// disabled it: for Install a press, an outcome, an abort or its expiry.
pub(crate) fn settle_focus(window: &MainWindow) {
    let focus = window.get_updates_focus();
    if (focus == 0 && !window.get_updates_can_confirm())
        || (focus == 2 && !window.get_updates_automatic_available())
    {
        window.set_updates_focus(1);
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    if window.get_updates_automatic_visible() {
        let mut controls = Vec::new();
        for (row, value) in [
            ("mode", window.get_updates_automatic_mode()),
            ("window", window.get_updates_automatic_window()),
        ] {
            for (suffix, label) in [("previous", "‹"), ("next", "›")] {
                controls.push(
                    UiControl::new(format!("updates.automatic.{row}.{suffix}"), label, true)
                        .with_value(value.as_str()),
                );
            }
        }
        controls.push(UiControl::new("updates.automatic.back", "Back", true));
        controls.push(
            UiControl::new("updates.automatic.status", "Setting", false)
                .with_value(window.get_updates_automatic_status().as_str()),
        );
        return controls;
    }
    let mut controls = vec![
        UiControl::new(
            "updates.confirm",
            window.get_updates_confirm_label().to_string(),
            window.get_updates_can_confirm(),
        ),
        UiControl::new(
            "updates.cancel",
            if window.get_install_fenced() {
                "Cancel"
            } else {
                "Back"
            },
            window.get_updates_can_cancel(),
        ),
    ];
    if !window.get_install_fenced() {
        controls.push(UiControl::new(
            "updates.automatic",
            window.get_updates_automatic_label().to_string(),
            window.get_updates_automatic_available(),
        ));
    }
    controls
}

/// The control under the cursor, by the focus numbers above.
pub(crate) fn selected(window: &MainWindow) -> Option<String> {
    let (ids, focus): (&[&str], i32) = if window.get_updates_automatic_visible() {
        (
            &[
                "updates.automatic.mode",
                "updates.automatic.window",
                "updates.automatic.back",
            ],
            window.get_updates_automatic_focus(),
        )
    } else {
        (
            &["updates.confirm", "updates.cancel", "updates.automatic"],
            window.get_updates_focus(),
        )
    };
    usize::try_from(focus)
        .ok()
        .and_then(|focus| ids.get(focus))
        .map(|id| (*id).to_owned())
}

/// Presses the control that a UI-control client names, when it is on screen
/// and enabled.
pub(crate) fn activate(window: &MainWindow, control_id: &str) -> bool {
    if !inventory(window)
        .iter()
        .any(|control| control.id == control_id && control.enabled)
    {
        return false;
    }
    match control_id {
        "updates.confirm" => window.invoke_updates_confirm(),
        "updates.cancel" => window.invoke_updates_cancel(),
        "updates.automatic" => window.invoke_updates_automatic_open(),
        "updates.automatic.mode.previous" => window.invoke_updates_automatic_adjust(0, -1),
        "updates.automatic.mode.next" => window.invoke_updates_automatic_adjust(0, 1),
        "updates.automatic.window.previous" => window.invoke_updates_automatic_adjust(1, -1),
        "updates.automatic.window.next" => window.invoke_updates_automatic_adjust(1, 1),
        "updates.automatic.back" => close_automatic(window),
        _ => return false,
    }
    true
}
