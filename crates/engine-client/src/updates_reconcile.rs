//! Startup/outcome I/O is independent of the one-slot Updates command queue.
use super::*;

pub(super) type Exchange =
    Arc<dyn Fn(&ReconcileRequest) -> Result<ReconcileReply, String> + Send + Sync>;
pub(super) struct Client {
    requests: mpsc::SyncSender<ReconcileRequest>,
    replies: mpsc::Receiver<Result<ReconcileReply, String>>,
    inflight: Option<(u64, Option<Ticket>)>,
    sequence: u64,
    last_poll: Option<Instant>,
    cleanup: bool,
}
impl Client {
    pub fn new(exchange: Exchange) -> Self {
        let (requests, work) = mpsc::sync_channel::<ReconcileRequest>(1);
        let (results, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(request) = work.recv() {
                if results.send(exchange(&request)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            replies,
            inflight: None,
            sequence: 0,
            last_poll: None,
            cleanup: false,
        }
    }

    pub fn tick(
        &mut self,
        window: &MainWindow,
        host: Option<&mut Host>,
        panel: &mut Panel,
        commands: &mpsc::SyncSender<(u64, ServiceRequest)>,
    ) {
        let Some(host) = host else { return }; // Invalid identity/listener keeps the startup fence.
        while let Ok(response) = self.replies.try_recv() {
            let inflight = self.inflight.take();
            let reply = response.ok().filter(|reply| {
                reply.validate().is_ok()
                    && Some(reply.nonce) == inflight.as_ref().map(|(nonce, _)| *nonce)
                    && reply.host == host.identity
            });
            let needs_message = host.startup_blocked || host.gate.fenced() || panel.opening;
            let Some(reply) = reply else {
                if needs_message {
                    window
                        .set_updates_message("Waiting for the device installation service…".into());
                }
                continue;
            };
            let offered = inflight.and_then(|(_, offered)| offered);
            // A Clear requested before a new offer cannot retire that offer or
            // move the host back to an older service instance.
            if matches!(reply.result, RecoveryDecision::Clear {})
                && offered.as_ref() != panel.ticket()
            {
                continue;
            }
            let resolved = match &reply.result {
                RecoveryDecision::Clear {} => None,
                RecoveryDecision::Aborted { ticket }
                | RecoveryDecision::Finished { ticket, .. } => Some(ticket),
                RecoveryDecision::Waiting {} => {
                    // A live writer holds the installation lock. Its status
                    // reports progress; this independent waiter only settles
                    // abandoned work or a terminal boot outcome.
                    if needs_message && (host.startup_blocked || panel.opening || !panel.awaiting) {
                        window.set_updates_message(
                            "Waiting for device maintenance or installation recovery…".into(),
                        );
                    }
                    continue;
                }
                RecoveryDecision::Blocked {} => {
                    if needs_message {
                        window.set_updates_message(
                            "Installation recovery needs attention. Waiting for a safe decision…"
                                .into(),
                        );
                    }
                    continue;
                }
            };
            // A delayed Clear or a different ticket cannot release a newly
            // acknowledged handoff, even if the root response is authentic.
            if host.gate.reconciled(&reply.service, resolved).is_err() {
                continue;
            }
            let startup = host.startup_blocked;
            host.startup_blocked = false;
            host.finish(Err(Failure::Stale));
            window.set_install_fenced(false);
            window.set_updates_can_cancel(true);
            let abandoned_offer =
                resolved.is_none() && offered.is_some_and(|ticket| ticket.service != reply.service);
            if resolved.is_some() || abandoned_offer {
                self.cleanup = resolved.is_some();
                panel.generation += 1;
                panel.busy = false;
                panel.awaiting = false;
                panel.opening = false;
                panel.offer = None;
                panel.active = None;
                window.set_updates_visible(true);
                window.set_updates_can_confirm(false);
                window.set_updates_message(
                    match reply.result {
                        RecoveryDecision::Clear {} => {
                            "The device service restarted before installation began. Return to Updates to check again."
                        }
                        RecoveryDecision::Aborted { .. } => {
                            "The interrupted installation did not start. You can return to the app."
                        }
                        RecoveryDecision::Finished {
                            outcome: InstallOutcome::Committed,
                            ..
                        } => "The update was installed successfully. You can return to the app.",
                        _ => "The update was rolled back. The previous software is running.",
                    }
                    .into(),
                );
            } else {
                self.cleanup = false;
                if startup && !panel.opening {
                    window.set_updates_visible(false);
                }
                if panel.opening {
                    panel.opening = false;
                    panel.busy = submit(
                        window,
                        commands,
                        panel.generation,
                        ServiceRequest::Offer { schema: SCHEMA },
                    );
                }
            }
        }
        let needed = host.startup_blocked
            || panel.opening
            || panel.awaiting
            || self.cleanup
            || host
                .pending
                .as_ref()
                .is_some_and(|pending| pending.acknowledged);
        if needed
            && self.inflight.is_none()
            && self
                .last_poll
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(500))
        {
            let Some(nonce) = self.sequence.checked_add(1) else {
                return;
            };
            self.last_poll = Some(Instant::now());
            let request = ReconcileRequest {
                schema: SCHEMA,
                nonce,
                host: host.identity.clone(),
                held: host.gate.held().cloned(),
            };
            if self.requests.try_send(request).is_ok() {
                self.sequence = nonce;
                self.inflight = Some((nonce, panel.ticket().cloned()));
            }
        }
    }
}
