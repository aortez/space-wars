//! One bounded setup worker per open panel. It sleeps when idle and exits on
//! close; gameplay never polls D-Bus or waits for an operation.

use std::cell::Cell;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_channel::Receiver;
use futures_lite::future;

use super::{Action, Command, Device, Inventory, View};

const SCAN_TIME: Duration = Duration::from_secs(30);
pub(super) const CALL_TIME: Duration = Duration::from_secs(3);

pub(super) trait Backend {
    async fn inventory(&self) -> Result<Inventory, String>;
    async fn start_scan(&self, adapter: &str) -> Result<(), String>;
    async fn stop_scan(&self, adapter: &str) -> Result<(), String>;
    async fn act(&self, action: &Action, device: &Device) -> Result<(), String>;
    async fn cancel(&self, action: &Action, device: &Device);
}

pub(super) async fn bounded<T>(
    duration: Duration,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    future::or(operation, async {
        async_io::Timer::after(duration).await;
        Err(
            "Bluetooth operation timed out. Check the controller is awake and nearby, then retry."
                .into(),
        )
    })
    .await
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Finished(Result<(), String>),
    Cancelled,
    Closed,
}

async fn operation(
    backend: &impl Backend,
    commands: &Receiver<Command>,
    action: &Action,
    device: &Device,
    duration: Duration,
) -> Outcome {
    let started = Cell::new(false);
    let outcome = future::or(
        async {
            loop {
                if commands.is_closed() {
                    break Outcome::Closed;
                }
                match commands.recv().await {
                    Ok(Command::Cancel) => break Outcome::Cancelled,
                    Err(_) => break Outcome::Closed,
                    // A stale queued click must not start a second operation.
                    Ok(_) => {}
                }
            }
        },
        async {
            started.set(true);
            Outcome::Finished(bounded(duration, backend.act(action, device)).await)
        },
    )
    .await;
    if started.get() && !matches!(outcome, Outcome::Finished(Ok(()))) {
        // Dropping a D-Bus call does not cancel the daemon's operation.
        let _ = bounded(CALL_TIME * 2, async {
            backend.cancel(action, device).await;
            Ok(())
        })
        .await;
    }
    outcome
}

fn publish(shared: &Mutex<View>, view: &View, commands: &Receiver<Command>) {
    let mut latest = shared.lock().unwrap();
    *latest = view.clone();
    // Enqueue and publish use the same mutex, so an in-flight inventory refresh
    // cannot clear the UI's busy gate before a queued command is consumed.
    latest.busy |= !commands.is_empty();
}

async fn refresh(backend: &impl Backend, view: &mut View) {
    match bounded(CALL_TIME, backend.inventory()).await {
        Ok(mut inventory) => {
            inventory.devices.sort_by(|a, b| {
                b.paired
                    .cmp(&a.paired)
                    .then(a.name.cmp(&b.name))
                    .then(a.id.cmp(&b.id))
            });
            inventory.devices.truncate(64);
            view.inventory = inventory;
        }
        Err(error) => {
            // Do not offer actions against stale devices/adapter state.
            view.inventory = Inventory::default();
            view.status = error;
        }
    }
}

fn validate<'a>(inventory: &'a Inventory, action: &Action, id: &str) -> Result<&'a Device, String> {
    let device = inventory
        .devices
        .iter()
        .find(|device| device.id == id)
        .ok_or("Controller is no longer available. Scan again.")?;
    if matches!(action, Action::Pair) && device.paired {
        return Err("Already paired. Choose Connect instead.".into());
    }
    if !matches!(action, Action::Pair) && !device.paired {
        return Err("Controller is not paired. Pair it first.".into());
    }
    Ok(device)
}

async fn stop_scan(
    backend: &impl Backend,
    scan: &mut Option<(String, Instant)>,
    view: &mut View,
    shared: &Mutex<View>,
) -> bool {
    view.scanning = false;
    if let Some((adapter, _)) = scan.take() {
        if let Err(error) = bounded(CALL_TIME, backend.stop_scan(&adapter)).await {
            // End the worker so its private D-Bus connection is dropped. BlueZ
            // then releases our discovery reference even if StopDiscovery failed.
            *view = View {
                status: error,
                ..View::default()
            };
            *shared.lock().unwrap() = view.clone();
            return false;
        }
    }
    true
}

