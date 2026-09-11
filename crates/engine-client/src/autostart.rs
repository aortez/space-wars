//! Idle activity policy. Scenario clocks and match outcomes remain in the host.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use engine_common::{AutostartSettings, Settings};
use slint::{ComponentHandle, Timer, TimerMode};
use spacewars_control::{UiAction, UiControl};

use crate::{MainWindow, UserActivity, launcher::Launcher, settings_writer::SettingsWriter};

pub(crate) struct Activity {
    pub id: &'static str,
    pub label: &'static str,
    pub scenario: &'static str,
    pub repeat_matches: bool,
}

const ACTIVITIES: [Activity; 2] = [
    Activity {
        id: "clock",
        label: "Clock",
        scenario: "clock",
        repeat_matches: false,
    },
    Activity {
        id: "spacewars-bots",
        label: "Spacewars bots",
        scenario: "spacewars",
        repeat_matches: true,
    },
];
const RESULT_DELAY: Duration = Duration::from_secs(8);

fn activity(id: &str) -> Option<&'static Activity> {
    ACTIVITIES.iter().find(|activity| activity.id == id)
}

#[derive(Default)]
struct IdleCountdown {
    deadline: Option<Instant>,
}

impl IdleCountdown {
    fn reset(&mut self) {
        self.deadline = None;
    }

    fn remaining(&mut self, now: Instant, eligible: bool, delay: Duration) -> Option<Duration> {
        if !eligible {
            self.reset();
            return None;
        }
        let deadline = *self.deadline.get_or_insert(now + delay);
        Some(deadline.saturating_duration_since(now))
    }
}

struct Runtime {
    idle: IdleCountdown,
    preferences: AutostartSettings,
    failed: bool,
    manual_launch: Option<crate::EffectiveLaunch>,
    result_remaining: Duration,
    last_tick: Instant,
    repeat_pending: Option<String>,
    repeats: u64,
    keys: BTreeSet<String>,
    consumed_keys: BTreeSet<String>,
}

