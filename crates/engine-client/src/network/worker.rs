//! Bounded Wi-Fi transaction, independent of the UI and D-Bus transport.
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_channel::Receiver;
use futures_lite::future;

use super::{Command, Inventory, Network, Phase, ProfileChange, View};

pub(super) const CALL_TIME: Duration = Duration::from_secs(5);
const CONNECT_TIME: Duration = Duration::from_secs(40);
const CONFIRM_TIME: Duration = Duration::from_secs(30);
// The daemon's timer outlives our process and leaves time for explicit cleanup.
pub(super) const ROLLBACK_SECONDS: u32 = 100;

#[derive(Default)]
pub(super) struct Trial {
    pub checkpoint: String,
    pub device: String,
    pub active: String,
    pub created_profile: Option<String>,
}

pub(super) trait Backend {
    async fn inventory(&self) -> Result<Inventory, String>;
    async fn scan(&self, devices: &[String]) -> Result<(), String>;
    async fn checkpoint(&self, device: &str) -> Result<String, String>;
    async fn activate(
        &self,
        network: &Network,
        password: Option<&str>,
        trial: &mut Trial,
    ) -> Result<(), String>;
    async fn ready(&self, trial: &Trial) -> Result<bool, String>;
    async fn keep(&self, trial: &Trial) -> Result<(), String>;
    async fn rollback(&self, trial: &Trial) -> Result<(), String>;
    async fn manage(&self, id: &str, change: ProfileChange) -> Result<(), String>;
}

pub(super) async fn bounded<T>(
    duration: Duration,
    task: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    future::or(task, async {
        async_io::Timer::after(duration).await;
        Err("Wi-Fi operation timed out.".into())
    })
    .await
}

fn publish(shared: &Mutex<View>, view: &View, commands: &Receiver<Command>) {
    let mut latest = shared.lock().unwrap();
    // Same lock as enqueue: a late refresh cannot reopen the busy gate.
    // Preserve the specific queued phase (scan, connect, save or restore).
    if commands.is_empty() {
        *latest = view.clone();
    } else {
        latest.inventory = view.inventory.clone();
        latest.inventory_error = view.inventory_error.clone();
    }
}

async fn refresh(backend: &impl Backend, view: &mut View) {
    match bounded(CALL_TIME, backend.inventory()).await {
        Ok(inventory) => {
            view.inventory = inventory;
            view.inventory_error = None;
        }
        Err(status) => {
            // Keep cached results/entry visible through a transient read error,
            // but never start a mutation using an unverified inventory.
            view.inventory.can_connect = false;
            view.inventory.can_scan = false;
            view.inventory.can_manage = false;
            view.inventory_error = Some(status);
        }
    }
}

async fn scan(
    backend: &impl Backend,
    commands: &Receiver<Command>,
    shared: &Mutex<View>,
    view: &mut View,
    last_scan: &mut Option<Instant>,
) {
    view.phase = Phase::Discovering;
    view.status = "Looking for nearby Wi-Fi networks…".into();
    publish(shared, view, commands);
    if commands.is_closed() {
        return;
    }
    if !view.inventory.can_scan || view.inventory.devices.is_empty() {
        view.status =
            "Wi-Fi scanning is unavailable. Check the adapter and operating system permissions."
                .into();
    } else if last_scan.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
        view.status =
            "Scan already requested. Results refresh automatically; wait a few seconds.".into();
    } else {
        *last_scan = Some(Instant::now());
        view.status = match bounded(CALL_TIME, backend.scan(&view.inventory.devices)).await {
            // RequestScan acknowledges the request, not completion. Inventory
            // polling picks up results without pretending this is a transaction.
            Ok(()) => "Scan requested. Nearby networks update automatically.".into(),
            Err(error) => error,
        };
    }
    view.phase = Phase::Idle;
    publish(shared, view, commands);
}

async fn cancelled(commands: &Receiver<Command>) -> Result<(), String> {
    loop {
        if commands.is_closed() {
            return Err("Wi-Fi setup closed.".into());
        }
        match commands.recv().await {
            Ok(Command::Cancel) | Err(_) => return Err("Connection cancelled.".into()),
            // Never forward clicks queued during an earlier phase.
            Ok(_) => {}
        }
    }
}

struct Timing {
    connect: Duration,
    confirm: Duration,
}

