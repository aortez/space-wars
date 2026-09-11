//! Read-only diagnostics. No network configuration, credentials, shell commands,
//! or scenario mutation. Sampling is off-thread and demand-driven by visibility.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use spacewars_control::{UiAction, UiControl};

use crate::{DeviceInfoRow, MainWindow};

mod system;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Field {
    id: String,
    label: String,
    value: String,
}

impl Field {
    fn new(id: &str, label: &str, value: impl Into<String>) -> Self {
        Self {
            id: format!("info.{id}"),
            label: label.into(),
            value: value.into(),
        }
    }
}

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// One outstanding sample at most, including across close/reopen. Generations
/// prevent a slow response from a previous visit being published as fresh data.
#[derive(Default)]
struct Sampling {
    visible: bool,
    generation: u64,
    in_flight: bool,
    next_sample: Option<Instant>,
}

impl Sampling {
    fn opened(&mut self) {
        self.visible = true;
        self.generation += 1;
        self.next_sample = None;
    }

    fn request(&mut self, now: Instant) -> Option<u64> {
        if !self.visible || self.in_flight || self.next_sample.is_some_and(|next| now < next) {
            return None;
        }
        self.in_flight = true;
        self.next_sample = Some(now + SAMPLE_INTERVAL);
        Some(self.generation)
    }

    fn received(&mut self, generation: u64) -> bool {
        self.in_flight = false;
        generation == self.generation
    }
}

pub(crate) fn install(window: &MainWindow, config_directory: PathBuf) -> std::io::Result<()> {
    let (request_tx, request_rx) = mpsc::channel::<u64>();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("device-info".into())
        .spawn(move || {
            let mut system = system::Collector::new(config_directory);
            let mut previous_generation = None;
            while let Ok(generation) = request_rx.recv() {
                if previous_generation != Some(generation) {
                    system.reset_cpu_sample();
                    previous_generation = Some(generation);
                }
                if result_tx.send((generation, system.collect())).is_err() {
                    break;
                }
            }
        })?;
    let sampling = Rc::new(RefCell::new(Sampling::default()));
    let receiver = Rc::new(result_rx);
    let timer = Timer::default();
    let weak = window.as_weak();
    // Slint invokes this for every visibility change, including host cleanup.
    window.on_device_info_visibility_changed(move || {
        let Some(window) = weak.upgrade() else { return };
        timer.stop();
        if !window.get_device_info_visible() {
            sampling.borrow_mut().visible = false;
            return;
        }
        sampling.borrow_mut().opened();
        window.set_device_info_scroll_offset(0.0);
        window.set_device_info_ready(false);
        window.set_device_info_rows(ModelRc::default());
        let refresh = {
            let sampling = Rc::clone(&sampling);
            let receiver = Rc::clone(&receiver);
            let request_tx = request_tx.clone();
            let weak = window.as_weak();
            move || {
                let Some(window) = weak.upgrade() else { return };
                if !window.get_device_info_visible() {
                    return;
                }
                while let Ok((generation, mut fields)) = receiver.try_recv() {
                    if sampling.borrow_mut().received(generation) {
                        fields.extend(runtime_fields(&window));
                        let rows: Vec<_> = fields
                            .into_iter()
                            .map(|field| DeviceInfoRow {
                                id: field.id.into(),
                                label: field.label.into(),
                                value: field.value.into(),
                            })
                            .collect();
                        // Avoid replacing the visual tree for identical samples.
                        if !window
                            .get_device_info_rows()
                            .iter()
                            .eq(rows.iter().cloned())
                        {
                            window.set_device_info_rows(ModelRc::new(VecModel::from(rows)));
                        }
                        window.set_device_info_ready(true);
                    }
                }
                if let Some(generation) = sampling.borrow_mut().request(Instant::now())
                    && request_tx.send(generation).is_err()
                {
                    window.set_device_info_rows(ModelRc::new(VecModel::from(vec![
                        DeviceInfoRow {
                            id: "info.error".into(),
                            label: "Diagnostics unavailable".into(),
                            value: "The system information worker stopped. Back remains available."
                                .into(),
                        },
                    ])));
                }
            }
        };
        refresh();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), refresh);
    });
    let weak = window.as_weak();
    window.on_device_info_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_launcher_busy()
            || !window.get_sound_visible()
            || (!window.get_launcher_visible() && !window.get_ingame_menu_visible())
        {
            return;
        }
        window.set_sound_focus_index(3);
        window.set_device_info_ready(false);
        window.set_device_info_visible(true);
    });
    Ok(())
}

fn runtime_fields(window: &MainWindow) -> Vec<Field> {
    let scenario = if window.get_launcher_visible() {
        "Launcher (no active scenario)".into()
    } else {
        format!("{} · paused", window.get_launcher_scenario())
    };
    vec![
        Field::new(
            "controllers",
            "Controllers · Current player assignments",
            window.get_device_controllers().to_string(),
        ),
        Field::new("scenario", "Application · Active scenario", scenario),
        Field::new(
            "performance",
            "Application · Frame / update rate",
            if window.get_launcher_visible() {
                "Not running"
            } else {
                "Paused (frame / update rates stop in menus)"
            },
        ),
    ]
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    match action {
        UiAction::Up | UiAction::Left => window.invoke_device_info_scroll(-1),
        UiAction::Down | UiAction::Right => window.invoke_device_info_scroll(1),
        UiAction::Back | UiAction::Controls | UiAction::Confirm => {
            window.set_device_info_visible(false)
        }
        UiAction::Start => {
            window.set_device_info_visible(false);
            if !window.get_launcher_visible() {
                window.set_sound_visible(false);
                window.invoke_ingame_resume();
            }
        }
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    let mut controls: Vec<_> = window
        .get_device_info_rows()
        .iter()
        .map(|row| {
            UiControl::new(row.id.to_string(), row.label.to_string(), false)
                .with_value(row.value.to_string())
        })
        .collect();
    controls.extend([
        UiControl::new("info.status", "Sampling status", false).with_value(
            if window.get_device_info_ready() {
                "ready"
            } else {
                "loading"
            },
        ),
        UiControl::new("info.scroll-up", "Scroll up", true),
        UiControl::new("info.scroll-down", "Scroll down", true),
        UiControl::new("info.scroll-offset", "Scroll offset", false)
            .with_value(format!("{:.0}", window.get_device_info_scroll_offset())),
        UiControl::new("info.back", "Back to App Settings", true),
    ]);
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_is_bounded_throttled_and_rejects_previous_visit() {
        let mut sampler = Sampling::default();
        let now = Instant::now();
        assert_eq!(sampler.request(now), None, "no sampling before first open");
        sampler.opened();
        let first = sampler.request(now).unwrap();
        assert_eq!(sampler.request(now + SAMPLE_INTERVAL), None);
        sampler.opened();
        assert_eq!(
            sampler.request(now),
            None,
            "reopen cannot queue behind a slow sample"
        );
        assert!(
            !sampler.received(first),
            "old sample must not appear in the new visit"
        );
        let second = sampler.request(now).unwrap();
        assert!(sampler.received(second));
        assert_eq!(sampler.request(now), None);
        sampler.visible = false;
        assert_eq!(
            sampler.request(now + SAMPLE_INTERVAL * 100),
            None,
            "closed panels do not sample"
        );
        sampler.opened();
        assert!(sampler.request(now + SAMPLE_INTERVAL).is_some());
    }
}
