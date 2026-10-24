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

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    match action {
        UiAction::Up | UiAction::Down | UiAction::Left | UiAction::Right => {
            window.set_updates_focus(1 - window.get_updates_focus());
        }
        UiAction::Confirm
            if window.get_updates_focus() == 0 && window.get_updates_can_confirm() =>
        {
            window.invoke_updates_confirm()
        }
        UiAction::Confirm | UiAction::Back if window.get_updates_can_cancel() => {
            window.invoke_updates_cancel()
        }
        _ => {}
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    [
        (
            "updates.confirm",
            window.get_updates_confirm_label().to_string(),
            window.get_updates_can_confirm(),
        ),
        (
            "updates.cancel",
            if window.get_install_fenced() {
                "Cancel"
            } else {
                "Back"
            }
            .into(),
            window.get_updates_can_cancel(),
        ),
    ]
    .into_iter()
    .map(|(id, label, enabled)| UiControl::new(id, label, enabled))
    .collect()
}