enum ConnectionTarget<'a> {
    Nearby(&'a str),
    Saved(&'a str),
}

async fn connect(
    backend: &impl Backend,
    commands: &Receiver<Command>,
    shared: &Mutex<View>,
    view: &mut View,
    target: ConnectionTarget<'_>,
    password: Option<String>,
    timing: Timing,
) -> Result<(), String> {
    // Validate against a new snapshot, not an AP path remembered by the UI.
    let inventory = bounded(CALL_TIME, backend.inventory()).await?;
    if commands.is_closed() {
        return Err("Wi-Fi setup closed.".into());
    }
    if !inventory.can_connect {
        return Err("Wi-Fi changes are not permitted by the operating system.".into());
    }
    let network = match target {
        ConnectionTarget::Nearby(id) => inventory
            .networks
            .iter()
            .find(|n| n.id == id)
            .ok_or("Network is no longer visible. Scan and select it again.")?,
        ConnectionTarget::Saved(id) => inventory
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or("Saved network no longer exists. Refresh and select it again.")?
            .network
            .as_ref()
            .ok_or("Saved network is not currently available. Scan and try again.")?,
    };
    if network.connected {
        return Err("Already connected to this network.".into());
    }
    if network.security == super::Security::Unsupported {
        return Err("This network's security is not supported yet.".into());
    }
    if password.is_none() && network.saved.is_none() && network.security != super::Security::Open {
        return Err("Enter a password for this network.".into());
    }
    if password
        .as_deref()
        .is_some_and(|password| !network.security.valid_password(password))
    {
        return Err("Invalid Wi-Fi password length or format.".into());
    }

    let mut trial = Trial {
        checkpoint: bounded(CALL_TIME, backend.checkpoint(&network.device)).await?,
        device: network.device.clone(),
        ..Trial::default()
    };
    view.phase = Phase::Connecting;
    view.status = format!("Connecting to {}…", network.name);
    publish(shared, view, commands);
    // A cancellation may race the activation reply. The checkpoint is already
    // known, and the candidate profile is volatile until Keep, so rollback also
    // covers an activation whose reply we never receive.
    let attempt = future::or(
        cancelled(commands),
        bounded(timing.connect, async {
            bounded(
                CALL_TIME,
                backend.activate(network, password.as_deref(), &mut trial),
            )
            .await?;
            loop {
                if bounded(CALL_TIME, backend.ready(&trial)).await? {
                    return Ok(());
                }
                async_io::Timer::after(Duration::from_millis(250)).await;
            }
        }),
    )
    .await;
    drop(password);

    let result = match attempt {
        Ok(()) => {
            let deadline = Instant::now() + timing.confirm;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() || commands.is_closed() {
                    break Err("Connection was not confirmed.".into());
                }
                view.phase = Phase::Confirm;
                view.remaining = left.as_secs().saturating_add(1);
                view.status = format!(
                    "Connected to {}. Keep this network?\nInternet access is not guaranteed. Restoring in {} seconds unless kept.",
                    network.name, view.remaining
                );
                publish(shared, view, commands);
                let command = future::or(async { commands.recv().await.ok() }, async {
                    async_io::Timer::after(left.min(Duration::from_secs(1))).await;
                    Some(Command::Refresh)
                })
                .await;
                if Instant::now() >= deadline || commands.is_closed() {
                    break Err("Connection was not confirmed.".into());
                }
                match command {
                    Some(Command::Keep) => {
                        view.phase = Phase::Saving;
                        view.status = "Saving this connection…".into();
                        publish(shared, view, commands);
                        break bounded(CALL_TIME * 3, async {
                            if !backend.ready(&trial).await? {
                                return Err("Connection was lost before confirmation.".into());
                            }
                            backend.keep(&trial).await
                        })
                        .await;
                    }
                    Some(Command::Cancel) | None => break Err("Connection cancelled.".into()),
                    Some(Command::Refresh) => {
                        match bounded(CALL_TIME, backend.ready(&trial)).await {
                            Ok(true) => {}
                            _ => break Err("Connection was lost before confirmation.".into()),
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        view.phase = Phase::Restoring;
        view.status = "Restoring the previous connection…".into();
        publish(shared, view, commands);
        return match bounded(CALL_TIME * 3, backend.rollback(&trial)).await {
            Ok(()) => Err(format!(
                "{error} Previous network configuration restored; reconnection may take a moment."
            )),
            Err(_) => Err(format!(
                "{error} Recovery could not be confirmed. Automatic rollback may still run; check the connection before retrying."
            )),
        };
    }
    Ok(())
}

pub(super) async fn run(
    backend: &impl Backend,
    commands: Receiver<Command>,
    shared: Arc<Mutex<View>>,
) {
    run_with_timing(backend, commands, shared, CONNECT_TIME, CONFIRM_TIME).await;
}

pub(super) async fn run_with_timing(
    backend: &impl Backend,
    commands: Receiver<Command>,
    shared: Arc<Mutex<View>>,
    connect_time: Duration,
    confirm_time: Duration,
) {
    let mut view = View::default();
    refresh(backend, &mut view).await;
    publish(&shared, &view, &commands);
    let mut last_scan: Option<Instant> = None;
    if view.inventory.can_scan && !commands.is_closed() {
        scan(backend, &commands, &shared, &mut view, &mut last_scan).await;
    }
    loop {
        if commands.is_closed() {
            break;
        }
        let command = future::or(async { commands.recv().await.ok() }, async {
            async_io::Timer::after(Duration::from_secs(3)).await;
            Some(Command::Refresh)
        })
        .await;
        let Some(command) = command else {
            break;
        };
        if commands.is_closed() {
            break;
        }
        match command {
            Command::Refresh => refresh(backend, &mut view).await,
            Command::Scan => {
                refresh(backend, &mut view).await;
                scan(backend, &commands, &shared, &mut view, &mut last_scan).await;
            }
            command @ (Command::Connect { .. } | Command::ConnectProfile { .. }) => {
                let (id, password, saved) = match command {
                    Command::Connect { id, password } => (id, password, false),
                    Command::ConnectProfile { id } => (id, None, true),
                    _ => unreachable!(),
                };
                let started = Instant::now();
                tracing::info!(
                    operation = "wifi-connect",
                    "Starting a checkpoint-backed Wi-Fi trial"
                );
                view.status = match connect(
                    backend,
                    &commands,
                    &shared,
                    &mut view,
                    if saved {
                        ConnectionTarget::Saved(&id)
                    } else {
                        ConnectionTarget::Nearby(&id)
                    },
                    password,
                    Timing {
                        connect: connect_time,
                        confirm: confirm_time,
                    },
                )
                .await
                {
                    Ok(()) => {
                        "Connection kept. Saved automatic-connection preferences still apply."
                            .into()
                    }
                    Err(error) => error,
                };
                tracing::info!(
                    operation = "wifi-connect",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    kept = view.status.starts_with("Connection kept"),
                    "Wi-Fi trial finished"
                );
                // Drop stale button presses; they cannot commit another attempt.
                while commands.try_recv().is_ok() {}
                refresh(backend, &mut view).await;
            }
            Command::Manage { id, change } => {
                view.phase = Phase::Managing;
                view.status = "Updating saved network…".into();
                publish(&shared, &view, &commands);
                view.status = match bounded(CALL_TIME * 3, async {
                    let inventory = backend.inventory().await?;
                    if commands.is_closed() {
                        return Err("Wi-Fi setup closed before the change started.".into());
                    }
                    if !inventory.can_manage {
                        return Err("Saved network changes are unavailable. Refresh and check permissions.".into());
                    }
                    let profile = inventory.profiles.iter().find(|p| p.id == id)
                        .ok_or("Saved network no longer exists. Refresh and select it again.")?;
                    if matches!(change, ProfileChange::Forget { allow_active: false }) && profile.connected {
                        return Err("This network is now active. Review the connection warning and confirm Forget again.".into());
                    }
                    if change == ProfileChange::Prefer && !profile.autoconnect {
                        return Err("Enable Connect automatically before preferring this network.".into());
                    }
                    backend.manage(&id, change).await
                }).await {
                    Ok(()) => match change {
                        ProfileChange::Forget { .. } => "Saved network forgotten.".into(),
                        ProfileChange::Autoconnect(_) => "Automatic connection setting saved. Your current connection has not been switched.".into(),
                        ProfileChange::Prefer => "Preferred network saved for future automatic connections. Your current connection has not been switched.".into(),
                    },
                    Err(error) => format!("{error} Check the refreshed saved-network settings before retrying."),
                };
                while commands.try_recv().is_ok() {}
                refresh(backend, &mut view).await;
            }
            Command::Cancel | Command::Keep => {}
        }
        view.phase = Phase::Idle;
        view.remaining = 0;
        publish(&shared, &view, &commands);
    }
}

#[test]
fn publishing_inventory_preserves_queued_operation_phase_and_status() {
    use super::Session;
    for command in [
        Command::Scan,
        Command::Connect {
            id: "id".into(),
            password: None,
        },
        Command::ConnectProfile { id: "id".into() },
        Command::Manage {
            id: "id".into(),
            change: ProfileChange::Prefer,
        },
    ] {
        let (mut session, latest, receiver) = Session::simulated(View::default());
        session.send(command);
        let queued = session.snapshot();
        publish(&latest, &View::default(), &receiver);
        assert_eq!(session.snapshot().phase, queued.phase);
        assert_eq!(session.snapshot().status, queued.status);
    }
    let (mut session, latest, receiver) = Session::simulated(View {
        phase: Phase::Confirm,
        ..View::default()
    });
    session.send(Command::Cancel);
    publish(
        &latest,
        &View {
            phase: Phase::Confirm,
            ..View::default()
        },
        &receiver,
    );
    assert_eq!(session.snapshot().phase, Phase::Restoring);
    session.send(Command::Cancel);
    assert_eq!(receiver.len(), 1);
}
