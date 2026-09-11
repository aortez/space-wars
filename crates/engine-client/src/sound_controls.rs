//! App-wide output preferences shared by the launcher and paused scenarios.
//! The UI is labelled App Settings; sound control IDs remain stable for clients.

use std::sync::{Arc, RwLock};

use engine_common::{AudioSettings, Settings};
use slint::ComponentHandle;
use spacewars_control::UiAction;

use crate::{MainWindow, host, settings_writer::SettingsWriter, ui_navigation};

// Fine adjustment for amplified cabinet speakers, without changing the meaning
// of existing saved gains. Above 20%, use the usual five-percent stops.
fn next_volume_percent(percent: i32, direction: i32) -> i32 {
    match direction.signum() {
        1 if percent < 20 => percent + 1,
        1 => ((percent / 5 + 1) * 5).min(100),
        -1 if percent <= 20 => (percent - 1).max(0),
        -1 => ((percent - 1) / 5 * 5).max(20),
        _ => percent,
    }
}

pub(crate) fn install(
    window: &MainWindow,
    controls: host::SharedScenarioControls,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
) {
    let initial = settings.read().unwrap().clone();
    publish(window, &initial);
    controls
        .borrow_mut()
        .set_audio_settings(initial.audio.normalized());

    let weak = window.as_weak();
    window.on_sound_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_launcher_busy()
            || (!window.get_launcher_visible() && !window.get_ingame_menu_visible())
        {
            return;
        }
        window.set_sound_focus_index(0);
        window.set_sound_visible(true);
    });

    let weak = window.as_weak();
    let adjustment_settings = Arc::clone(&settings);
    let adjust_writer = writer.clone();
    window.on_sound_adjust(move |index, delta| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_sound_visible() || window.get_launcher_busy() {
            return;
        }
        window.set_sound_focus_index(index);
        let snapshot = {
            let mut settings = adjustment_settings.write().unwrap();
            if !adjust_settings(&mut settings, index, delta) {
                return;
            }
            settings.clone()
        };
        publish(&window, &snapshot);
        controls.borrow_mut().set_audio_settings(snapshot.audio);
        adjust_writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
    });

    let weak = window.as_weak();
    window.on_sound_retry(move || {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_sound_visible() || window.get_launcher_busy() {
            return;
        }
        let snapshot = settings.read().unwrap().clone();
        writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
        window.set_sound_focus_index(4);
    });
}

fn publish(window: &MainWindow, settings: &Settings) {
    let audio = settings.audio.normalized();
    window.set_sound_volume_percent((audio.master_volume * 100.0).round() as i32);
    window.set_sound_muted(audio.muted);
    window.set_performance_overlay_enabled(settings.video.show_fps);
}

fn adjust_settings(settings: &mut Settings, index: i32, delta: i32) -> bool {
    if index == 2 {
        let enabled = if delta == 0 {
            !settings.video.show_fps
        } else {
            delta > 0
        };
        let changed = enabled != settings.video.show_fps;
        settings.video.show_fps = enabled;
        changed
    } else {
        let audio = adjusted(settings.audio, index, delta);
        let changed = audio != settings.audio;
        settings.audio = audio;
        changed
    }
}

