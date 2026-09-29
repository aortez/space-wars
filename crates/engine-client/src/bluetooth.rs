//! On-demand controller pairing. No Bluetooth work runs on the UI/game thread.
//! Closing this panel releases only our discovery session, never the shared radio.

use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
mod bluez;
#[cfg(test)]
mod tests;
mod worker;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Device {
    pub id: String,
    pub adapter: String,
    pub name: String,
    pub address: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Inventory {
    pub adapter: Option<String>,
    pub devices: Vec<Device>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Pair,
    Connect,
    Disconnect,
    Forget,
}

impl Action {
    fn label(&self) -> &'static str {
        match self {
            Self::Pair => "Pairing",
            Self::Connect => "Connecting",
            Self::Disconnect => "Disconnecting",
            Self::Forget => "Forgetting",
        }
    }

    fn completed_message(&self) -> &'static str {
        // This is the last operation's result, not a live connection claim.
        // The device detail/list reads current state from the inventory.
        match self {
            Self::Pair => "Last action: Pair and connect completed.",
            Self::Connect => "Last action: Connect request completed.",
            Self::Disconnect => "Last action: Disconnect request completed.",
            Self::Forget => {
                "Pairing forgotten. Button mappings and player preferences were not changed."
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Command {
    Refresh,
    Scan,
    StopScan,
    Act(Action, String),
    Cancel,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct View {
    pub inventory: Inventory,
    pub busy: bool,
    pub scanning: bool,
    pub status: String,
}

#[derive(Debug)]
struct Session {
    commands: async_channel::Sender<Command>,
    latest: Arc<Mutex<View>>,
}

impl Session {
    fn new() -> Self {
        let (commands, receiver) = async_channel::bounded(4);
        let latest = Arc::new(Mutex::new(View {
            busy: true,
            status: "Reading Bluetooth controllers…".into(),
            ..View::default()
        }));
        let shared = Arc::clone(&latest);
        if let Err(error) = std::thread::Builder::new()
            .name("bluetooth-setup".into())
            .spawn(move || {
                #[cfg(target_os = "linux")]
                async_io::block_on(async {
                    match bluez::Bluez::open().await {
                        Ok(backend) => worker::run(&backend, receiver, Arc::clone(&shared)).await,
                        Err(error) => *shared.lock().unwrap() = View { status: error, ..View::default() },
                    }
                });
                #[cfg(not(target_os = "linux"))]
                {
                    drop(receiver);
                    *shared.lock().unwrap() = View {
                        status: "In-app Bluetooth setup is available on Linux. Pair this controller in your system settings.".into(),
                        ..View::default()
                    };
                }
            })
        {
            *latest.lock().unwrap() = View {
                status: format!("Could not start Bluetooth setup: {error}"),
                ..View::default()
            };
        }
        Self { commands, latest }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Closing wakes a pending operation's cancellation branch immediately.
        self.commands.close();
    }
}

#[derive(Debug)]
enum Page {
    Devices,
    Device(String),
    Forget(String),
}

#[derive(Debug)]
pub(crate) struct Panel {
    session: Session,
    view: View,
    page: Page,
}

impl Panel {
    #[cfg(test)]
    pub(crate) fn simulated(
        view: View,
    ) -> (Self, Arc<Mutex<View>>, async_channel::Receiver<Command>) {
        let (commands, receiver) = async_channel::bounded(4);
        let latest = Arc::new(Mutex::new(view.clone()));
        (
            Self {
                session: Session {
                    commands,
                    latest: Arc::clone(&latest),
                },
                view,
                page: Page::Devices,
            },
            latest,
            receiver,
        )
    }

    pub fn new() -> Self {
        Self {
            session: Session::new(),
            view: View {
                busy: true,
                status: "Reading Bluetooth controllers…".into(),
                ..View::default()
            },
            page: Page::Devices,
        }
    }

    pub fn poll(&mut self) {
        if let Ok(view) = self.session.latest.try_lock() {
            self.view = view.clone();
        }
        if !self.view.busy {
            if let Page::Device(id) | Page::Forget(id) = &self.page {
                if !self.view.inventory.devices.iter().any(|d| &d.id == id) {
                    self.page = Page::Devices;
                }
            }
        }
    }

    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        let mut add = |id: &str, label: &str| {
            rows.push((format!("controllers.bluetooth.{id}"), label.into()))
        };
        if self.view.busy {
            add("cancel", "Cancel operation");
            return rows;
        }
        match &self.page {
            Page::Devices => {
                if self.view.scanning {
                    add("stop", "Stop scanning");
                } else if self.view.inventory.adapter.is_some() {
                    add("scan", "Scan for controllers (30 seconds)");
                }
                for device in &self.view.inventory.devices {
                    add(
                        &format!("device.{}", device.id),
                        &format!(
                            "{} · {} · {}",
                            device.name,
                            device.address,
                            if device.connected {
                                "Connected"
                            } else if device.paired {
                                "Paired"
                            } else {
                                "Nearby"
                            }
                        ),
                    );
                }
                add("refresh", "Refresh");
            }
            Page::Device(id) => {
                if let Some(device) = self.view.inventory.devices.iter().find(|d| &d.id == id) {
                    if !device.paired {
                        add("pair", "Pair and connect");
                    } else if device.connected {
                        add("disconnect", "Disconnect");
                    } else {
                        add("connect", "Connect");
                    }
                    if device.paired {
                        add("forget", "Forget controller…");
                    }
                }
            }
            Page::Forget(_) => add("confirm-forget", "Yes, forget this controller"),
        }
        rows
    }

    pub fn detail(&self) -> String {
        let info = match &self.page {
            Page::Devices => "Put the controller in pairing mode, then scan. Only game controllers are listed. After connecting, go Back to assign a player or test its buttons.".into(),
            Page::Device(id) | Page::Forget(id) => {
                self.view.inventory.devices.iter().find(|d| &d.id == id).map_or_else(
                    || "This controller is no longer available. Go Back and scan again.".into(),
                    |d| format!("{} · {}\nBluetooth: {}\n{}", d.name, d.address, if d.connected {
                        "Connected"
                    } else if d.paired {
                        "Paired, disconnected"
                    } else {
                        "Not paired"
                    }, if matches!(self.page, Page::Forget(_)) {
                        "Forget this pairing? It will disconnect and must be paired again. Button mappings and player preferences are kept."
                    } else { "Go Back to test inputs and choose a player. Pairing keeps existing assignments." }),
                )
            }
        };
        if self.view.status.is_empty() {
            info
        } else {
            format!("{}\n{info}", self.view.status)
        }
    }

    pub fn command(&mut self, command: &str) {
        if !self.rows().iter().any(|(id, _)| id == command) {
            return;
        }
        let command = command.strip_prefix("controllers.bluetooth.").unwrap();
        if let Some(id) = command.strip_prefix("device.") {
            self.page = Page::Device(id.into());
            return;
        }
        let request = match command {
            "refresh" => {
                if self.session.commands.is_closed() {
                    self.session = Session::new();
                }
                Command::Refresh
            }
            "scan" => Command::Scan,
            "stop" => Command::StopScan,
            "cancel" => Command::Cancel,
            "forget" => {
                if let Page::Device(id) = &self.page {
                    self.page = Page::Forget(id.clone());
                }
                return;
            }
            action => {
                let (Page::Device(id) | Page::Forget(id)) = &self.page else {
                    return;
                };
                Command::Act(
                    match action {
                        "pair" => Action::Pair,
                        "connect" => Action::Connect,
                        "disconnect" => Action::Disconnect,
                        "confirm-forget" => Action::Forget,
                        _ => return,
                    },
                    id.clone(),
                )
            }
        };
        let mut latest = self.session.latest.lock().unwrap();
        match self.session.commands.try_send(request) {
            Ok(()) => {
                self.view.busy = true;
                self.view.status = "Working…".into();
                // Prevent a stale worker snapshot from re-enabling commands.
                *latest = self.view.clone();
            }
            Err(_) => {
                self.view.status =
                    "Bluetooth setup is unavailable or busy. Go Back and try again.".into()
            }
        }
    }

    /// True returns to the ordinary Controllers screen, dropping the session.
    pub fn back(&mut self) -> bool {
        if self.view.busy || matches!(self.page, Page::Devices) {
            return true;
        }
        self.page = match &self.page {
            Page::Forget(id) => Page::Device(id.clone()),
            _ => Page::Devices,
        };
        false
    }
}
