//! Device-level controller setup. Drafts are isolated; a short, reversible
//! trial precedes persistence. Player-slot policy is kept in Assignments, not
//! in scenarios or the button-profile schema.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use engine_common::{ControllerControl, ControllerProfile, Settings};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use spacewars_control::{UiAction, UiControl};

use crate::controller_assignments::Assignments;
use crate::controller_profile::{Capture, RawState};
use crate::input::{GamepadSeatInput, SharedGamepadInput};
use crate::settings_writer::SettingsWriter;
use crate::{ControllerRow, MainWindow};

const TRIAL_TIME: Duration = Duration::from_secs(15);
const CAPTURE_TIME: Duration = Duration::from_secs(120);
const TEST_TIME: Duration = Duration::from_secs(60);
const MAX_PROFILES: usize = 32;

pub(crate) type SharedControllers = Rc<RefCell<Controllers>>;

#[derive(Clone, Debug)]
pub(crate) struct Device {
    pub id: usize,
    pub key: String,
    pub name: String,
    pub seat: Option<usize>,
}

#[derive(Debug)]
enum Page {
    Devices,
    Device(usize),
    Identify(Instant),
    Capture {
        id: usize,
        capture: Capture,
        deadline: Instant,
    },
    Review {
        id: usize,
        profile: ControllerProfile,
    },
    Trial {
        id: usize,
        profile: Option<ControllerProfile>,
        deadline: Instant,
    },
    Test {
        id: usize,
        deadline: Instant,
    },
}

impl Page {
    fn device(&self) -> Option<usize> {
        match self {
            Self::Device(id)
            | Self::Capture { id, .. }
            | Self::Review { id, .. }
            | Self::Trial { id, .. }
            | Self::Test { id, .. } => Some(*id),
            Self::Devices | Self::Identify(_) => None,
        }
    }

    fn captures_input(&self) -> bool {
        matches!(
            self,
            Self::Capture { .. } | Self::Identify(_) | Self::Test { .. }
        )
    }
}

pub(crate) struct Controllers {
    devices: BTreeMap<usize, Device>,
    profiles: BTreeMap<String, ControllerProfile>,
    assignments: Assignments,
    gamepads: SharedGamepadInput,
    page: Page,
    pub epoch: u64,
    visible: bool,
    status: String,
    live: String,
    presses: u64,
    releases: u64,
    test_back_since: Option<Instant>,
    test_armed: bool,
    last_publish: Option<Instant>,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
}

pub(crate) fn install(
    window: &MainWindow,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
    gamepads: SharedGamepadInput,
) -> SharedControllers {
    let mut profiles = BTreeMap::new();
    let mut invalid = 0;
    for profile in &settings.read().unwrap().controls.controller_profiles {
        if profile.valid()
            && profiles.len() < MAX_PROFILES
            && !profiles.contains_key(&profile.device_key)
        {
            profiles.insert(profile.device_key.clone(), profile.clone());
        } else {
            invalid += 1;
        }
    }
    let preferences = {
        let settings = settings.read().unwrap();
        [
            settings.controls.player_1_device.clone(),
            settings.controls.player_2_device.clone(),
        ]
    };
    let state = Rc::new(RefCell::new(Controllers {
        devices: BTreeMap::new(),
        profiles,
        assignments: Assignments::new(preferences),
        gamepads,
        page: Page::Devices,
        epoch: 0,
        visible: false,
        status: if invalid > 0 {
            format!(
                "Ignored {invalid} invalid or duplicate saved profiles; defaults remain available."
            )
        } else {
            String::new()
        },
        live: String::new(),
        presses: 0,
        releases: 0,
        test_back_since: None,
        test_armed: false,
        last_publish: None,
        settings,
        writer,
    }));
    let weak = window.as_weak();
    let open_state = Rc::clone(&state);
    window.on_controllers_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_sound_visible() || window.get_launcher_busy() {
            return;
        }
        let mut state = open_state.borrow_mut();
        state.change_page(Page::Devices);
        state.visible = true;
        window.set_controllers_visible(true);
        window.set_controllers_focus_index(0);
        state.publish(&window, Instant::now());
    });
    let weak = window.as_weak();
    let commands = Rc::clone(&state);
    window.on_controllers_command(move |command| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_controllers_visible()
            || !window.get_sound_visible()
            || window.get_launcher_busy()
        {
            return;
        }
        commands
            .borrow_mut()
            .command(&window, command.as_str(), Instant::now());
    });
    let closed = Rc::clone(&state);
    let weak = window.as_weak();
    window.on_controllers_visibility_changed(move || {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_controllers_visible() {
            // The parent menu can close during launch, restart, or disconnect.
            // Never leave an unconfirmed mapping active behind a hidden screen.
            if let Ok(mut state) = closed.try_borrow_mut() {
                state.visible = false;
                state.change_page(Page::Devices);
            }
        }
    });
    state
}