pub(super) async fn run(
    backend: &impl Backend,
    commands: Receiver<Command>,
    shared: Arc<Mutex<View>>,
) {
    run_with_scan_time(backend, commands, shared, SCAN_TIME).await;
}

async fn run_with_scan_time(
    backend: &impl Backend,
    commands: Receiver<Command>,
    shared: Arc<Mutex<View>>,
    scan_time: Duration,
) {
    let mut view = View {
        status: "Choose a remembered controller, or scan to add one.".into(),
        ..View::default()
    };
    refresh(backend, &mut view).await;
    if view.inventory.adapter.is_none() && view.status.starts_with("Choose") {
        view.status = "No powered Bluetooth adapter. Check Bluetooth is enabled in the operating system, then Refresh.".into();
    }
    publish(&shared, &view, &commands);
    let mut scan: Option<(String, Instant)> = None;
    loop {
        if commands.is_closed() {
            break;
        }
        let poll_time = scan
            .as_ref()
            .map_or(Duration::from_secs(2), |(_, deadline)| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(1))
            });
        let command = future::or(async { commands.recv().await.ok().map(Some) }, async {
            async_io::Timer::after(poll_time).await;
            Some(None)
        })
        .await;
        let Some(command) = command else { break };
        if scan
            .as_ref()
            .is_some_and(|(_, deadline)| Instant::now() >= *deadline)
        {
            if !stop_scan(backend, &mut scan, &mut view, &shared).await {
                return;
            }
            view.status = "Scan finished. Choose a controller, or scan again.".into();
        }
        match command {
            None | Some(Command::Refresh) => refresh(backend, &mut view).await,
            Some(Command::Cancel | Command::StopScan) => {
                if !stop_scan(backend, &mut scan, &mut view, &shared).await {
                    return;
                }
                view.status = "Scan stopped.".into();
            }
            Some(Command::Scan) => {
                refresh(backend, &mut view).await;
                if commands.is_closed() {
                    break;
                }
                if let Some(adapter) = &view.inventory.adapter {
                    if scan.is_none() {
                        // Remember our requested session even when StartDiscovery
                        // times out: StopDiscovery must still be attempted on close.
                        scan = Some((adapter.clone(), Instant::now() + scan_time));
                        match bounded(CALL_TIME, backend.start_scan(adapter)).await {
                            Ok(()) => {
                                view.scanning = true;
                                view.status =
                                    "Scanning for 30 seconds… Put the controller in pairing mode."
                                        .into();
                            }
                            Err(error) => {
                                if !stop_scan(backend, &mut scan, &mut view, &shared).await {
                                    return;
                                }
                                view.status = error;
                            }
                        }
                    }
                } else {
                    view.status =
                        "No powered Bluetooth adapter. Enable it in system settings, then Refresh."
                            .into();
                }
            }
            Some(Command::Act(action, id)) => {
                // Pair/connect must not extend our scan beyond its time limit.
                if !stop_scan(backend, &mut scan, &mut view, &shared).await {
                    return;
                }
                refresh(backend, &mut view).await;
                match validate(&view.inventory, &action, &id).cloned() {
                    Err(error) => view.status = error,
                    Ok(device) => {
                        view.busy = true;
                        view.status = format!("{} {}…", action.label(), device.name);
                        publish(&shared, &view, &commands);
                        let seconds = if action == Action::Pair { 40 } else { 15 };
                        tracing::info!(?action, address = %device.address, "Bluetooth controller setup started.");
                        let outcome = operation(
                            backend,
                            &commands,
                            &action,
                            &device,
                            Duration::from_secs(seconds),
                        )
                        .await;
                        tracing::info!(?action, address = %device.address, ?outcome, "Bluetooth controller setup finished.");
                        match outcome {
                            Outcome::Closed => break,
                            Outcome::Cancelled => view.status = "Cancelled. A pairing that already completed is kept; use Forget to remove it.".into(),
                            Outcome::Finished(Err(error)) => view.status = error,
                            Outcome::Finished(Ok(())) => view.status = action.completed_message().into(),
                        }
                        refresh(backend, &mut view).await;
                    }
                }
            }
        }
        view.busy = false;
        publish(&shared, &view, &commands);
    }
    if let Some((adapter, _)) = scan {
        let _ = bounded(CALL_TIME, backend.stop_scan(&adapter)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        hang: Cell<bool>,
        fail: Cell<bool>,
        scan_fail: Cell<bool>,
        stop_fail: Cell<bool>,
        events: Option<async_channel::Sender<String>>,
    }

    impl Fake {
        fn record(&self, call: String) {
            self.calls.borrow_mut().push(call.clone());
            if let Some(events) = &self.events {
                events.try_send(call).unwrap();
            }
        }
    }

    fn device() -> Device {
        Device {
            id: "pad".into(),
            adapter: "adapter".into(),
            name: "Test pad".into(),
            ..Device::default()
        }
    }

    impl Backend for Fake {
        async fn inventory(&self) -> Result<Inventory, String> {
            if self.fail.get() {
                return Err("adapter unavailable".into());
            }
            Ok(Inventory {
                adapter: Some("adapter".into()),
                devices: vec![device()],
            })
        }
        async fn start_scan(&self, adapter: &str) -> Result<(), String> {
            self.record(format!("start:{adapter}"));
            if self.scan_fail.get() {
                Err("scan failed".into())
            } else {
                Ok(())
            }
        }
        async fn stop_scan(&self, adapter: &str) -> Result<(), String> {
            self.record(format!("stop:{adapter}"));
            if self.stop_fail.get() {
                Err("stop failed".into())
            } else {
                Ok(())
            }
        }
        async fn act(&self, action: &Action, device: &Device) -> Result<(), String> {
            self.record(format!("{action:?}:{}", device.id));
            if self.hang.get() {
                future::pending::<()>().await;
            }
            if self.fail.get() {
                Err("pairing rejected".into())
            } else {
                Ok(())
            }
        }
        async fn cancel(&self, action: &Action, device: &Device) {
            self.record(format!("cancel:{action:?}:{}", device.id));
        }
    }

    #[test]
    fn selected_device_is_validated_before_mutation() {
        async_io::block_on(async {
            let inventory = Fake::default().inventory().await.unwrap();
            assert!(validate(&inventory, &Action::Pair, "pad").is_ok());
            assert!(validate(&inventory, &Action::Pair, "another-pad").is_err());
            assert!(validate(&inventory, &Action::Forget, "pad").is_err());
            let mut paired = inventory;
            paired.devices[0].paired = true;
            assert!(validate(&paired, &Action::Pair, "pad").is_err());
            assert!(validate(&paired, &Action::Forget, "pad").is_ok());
        });
    }

    #[test]
    fn cancellation_close_and_timeout_cancel_the_pending_daemon_operation() {
        async_io::block_on(async {
            for mode in ["cancel", "close", "timeout"] {
                let backend = Fake::default();
                backend.hang.set(true);
                let (tx, rx) = async_channel::bounded(2);
                if mode == "cancel" {
                    tx.try_send(Command::Cancel).unwrap();
                }
                if mode == "close" {
                    tx.close();
                }
                let duration = if mode == "timeout" {
                    Duration::ZERO
                } else {
                    Duration::from_secs(100)
                };
                let outcome = operation(&backend, &rx, &Action::Pair, &device(), duration).await;
                match mode {
                    "cancel" => assert_eq!(outcome, Outcome::Cancelled),
                    "close" => assert_eq!(outcome, Outcome::Closed),
                    _ => assert!(
                        matches!(outcome, Outcome::Finished(Err(ref error)) if error.contains("timed out"))
                    ),
                }
                if mode == "timeout" {
                    assert_eq!(&*backend.calls.borrow(), &["Pair:pad", "cancel:Pair:pad"]);
                } else {
                    assert!(
                        backend.calls.borrow().is_empty(),
                        "cancellation before start must not reach the daemon"
                    );
                }
            }
        });
    }

    #[test]
    fn completion_and_rejection_are_not_confused_with_cancellation() {
        async_io::block_on(async {
            let backend = Fake::default();
            let (_tx, rx) = async_channel::bounded(1);
            assert_eq!(
                operation(&backend, &rx, &Action::Pair, &device(), CALL_TIME).await,
                Outcome::Finished(Ok(()))
            );
            assert_eq!(&*backend.calls.borrow(), &["Pair:pad"]);
            backend.fail.set(true);
            assert_eq!(
                operation(&backend, &rx, &Action::Pair, &device(), CALL_TIME).await,
                Outcome::Finished(Err("pairing rejected".into()))
            );
        });
    }

    #[test]
    fn failed_refresh_disables_actions_against_stale_devices() {
        async_io::block_on(async {
            let backend = Fake::default();
            let mut view = View::default();
            refresh(&backend, &mut view).await;
            assert_eq!(view.inventory.devices.len(), 1);
            backend.fail.set(true);
            refresh(&backend, &mut view).await;
            assert!(view.inventory.devices.is_empty());
            assert!(view.inventory.adapter.is_none());
            assert_eq!(view.status, "adapter unavailable");
        });
    }

    #[test]
    fn closing_with_queued_mutation_never_starts_it() {
        async_io::block_on(async {
            let backend = Fake::default();
            let (tx, rx) = async_channel::bounded(4);
            tx.try_send(Command::Act(Action::Pair, "pad".into()))
                .unwrap();
            tx.close();
            run(&backend, rx, Arc::new(Mutex::new(View::default()))).await;
            assert!(backend.calls.borrow().is_empty());
        });
    }

    #[test]
    fn active_scan_is_released_on_close_deadline_failure_and_pairing() {
        async_io::block_on(async {
            for mode in [
                "close",
                "deadline",
                "scan-failure",
                "stop-failure",
                "pair",
                "pair-close",
            ] {
                let (events, observed) = async_channel::unbounded();
                let backend = Fake {
                    events: Some(events),
                    ..Fake::default()
                };
                backend.scan_fail.set(mode == "scan-failure");
                backend.stop_fail.set(mode == "stop-failure");
                backend.hang.set(matches!(mode, "pair" | "pair-close"));
                let (tx, rx) = async_channel::bounded(4);
                tx.try_send(Command::Scan).unwrap();
                let view = Arc::new(Mutex::new(View::default()));
                let duration = if mode == "deadline" {
                    Duration::ZERO
                } else {
                    SCAN_TIME
                };
                let worker = run_with_scan_time(&backend, rx, Arc::clone(&view), duration);
                let driver = async {
                    assert_eq!(observed.recv().await.unwrap(), "start:adapter");
                    match mode {
                        "close" => {
                            tx.close();
                        }
                        "stop-failure" => tx.send(Command::StopScan).await.unwrap(),
                        "pair" | "pair-close" => tx
                            .send(Command::Act(Action::Pair, "pad".into()))
                            .await
                            .unwrap(),
                        _ => {}
                    }
                    assert_eq!(observed.recv().await.unwrap(), "stop:adapter");
                    if matches!(mode, "pair" | "pair-close") {
                        assert_eq!(observed.recv().await.unwrap(), "Pair:pad");
                        if mode == "pair-close" {
                            tx.close();
                        } else {
                            tx.send(Command::Cancel).await.unwrap();
                        }
                        assert_eq!(observed.recv().await.unwrap(), "cancel:Pair:pad");
                    }
                    tx.close();
                };
                // This is a hang guard, not a performance/timing assertion. The
                // deadline case uses a zero-length scan, and flows synchronize
                // through events instead of sleeping or polling wall time.
                bounded(Duration::from_secs(10), async {
                    future::zip(worker, driver).await;
                    Ok(())
                })
                .await
                .unwrap();
                assert_eq!(
                    backend
                        .calls
                        .borrow()
                        .iter()
                        .filter(|c| *c == "stop:adapter")
                        .count(),
                    1
                );
                if mode == "stop-failure" {
                    let view = view.lock().unwrap();
                    assert!(view.inventory.devices.is_empty());
                    assert_eq!(view.status, "stop failed");
                }
            }
        });
    }

    #[test]
    fn pending_commands_keep_ui_busy_through_an_inventory_publication() {
        let (tx, rx) = async_channel::bounded(1);
        tx.try_send(Command::Scan).unwrap();
        let shared = Mutex::new(View::default());
        publish(&shared, &View::default(), &rx);
        assert!(shared.lock().unwrap().busy);
        rx.try_recv().unwrap();
        publish(&shared, &View::default(), &rx);
        assert!(!shared.lock().unwrap().busy);
    }
}