fn adjusted(audio: AudioSettings, index: i32, delta: i32) -> AudioSettings {
    let mut audio = audio.normalized();
    match index {
        0 => {
            let percent = (audio.master_volume * 100.0).round() as i32;
            audio.master_volume = next_volume_percent(percent, delta) as f32 / 100.0;
        }
        1 => audio.muted = if delta == 0 { !audio.muted } else { delta > 0 },
        _ => {}
    }
    audio
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    let index = window.get_sound_focus_index();
    let count = if window.get_settings_save_error().is_empty() {
        5
    } else {
        6
    };
    match action {
        UiAction::Up | UiAction::Down => {
            window.set_sound_focus_index(ui_navigation::moved_selection(
                index,
                count,
                if action == UiAction::Up { -1 } else { 1 },
            ))
        }
        UiAction::Left | UiAction::Right if index <= 2 => {
            window.invoke_sound_adjust(index, if action == UiAction::Left { -1 } else { 1 })
        }
        UiAction::Left | UiAction::Right if count == 6 && index >= 4 => {
            window.set_sound_focus_index(if index == 4 { 5 } else { 4 })
        }
        UiAction::Confirm if index == 1 || index == 2 => window.invoke_sound_adjust(index, 0),
        UiAction::Confirm if index == 3 => window.invoke_device_info_open(),
        UiAction::Confirm if index == 5 && count == 6 => window.invoke_sound_retry(),
        UiAction::Back | UiAction::Controls | UiAction::Confirm
            if action != UiAction::Confirm || index == 4 =>
        {
            window.set_sound_visible(false)
        }
        UiAction::Start => {
            window.set_sound_visible(false);
            if !window.get_launcher_visible() {
                window.invoke_ingame_resume();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_counter_is_global_and_adjusting_it_does_not_change_audio() {
        let mut settings = Settings::default();
        settings.audio.master_volume = 0.05;
        settings.audio.muted = true;
        let audio = settings.audio;
        assert!(!settings.video.show_fps);
        assert!(adjust_settings(&mut settings, 2, 0));
        assert!(settings.video.show_fps);
        assert!(!adjust_settings(&mut settings, 2, 1));
        assert!(adjust_settings(&mut settings, 2, -1));
        assert!(!settings.video.show_fps);
        assert!(!adjust_settings(&mut settings, 2, -1));
        assert_eq!(settings.audio, audio);
        assert!(adjust_settings(&mut settings, 2, 1));
        assert!(adjust_settings(&mut settings, 0, -1));
        assert!(
            settings.video.show_fps,
            "audio adjustments preserve the overlay"
        );
    }

    #[test]
    fn volume_steps_clamp_without_unmuting() {
        let mut audio = AudioSettings {
            master_volume: 0.98,
            muted: true,
        };
        audio = adjusted(audio, 0, 1);
        assert_eq!(audio.master_volume, 1.0);
        assert_eq!(adjusted(audio, 0, 1), audio);
        for _ in 0..100 {
            audio = adjusted(audio, 0, -1);
        }
        assert_eq!(audio.master_volume, 0.0);
        assert!(audio.muted);
        assert_eq!(adjusted(audio, 0, 1).master_volume, 0.01);
    }

    #[test]
    fn quiet_volume_has_fine_reversible_steps_and_preserves_saved_gain() {
        let stops: Vec<_> = (0..=20).chain((25..=100).step_by(5)).collect();
        for pair in stops.windows(2) {
            assert_eq!(next_volume_percent(pair[0], 1), pair[1]);
            assert_eq!(next_volume_percent(pair[1], -1), pair[0]);
        }
        assert_eq!(next_volume_percent(10, -1), 9);
        assert_eq!(next_volume_percent(23, -1), 20);
        assert_eq!(next_volume_percent(23, 1), 25);
        let saved = AudioSettings {
            master_volume: 0.10,
            muted: false,
        };
        assert_eq!(adjusted(saved, 0, 0), saved);
        assert_eq!(adjusted(saved, 0, -1).master_volume, 0.09);
    }

    #[test]
    fn mute_toggle_preserves_volume_and_directional_input_is_idempotent() {
        let audio = AudioSettings::default();
        let muted = adjusted(audio, 1, 0);
        assert!(muted.muted);
        assert_eq!(muted.master_volume, audio.master_volume);
        assert_eq!(adjusted(muted, 1, 1), muted);
        assert_eq!(adjusted(muted, 1, -1), audio);
        assert_eq!(adjusted(audio, 1, -1), audio);
        assert_eq!(adjusted(muted, 1, 0), audio);
    }
}