pub(crate) fn install(
    window: &MainWindow,
    launcher: Rc<Launcher>,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
    catalog: crate::SharedNesRomCatalog,
    enabled_for_process: bool,
) -> Timer {
    let preferences = settings.read().unwrap().autostart.normalized();
    publish_preferences(window, &preferences);
    let runtime = Rc::new(RefCell::new(Runtime {
        idle: IdleCountdown::default(),
        preferences,
        failed: false,
        manual_launch: None,
        result_remaining: RESULT_DELAY,
        last_tick: Instant::now(),
        repeat_pending: None,
        repeats: 0,
        keys: BTreeSet::new(),
        consumed_keys: BTreeSet::new(),
    }));

    let weak = window.as_weak();
    let state = Rc::clone(&runtime);
    window.global::<UserActivity>().on_notify(move || {
        let Some(window) = weak.upgrade() else {
            return false;
        };
        state.borrow_mut().idle.reset();
        if window.get_autostart_running() && !window.get_ingame_menu_visible() {
            window.invoke_autostart_return();
            return true;
        }
        false
    });

    let weak = window.as_weak();
    let state = Rc::clone(&runtime);
    window
        .global::<UserActivity>()
        .on_key_event(move |key, down| {
            let Some(window) = weak.upgrade() else {
                return false;
            };
            let key = key.to_string();
            let consumed = {
                let mut state = state.borrow_mut();
                state.idle.reset();
                if down {
                    state.keys.insert(key.clone());
                } else {
                    state.keys.remove(&key);
                }
                let consumed = state.consumed_keys.contains(&key);
                if !down {
                    state.consumed_keys.remove(&key);
                }
                consumed
            };
            if down && window.get_autostart_running() && !window.get_ingame_menu_visible() {
                state.borrow_mut().consumed_keys.insert(key);
                window.invoke_autostart_return();
                true
            } else {
                consumed
            }
        });

    let weak = window.as_weak();
    let state = Rc::clone(&runtime);
    window.global::<UserActivity>().on_focus_lost(move || {
        let mut state = state.borrow_mut();
        state.keys.clear();
        state.consumed_keys.clear();
        state.idle.reset();
        if let Some(window) = weak.upgrade() {
            window.global::<UserActivity>().set_pointer_held(false);
        }
    });

    let weak = window.as_weak();
    let state = Rc::clone(&runtime);
    let return_launcher = Rc::clone(&launcher);
    let return_settings = Arc::clone(&settings);
    window.on_autostart_return(move || {
        let Some(window) = weak.upgrade() else { return };
        return_launcher.cancel_automatic();
        window.set_autostart_running(false);
        window.set_autostart_activity("".into());
        window.set_autostart_caption("".into());
        let manual = {
            let mut state = state.borrow_mut();
            state.idle.reset();
            state.repeat_pending = None;
            state.result_remaining = RESULT_DELAY;
            state.manual_launch.take()
        };
        window.invoke_ingame_return_launcher();
        if let Some(launch) = manual {
            crate::show_launcher(&window, &launch, &return_settings.read().unwrap(), &catalog);
        }
    });

    let weak = window.as_weak();
    window.on_autostart_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_sound_visible() && !window.get_launcher_busy() {
            window.set_sound_focus_index(4);
            window.set_autostart_focus_index(0);
            window.set_autostart_settings_visible(true);
        }
    });

    let weak = window.as_weak();
    let adjust_settings = Arc::clone(&settings);
    let state = Rc::clone(&runtime);
    window.on_autostart_adjust(move |index, delta| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_autostart_settings_visible() {
            return;
        }
        window.set_autostart_focus_index(index);
        let snapshot = {
            let mut settings = adjust_settings.write().unwrap();
            let p = &mut settings.autostart;
            if index == 0 {
                let current = if !p.enabled {
                    0
                } else {
                    ACTIVITIES
                        .iter()
                        .position(|a| a.id == p.activity)
                        .map_or(0, |i| i + 1)
                };
                let next = (current as i32 + delta.signum()).rem_euclid(3) as usize;
                p.enabled = next != 0;
                if next != 0 {
                    p.activity = ACTIVITIES[next - 1].id.into();
                }
            } else if index == 1 {
                let stops = [5, 10, 30, 60, 120, 300, 600];
                p.delay_seconds = if delta > 0 {
                    stops
                        .into_iter()
                        .find(|&n| n > p.delay_seconds)
                        .unwrap_or(5)
                } else {
                    stops
                        .into_iter()
                        .rev()
                        .find(|&n| n < p.delay_seconds)
                        .unwrap_or(600)
                };
            } else {
                return;
            }
            settings.clone()
        };
        state.borrow_mut().failed = false;
        state.borrow_mut().idle.reset();
        publish_preferences(&window, &snapshot.autostart);
        writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
    });

    let weak = window.as_weak();
    let state = Rc::clone(&runtime);
    let start_settings = Arc::clone(&settings);
    let start_launcher = Rc::clone(&launcher);
    window.on_autostart_start_now(move || {
        let Some(window) = weak.upgrade() else { return };
        let p = start_settings.read().unwrap().autostart.normalized();
        if !window.get_launcher_visible()
            || window.get_launcher_busy()
            || window.get_settings_save_pending()
            || !window.get_settings_save_error().is_empty()
        {
            return;
        }
        if let Some(activity) = activity(&p.activity).filter(|_| p.enabled) {
            state.borrow_mut().failed = false;
            begin(&window, &start_launcher, &state, activity);
        }
    });

    let timer = Timer::default();
    let weak = window.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(window) = weak.upgrade() else { return };
        let now = Instant::now();
        let p = settings.read().unwrap().autostart.normalized();
        let (elapsed, held) = {
            let mut state = runtime.borrow_mut();
            let elapsed = now.saturating_duration_since(state.last_tick);
            state.last_tick = now;
            if state.preferences != p {
                state.preferences = p.clone();
                state.failed = false;
                state.idle.reset();
            }
            (elapsed, !state.keys.is_empty())
        };
        let mut phase = "inactive";
        let mut caption = String::new();
        if window.get_autostart_running() {
            runtime.borrow_mut().idle.reset();
            if !window.get_launcher_error_text().is_empty() || !window.get_scenario_error_text().is_empty() {
                runtime.borrow_mut().failed = true;
                phase = "failed";
                caption = "Automatic activity stopped. Press a button to return to the menu.".into();
            } else if window.get_launcher_busy() {
                phase = "launching";
            } else if window.get_ingame_menu_visible() {
                phase = "paused";
            } else if window.get_game_over_visible() && activity(window.get_autostart_activity().as_str()).is_some_and(|a| a.repeat_matches) {
                phase = "result";
                let mut state = runtime.borrow_mut();
                state.result_remaining = state.result_remaining.saturating_sub(elapsed);
                caption = if p.enabled {
                    format!("Next world in {} seconds · Press a button for the menu", state.result_remaining.as_secs_f64().ceil() as u64)
                } else {
                    "Auto-start is Off · Press a button for the menu".into()
                };
                if state.result_remaining.is_zero() && state.repeat_pending.is_none() && p.enabled && !state.failed {
                    state.repeat_pending = Some(window.get_scenario_instance().to_string());
                    drop(state);
                    window.invoke_ingame_new_match();
                }
            } else {
                phase = "running";
                let mut state = runtime.borrow_mut();
                if state.repeat_pending.as_ref().is_some_and(|revision| revision != window.get_scenario_instance().as_str()) {
                    state.repeat_pending = None;
                    state.repeats += 1;
                }
                state.result_remaining = RESULT_DELAY;
                caption = "Automatic activity · Press a button to return to the menu".into();
            }
        } else {
            let eligible = enabled_for_process && p.enabled && !runtime.borrow().failed
                && window.get_launcher_visible() && !window.get_launcher_busy()
                && !window.get_sound_visible() && !window.get_launcher_settings_visible()
                && !window.get_launcher_controls_visible() && !window.get_touch_test_visible()
                && !window.get_settings_save_pending() && window.get_settings_save_error().is_empty()
                && window.get_launcher_error_text().is_empty() && !held
                && !window.global::<UserActivity>().get_pointer_held()
                && !window.global::<UserActivity>().get_gamepad_held();
            let remaining = runtime.borrow_mut().idle.remaining(now, eligible, Duration::from_secs(p.delay_seconds.into()));
            if let Some(remaining) = remaining {
                if let Some(activity) = activity(&p.activity) {
                    phase = "countdown";
                    caption = format!("{} starts in {} seconds", activity.label, remaining.as_secs_f64().ceil() as u64);
                    if remaining.is_zero() {
                        begin(&window, &launcher, &runtime, activity);
                        phase = "launching";
                        caption.clear();
                    }
                } else {
                    phase = "unavailable";
                    caption = format!("Auto-start activity unavailable: {}", p.activity);
                }
            } else if !p.enabled { phase = "disabled"; }
            else if !enabled_for_process { phase = "suppressed"; }
            else if runtime.borrow().failed {
                phase = "failed";
                caption = "Auto-start stopped after an error · Open Auto-start settings to retry".into();
            }
        }
        window.set_autostart_caption(caption.into());
        window.set_autostart_diagnostics(format!(
            "autostart_enabled={}\nautostart_activity={}\nautostart_phase={}\nautostart_session={}\nautostart_repeats={}\nautostart_delay_seconds={}",
            p.enabled, p.activity, phase, if window.get_autostart_running() { "automatic" } else { "manual" }, runtime.borrow().repeats, p.delay_seconds,
        ).into());
    });
    timer
}

