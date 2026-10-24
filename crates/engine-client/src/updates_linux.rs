use super::*;
use crate::{
    input,
    settings_writer::{SaveReceipt, SettingsWriter},
};
use engine_common::Settings;
use slint::{Timer, TimerMode};
use spacewars_control::install::{transport, wire::*, *};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, RwLock, mpsc},
    time::{Duration, Instant},
};

type Reply = mpsc::SyncSender<HostReply>;
struct Incoming {
    request: HostRequest,
    reply: Reply,
}
struct Pending {
    request: Prepare,
    replies: Vec<Reply>,
    receipt: Option<SaveReceipt>,
    writer_revision: u64,
    frame: u64,
    acknowledged: bool,
}
struct Host {
    identity: HostInstance,
    gate: HostGate,
    pending: Option<Pending>,
    revision: u64,
    settings_revision: u64,
    document: Vec<u8>,
    scene: String,
    failure: Option<Failure>,
}
impl Host {
    fn new(identity: HostInstance) -> Self {
        Self {
            identity,
            gate: HostGate::default(),
            pending: None,
            revision: 1,
            settings_revision: 1,
            document: Vec::new(),
            scene: String::new(),
            failure: None,
        }
    }
    fn observe(&mut self, window: &MainWindow, settings: &Settings) {
        let document = serde_json::to_vec(settings).expect("settings serialize");
        let scene = format!(
            "{}:{}:{}:{}:{}",
            window.get_scenario_instance(),
            window.get_launcher_visible(),
            window.get_ingame_menu_visible(),
            window.get_launcher_busy(),
            window.get_updates_visible()
        );
        if document != self.document {
            self.document = document;
            self.settings_revision += 1;
            self.revision += 1;
        }
        if scene != self.scene {
            self.scene = scene;
            self.revision += 1;
        }
    }
    fn available(window: &MainWindow) -> bool {
        window.get_updates_visible()
            && !window.get_launcher_busy()
            && !window.get_clock_controls_pending()
            && !window.get_training_active()
            && !window.get_genomes_busy()
            && (window.get_launcher_visible() || window.get_ingame_menu_visible())
    }
    fn abort(&mut self, window: &MainWindow, aborted: &Aborted) {
        if self.gate.aborted(aborted).is_ok() {
            self.finish(Err(Failure::Stale));
            window.set_install_fenced(false);
            window.set_updates_can_cancel(true);
            window.set_updates_message(
                self.failure
                    .map(failure_message)
                    .unwrap_or("Installation cancelled. You can return to the app.")
                    .into(),
            );
        }
    }
    fn finish(&mut self, result: Result<Acknowledgement, Failure>) {
        if let Some(pending) = self.pending.as_mut() {
            let reply = HostReply {
                schema: SCHEMA,
                error: result.as_ref().err().copied(),
                view: None,
                acknowledgement: result.ok(),
            };
            for sender in pending.replies.drain(..) {
                let _ = sender.try_send(reply.clone());
            }
        }
        if !self.gate.fenced() {
            self.pending = None;
        }
    }
    fn request(
        &mut self,
        window: &MainWindow,
        settings: &Settings,
        writer: &SettingsWriter,
        incoming: Incoming,
        now: u64,
        frames: Option<u64>,
    ) {
        self.observe(window, settings);
        let result = match incoming.request {
            HostRequest::Observe { nonce, .. } => {
                let _ = incoming.reply.try_send(HostReply {
                    schema: SCHEMA,
                    error: None,
                    acknowledgement: None,
                    view: Some(HostView {
                        nonce,
                        host: self.identity.clone(),
                        revision: self.revision,
                        settings_revision: self.settings_revision,
                        game: !window.get_launcher_visible(),
                        fenced: self.gate.fenced(),
                        available: Self::available(window) && frames.is_some(),
                    }),
                });
                return;
            }
            HostRequest::Abort { aborted, .. } => {
                let result = self.gate.aborted(&aborted).map_err(Failure::from);
                if result.is_ok() {
                    self.abort(window, &aborted);
                }
                result
            }
            HostRequest::Prepare { prepare, .. } => {
                if let Some(frame) = frames.filter(|_| Self::available(window)) {
                    let result = self
                        .gate
                        .prepare(
                            prepare.clone(),
                            &self.identity,
                            self.revision,
                            self.settings_revision,
                            now,
                        )
                        .map_err(Failure::from);
                    if result.is_ok() {
                        if let Some(pending) = self.pending.as_mut() {
                            if pending.replies.len() >= 4 {
                                let _ = incoming.reply.try_send(failed(Failure::Busy));
                            } else {
                                pending.replies.push(incoming.reply);
                            }
                        } else {
                            self.failure = None;
                            let receipt = writer.save_exact(settings.clone());
                            self.pending = Some(Pending {
                                request: prepare,
                                replies: vec![incoming.reply],
                                receipt: Some(receipt),
                                writer_revision: writer.status().revision,
                                frame,
                                acknowledged: false,
                            });
                            window.set_install_fenced(true);
                            window.set_updates_visible(true);
                            window.set_updates_can_confirm(false);
                            window.set_updates_message(
                                "Saving settings and preparing to install…".into(),
                            );
                            window.window().request_redraw();
                        }
                        return;
                    }
                    result
                } else {
                    Err(Failure::NotReady)
                }
            }
        };
        let _ = incoming.reply.try_send(HostReply {
            schema: SCHEMA,
            error: result.err(),
            view: None,
            acknowledgement: None,
        });
    }
    fn tick(
        &mut self,
        window: &MainWindow,
        settings: &Settings,
        writer: &SettingsWriter,
        now: u64,
        frames: Option<u64>,
    ) {
        self.observe(window, settings);
        self.gate.expire(now);
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let ticket = pending.request.ticket.clone();
        if !pending.acknowledged
            && (self.revision != pending.request.host_revision
                || self.settings_revision != pending.request.settings_revision
                || writer.status().revision != pending.writer_revision
                || !Self::available(window))
        {
            let _ = self.gate.interrupt(&ticket);
            self.failure = Some(Failure::Stale);
            window.set_updates_message(failure_message(Failure::Stale).into());
            self.finish(Err(Failure::Stale));
        } else if !self.gate.fenced() {
            self.failure = Some(Failure::Expired);
            window.set_updates_message(failure_message(Failure::Expired).into());
            self.finish(Err(Failure::Expired));
        } else {
            if let Some(receipt) = &pending.receipt {
                match receipt.try_recv() {
                    Ok(result) => {
                        pending.receipt = None;
                        let saved = self.gate.saved(
                            &ticket,
                            pending.request.settings_revision,
                            result.is_ok(),
                        );
                        if saved.is_err() {
                            window.set_updates_message(
                                "Settings could not be saved. Installation stopped.".into(),
                            );
                            self.failure = Some(Failure::SaveFailed);
                            self.finish(Err(Failure::SaveFailed));
                            window.set_install_fenced(false);
                            window.set_updates_can_cancel(true);
                            return;
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let _ = self
                            .gate
                            .saved(&ticket, pending.request.settings_revision, false);
                        self.failure = Some(Failure::SaveFailed);
                        window.set_updates_message(failure_message(Failure::SaveFailed).into());
                        self.finish(Err(Failure::SaveFailed));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(pending) = self.pending.as_ref() {
                // Two completed page flips after publication exclude an already
                // in-flight frame that did not yet contain this updater screen.
                if frames.is_some_and(|n| n.saturating_sub(pending.frame) >= 2)
                    && window.get_updates_visible()
                    && window.get_install_fenced()
                {
                    let _ = self.gate.presented(&ticket);
                } else {
                    window.window().request_redraw();
                }
                if let Ok(ack) = self.gate.acknowledge(&ticket, now) {
                    self.pending.as_mut().unwrap().acknowledged = true;
                    window.set_updates_can_cancel(false);
                    window.set_updates_message(
                        "Ready to install. Waiting for the device service…".into(),
                    );
                    self.finish(Ok(ack));
                }
            }
        }
        window.set_install_fenced(self.gate.fenced());
        if !self.gate.fenced() {
            window.set_updates_can_cancel(true);
        }
    }
}
fn failed(error: Failure) -> HostReply {
    HostReply {
        schema: SCHEMA,
        error: Some(error),
        view: None,
        acknowledgement: None,
    }
}
fn presented_frames() -> Option<u64> {
    #[cfg(feature = "pi-kiosk")]
    {
        i_slint_backend_linuxkms::profiling::presented_frames()
    }
    #[cfg(not(feature = "pi-kiosk"))]
    {
        None
    }
}
fn runtime_identity(invocation: Option<String>) -> Option<HostInstance> {
    // The shipped systemd unit uses --fullscreen. The --kiosk CLI flag also
    // requests direct launch, so it cannot determine install-host eligibility.
    if !cfg!(feature = "pi-kiosk") {
        return None;
    }
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let end = stat.rfind(')')?;
    let value = HostInstance {
        pid: std::process::id(),
        start_ticks: stat[end + 1..].split_whitespace().nth(19)?.parse().ok()?,
        invocation: invocation?,
    };
    value.validate().ok()?;
    Some(value)
}

pub(crate) struct Runtime {
    _timer: Timer,
    _server: Option<transport::Server>,
}
struct Panel {
    offer: Option<Offer>,
    busy: bool,
    awaiting: bool,
    last_poll: Instant,
    generation: u64,
}
type Exchange = Arc<dyn Fn(&ServiceRequest) -> Result<ServiceReply, String> + Send + Sync>;

pub(crate) fn install(
    window: &MainWindow,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
    input: input::SharedInput,
) -> Runtime {
    let host = runtime_identity(std::env::var("INVOCATION_ID").ok()).map(Host::new);
    let available = host.is_some();
    start(
        window,
        settings,
        writer,
        input,
        host,
        PathBuf::from(HOST_SOCKET),
        Arc::new(move |request| {
            if !available {
                return Ok(ServiceReply {
                    schema: SCHEMA,
                    error: Some(Failure::Unavailable),
                    status: Status {
                        phase: Phase::Unavailable,
                        ticket: None,
                    },
                    offer: None,
                    aborted: None,
                });
            }
            transport::exchange(
                Path::new(SERVICE_SOCKET),
                0,
                None,
                request,
                Duration::from_secs(4),
            )
            .map_err(|e| e.to_string())
        }),
        Rc::new(presented_frames),
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn start(
    window: &MainWindow,
    settings: Arc<RwLock<Settings>>,
    writer: SettingsWriter,
    input: input::SharedInput,
    host: Option<Host>,
    socket: PathBuf,
    exchange: Exchange,
    frames: Rc<dyn Fn() -> Option<u64>>,
    service_uid: u32,
) -> Runtime {
    let host = Rc::new(RefCell::new(host));
    let activity_host = host.clone();
    let activity_window = window.as_weak();
    window.on_updates_activity(move || {
        if let Some(w) = activity_window.upgrade()
            && !w.get_updates_visible()
            && let Some(host) = activity_host.borrow_mut().as_mut()
        {
            host.revision += 1;
        }
    });
    let interruption_host = host.clone();
    window.on_updates_interrupted(move || {
        if let Some(host) = interruption_host.borrow_mut().as_mut() {
            host.revision += 1;
        }
    });
    let (incoming, requests) = mpsc::sync_channel::<Incoming>(4);
    let server = if host.borrow().is_some() {
        match transport::Server::bind(
            &socket,
            Arc::new(move |p| p.uid == service_uid),
            Arc::new(move |bytes, _| {
                let result = HostRequest::from_json(bytes)
                    .map_err(|_| Failure::Invalid)
                    .and_then(|request| {
                        let (reply, receipt) = mpsc::sync_channel(1);
                        incoming
                            .try_send(Incoming { request, reply })
                            .map_err(|_| Failure::Busy)?;
                        receipt
                            .recv_timeout(Duration::from_secs(31))
                            .map_err(|_| Failure::Expired)
                    })
                    .unwrap_or_else(failed);
                serde_json::to_vec(&result).unwrap()
            }),
        ) {
            Ok(server) => Some(server),
            Err(error) => {
                tracing::error!(%error, "install host socket unavailable");
                host.borrow_mut().take();
                None
            }
        }
    } else {
        None
    };
    let (commands, work) = mpsc::sync_channel::<(u64, ServiceRequest)>(1);
    let (replies, results) = mpsc::sync_channel(2);
    std::thread::spawn(move || {
        while let Ok((generation, request)) = work.recv() {
            if replies.send((generation, exchange(&request))).is_err() {
                break;
            }
        }
    });
    let panel = Rc::new(RefCell::new(Panel {
        offer: None,
        busy: false,
        awaiting: false,
        last_poll: Instant::now(),
        generation: 0,
    }));
    let weak = window.as_weak();
    let state = panel.clone();
    let sender = commands.clone();
    window.on_updates_open(move || {
        let Some(w) = weak.upgrade() else { return };
        if w.get_install_fenced()
            || w.get_launcher_busy()
            || (!w.get_launcher_visible() && !w.get_ingame_menu_visible())
        {
            return;
        }
        w.set_updates_visible(true);
        w.set_updates_focus(1);
        w.set_updates_can_confirm(false);
        w.set_updates_can_cancel(true);
        w.set_updates_message("Checking installation availability…".into());
        let mut p = state.borrow_mut();
        p.generation += 1;
        p.offer = None;
        p.awaiting = false;
        p.busy = submit(
            &w,
            &sender,
            p.generation,
            ServiceRequest::Offer { schema: SCHEMA },
        );
    });
    let weak = window.as_weak();
    let state = panel.clone();
    let sender = commands.clone();
    window.on_updates_confirm(move || {
        let Some(w) = weak.upgrade() else { return };
        let mut p = state.borrow_mut();
        if p.busy || !w.get_updates_can_confirm() {
            return;
        }
        let Some(offer) = p.offer.as_ref() else {
            return;
        };
        let consent = Consent {
            schema: SCHEMA,
            ticket: offer.ticket.clone(),
            end_game: offer.ends_game,
        };
        p.busy = submit(
            &w,
            &sender,
            p.generation,
            ServiceRequest::Confirm {
                schema: SCHEMA,
                consent,
            },
        );
        p.awaiting = p.busy;
        if p.busy {
            w.set_updates_can_confirm(false);
            w.set_updates_message("Preparing installation…".into());
        }
    });
    let weak = window.as_weak();
    let state = panel.clone();
    let sender = commands.clone();
    window.on_updates_cancel(move || {
        let Some(w) = weak.upgrade() else { return };
        let mut p = state.borrow_mut();
        if !w.get_updates_can_cancel() {
            return;
        }
        if let Some(offer) = p.offer.as_ref() {
            let command = ServiceRequest::Cancel {
                schema: SCHEMA,
                ticket: offer.ticket.clone(),
            };
            if submit(&w, &sender, p.generation, command) {
                p.busy = true;
                p.awaiting = true;
                w.set_updates_message("Cancelling installation…".into());
            }
        } else if !w.get_install_fenced() {
            w.set_updates_visible(false);
            p.generation += 1;
            p.busy = false;
        }
    });
    let timer = Timer::default();
    let weak = window.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(20), move || {
        let Some(w) = weak.upgrade() else { return };
        let now = transport::boot_ms().unwrap_or(u64::MAX);
        let batch: Vec<_> = requests.try_iter().take(4).collect();
        if let Some(host) = host.borrow_mut().as_mut()
            && (w.get_updates_visible() || host.pending.is_some() || !batch.is_empty())
        {
            let mut snapshot = settings.read().unwrap().clone();
            let draft_valid = if w.get_launcher_visible() {
                match crate::launcher_selections_from_window(&w, &snapshot) {
                    Ok(selections) => {
                        crate::apply_launcher_selections(&mut snapshot, &selections);
                        true
                    }
                    Err(_) => false,
                }
            } else {
                true
            };
            for incoming in batch {
                let was_preparing = host.pending.is_some();
                host.request(
                    &w,
                    &snapshot,
                    &writer,
                    incoming,
                    now,
                    if draft_valid { frames() } else { None },
                );
                if !was_preparing && host.pending.is_some() {
                    // Preserve the saved launcher draft in memory too, so a
                    // later ordinary save after cancellation cannot undo it.
                    *settings.write().unwrap() = snapshot.clone();
                }
            }
            host.tick(
                &w,
                &snapshot,
                &writer,
                now,
                if draft_valid { frames() } else { None },
            );
            if host.gate.fenced() {
                input.borrow_mut().clear();
            }
        }
        let mut p = panel.borrow_mut();
        while let Ok((generation, reply)) = results.try_recv() {
            if generation != p.generation {
                continue;
            }
            p.busy = false;
            match reply {
                Ok(reply) => {
                    if let Some(aborted) = &reply.aborted {
                        if let Some(host) = host.borrow_mut().as_mut() {
                            host.abort(&w, aborted);
                        }
                        if p.offer.as_ref().is_some_and(|o| o.ticket == aborted.ticket) {
                            p.offer = None;
                            p.awaiting = false;
                        }
                        w.set_updates_can_confirm(false);
                        w.set_updates_can_cancel(!w.get_install_fenced());
                    } else if let Some(offer) = reply.offer {
                        w.set_updates_message(
                            format!(
                                "{:?} · generation {}\n{}\nImage {}\n\n{}",
                                offer.candidate.channel,
                                offer.candidate.generation,
                                offer.candidate.build_id,
                                offer.candidate.id.as_str(),
                                if offer.ends_game {
                                    "This ends the current game and restarts the device."
                                } else {
                                    "Installation restarts the device."
                                }
                            )
                            .into(),
                        );
                        w.set_updates_confirm_label(
                            if offer.ends_game {
                                "End game, install & restart"
                            } else {
                                "Install & restart"
                            }
                            .into(),
                        );
                        w.set_updates_can_confirm(true);
                        p.offer = Some(offer);
                    } else if let Some(error) = reply.error {
                        w.set_updates_can_confirm(false);
                        w.set_updates_message(if w.get_install_fenced() {
                            "Waiting for the device service. The app remains held for installation."
                                .into()
                        } else {
                            failure_message(error).into()
                        });
                    }
                }
                Err(_) => {
                    w.set_updates_can_confirm(false);
                    w.set_updates_message(
                        if w.get_install_fenced() {
                            "Device service disconnected. Waiting for an installation decision…"
                        } else {
                            "The device installation service is unavailable."
                        }
                        .into(),
                    );
                }
            }
        }
        if !p.awaiting
            && p.offer
                .as_ref()
                .is_some_and(|offer| now >= offer.expires_ms)
        {
            w.set_updates_can_confirm(false);
            w.set_updates_message(failure_message(Failure::Expired).into());
        }
        if w.get_updates_visible()
            && !p.busy
            && p.awaiting
            && p.offer.is_some()
            && p.last_poll.elapsed() >= Duration::from_millis(250)
        {
            p.last_poll = Instant::now();
            p.busy = commands
                .try_send((p.generation, ServiceRequest::Status { schema: SCHEMA }))
                .is_ok();
        }
    });
    Runtime {
        _timer: timer,
        _server: server,
    }
}

fn submit(
    window: &MainWindow,
    sender: &mpsc::SyncSender<(u64, ServiceRequest)>,
    generation: u64,
    request: ServiceRequest,
) -> bool {
    match sender.try_send((generation, request)) {
        Ok(()) => true,
        Err(error) => {
            window.set_updates_message(match error {
                mpsc::TrySendError::Full(_) => "Updates is busy. Please try again.".into(),
                mpsc::TrySendError::Disconnected(_) => {
                    "The device installation service is unavailable.".into()
                }
            });
            false
        }
    }
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;

fn failure_message(error: Failure) -> &'static str {
    match error {
        Failure::Unavailable => "Installation is not available in this software build.",
        Failure::NotReady => "No verified update is ready to install.",
        Failure::Busy => "The device is busy. Return to Updates to try again.",
        Failure::Expired | Failure::Stale => {
            "This installation offer has expired or changed. Return to Updates to check again."
        }
        Failure::SaveFailed => "Settings could not be saved. Installation stopped.",
        Failure::TooLate => {
            "The installation handoff is already complete. Waiting for the device service…"
        }
        _ => "The installation request was rejected. Return to Updates to check again.",
    }
}
