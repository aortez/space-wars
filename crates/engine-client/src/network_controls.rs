//! Controller, touch and keyboard frontend. Credentials reach Slint only while
//! explicitly revealed; the control inventory always receives a mask.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use spacewars_control::{UiAction, UiControl};

use crate::{
    MainWindow, NetworkRow,
    network::{Command, Network, Phase, ProfileChange, SavedNetwork, Security, Session, View},
};

#[derive(Default, PartialEq, Eq)]
enum Page {
    #[default]
    List,
    Saved,
    Profile(String),
    Forget {
        id: String,
        allow_active: bool,
    },
    Selected(String),
    Password(String),
}

struct Panel {
    session: Session,
    page: Page,
    password: String,
    alphabet: usize,
    message: String,
}

fn alphabet(index: usize) -> &'static str {
    match index {
        1 => "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.@",
        2 => "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~01234567",
        _ => "abcdefghijklmnopqrstuvwxyz0123456789-_.@",
    }
}

impl Panel {
    fn new(session: Session) -> Self {
        Self {
            session,
            page: Page::List,
            password: String::new(),
            alphabet: 0,
            message: String::new(),
        }
    }

    fn network<'a>(&self, view: &'a View) -> Option<&'a Network> {
        let id = match &self.page {
            Page::Selected(id) | Page::Password(id) => id,
            _ => return None,
        };
        view.inventory.networks.iter().find(|n| &n.id == id)
    }

    fn profile<'a>(&self, view: &'a View) -> Option<&'a SavedNetwork> {
        let id = match &self.page {
            Page::Profile(id) | Page::Forget { id, .. } => id,
            _ => return None,
        };
        view.inventory.profiles.iter().find(|p| &p.id == id)
    }

    fn rows(&self, view: &View) -> Vec<NetworkRow> {
        let mut rows = Vec::new();
        let mut add = |id: &str, label: &str, enabled| {
            rows.push(NetworkRow {
                id: id.into(),
                label: label.into(),
                enabled,
                ..NetworkRow::default()
            })
        };
        if view.phase == Phase::Confirm {
            add("network.keep", "Keep this network", true);
            add("network.cancel", "Restore previous network", true);
        } else if view.phase.can_cancel() {
            add("network.cancel", "Cancel / restore", true);
        } else if !view.phase.is_trial() {
            match &self.page {
                Page::List => {
                    add(
                        "network.scan",
                        if view.phase == Phase::Discovering {
                            "Scanning…"
                        } else {
                            "Rescan"
                        },
                        view.phase == Phase::Idle,
                    );
                    add("network.saved", "Saved networks…", true);
                    for network in &view.inventory.networks {
                        add(
                            &format!("network.select.{}", network.id),
                            &network.name,
                            true,
                        );
                    }
                }
                Page::Saved => {
                    add(
                        "network.refresh",
                        "Refresh saved networks",
                        view.phase == Phase::Idle,
                    );
                    for profile in &view.inventory.profiles {
                        add(
                            &format!("network.profile.{}", profile.id),
                            &profile.ssid_name,
                            true,
                        );
                    }
                }
                Page::Profile(_) => {
                    if let Some(profile) = self.profile(view) {
                        let idle = view.phase == Phase::Idle;
                        add(
                            "network.profile-connect",
                            "Connect using this profile",
                            idle && view.inventory.can_connect
                                && !profile.connected
                                && profile
                                    .network
                                    .as_ref()
                                    .is_some_and(|n| n.security != Security::Unsupported),
                        );
                        add(
                            "network.autoconnect",
                            if profile.autoconnect {
                                "Connect automatically: On"
                            } else {
                                "Connect automatically: Off"
                            },
                            idle && view.inventory.can_manage,
                        );
                        add(
                            "network.prefer",
                            if view.inventory.preferred(profile) {
                                "Preferred network"
                            } else {
                                "Prefer this network"
                            },
                            idle && view.inventory.can_manage
                                && profile.autoconnect
                                && !view.inventory.preferred(profile),
                        );
                        add(
                            "network.forget",
                            "Forget network…",
                            idle && view.inventory.can_manage,
                        );
                    }
                }
                Page::Forget { .. } => {
                    add("network.back", "Cancel", true);
                    add(
                        "network.forget-confirm",
                        "Forget this saved profile",
                        view.phase == Phase::Idle
                            && view.inventory.can_manage
                            && self.profile(view).is_some(),
                    );
                    return rows;
                }
                Page::Selected(_) => {
                    if let Some(network) = self.network(view) {
                        let enabled = view.inventory.can_connect
                            && view.phase == Phase::Idle
                            && !network.connected
                            && network.security != Security::Unsupported;
                        if network.saved.is_some() {
                            add(
                                "network.connect-saved",
                                "Connect using saved settings",
                                enabled,
                            );
                        }
                        if network.security == Security::Open {
                            add("network.connect-open", "Connect to open network", enabled);
                        } else {
                            add("network.password", "Enter password…", enabled);
                        }
                    }
                }
                Page::Password(_) => {
                    for (index, c) in alphabet(self.alphabet).chars().enumerate() {
                        add(&format!("network.key.{index}"), &c.to_string(), true);
                    }
                    add("network.case", "a / A", true);
                    add("network.symbols", "! # ?", true);
                    add("network.space", "Space", true);
                    add("network.erase", "Erase", true);
                    add("network.connect-password", "Connect", true);
                    add("network.reveal", "Show password", true);
                    add("network.back", "Cancel", true);
                    return rows;
                }
            }
        }
        add("network.back", "Back", true);
        for row in &mut rows {
            if let Some(id) = row.id.strip_prefix("network.profile.")
                && let Some(profile) = view.inventory.profiles.iter().find(|p| p.id == id)
            {
                row.detail = format!(
                    "{} · {}",
                    profile.id,
                    if profile.network.is_some() {
                        "In range"
                    } else {
                        "Out of range"
                    }
                )
                .into();
                row.badge = match (
                    profile.connected,
                    view.inventory.preferred(profile),
                    profile.autoconnect,
                ) {
                    (true, true, _) => "Preferred",
                    (true, false, _) => "Active",
                    (false, true, _) => "Preferred",
                    (_, _, false) => "Manual only",
                    _ => "Automatic",
                }
                .into();
            }
            if let Some(id) = row.id.strip_prefix("network.select.")
                && let Some(network) = view.inventory.networks.iter().find(|n| n.id == id)
            {
                row.detail = format!(
                    "{} · Signal {}% · {}",
                    network.security.label(),
                    network.strength,
                    network.interface
                )
                .into();
                row.badge = if network.connected {
                    "Connected"
                } else if network.saved.is_some() {
                    "Saved"
                } else {
                    ""
                }
                .into();
            }
        }
        rows
    }

    fn publish(&mut self, window: &MainWindow) {
        let view = self.session.snapshot();
        if view.phase.is_trial() {
            self.password.clear();
            self.page = Page::List;
        } else if matches!(self.page, Page::Selected(_) | Page::Password(_))
            && self.network(&view).is_none()
        {
            self.password.clear();
            self.page = Page::List;
            self.message =
                "Selected network is no longer visible. Scan and select it again.".into();
        }
        if matches!(self.page, Page::Profile(_) | Page::Forget { .. })
            && self.profile(&view).is_none()
        {
            self.page = Page::Saved;
            self.message =
                "Saved profile is no longer available. Refresh to check the list.".into();
        }
        let active = self.profile(&view).is_some_and(|p| p.connected);
        if let Page::Forget { allow_active, .. } = &mut self.page
            && active
            && !*allow_active
        {
            *allow_active = true;
            // A new active-network warning requires a deliberate second choice.
            window.set_network_focus_index(0);
        }
        window.set_network_forget_confirmation(matches!(self.page, Page::Forget { .. }));
        window.set_network_profile_visible(matches!(self.page, Page::Profile(_)));
        let password_page = matches!(self.page, Page::Password(_));
        if !password_page {
            window.set_network_password_revealed(false);
        }
        window.set_network_password_text(
            if password_page && window.get_network_password_revealed() {
                self.password.as_str().into()
            } else {
                "".into()
            },
        );
        window.set_network_password_mask("•".repeat(self.password.len()).into());
        let count = format!("{} characters", self.password.len());
        window.set_network_password_caption(if self.message.is_empty() {
            count.into()
        } else {
            format!("{count} · {}", self.message).into()
        });
        window.set_network_password_visible(password_page);
        window.set_network_title(
            if password_page {
                "Wi-Fi Password"
            } else if matches!(self.page, Page::Forget { .. }) {
                "Forget Network?"
            } else if matches!(self.page, Page::Saved | Page::Profile(_)) {
                "Saved Networks"
            } else {
                "Network"
            }
            .into(),
        );
        let subtitle = if let Some(profile) = self.profile(&view) {
            format!(
                "{} · {}",
                profile.ssid_name,
                if profile.connected {
                    "Active connection"
                } else if profile.network.is_some() {
                    "In range"
                } else {
                    "Out of range"
                }
            )
        } else if self.page == Page::Saved {
            format!(
                "{} saved profiles · including networks out of range",
                view.inventory.profiles.len()
            )
        } else {
            self.network(&view).map_or_else(
                || view.inventory.summary.clone(),
                |n| format!("{} · {} · {}", n.name, n.interface, n.security.label()),
            )
        };
        window.set_network_subtitle(match view.phase {
            Phase::Confirm => "Temporary connection · waiting for confirmation".into(),
            Phase::Connecting => "Trying a connection · previous network protected".into(),
            Phase::Saving => "Saving your confirmed connection".into(),
            Phase::Restoring => "Restoring the previous network".into(),
            Phase::Managing => "Saving network preferences".into(),
            Phase::Idle | Phase::Discovering => subtitle.into(),
        });
        let detail = if password_page {
            let mask = "•".repeat(self.password.len().min(24));
            format!(
                "{}  ({} characters)\n{}",
                mask,
                self.password.len(),
                self.message
            )
        } else if let Page::Forget { allow_active, .. } = self.page {
            let profile = self.profile(&view).unwrap();
            format!(
                "Profile: {}\nID: {}\n{}",
                profile.name.chars().take(48).collect::<String>(),
                profile.id,
                if allow_active {
                    "This profile is active. Forgetting it may disconnect Wi-Fi and end remote access. Reconnecting may need its password."
                } else {
                    "Remove this saved profile and its credentials? You can add the network again later."
                }
            )
        } else if let Some(error) = &view.inventory_error {
            if view.status.is_empty() {
                error.clone()
            } else {
                format!("{}\n{error}", view.status)
            }
        } else if !self.message.is_empty() {
            self.message.clone()
        } else if let Some(profile) = self.profile(&view) {
            format!(
                "Profile: {} · ID: {}\n{}",
                profile.name.chars().take(48).collect::<String>(),
                profile.id,
                if view.phase == Phase::Managing || !view.status.is_empty() {
                    view.status.as_str()
                } else if !view.inventory.can_manage {
                    "Saved-network changes are unavailable. Refresh and check system permissions."
                } else {
                    "Preferences apply to future automatic connections. Connect now is separate."
                }
            )
        } else if self.page == Page::Saved && view.status.is_empty() {
            if view.inventory.can_manage {
                "Choose a saved profile to connect, change automatic connection, or forget it."
            } else {
                "Saved-network changes are unavailable. Refresh and check system permissions."
            }
            .into()
        } else if view.status.is_empty() {
            "Choose a nearby network. New connections are tried temporarily until you choose Keep. Open networks are not encrypted.".into()
        } else {
            view.status.clone()
        };
        window.set_network_detail(detail.into());
        let mut rows = self.rows(&view);
        if window.get_network_password_revealed()
            && let Some(row) = rows.iter_mut().find(|r| r.id == "network.reveal")
        {
            row.label = "Hide password".into();
        }
        let old = window.get_network_rows();
        if !old.iter().eq(rows.iter().cloned()) {
            let selected = old
                .row_data(window.get_network_focus_index().max(0) as usize)
                .map(|r| r.id);
            let focus = selected
                .and_then(|id| rows.iter().position(|r| r.id == id && r.enabled))
                .unwrap_or_else(|| rows.iter().position(|r| r.enabled).unwrap_or(0));
            window.set_network_rows(ModelRc::new(VecModel::from(rows)));
            window.set_network_focus_index(focus as i32);
        }
    }

    fn command(&mut self, window: &MainWindow, id: &str) {
        let view = self.session.snapshot();
        if !self
            .rows(&view)
            .iter()
            .any(|row| row.id == id && row.enabled)
        {
            return;
        }
        self.message.clear();
        if id == "network.back" {
            window.set_network_password_revealed(false);
            self.password.clear();
            self.page = match &self.page {
                Page::Password(id) => Page::Selected(id.clone()),
                Page::Selected(_) | Page::Saved => Page::List,
                Page::Profile(_) => Page::Saved,
                Page::Forget { id, .. } => Page::Profile(id.clone()),
                Page::List => {
                    window.set_network_visible(false);
                    return;
                }
            };
        } else if id == "network.saved" {
            self.page = Page::Saved;
        } else if let Some(id) = id.strip_prefix("network.profile.") {
            self.page = Page::Profile(id.into());
        } else if id == "network.forget" {
            if let Some(profile) = self.profile(&view) {
                self.page = Page::Forget {
                    id: profile.id.clone(),
                    allow_active: profile.connected,
                };
            }
        } else if let Some(id) = id.strip_prefix("network.select.") {
            self.page = Page::Selected(id.into());
        } else if id == "network.password" {
            if let Page::Selected(id) = &self.page {
                self.page = Page::Password(id.clone());
                self.password.clear();
                window.set_network_password_revealed(false);
            }
        } else if id == "network.reveal" {
            window.set_network_password_revealed(!window.get_network_password_revealed());
        } else if id == "network.case" {
            self.alphabet = usize::from(self.alphabet != 1);
        } else if id == "network.symbols" {
            self.alphabet = if self.alphabet == 2 { 0 } else { 2 };
        } else if id == "network.erase" {
            self.password.pop();
        } else if id == "network.space" {
            self.type_text(" ");
        } else if let Some(index) = id
            .strip_prefix("network.key.")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(c) = alphabet(self.alphabet).chars().nth(index) {
                self.type_text(&c.to_string());
            }
        } else {
            let command = match id {
                "network.scan" => Command::Scan,
                "network.refresh" => Command::Refresh,
                "network.profile-connect" => {
                    let Some(profile) = self.profile(&view) else {
                        return;
                    };
                    Command::ConnectProfile {
                        id: profile.id.clone(),
                    }
                }
                "network.autoconnect" | "network.prefer" | "network.forget-confirm" => {
                    let Some(profile) = self.profile(&view) else {
                        return;
                    };
                    let change = match id {
                        "network.autoconnect" => ProfileChange::Autoconnect(!profile.autoconnect),
                        "network.prefer" => ProfileChange::Prefer,
                        _ => {
                            let Page::Forget { allow_active, .. } = self.page else {
                                return;
                            };
                            ProfileChange::Forget { allow_active }
                        }
                    };
                    Command::Manage {
                        id: profile.id.clone(),
                        change,
                    }
                }
                "network.keep" => Command::Keep,
                "network.cancel" => Command::Cancel,
                "network.connect-saved" | "network.connect-open" | "network.connect-password" => {
                    if view.phase != Phase::Idle || !view.inventory.can_connect {
                        self.message = "Wait for network discovery, then try Connect again.".into();
                        self.publish(window);
                        return;
                    }
                    let Some(network) = self.network(&view) else {
                        return;
                    };
                    if id == "network.connect-password"
                        && !network.security.valid_password(&self.password)
                    {
                        self.message = if network.security == Security::Sae {
                            "Use 1–63 printable characters."
                        } else {
                            "Use 8–63 printable characters, or 64 hex digits."
                        }
                        .into();
                        self.publish(window);
                        return;
                    }
                    Command::Connect {
                        id: network.id.clone(),
                        password: if id == "network.connect-password" {
                            Some(std::mem::take(&mut self.password))
                        } else if id == "network.connect-open" {
                            Some(String::new())
                        } else {
                            None
                        },
                    }
                }
                _ => return,
            };
            let next = match &command {
                Command::Manage { id, change } => {
                    if matches!(change, ProfileChange::Forget { .. }) {
                        Page::Saved
                    } else {
                        Page::Profile(id.clone())
                    }
                }
                Command::Refresh => Page::Saved,
                _ => Page::List,
            };
            self.session.send(command);
            self.page = next;
        }
        self.publish(window);
    }

    fn type_text(&mut self, text: &str) {
        if !matches!(self.page, Page::Password(_)) {
            return;
        }
        for c in text.chars().filter(|c| c.is_ascii() && !c.is_control()) {
            if self.password.len() < 64 {
                self.password.push(c);
            }
        }
    }
}

