//! Live bounded service endpoint. The native worker owns disk/host I/O while
//! the coordinator remains available for status and pre-admission cancellation.
use super::*;
use spacewars_control::install::{transport, wire::*};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

// Kept private: an adapter cannot enable production installation without also
// implementing the final serialized native gates here.
pub(super) trait Backend: Send + Sync + 'static {
    fn context(&self) -> Result<Context, Failure>;
    fn prepare(&self, request: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure>;
    fn abort(&self, aborted: &Aborted);
    fn accept(
        &self,
        admission: &Admission,
        ack: &Acknowledgement,
        peer: &HostInstance,
    ) -> Result<(), Failure>;
    fn execute(&self, prepare: &Prepare, admission: &Admission) -> Result<(), Failure> {
        let (ack, peer) = self.prepare(prepare)?;
        self.accept(admission, &ack, &peer)
    }
    fn settled(&self) -> bool {
        false
    }
}

pub(super) struct Admission {
    state: Arc<Mutex<Coordinator>>,
    ticket: Ticket,
    stop: Arc<AtomicBool>,
}
impl Admission {
    pub fn consume(
        &self,
        ack: &Acknowledgement,
        peer: &HostInstance,
        context: &Context,
    ) -> Result<ManualAuthorization, Failure> {
        if self.stopped() {
            return Err(Failure::NotReady);
        }
        self.state
            .lock()
            .unwrap()
            .consume(
                ack,
                peer,
                context,
                transport::boot_ms().map_err(|_| Failure::NotReady)?,
            )
            .map_err(Failure::from)
    }
    pub fn pending(&self) -> Result<(), Failure> {
        let state = self.state.lock().unwrap();
        let (_, prepare) = state.selected(&self.ticket).map_err(Failure::from)?;
        if self.stopped()
            || prepare.is_none_or(|p| !transport::boot_ms().is_ok_and(|now| now < p.expires_ms))
        {
            return Err(Failure::Expired);
        }
        Ok(())
    }
    pub fn phase(&self, phase: Phase) {
        let mut state = self.state.lock().unwrap();
        if matches!(&state.state, State::Consumed(t) if t == &self.ticket) {
            state.progress = Some(phase);
        }
    }
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

pub(super) fn host_exchange(
    request: &HostRequest,
    timeout: Duration,
) -> Result<(HostReply, HostInstance), Failure> {
    let before = crate::kiosk_health::install_instance().map_err(|_| Failure::NotReady)?;
    let reply = transport::exchange(
        Path::new(HOST_SOCKET),
        1000,
        Some(before.pid),
        request,
        timeout,
    )
    .map_err(|_| Failure::NotReady)?;
    let after = crate::kiosk_health::install_instance().map_err(|_| Failure::NotReady)?;
    if before != after {
        return Err(Failure::Stale);
    }
    Ok((reply, before))
}

enum Work {
    Request(ServiceRequest, mpsc::SyncSender<ServiceReply>),
    Reconcile,
}

pub struct Runtime {
    stop: Arc<AtomicBool>,
    recovery: Option<super::reconciliation::Runtime>,
    epoch: Id,
    // Drop listener before closing the worker queue.
    server: Option<transport::Server>,
    sender: Option<mpsc::SyncSender<Work>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Runtime {
    /// The caller holds the service's exclusive store lock and owns this path.
    pub fn bind_native(
        boot_id: String,
        updates: crate::update_runtime::Handle,
    ) -> crate::Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let path = Path::new(SERVICE_SOCKET);
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_socket() && m.uid() == 0 => std::fs::remove_file(path)?,
            Ok(_) => return Err(crate::Error::Invalid("unsafe install socket path".into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut runtime = Self::bind(
            path,
            boot_id.clone(),
            1000,
            Arc::new(super::native::Native::new(boot_id.clone(), updates)),
            true,
        )?;
        runtime.recovery = Some(super::reconciliation::Runtime::bind_native(
            runtime.epoch.clone(),
        )?);
        Ok(runtime)
    }

    fn bind(
        path: &Path,
        boot_id: String,
        uid: u32,
        backend: Arc<dyn Backend>,
        enabled: bool,
    ) -> crate::Result<Self> {
        let coordinator = Arc::new(Mutex::new(Coordinator::new(boot_id)?));
        let epoch = coordinator.lock().unwrap().epoch.clone();
        let (sender, receiver) = mpsc::sync_channel::<Work>(4);
        let shared = coordinator.clone();
        let submit = sender.clone();
        let server = transport::Server::bind(
            path,
            Arc::new(move |p| p.uid == 0 || p.uid == uid),
            Arc::new(move |bytes, _| {
                let request = ServiceRequest::from_json(bytes);
                let response = match request {
                    Err(_) => reply(&shared.lock().unwrap(), Some(Failure::Invalid), enabled),
                    Ok(_) if !enabled => {
                        reply(&shared.lock().unwrap(), Some(Failure::Unavailable), false)
                    }
                    Ok(ServiceRequest::Status { .. }) => reply(&shared.lock().unwrap(), None, true),
                    Ok(ServiceRequest::Cancel { ticket, .. }) => {
                        let mut state = shared.lock().unwrap();
                        let error = state.cancel(&ticket).err().map(Failure::from);
                        let response = reply(&state, error, true);
                        drop(state);
                        let _ = submit.try_send(Work::Reconcile);
                        response
                    }
                    Ok(request) => {
                        let (tx, rx) = mpsc::sync_channel(1);
                        if submit.try_send(Work::Request(request, tx)).is_err() {
                            reply(&shared.lock().unwrap(), Some(Failure::Busy), true)
                        } else {
                            rx.recv_timeout(Duration::from_secs(3)).unwrap_or_else(|_| {
                                reply(&shared.lock().unwrap(), Some(Failure::Busy), true)
                            })
                        }
                    }
                };
                serde_json::to_vec(&response).unwrap()
            }),
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                let mut prepare = None;
                if let Work::Request(request, response) = work {
                    // Confirmation is idempotent; retries never refresh deadlines.
                    let context = backend.context();
                    let settled = context
                        .as_ref()
                        .is_ok_and(|c| c.host.as_ref().is_some_and(|h| !h.fenced))
                        && backend.settled();
                    let mut state = coordinator.lock().unwrap();
                    if settled && matches!(state.state, State::Consumed(_)) {
                        state.state = State::Idle;
                        state.progress = None;
                    }
                    let mut offer = None;
                    if context.is_err() {
                        // Failed observation cannot leave an old offer revivable
                        // when the host reconnects. A wrong ticket still cannot
                        // revoke somebody else's current handoff.
                        let ticket = match &request {
                            ServiceRequest::Confirm { consent, .. } => Some(consent.ticket.clone()),
                            ServiceRequest::Offer { .. } => state.status().ticket,
                            _ => None,
                        };
                        if let Some(ticket) = ticket {
                            let _ = state.cancel(&ticket);
                        }
                    }
                    let result = context.and_then(|context| {
                        let now = transport::boot_ms().map_err(|_| Failure::NotReady)?;
                        match request {
                            ServiceRequest::Offer { .. } => {
                                offer = Some(state.offer(&context, now).map_err(Failure::from)?);
                            }
                            ServiceRequest::Confirm { consent, .. } => {
                                prepare = Some(
                                    state
                                        .confirm(&consent, &context, now)
                                        .map_err(Failure::from)?,
                                );
                            }
                            _ => return Err(Failure::Invalid),
                        }
                        Ok(())
                    });
                    let mut value = reply(&state, result.err(), true);
                    value.offer = offer;
                    let _ = response.send(value);
                }
                if let Some(prepare) = prepare {
                    let admission = Admission {
                        state: coordinator.clone(),
                        ticket: prepare.ticket.clone(),
                        stop: stopping.clone(),
                    };
                    if backend.execute(&prepare, &admission).is_err() {
                        let mut state = coordinator.lock().unwrap();
                        if state.cancel(&prepare.ticket).is_err()
                            && matches!(state.state, State::Consumed(_))
                        {
                            state.progress = Some(Phase::RecoveryRequired);
                        }
                    }
                }
                let aborted = reply(&coordinator.lock().unwrap(), None, true).aborted;
                if let Some(aborted) = aborted {
                    backend.abort(&aborted);
                }
            }
        });
        Ok(Self {
            stop,
            recovery: None,
            epoch,
            server: Some(server),
            sender: Some(sender),
            worker: Some(worker),
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // Stop reconciliation before allowing a parked writer to release its
        // install lock. A queued reboot is still live during service teardown.
        self.recovery.take();
        self.stop.store(true, Ordering::Relaxed);
        self.server.take();
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn reply(state: &Coordinator, error: Option<Failure>, enabled: bool) -> ServiceReply {
    let status = if enabled {
        state.status()
    } else {
        Status {
            phase: Phase::Unavailable,
            ticket: None,
        }
    };
    let aborted = if status.phase == Phase::Aborted {
        status.ticket.clone().map(|ticket| Aborted {
            schema: SCHEMA,
            ticket,
        })
    } else {
        None
    };
    ServiceReply {
        schema: SCHEMA,
        error,
        status,
        offer: None,
        aborted,
    }
}

#[cfg(test)]
#[path = "install_service_tests.rs"]
mod tests;