fn begin(
    window: &MainWindow,
    launcher: &Rc<Launcher>,
    runtime: &Rc<RefCell<Runtime>>,
    activity: &Activity,
) {
    let mut state = runtime.borrow_mut();
    state.manual_launch = crate::launch_options_from_window(window).ok();
    state.idle.reset();
    state.result_remaining = RESULT_DELAY;
    state.repeat_pending = None;
    state.repeats = 0;
    drop(state);
    window.set_autostart_running(true);
    window.set_autostart_activity(activity.id.into());
    window.set_autostart_settings_visible(false);
    window.set_sound_visible(false);
    launcher.start_automatic(activity);
}

fn publish_preferences(window: &MainWindow, p: &AutostartSettings) {
    window.set_autostart_choice(if !p.enabled {
        "Off".into()
    } else {
        activity(&p.activity)
            .map_or_else(
                || format!("Unavailable: {}", p.activity),
                |a| a.label.into(),
            )
            .into()
    });
    window.set_autostart_delay(p.delay_seconds.to_string().into());
    window.set_autostart_start_available(p.enabled && activity(&p.activity).is_some());
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    let index = window.get_autostart_focus_index();
    match action {
        UiAction::Up | UiAction::Down => {
            window.set_autostart_focus_index(crate::ui_navigation::moved_selection(
                index,
                4,
                if action == UiAction::Up { -1 } else { 1 },
            ))
        }
        UiAction::Left | UiAction::Right if index <= 1 => {
            window.invoke_autostart_adjust(index, if action == UiAction::Left { -1 } else { 1 })
        }
        UiAction::Confirm if index == 0 => window.invoke_autostart_adjust(0, 1),
        UiAction::Confirm if index == 2 => window.invoke_autostart_start_now(),
        UiAction::Back | UiAction::Controls | UiAction::Start => {
            window.set_autostart_settings_visible(false)
        }
        UiAction::Confirm if index == 3 => window.set_autostart_settings_visible(false),
        _ => {}
    }
}