pub(crate) fn install(window: &MainWindow) {
    install_with(window, Session::new);
}

fn install_with(
    window: &MainWindow,
    make_session: impl Fn() -> Session + 'static,
) -> Rc<RefCell<Option<Panel>>> {
    let state = Rc::new(RefCell::new(None::<Panel>));
    let weak = window.as_weak();
    window.on_network_open(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !window.get_sound_visible()
            || window.get_launcher_busy()
            || (!window.get_launcher_visible() && !window.get_ingame_menu_visible())
        {
            return;
        }
        window.set_sound_focus_index(8);
        window.set_network_focus_index(0);
        window.set_network_visible(true);
    });
    let weak = window.as_weak();
    let panel = Rc::clone(&state);
    window.on_network_command(move |id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !window.get_network_visible() {
            return;
        }
        if let Some(panel) = panel.borrow_mut().as_mut() {
            panel.command(&window, id.as_str());
        }
    });
    let weak = window.as_weak();
    let panel = Rc::clone(&state);
    window.on_network_text(move |text| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !window.get_network_visible() || !window.get_network_password_visible() {
            return;
        }
        if let Some(panel) = panel.borrow_mut().as_mut() {
            panel.type_text(text.as_str());
            panel.publish(&window);
        }
    });
    let weak = window.as_weak();
    let timer = Timer::default();
    let panel = state.clone();
    window.on_network_conceal(move || {
        let Some(window) = weak.upgrade() else { return };
        window.set_network_password_revealed(false);
        window.set_network_password_text("".into());
        if let Some(panel) = panel.borrow_mut().as_mut() {
            panel.publish(&window);
        }
    });
    let weak = window.as_weak();
    let installed = state.clone();
    window.on_network_visibility_changed(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        timer.stop();
        // Close is asynchronous: dropping the session cancels/rolls back in its
        // worker, without blocking the render thread. Reopen is serialized.
        state.borrow_mut().take();
        if !window.get_network_visible() {
            window.set_network_rows(ModelRc::default());
            window.set_network_detail("".into());
            window.set_network_password_visible(false);
            window.set_network_forget_confirmation(false);
            window.set_network_profile_visible(false);
            window.set_network_password_revealed(false);
            window.set_network_password_text("".into());
            window.set_network_password_mask("".into());
            window.set_network_password_caption("".into());
            return;
        }
        let mut panel = Panel::new(make_session());
        panel.publish(&window);
        *state.borrow_mut() = Some(panel);
        let panel = state.clone();
        let weak = window.as_weak();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some(panel) = panel.borrow_mut().as_mut() {
                panel.publish(&window);
            }
        });
    });
    installed
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    let rows = window.get_network_rows();
    let count = rows.row_count();
    if count == 0 {
        return;
    }
    let index = window.get_network_focus_index().max(0) as usize;
    if window.get_network_password_visible() {
        match action {
            UiAction::Start => {
                window.invoke_network_command("network.connect-password".into());
                return;
            }
            UiAction::Controls => {
                window.invoke_network_command("network.erase".into());
                return;
            }
            _ => {}
        }
    }
    if window.get_network_password_visible() && matches!(action, UiAction::Up | UiAction::Down) {
        let columns = window.get_network_keyboard_columns().max(1) as usize;
        let next = keyboard_vertical(index, columns, action == UiAction::Down);
        window.set_network_focus_index(next as i32);
        return;
    }
    let step = match action {
        UiAction::Up => {
            -if window.get_network_password_visible() {
                8
            } else {
                1
            }
        }
        UiAction::Down => {
            if window.get_network_password_visible() {
                8
            } else {
                1
            }
        }
        UiAction::Left => -1,
        UiAction::Right => 1,
        _ => 0,
    };
    if step != 0 {
        let mut next = (index as i32 + step).rem_euclid(count as i32) as usize;
        for _ in 0..count {
            if rows.row_data(next).is_some_and(|row| row.enabled) {
                break;
            }
            next = (next as i32 + step.signum()).rem_euclid(count as i32) as usize;
        }
        window.set_network_focus_index(next as i32);
    } else if action == UiAction::Confirm {
        if let Some(row) = rows.row_data(index).filter(|r| r.enabled) {
            window.invoke_network_command(row.id);
        }
    } else if matches!(action, UiAction::Back | UiAction::Controls) {
        window.invoke_network_command("network.back".into());
    }
    // Outside password entry Start is ignored; it never resumes the scenario.
}

