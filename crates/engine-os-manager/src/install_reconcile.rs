//! Independent bounded worker for startup/outcome decisions. It never writes a
//! root slot, changes boot selectors, restarts recovery or replays consent.
use super::{
    HostObservation,
    journal::{Journal, Outcome, Resolution},
    service::host_exchange,
};
use crate::{Error, Result};
use engine_ota::{RecoveryStore, boot_linux::BootController, install_lock::InstallLock};
use spacewars_control::install::{Id, Message, SCHEMA, transport, wire::*};
use std::{
    path::Path,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

type Decide = Arc<dyn Fn(&ReconcileRequest) -> Result<RecoveryDecision> + Send + Sync>;
pub(super) struct Runtime {
    server: Option<transport::Server>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Runtime {
    pub fn bind_native(epoch: Id) -> Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let path = Path::new(RECONCILE_SOCKET);
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_socket() && m.uid() == 0 => std::fs::remove_file(path)?,
            Ok(_) => {
                return Err(Error::Invalid(
                    "unsafe install reconciliation socket".into(),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut renewed = None::<Instant>;
        Self::bind_with_background(
            path,
            1000,
            epoch,
            Arc::new(native_decision),
            Some(Box::new(move || {
                // Journal reads need no install authority and remain possible while
                // the independent recovery service holds the installation lock.
                let token = Journal::existing_native()
                    .ok()
                    .flatten()
                    .and_then(|j| j.deployment().ok().flatten());
                if let Some(token) = token {
                    if renewed.is_none_or(|last| last.elapsed() >= Duration::from_secs(30)) {
                        renewed = Some(Instant::now());
                        let _ = super::hold::renew(&token);
                    }
                    if let Ok(host) = crate::kiosk_health::install_instance() {
                        let _ = native_decision(&ReconcileRequest {
                            schema: SCHEMA,
                            nonce: transport::boot_ms().unwrap_or(1).max(1),
                            host,
                            held: None,
                        });
                    }
                }
            })),
        )
    }

    #[cfg(test)]
    pub(super) fn bind(path: &Path, uid: u32, epoch: Id, decide: Decide) -> Result<Self> {
        Self::bind_with_background(path, uid, epoch, decide, None)
    }

    fn bind_with_background(
        path: &Path,
        uid: u32,
        epoch: Id,
        decide: Decide,
        mut background: Option<Box<dyn FnMut() + Send>>,
    ) -> Result<Self> {
        let (send, receive) =
            mpsc::sync_channel::<(ReconcileRequest, mpsc::SyncSender<ReconcileReply>)>(1);
        let response_epoch = epoch.clone();
        let server = transport::Server::bind(
            path,
            Arc::new(move |peer| peer.uid == uid),
            Arc::new(move |bytes, peer| {
                let Ok(request) = ReconcileRequest::from_json(bytes) else {
                    return br#"{"error":"invalid"}"#.to_vec();
                };
                let mut response = ReconcileReply {
                    schema: SCHEMA,
                    nonce: request.nonce,
                    host: request.host.clone(),
                    service: response_epoch.clone(),
                    result: RecoveryDecision::Blocked {},
                };
                if peer.pid == request.host.pid {
                    let (reply, result) = mpsc::sync_channel(1);
                    response.result = RecoveryDecision::Waiting {};
                    if send.try_send((request, reply)).is_ok()
                        && let Ok(value) = result.recv_timeout(Duration::from_secs(5))
                    {
                        response = value;
                    }
                }
                serde_json::to_vec(&response).unwrap()
            }),
        )?;
        let worker = thread::spawn(move || {
            let mut last_error = None;
            let mut maintained = Instant::now();
            loop {
                if maintained.elapsed() >= Duration::from_secs(2) {
                    if let Some(background) = &mut background {
                        background();
                    }
                    maintained = Instant::now();
                }
                let (request, reply) = match receive.recv_timeout(Duration::from_secs(2)) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let result = match decide(&request) {
                    Ok(result) => {
                        last_error = None;
                        result
                    }
                    Err(Error::Ota(engine_ota::Error::Busy)) => RecoveryDecision::Waiting {},
                    Err(error) => {
                        let detail = error.to_string();
                        if last_error.as_ref() != Some(&detail) {
                            tracing::warn!(%error, "installation reconciliation blocked");
                            last_error = Some(detail);
                        }
                        RecoveryDecision::Blocked {}
                    }
                };
                let _ = reply.try_send(ReconcileReply {
                    schema: SCHEMA,
                    nonce: request.nonce,
                    host: request.host,
                    service: epoch.clone(),
                    result,
                });
            }
        });
        Ok(Self {
            server: Some(server),
            worker: Some(worker),
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.server.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn native_decision(request: &ReconcileRequest) -> Result<RecoveryDecision> {
    let before = crate::kiosk_health::install_instance()?;
    if before != request.host {
        return Err(invalid("reconciliation host changed"));
    }
    let decision = native_locked(request)?;
    if crate::kiosk_health::install_instance()? != before {
        return Err(invalid("reconciliation host changed"));
    }
    Ok(decision)
}

fn native_locked(request: &ReconcileRequest) -> Result<RecoveryDecision> {
    let lock = InstallLock::acquire()?;
    let recovery_path = Path::new(crate::recovery_service::RECOVERY_DIRECTORY);
    match std::fs::symlink_metadata(recovery_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Unprovisioned images have never supported manual installation.
            // An existing handoff directory is not evidence of a fresh device.
            return Ok(
                if Journal::existing_native()?.is_none() && request.held.is_none() {
                    RecoveryDecision::Clear {}
                } else {
                    RecoveryDecision::Blocked {}
                },
            );
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    engine_ota::bootstrap::require_no_active_bootstrap(&lock)?;
    let boot = BootController::open(&lock)?;
    let mut recovery = RecoveryStore::open(recovery_path)?;
    let mut journal = Journal::open_native()?;
    let mut resolution = journal.reconcile(&lock, &recovery)?;
    if matches!(resolution, Resolution::InProgress { .. }) {
        if !boot.settle_interrupted_install(&mut recovery)? {
            return Ok(RecoveryDecision::Waiting {});
        }
        resolution = journal.reconcile(&lock, &recovery)?;
    }
    // A terminal file alone cannot approve a mismatched boot or an unfinished
    // watchdog completion. Recovery continues independently while the UI waits.
    boot.authorize_maintenance(&recovery)?;
    let decision = decision(&resolution, request.held.as_ref());
    if matches!(
        decision,
        RecoveryDecision::Aborted { .. } | RecoveryDecision::Finished { .. }
    ) {
        let host = observe_host(request)?;
        if !host.fenced && request.held.is_none() {
            let ticket = match &resolution {
                Resolution::Aborted(aborted) => &aborted.ticket,
                Resolution::Finished { ticket, .. } => ticket,
                _ => unreachable!(),
            };
            if let Some(token) = journal.deployment()? {
                super::hold::release(&token)?;
            }
            journal.release(&lock, &recovery, ticket, || observe_host(request))?;
            return Ok(RecoveryDecision::Clear {});
        }
    }
    Ok(decision)
}

fn observe_host(request: &ReconcileRequest) -> Result<HostObservation> {
    let observed_ms = transport::boot_ms()?;
    let (reply, peer) = host_exchange(
        &HostRequest::Observe {
            schema: SCHEMA,
            nonce: request.nonce,
        },
        Duration::from_secs(1),
    )
    .map_err(|_| invalid("host observation unavailable"))?;
    let view = reply
        .view
        .ok_or_else(|| invalid("host observation unavailable"))?;
    if peer != request.host || view.host != peer || view.nonce != request.nonce {
        return Err(invalid("host observation changed"));
    }
    Ok(HostObservation {
        instance: peer,
        revision: view.revision,
        settings_revision: view.settings_revision,
        game: view.game,
        fenced: view.fenced,
        observed_ms,
    })
}

fn decision(
    resolution: &Resolution,
    held: Option<&spacewars_control::install::Ticket>,
) -> RecoveryDecision {
    match resolution {
        Resolution::Clear if held.is_none() => RecoveryDecision::Clear {},
        Resolution::Aborted(aborted) if held.is_none_or(|held| held == &aborted.ticket) => {
            RecoveryDecision::Aborted {
                ticket: aborted.ticket.clone(),
            }
        }
        Resolution::Finished {
            ticket,
            transaction,
            outcome,
        } if held.is_none_or(|held| held == ticket) => RecoveryDecision::Finished {
            ticket: ticket.clone(),
            transaction: Id::from_bytes(*transaction.as_bytes())
                .expect("recovery transaction hash"),
            outcome: match outcome {
                Outcome::Committed => InstallOutcome::Committed,
                Outcome::RolledBack => InstallOutcome::RolledBack,
            },
        },
        Resolution::InProgress { .. } => RecoveryDecision::Waiting {},
        _ => RecoveryDecision::Blocked {},
    }
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "install_reconcile_tests.rs"]
mod tests;
