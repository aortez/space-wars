//! On-demand Wi-Fi setup. NetworkManager owns credentials, activation and the
//! rollback timer; the game never writes connection files or runs sudo/nmcli.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[cfg(target_os = "linux")]
mod nm;
#[cfg(test)]
mod tests;
mod worker;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Security {
    Open,
    WpaPsk,
    Sae,
    #[default]
    Unsupported,
}

impl Security {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::WpaPsk => "WPA/WPA2 Personal",
            Self::Sae => "WPA3 Personal",
            Self::Unsupported => "Unsupported security",
        }
    }

    pub fn valid_password(self, password: &str) -> bool {
        match self {
            Self::Open => password.is_empty(),
            Self::WpaPsk => {
                ((8..=63).contains(&password.len())
                    && password.bytes().all(|b| (32..=126).contains(&b)))
                    || (password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit()))
            }
            Self::Sae => {
                (1..=63).contains(&password.len())
                    && password.bytes().all(|b| (32..=126).contains(&b))
            }
            Self::Unsupported => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Network {
    pub id: String,
    pub device: String,
    pub interface: String,
    pub ap: String,
    pub ssid: Vec<u8>,
    pub name: String,
    pub security: Security,
    pub strength: u8,
    pub connected: bool,
    pub saved: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SavedNetwork {
    /// NetworkManager UUID, independent of the SSID and transient D-Bus path.
    pub id: String,
    pub path: String,
    pub name: String,
    pub ssid_name: String,
    pub autoconnect: bool,
    pub priority: i32,
    pub connected: bool,
    pub network: Option<Network>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Inventory {
    pub devices: Vec<String>,
    pub networks: Vec<Network>,
    pub profiles: Vec<SavedNetwork>,
    pub can_manage: bool,
    pub can_connect: bool,
    pub can_scan: bool,
    pub summary: String,
}

impl Inventory {
    pub fn preferred(&self, profile: &SavedNetwork) -> bool {
        let mut alternatives = self
            .profiles
            .iter()
            .filter(|p| p.id != profile.id && p.autoconnect)
            .peekable();
        profile.autoconnect
            && (profile.priority > 0 || alternatives.peek().is_some())
            && alternatives.all(|p| p.priority < profile.priority)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProfileChange {
    Autoconnect(bool),
    Prefer,
    Forget { allow_active: bool },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    #[default]
    Idle,
    Discovering,
    Connecting,
    Confirm,
    Saving,
    Restoring,
    Managing,
}

impl Phase {
    pub fn is_trial(self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::Confirm | Self::Saving | Self::Restoring
        )
    }

    pub fn can_cancel(self) -> bool {
        matches!(self, Self::Connecting | Self::Confirm)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct View {
    pub inventory: Inventory,
    pub phase: Phase,
    pub status: String,
    pub inventory_error: Option<String>,
    pub remaining: u64,
}

// No Debug or Clone: passwords must not enter diagnostics, snapshots or logs.
pub(crate) enum Command {
    Refresh,
    Scan,
    Connect {
        id: String,
        password: Option<String>,
    },
    ConnectProfile {
        id: String,
    },
    Manage {
        id: String,
        change: ProfileChange,
    },
    Keep,
    Cancel,
}

static WORKER_RUNNING: AtomicBool = AtomicBool::new(false);

struct WorkerGuard;
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        WORKER_RUNNING.store(false, Ordering::Release);
    }
}

pub(crate) struct Session {
    commands: async_channel::Sender<Command>,
    latest: Arc<Mutex<View>>,
}

impl Session {
    pub fn new() -> Self {
        let (commands, receiver) = async_channel::bounded(2);
        let latest = Arc::new(Mutex::new(View {
            phase: Phase::Discovering,
            status: "Reading Wi-Fi networks…".into(),
            ..View::default()
        }));
        if WORKER_RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            receiver.close();
            *latest.lock().unwrap() = View {
                status: "Previous Wi-Fi operation is finishing. Wait a moment, then Rescan.".into(),
                ..View::default()
            };
        } else {
            let guard = WorkerGuard;
            let shared = Arc::clone(&latest);
            if std::thread::Builder::new().name("wifi-setup".into()).spawn(move || {
                let _guard = guard;
                #[cfg(target_os = "linux")]
                async_io::block_on(async {
                    match nm::NetworkManager::open().await {
                        Ok(backend) => worker::run(&backend, receiver, Arc::clone(&shared)).await,
                        Err(status) => *shared.lock().unwrap() = View { status, ..View::default() },
                    }
                });
                #[cfg(not(target_os = "linux"))]
                {
                    drop(receiver);
                    *shared.lock().unwrap() = View {
                        status: "Wi-Fi setup needs Linux with NetworkManager. Use your operating system's network settings.".into(),
                        ..View::default()
                    };
                }
            }).is_err() {
                *latest.lock().unwrap() = View {
                    status: "Could not start Wi-Fi setup. Go Back and retry.".into(),
                    ..View::default()
                };
            }
        }
        Self { commands, latest }
    }

    pub fn snapshot(&self) -> View {
        self.latest.lock().unwrap().clone()
    }

    pub fn send(&mut self, command: Command) {
        if self.commands.is_closed() && matches!(command, Command::Scan | Command::Refresh) {
            *self = Self::new();
            return;
        }
        let mut view = self.latest.lock().unwrap();
        let allowed = match command {
            Command::Cancel => view.phase.can_cancel(),
            Command::Keep => view.phase == Phase::Confirm,
            _ => view.phase == Phase::Idle,
        };
        if !allowed {
            return;
        }
        let (phase, status) = match command {
            Command::Connect { .. } | Command::ConnectProfile { .. } => (
                Phase::Connecting,
                "Connecting… The previous network will be restored unless you keep this connection.",
            ),
            Command::Keep => (Phase::Saving, "Keeping this network…"),
            Command::Cancel => (Phase::Restoring, "Restoring previous connection…"),
            Command::Scan => (Phase::Discovering, "Requesting a Wi-Fi scan…"),
            Command::Refresh => (Phase::Discovering, "Reading Wi-Fi networks…"),
            Command::Manage { .. } => (Phase::Managing, "Updating saved network…"),
        };
        if self.commands.try_send(command).is_ok() {
            view.phase = phase;
            view.status = status.into();
        }
    }

    #[cfg(test)]
    pub fn simulated(view: View) -> (Self, Arc<Mutex<View>>, async_channel::Receiver<Command>) {
        let (commands, receiver) = async_channel::bounded(2);
        let latest = Arc::new(Mutex::new(view));
        (
            Self {
                commands,
                latest: latest.clone(),
            },
            latest,
            receiver,
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.commands.close();
    }
}
