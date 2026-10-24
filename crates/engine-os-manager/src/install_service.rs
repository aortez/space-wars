//! Live bounded service endpoint. The production backend stays unavailable until
//! the native journal/locks/writer adapter can atomically accept authorization.
use super::*;
use spacewars_control::install::{transport, wire::*};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

// Kept private: an adapter cannot enable production installation without also
// implementing the final serialized native gates here.
trait Backend: Send + Sync + 'static {
    fn context(&self) -> Result<Context, Failure>;
    fn prepare(&self, request: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure>;
    fn abort(&self, aborted: &Aborted);
    fn accept(
        &self,
        coordinator: &mut Coordinator,
        ack: &Acknowledgement,
        peer: &HostInstance,
    ) -> Result<(), Failure>;
}

struct Native;
impl Backend for Native {
    fn context(&self) -> Result<Context, Failure> {
        Err(Failure::Unavailable)
    }
    fn prepare(&self, request: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure> {
        let (reply, peer) = host_exchange(
            &HostRequest::Prepare {
                schema: SCHEMA,
                prepare: request.clone(),
            },
            Duration::from_secs(31),
        )?;
        if let Some(error) = reply.error {
            return Err(error);
        }
        Ok((reply.acknowledgement.ok_or(Failure::Invalid)?, peer))
    }
    fn abort(&self, aborted: &Aborted) {
        let _ = host_exchange(
            &HostRequest::Abort {
                schema: SCHEMA,
                aborted: aborted.clone(),
            },
            Duration::from_secs(2),
        );
    }
    fn accept(
        &self,
        _: &mut Coordinator,
        _: &Acknowledgement,
        _: &HostInstance,
    ) -> Result<(), Failure> {
        Err(Failure::Unavailable)
    }
}

fn host_exchange(
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
    // Drop listener before closing the worker queue.
    server: Option<transport::Server>,
    sender: Option<mpsc::SyncSender<Work>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Runtime {
    /// The caller holds the service's exclusive store lock and owns this path.
    pub fn bind_native(boot_id: String) -> crate::Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let path = Path::new(SERVICE_SOCKET);
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_socket() && m.uid() == 0 => std::fs::remove_file(path)?,
            Ok(_) => return Err(crate::Error::Invalid("unsafe install socket path".into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Self::bind(path, boot_id, 1000, Arc::new(Native), false)
    }

    fn bind(
        path: &Path,
        boot_id: String,
        uid: u32,
        backend: Arc<dyn Backend>,
        enabled: bool,
    ) -> crate::Result<Self> {
        let coordinator = Arc::new(Mutex::new(Coordinator::new(boot_id)?));
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
        let worker = thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                let mut prepare = None;
                if let Work::Request(request, response) = work {
                    // Confirmation is idempotent; retries never refresh deadlines.
                    let context = backend.context();
                    let mut state = coordinator.lock().unwrap();
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
                    let accepted = backend.prepare(&prepare);
                    let mut state = coordinator.lock().unwrap();
                    let result =
                        accepted.and_then(|(ack, peer)| backend.accept(&mut state, &ack, &peer));
                    if result.is_err() {
                        let _ = state.cancel(&prepare.ticket);
                    }
                }
                let aborted = reply(&coordinator.lock().unwrap(), None, true).aborted;
                if let Some(aborted) = aborted {
                    backend.abort(&aborted);
                }
            }
        });
        Ok(Self {
            server: Some(server),
            sender: Some(sender),
            worker: Some(worker),
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
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