fn keyboard_vertical(index: usize, columns: usize, down: bool) -> usize {
    match (index, down) {
        (0..=39, true) if index + columns < 40 => index + columns,
        (0..=39, true) => 40 + (index % columns) * 4 / columns,
        (0..=39, false) if index >= columns => index - columns,
        (0..=39, false) => 44 + index * 3 / columns,
        (40..=43, true) => 44 + (index - 40) * 3 / 4,
        (40..=43, false) => 40 - columns + (index - 40) * columns / 4,
        (_, true) => (index.saturating_sub(44) * columns / 3).min(39),
        (_, false) => (40 + index.saturating_sub(44) * 4 / 3).min(43),
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    let mut result: Vec<_> = window
        .get_network_rows()
        .iter()
        .map(|r| {
            let mut control = UiControl::new(r.id.to_string(), r.label.to_string(), r.enabled);
            if !r.detail.is_empty() {
                control = control.with_value(if r.badge.is_empty() {
                    r.detail.to_string()
                } else {
                    format!("{} · {}", r.detail, r.badge)
                });
            }
            control
        })
        .collect();
    result.push(
        UiControl::new("network.status", "Network status", false)
            .with_value(window.get_network_detail().to_string()),
    );
    result.push(
        UiControl::new("network.summary", "Wi-Fi connection", false)
            .with_value(window.get_network_subtitle().to_string()),
    );
    result
}

#[cfg(test)]
#[path = "network/ui_tests.rs"]
mod tests;