impl Controllers {
    pub fn connect(&mut self, device: Device) {
        self.assignments.connect(device.id, device.key.clone());
        self.devices.insert(device.id, device);
        self.assignments_changed();
    }

    pub fn connect_many(&mut self, devices: Vec<Device>) {
        self.assignments
            .connect_many(devices.iter().map(|device| (device.id, device.key.clone())));
        for device in devices {
            self.devices.insert(device.id, device);
        }
        self.assignments_changed();
    }

    pub fn disconnect(&mut self, id: usize) {
        self.assignments.disconnect(id);
        self.devices.remove(&id);
        self.assignments_changed();
        if self.page.device() == Some(id) {
            self.change_page(Page::Devices);
            self.status = "Controller disconnected. Unsaved mapping discarded.".into();
        }
    }

    pub fn seat(&self, id: usize) -> Option<usize> {
        self.assignments.seat(id)
    }

    pub fn connected_id(&self, seat: usize) -> Option<usize> {
        self.assignments.connected_id(seat)
    }

    pub fn has_disconnected_seat(&self) -> bool {
        self.assignments.has_disconnected_seat()
    }

    pub fn disconnected_player_labels(&self) -> String {
        (0..2)
            .filter(|seat| self.assignments.reserved(*seat) && self.connected_id(*seat).is_none())
            .map(|seat| format!("P{}", seat + 1))
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn assignments_changed(&mut self) {
        for device in self.devices.values_mut() {
            device.seat = self.assignments.seat(device.id);
        }
        // Clear synchronously, including touch/CLI activations between polls.
        // The pump observes the epoch and gates every device until neutral.
        self.gamepads.borrow_mut().clear_for_reassignment();
        self.epoch = self.epoch.wrapping_add(1);
        self.last_publish = None;
    }

    fn save_preferences(&self, window: &MainWindow, preferences: [Option<String>; 2]) {
        let snapshot = {
            let mut settings = self.settings.write().unwrap();
            [
                settings.controls.player_1_device,
                settings.controls.player_2_device,
            ] = preferences;
            settings.clone()
        };
        self.writer.save(snapshot);
        window.set_settings_save_pending(true);
        window.set_settings_save_error("".into());
    }

    fn player_summary(&self) -> String {
        (0..2)
            .map(|seat| {
                let name = self
                    .assignments
                    .connected_id(seat)
                    .and_then(|id| self.devices.get(&id))
                    .map(|device| device.name.as_str())
                    .unwrap_or(if self.assignments.reserved(seat) {
                        "Reserved — controller disconnected"
                    } else {
                        "No controller"
                    });
                format!("P{}: {name}", seat + 1)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn profile(&self, id: usize) -> Option<&ControllerProfile> {
        let device = self.devices.get(&id)?;
        if let Page::Trial {
            id: trial_id,
            profile,
            ..
        } = &self.page
            && self
                .devices
                .get(trial_id)
                .is_some_and(|trial| trial.key == device.key)
        {
            return profile.as_ref();
        }
        self.profiles.get(&device.key)
    }

    pub fn captures_input(&self) -> bool {
        self.visible && self.page.captures_input()
    }

    fn change_page(&mut self, page: Page) {
        self.page = page;
        self.epoch = self.epoch.wrapping_add(1);
        self.test_back_since = None;
        self.test_armed = false;
        self.last_publish = None;
    }

    /// Called for every physical edge as well as the periodic held snapshot.
    /// Repeated events deliberately do not inflate the diagnostic counters.
    pub fn observe(
        &mut self,
        id: usize,
        raw: &RawState,
        mapped: &GamepadSeatInput,
        edge: Option<bool>,
        now: Instant,
    ) {
        if !self.visible {
            return;
        }
        if matches!(self.page, Page::Identify(_)) && edge == Some(true) {
            self.change_page(Page::Device(id));
            self.status = "Controller identified. Release the button to navigate.".into();
            return;
        }
        if self.page.device() != Some(id) {
            return;
        }
        if matches!(self.page, Page::Test { .. }) && !self.test_armed {
            self.test_armed = raw.neutral();
            self.live = if self.test_armed {
                live_text(mapped, 0, 0)
            } else {
                "Release all controls to start testing.".into()
            };
            return;
        }
        match edge {
            Some(true) => self.presses = self.presses.saturating_add(1),
            Some(false) => self.releases = self.releases.saturating_add(1),
            None => {}
        }
        self.live = live_text(mapped, self.presses, self.releases);
        if let Page::Capture {
            capture, deadline, ..
        } = &mut self.page
        {
            let previous = capture.index;
            capture.update(raw, now);
            if capture.cancelled {
                self.change_page(Page::Device(id));
                self.status = "Setup cancelled; previous mapping is unchanged.".into();
                return;
            }
            if capture.index != previous {
                *deadline = now + CAPTURE_TIME;
            }
            if capture.target().is_none() {
                let device = &self.devices[&id];
                let profile = ControllerProfile {
                    device_key: device.key.clone(),
                    name: device.name.clone(),
                    bindings: capture.bindings.clone(),
                };
                if profile.valid() {
                    self.change_page(Page::Review { id, profile });
                } else {
                    self.change_page(Page::Device(id));
                    self.status = "Invalid draft discarded; previous mapping is unchanged.".into();
                }
            }
        } else if matches!(self.page, Page::Test { .. }) {
            if mapped.east {
                let since = *self.test_back_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_secs(2) {
                    self.change_page(Page::Device(id));
                }
            } else {
                self.test_back_since = None;
            }
        }
    }

    pub fn tick(&mut self, window: &MainWindow, now: Instant) {
        if !self.visible {
            return;
        }
        self.expire(now);
        if self
            .last_publish
            .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(100))
        {
            self.publish(window, now);
        }
    }

    fn expire(&mut self, now: Instant) {
        let expired = match &self.page {
            Page::Identify(deadline) if now >= *deadline => Some(Page::Devices),
            Page::Capture { id, deadline, .. }
            | Page::Trial { id, deadline, .. }
            | Page::Test { id, deadline }
                if now >= *deadline =>
            {
                Some(Page::Device(*id))
            }
            _ => None,
        };
        if let Some(page) = expired {
            self.change_page(page);
            self.status = "Setup timed out. Your saved mapping is unchanged.".into();
        }
    }

    fn command(&mut self, window: &MainWindow, command: &str, now: Instant) {
        // Expire before processing commands, even if the UI timer is delayed.
        self.expire(now);
        if !self.rows().iter().any(|row| row.id == command) {
            return;
        }
        self.status.clear();
        if command == "controllers.back" {
            match self.page {
                Page::Devices => {
                    self.visible = false;
                    self.change_page(Page::Devices);
                    window.set_controllers_visible(false);
                    window.set_sound_focus_index(7);
                }
                Page::Device(_) | Page::Identify(_) => self.change_page(Page::Devices),
                _ => self.change_page(Page::Device(self.page.device().unwrap())),
            }
        } else if command == "controllers.identify" {
            self.change_page(Page::Identify(now + Duration::from_secs(20)));
        } else if command == "controllers.reset-players" {
            self.assignments.reset();
            self.assignments_changed();
            self.save_preferences(window, [None, None]);
            self.status = "Player preferences cleared. First two available controllers assigned; release controls to continue.".into();
        } else if let Some(id) = command
            .strip_prefix("controllers.device.")
            .and_then(|id| id.parse().ok())
        {
            if self.devices.contains_key(&id) {
                self.change_page(Page::Device(id));
            }
        } else if let Some(id) = self.page.device() {
            match command {
                "controllers.assign-p1" | "controllers.assign-p2" => {
                    let seat = usize::from(command == "controllers.assign-p2");
                    if self.assignments.assign(id, seat) {
                        self.assignments_changed();
                        self.save_preferences(window, self.assignments.preferences());
                        self.status = if self.assignments.model_is_ambiguous(id) {
                            format!(
                                "Assigned P{}. Identical model: session only; choose again after reconnect. Release controls to continue.",
                                seat + 1
                            )
                        } else {
                            format!(
                                "Assigned P{}. Saving player preferences. Release controls to continue.",
                                seat + 1
                            )
                        };
                    }
                }
                "controllers.remap" => {
                    if self.profiles.len() >= MAX_PROFILES
                        && !self.profiles.contains_key(&self.devices[&id].key)
                    {
                        self.status =
                            "Profile limit reached. Restore defaults on an unused profile first."
                                .into();
                    } else {
                        self.change_page(Page::Capture {
                            id,
                            capture: Capture::new(),
                            deadline: now + CAPTURE_TIME,
                        });
                    }
                }
                "controllers.skip" => {
                    if let Page::Capture {
                        capture, deadline, ..
                    } = &mut self.page
                    {
                        capture.skip();
                        *deadline = now + CAPTURE_TIME;
                    }
                }
                "controllers.test" => {
                    self.presses = 0;
                    self.releases = 0;
                    self.live.clear();
                    self.change_page(Page::Test {
                        id,
                        deadline: now + TEST_TIME,
                    });
                }
                "controllers.defaults" => self.change_page(Page::Trial {
                    id,
                    profile: None,
                    deadline: now + TRIAL_TIME,
                }),
                "controllers.try" => {
                    if let Page::Review { profile, .. } = &self.page {
                        self.change_page(Page::Trial {
                            id,
                            profile: Some(profile.clone()),
                            deadline: now + TRIAL_TIME,
                        });
                    }
                }
                "controllers.keep" => {
                    if let Page::Trial { profile, .. } = &self.page {
                        let key = self.devices[&id].key.clone();
                        if let Some(profile) = profile {
                            self.profiles.insert(key, profile.clone());
                        } else {
                            self.profiles.remove(&key);
                        }
                        let snapshot = {
                            let mut settings = self.settings.write().unwrap();
                            settings.controls.controller_profiles =
                                self.profiles.values().cloned().collect();
                            settings.clone()
                        };
                        self.writer.save(snapshot);
                        window.set_settings_save_pending(true);
                        window.set_settings_save_error("".into());
                        self.change_page(Page::Device(id));
                        self.status = "Mapping accepted. Saving locally…".into();
                    }
                }
                _ => {}
            }
        }
        window.set_controllers_focus_index(0);
        self.publish(window, now);
    }

    fn rows(&self) -> Vec<ControllerRow> {
        let mut rows = Vec::new();
        let mut add = |id: &str, label: &str| {
            rows.push(ControllerRow {
                id: id.into(),
                label: label.into(),
            })
        };
        match &self.page {
            Page::Devices => {
                for device in self.devices.values() {
                    add(
                        &format!("controllers.device.{}", device.id),
                        &format!("{} · {}", seat_label(device), device.name),
                    );
                }
                if !self.devices.is_empty() {
                    add("controllers.identify", "Identify by pressing a button");
                }
                add("controllers.reset-players", "Reset player assignments");
            }
            Page::Device(_) => {
                add("controllers.assign-p1", "Use as Player 1");
                add("controllers.assign-p2", "Use as Player 2");
                add("controllers.remap", "Set up mapping…");
                add("controllers.test", "Test buttons and joystick");
                add("controllers.defaults", "Try default mapping…");
            }
            Page::Capture { capture, .. } => {
                if capture.target().is_some_and(|target| !target.required()) {
                    add("controllers.skip", "Skip this optional button");
                }
            }
            Page::Review { .. } => add("controllers.try", "Try this mapping (15 seconds)"),
            Page::Trial { .. } => add("controllers.keep", "Keep and save mapping"),
            Page::Identify(_) | Page::Test { .. } => {}
        }
        add(
            "controllers.back",
            match self.page {
                Page::Capture { .. } | Page::Review { .. } | Page::Identify(_) => "Cancel",
                Page::Trial { .. } => "Revert",
                _ => "Back",
            },
        );
        rows
    }

    fn publish(&mut self, window: &MainWindow, now: Instant) {
        self.last_publish = Some(now);
        let device = self.page.device().and_then(|id| self.devices.get(&id));
        window.set_controllers_subtitle(
            device
                .map_or_else(
                    || "Player assignments · button mappings".into(),
                    |device| format!("{} · {}", seat_label(device), device.name),
                )
                .into(),
        );
        window.set_controllers_hint(
            match self.page {
                Page::Capture { .. } => "Press and release · Esc / touch Cancel",
                Page::Identify(_) => "Press a controller button · Esc cancels",
                Page::Trial { .. } => "New A saves · B reverts · Timeout restores",
                Page::Test { .. } => "Hold B for 2s to return · Esc / touch Back",
                _ => "Up / Down select · A confirm · B back",
            }
            .into(),
        );
        window.set_controllers_assignments(self.player_summary().into());
        let remaining = |deadline: Instant| {
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .div_ceil(1000)
        };
        let text = match &self.page {
            Page::Devices => {
                let hint = if self.devices.is_empty() {
                    "Connect a controller. Keyboard and touch still work."
                } else {
                    "Choose or identify a controller. Either controller can navigate menus."
                };
                format!("{}\n\n{hint}", self.player_summary())
            }
            Page::Device(id) => {
                let customized = self.profile(*id).is_some();
                format!(
                    "{} mapping for all games and menus. Player choices swap occupied slots.\n{}\nPicade Esc/Enter utility keys stay unchanged.",
                    if customized { "Custom" } else { "Default" },
                    if self.assignments.model_is_ambiguous(*id) {
                        "Identical models: assignments are session-only; choose again after reconnect."
                    } else {
                        "Distinct device models are remembered. Missing controllers keep their slots."
                    }
                )
            }
            Page::Identify(deadline) => format!(
                "Press a button on the controller to configure.\nReturns in {}s.",
                remaining(*deadline)
            ),
            Page::Capture {
                capture, deadline, ..
            } => {
                let target = capture.target();
                format!(
                    "{} / {}: {}\nPress, then release. {}\n{}\n{}s before cancel.",
                    capture.index + 1,
                    ControllerControl::ALL.len(),
                    target.map_or("Finishing…", |t| t.label()),
                    if target.is_some_and(|t| t.required()) {
                        "Required. Hold 2s to cancel."
                    } else {
                        "No such button? Hold ANY button for 2s to skip."
                    },
                    if capture.message.is_empty() {
                        "Release all controls before each prompt."
                    } else {
                        &capture.message
                    },
                    remaining(*deadline)
                )
            }
            Page::Review { profile, .. } => format!(
                "{} controls assigned. Others are disabled. Nothing saved yet.\nTry the new mapping, then press its A / South to keep it, or B / East to revert. Doing nothing restores the old mapping after 15 seconds.",
                profile.bindings.len()
            ),
            Page::Trial { deadline, .. } => format!(
                "Trying mapping — {}s to confirm.\nUse NEW A / South to save, B / East to revert.\n{}",
                remaining(*deadline),
                self.live
            ),
            Page::Test { deadline, .. } => format!(
                "{}\nHold B / East for 2s to return (or Esc / touch Back).\nAuto-return in {}s.",
                self.live,
                remaining(*deadline)
            ),
        };
        window.set_controllers_detail(text.into());
        window.set_controllers_status(self.status.clone().into());
        let rows = self.rows();
        let old = window.get_controllers_rows();
        if old.row_count() != rows.len() || old.iter().zip(&rows).any(|(a, b)| a != *b) {
            window.set_controllers_rows(ModelRc::new(VecModel::from(rows)));
            window.set_controllers_focus_index(0);
        }
    }
}

fn seat_label(device: &Device) -> String {
    device
        .seat
        .map_or_else(|| "Unassigned".into(), |seat| format!("P{}", seat + 1))
}

fn live_text(pad: &GamepadSeatInput, presses: u64, releases: u64) -> String {
    let buttons = [
        (pad.dpad_up, "Up"),
        (pad.dpad_down, "Down"),
        (pad.dpad_left, "Left"),
        (pad.dpad_right, "Right"),
        (pad.south, "A / South"),
        (pad.east, "B / East"),
        (pad.west, "X / West"),
        (pad.north, "Y / North"),
        (pad.left_bumper, "L1"),
        (pad.right_bumper, "R1"),
        (pad.left_trigger >= 0.5, "L2"),
        (pad.right_trigger >= 0.5, "R2"),
        (pad.start, "Start"),
        (pad.select, "Select"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect::<Vec<_>>()
    .join(" · ");
    format!(
        "Held: {}\nLeft stick {:.2}, {:.2} · Right {:.2}, {:.2}\nPresses {presses} · Releases {releases}",
        if buttons.is_empty() { "none" } else { &buttons },
        pad.left_stick_x,
        pad.left_stick_y,
        pad.right_stick_x,
        pad.right_stick_y
    )
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    let rows = window.get_controllers_rows();
    let count = rows.row_count() as i32;
    let index = window.get_controllers_focus_index();
    match action {
        UiAction::Up | UiAction::Down => {
            window.set_controllers_focus_index(crate::ui_navigation::moved_selection(
                index,
                count,
                if action == UiAction::Up { -1 } else { 1 },
            ))
        }
        UiAction::Confirm => {
            if let Some(row) = rows.row_data(index as usize) {
                window.invoke_controllers_command(row.id);
            }
        }
        UiAction::Back | UiAction::Controls => {
            window.invoke_controllers_command("controllers.back".into())
        }
        _ => {}
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    let mut result = window
        .get_controllers_rows()
        .iter()
        .map(|row| UiControl::new(row.id.as_str(), row.label.as_str(), true))
        .collect::<Vec<_>>();
    result.push(
        UiControl::new("controllers.players", "Player assignments", false)
            .with_value(window.get_controllers_assignments().to_string()),
    );
    result.push(
        UiControl::new("controllers.detail", "Controller status", false)
            .with_value(window.get_controllers_detail().to_string()),
    );
    result.push(
        UiControl::new("controllers.status", "Setup message", false)
            .with_value(window.get_controllers_status().to_string()),
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{ControllerBinding, ControllerSource};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    struct TestPlatform;
    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
        }
    }

    fn device(id: usize) -> Device {
        Device {
            id,
            key: "gilrs-v1:linux:picade".into(),
            name: "Space-Wars Picade".into(),
            seat: Some(id % 2),
        }
    }

    fn profile(offset: u32) -> ControllerProfile {
        ControllerProfile {
            device_key: device(0).key,
            name: device(0).name,
            bindings: ControllerControl::ALL
                .into_iter()
                .enumerate()
                .map(|(i, control)| ControllerBinding {
                    control,
                    source: ControllerSource::Button {
                        code: offset + i as u32,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn setup_trial_cancel_disconnect_and_persistence_use_real_menu_callbacks() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        window.set_launcher_visible(true);
        window.set_sound_visible(true);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        let settings = Arc::new(RwLock::new(Settings::default()));
        let writer = SettingsWriter::new(path.clone()).unwrap();
        let state = install(
            &window,
            Arc::clone(&settings),
            writer.clone(),
            crate::input::new_shared_input().1,
        );
        state.borrow_mut().connect(device(0));
        state.borrow_mut().connect(device(1)); // Identical models, not seat keys.
        let mut other = device(2);
        other.key = "gilrs-v1:linux:other".into();
        state.borrow_mut().connect(other);
        window.invoke_controllers_open();
        window.invoke_controllers_command("controllers.device.0".into());
        window.invoke_controllers_command("controllers.remap".into());
        let now = Instant::now();
        let neutral = RawState::default();
        let pad = GamepadSeatInput::default();
        for (index, _) in ControllerControl::ALL.iter().enumerate() {
            let raw = RawState {
                buttons: vec![(index as u32, 1.0)],
                ..Default::default()
            };
            let mut state = state.borrow_mut();
            // Other controller presses must not advance capture.
            state.observe(1, &raw, &pad, Some(true), now);
            state.observe(0, &neutral, &pad, None, now);
            state.observe(0, &raw, &pad, Some(true), now);
            state.observe(0, &neutral, &pad, Some(false), now);
        }
        assert!(matches!(state.borrow().page, Page::Review { .. }));
        assert!(
            settings
                .read()
                .unwrap()
                .controls
                .controller_profiles
                .is_empty()
        );
        assert!(!path.exists(), "draft must never reach disk");
        window.invoke_controllers_command("controllers.try".into());
        assert_eq!(state.borrow().profile(0), Some(&profile(0)));
        assert_eq!(state.borrow().profile(1), Some(&profile(0)));
        assert!(state.borrow().profile(2).is_none());
        // Explicit monotonic time: no sleeps or timing-sensitive assertions.
        state.borrow_mut().command(
            &window,
            "controllers.keep",
            now + TRIAL_TIME + Duration::from_secs(1),
        );
        assert!(state.borrow().profile(0).is_none());
        assert!(!path.exists());

        state.borrow_mut().change_page(Page::Trial {
            id: 0,
            profile: Some(profile(100)),
            deadline: now + TRIAL_TIME,
        });
        window.invoke_controllers_command("controllers.keep".into());
        assert_eq!(state.borrow().profile(1), Some(&profile(100)));
        // Ordered writer barrier: verify the saved document, not wall timing.
        writer
            .save_blocking(settings.read().unwrap().clone())
            .unwrap();
        let stored = crate::settings::load_settings(&path).unwrap().settings;
        assert_eq!(stored.controls.controller_profiles, vec![profile(100)]);

        window.invoke_controllers_command("controllers.defaults".into());
        assert!(state.borrow().profile(0).is_none());
        window.invoke_controllers_command("controllers.back".into());
        assert_eq!(state.borrow().profile(0), Some(&profile(100)));
        window.invoke_controllers_command("controllers.defaults".into());
        state.borrow_mut().disconnect(0);
        assert_eq!(
            state.borrow().profile(1),
            Some(&profile(100)),
            "disconnect rolls back for all identical models"
        );
        state.borrow_mut().connect(device(8)); // Connection order changed.
        assert_eq!(state.borrow().profile(8), Some(&profile(100)));

        state.borrow_mut().change_page(Page::Trial {
            id: 8,
            profile: Some(profile(200)),
            deadline: now + TRIAL_TIME,
        });
        window.set_controllers_visible(false);
        window.invoke_controllers_visibility_changed();
        assert_eq!(
            state.borrow().profile(8),
            Some(&profile(100)),
            "parent menu close rolls back"
        );

        let reloaded = install(
            &window,
            Arc::new(RwLock::new(stored)),
            writer,
            crate::input::new_shared_input().1,
        );
        reloaded.borrow_mut().connect(device(12));
        assert_eq!(
            reloaded.borrow().profile(12),
            Some(&profile(100)),
            "saved mappings survive process-style reload"
        );
        window.invoke_controllers_open();
        window.invoke_controllers_command("controllers.device.12".into());
        window.invoke_controllers_command("controllers.defaults".into());
        window.invoke_controllers_command("controllers.keep".into());
        assert!(
            reloaded.borrow().profiles.is_empty(),
            "confirmed defaults remove the profile"
        );
    }

    #[test]
    fn identity_does_not_depend_on_seat_and_invalid_profiles_are_ignored() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        let mut invalid = profile(0);
        invalid.bindings[1].source = invalid.bindings[0].source;
        settings.controls.controller_profiles = vec![invalid, profile(100), profile(200)];
        let state = install(
            &window,
            Arc::new(RwLock::new(settings)),
            SettingsWriter::new(directory.path().join("settings.toml")).unwrap(),
            crate::input::new_shared_input().1,
        );
        state.borrow_mut().connect(device(15));
        assert_eq!(state.borrow().profile(15), Some(&profile(100)));
        assert!(state.borrow().status.contains("2 invalid or duplicate"));
    }

    #[test]
    fn identify_and_tester_ignore_entry_holds_and_other_devices() {
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        window.set_launcher_visible(true);
        window.set_sound_visible(true);
        let directory = tempfile::tempdir().unwrap();
        let state = install(
            &window,
            Arc::new(RwLock::new(Settings::default())),
            SettingsWriter::new(directory.path().join("settings.toml")).unwrap(),
            crate::input::new_shared_input().1,
        );
        state.borrow_mut().connect(device(0));
        state.borrow_mut().connect(device(1));
        window.invoke_controllers_open();
        window.invoke_controllers_command("controllers.identify".into());
        let now = Instant::now();
        let raw = RawState {
            buttons: vec![(1, 1.0)],
            ..Default::default()
        };
        let held = GamepadSeatInput {
            east: true,
            ..Default::default()
        };
        state.borrow_mut().observe(1, &raw, &held, Some(true), now);
        assert!(matches!(state.borrow().page, Page::Device(1)));
        window.invoke_controllers_command("controllers.test".into());
        state.borrow_mut().observe(1, &raw, &held, None, now);
        state.borrow_mut().observe(
            1,
            &RawState::default(),
            &GamepadSeatInput::default(),
            Some(false),
            now,
        );
        assert_eq!(state.borrow().presses, 0);
        assert_eq!(
            state.borrow().releases,
            0,
            "opening button's release is not a test edge"
        );
        state.borrow_mut().observe(0, &raw, &held, Some(true), now);
        assert_eq!(state.borrow().presses, 0);
        state.borrow_mut().observe(1, &raw, &held, Some(true), now);
        state.borrow_mut().observe(
            1,
            &RawState::default(),
            &GamepadSeatInput::default(),
            Some(false),
            now + Duration::from_millis(100),
        );
        assert_eq!(state.borrow().presses, 1);
        assert_eq!(state.borrow().releases, 1);
        state.borrow_mut().observe(1, &raw, &held, Some(true), now);
        state
            .borrow_mut()
            .observe(1, &raw, &held, None, now + Duration::from_secs(2));
        assert!(matches!(state.borrow().page, Page::Device(1)));
    }
}