pub(crate) fn controls(window: &MainWindow) -> Vec<UiControl> {
    let mut controls = Vec::new();
    for (id, value) in [
        ("activity", window.get_autostart_choice()),
        ("delay", window.get_autostart_delay()),
    ] {
        for (suffix, label) in [("previous", "‹"), ("next", "›")] {
            controls.push(
                UiControl::new(format!("autostart.{id}.{suffix}"), label, true)
                    .with_value(value.as_str()),
            );
        }
    }
    controls.push(UiControl::new(
        "autostart.start-now",
        "Start now",
        window.get_launcher_visible()
            && window.get_autostart_start_available()
            && !window.get_settings_save_pending()
            && window.get_settings_save_error().is_empty(),
    ));
    controls.push(UiControl::new("autostart.back", "Back", true));
    controls.push(
        UiControl::new("autostart.save-status", "Settings save", false).with_value(
            if window.get_settings_save_pending() {
                "saving"
            } else if !window.get_settings_save_error().is_empty() {
                "error"
            } else {
                "saved"
            },
        ),
    );
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_visits_require_the_whole_delay_and_input_wins_at_the_deadline() {
        let now = Instant::now();
        let delay = Duration::from_secs(30);
        let mut idle = IdleCountdown::default();
        assert_eq!(idle.remaining(now, true, delay), Some(delay));
        assert_eq!(
            idle.remaining(now + delay - Duration::from_nanos(1), true, delay),
            Some(Duration::from_nanos(1))
        );
        idle.reset(); // Input handled before this timer poll.
        assert_eq!(idle.remaining(now + delay, true, delay), Some(delay));
        assert_eq!(
            idle.remaining(now + delay * 2, true, delay),
            Some(Duration::ZERO)
        );
        assert_eq!(idle.remaining(now + delay * 3, false, delay), None);
        assert_eq!(idle.remaining(now + delay * 9, true, delay), Some(delay));
    }

    #[test]
    fn disabled_or_held_input_cannot_accumulate_idle_time() {
        let now = Instant::now();
        let delay = Duration::from_secs(5);
        let mut idle = IdleCountdown::default();
        for seconds in 0..100 {
            assert_eq!(
                idle.remaining(now + Duration::from_secs(seconds), false, delay),
                None
            );
        }
        assert_eq!(
            idle.remaining(now + Duration::from_secs(100), true, delay),
            Some(delay)
        );
    }

    #[test]
    fn invalid_timer_fields_preserve_other_preferences_and_unknown_activities() {
        for invalid in ["-1", "true", "\"oops\"", "[]", "{}"] {
            let text = format!(
                "[audio]\nmaster_volume = 0.05\n[spacewars_match]\ntime_limit_seconds = {invalid}\n[autostart]\nenabled = true\nactivity = 'future-garden'\ndelay_seconds = {invalid}"
            );
            let settings: Settings = toml::from_str(&text).unwrap();
            assert_eq!(settings.audio.master_volume, 0.05);
            assert_eq!(settings.spacewars_match.time_limit_seconds, 600);
            assert_eq!(settings.autostart.delay_seconds, 30);
            assert!(settings.autostart.enabled);
            assert_eq!(settings.autostart.activity, "future-garden");
            assert!(activity(&settings.autostart.activity).is_none());
        }
        let settings: Settings =
            toml::from_str("[spacewars_match]\ntime_limit_seconds=0\n[autostart]\ndelay_seconds=0")
                .unwrap();
        assert_eq!(settings.spacewars_match.time_limit_seconds, 0);
        assert_eq!(settings.autostart.delay_seconds, 5);
    }
}
