use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

struct TestBackend {
    path: std::path::PathBuf,
    host: HostInstance,
    ready: Ready,
    accepted: AtomicUsize,
    revision: Arc<AtomicU64>,
}
impl Backend for TestBackend {
    fn context(&self) -> Result<Context, Failure> {
        if self.revision.load(Ordering::SeqCst) == 0 {
            return Err(Failure::NotReady);
        }
        let nonce = transport::boot_ms().unwrap();
        let reply: HostReply = transport::exchange(
            &self.path,
            uid(),
            Some(self.host.pid),
            &HostRequest::Observe {
                schema: SCHEMA,
                nonce,
            },
            Duration::from_secs(1),
        )
        .map_err(|_| Failure::NotReady)?;
        let view = reply.view.ok_or(Failure::NotReady)?;
        if view.nonce != nonce || view.host != self.host || !view.available {
            return Err(Failure::Stale);
        }
        Ok(Context {
            boot_id: "boot".into(),
            ready: Some(self.ready.clone()),
            maintenance_clear: true,
            host: Some(HostObservation {
                instance: self.host.clone(),
                revision: view.revision,
                settings_revision: view.settings_revision,
                game: view.game,
                fenced: view.fenced,
                observed_ms: nonce,
            }),
        })
    }
    fn prepare(&self, prepare: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure> {
        let reply: HostReply = transport::exchange(
            &self.path,
            uid(),
            Some(self.host.pid),
            &HostRequest::Prepare {
                schema: SCHEMA,
                prepare: prepare.clone(),
            },
            Duration::from_secs(3),
        )
        .map_err(|_| Failure::NotReady)?;
        Ok((
            reply.acknowledgement.ok_or(Failure::NotReady)?,
            self.host.clone(),
        ))
    }
    fn abort(&self, aborted: &Aborted) {
        let _: Result<HostReply, _> = transport::exchange(
            &self.path,
            uid(),
            Some(self.host.pid),
            &HostRequest::Abort {
                schema: SCHEMA,
                aborted: aborted.clone(),
            },
            Duration::from_secs(1),
        );
    }
    fn accept(
        &self,
        state: &mut Coordinator,
        ack: &Acknowledgement,
        peer: &HostInstance,
    ) -> Result<(), Failure> {
        let context = self.context()?;
        let authorization = state
            .consume(ack, peer, &context, transport::boot_ms().unwrap())
            .map_err(Failure::from)?;
        assert_eq!(authorization.selection_generation(), 4);
        self.accepted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
struct Fixture {
    _dir: tempfile::TempDir,
    _host: transport::Server,
    service: Runtime,
    path: std::path::PathBuf,
    backend: Arc<TestBackend>,
    entered: mpsc::Receiver<()>,
    release: mpsc::SyncSender<()>,
    fenced: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("service.sock");
        let host_path = dir.path().join("host.sock");
        let host = HostInstance {
            pid: std::process::id(),
            start_ticks: 12,
            invocation: "a".repeat(32),
        };
        let revision = Arc::new(AtomicU64::new(1));
        let observed = revision.clone();
        let identity = host.clone();
        let fenced = Arc::new(AtomicBool::new(false));
        let fence = fenced.clone();
        let (entered, entries) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let released = Mutex::new(released);
        let server = transport::Server::bind(
            &host_path,
            Arc::new(|p| p.uid == uid()),
            Arc::new(move |bytes, _| {
                let mut reply = HostReply {
                    schema: SCHEMA,
                    error: None,
                    view: None,
                    acknowledgement: None,
                };
                match HostRequest::from_json(bytes).unwrap() {
                    HostRequest::Observe { nonce, .. } => {
                        reply.view = Some(HostView {
                            nonce,
                            host: identity.clone(),
                            revision: observed.load(Ordering::SeqCst),
                            settings_revision: 1,
                            game: true,
                            fenced: fence.load(Ordering::SeqCst),
                            available: true,
                        })
                    }
                    HostRequest::Prepare { prepare, .. } => {
                        fence.store(true, Ordering::SeqCst);
                        entered.send(()).unwrap();
                        released
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(3))
                            .unwrap();
                        reply.acknowledgement = Some(Acknowledgement {
                            schema: SCHEMA,
                            ticket: prepare.ticket,
                            host: identity.clone(),
                            host_revision: prepare.host_revision,
                            settings_revision: prepare.settings_revision,
                        });
                    }
                    HostRequest::Abort { .. } => fence.store(false, Ordering::SeqCst),
                }
                serde_json::to_vec(&reply).unwrap()
            }),
        )
        .unwrap();
        let backend = Arc::new(TestBackend {
            path: host_path,
            host,
            ready: Ready {
                candidate: Candidate {
                    id: Id::from_bytes([2; 32]).unwrap(),
                    channel: Channel::Release,
                    generation: 11,
                    build_id: "test-image".into(),
                },
                selection_generation: 4,
            },
            accepted: AtomicUsize::new(0),
            revision,
        });
        let service = Runtime::bind(&path, "boot".into(), uid(), backend.clone(), true).unwrap();
        Self {
            _dir: dir,
            _host: server,
            service,
            path,
            backend,
            entered: entries,
            release,
            fenced,
        }
    }
    fn request(&self, request: ServiceRequest) -> ServiceReply {
        transport::exchange(&self.path, uid(), None, &request, Duration::from_secs(4)).unwrap()
    }
    fn prepare(&self) -> Offer {
        let offer = self
            .request(ServiceRequest::Offer { schema: SCHEMA })
            .offer
            .unwrap();
        let response = self.request(ServiceRequest::Confirm {
            schema: SCHEMA,
            consent: Consent {
                schema: SCHEMA,
                ticket: offer.ticket.clone(),
                end_game: true,
            },
        });
        assert_eq!(response.status.phase, Phase::Preparing);
        self.entered.recv_timeout(Duration::from_secs(2)).unwrap();
        offer
    }
    fn wait(&self, phase: Phase) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self
            .request(ServiceRequest::Status { schema: SCHEMA })
            .status
            .phase
            != phase
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
}
#[test]
fn actual_service_and_host_sockets_consume_once_and_reject_late_cancel() {
    let f = Fixture::new();
    let offer = f.prepare();
    f.release.send(()).unwrap();
    f.wait(Phase::Acknowledged);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.request(ServiceRequest::Cancel {
            schema: SCHEMA,
            ticket: offer.ticket.clone()
        })
        .error,
        Some(Failure::TooLate)
    );
    assert_eq!(
        f.request(ServiceRequest::Confirm {
            schema: SCHEMA,
            consent: Consent {
                schema: SCHEMA,
                ticket: offer.ticket,
                end_game: true
            }
        })
        .error,
        Some(Failure::TooLate)
    );
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 1);
    assert!(f.fenced.load(Ordering::SeqCst));
}
#[test]
fn cancellation_stays_responsive_during_delayed_ack_and_survives_lost_reply() {
    let f = Fixture::new();
    let offer = f.prepare();
    let before = Instant::now();
    let response = f.request(ServiceRequest::Cancel {
        schema: SCHEMA,
        ticket: offer.ticket.clone(),
    });
    assert_eq!(response.aborted.unwrap().ticket, offer.ticket);
    assert!(before.elapsed() < Duration::from_secs(1));
    assert!(
        f.request(ServiceRequest::Status { schema: SCHEMA })
            .aborted
            .is_some()
    );
    f.release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while f.fenced.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);
}
#[test]
fn changed_host_aborts_and_service_restart_cannot_reuse_consent() {
    let mut f = Fixture::new();
    let offer = f.prepare();
    f.backend.revision.store(2, Ordering::SeqCst);
    f.release.send(()).unwrap();
    f.wait(Phase::Aborted);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);
    let other = f._dir.path().join("replacement.sock");
    f.service = Runtime::bind(&other, "boot".into(), uid(), f.backend.clone(), true).unwrap();
    f.path = other;
    assert_eq!(
        f.request(ServiceRequest::Confirm {
            schema: SCHEMA,
            consent: Consent {
                schema: SCHEMA,
                ticket: offer.ticket,
                end_game: true
            }
        })
        .error,
        Some(Failure::Stale)
    );
}
#[test]
fn production_backend_exposes_no_install_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.sock");
    let _runtime = Runtime::bind(&path, "boot".into(), uid(), Arc::new(Native), false).unwrap();
    for request in [
        ServiceRequest::Status { schema: SCHEMA },
        ServiceRequest::Offer { schema: SCHEMA },
    ] {
        let reply: ServiceReply =
            transport::exchange(&path, uid(), None, &request, Duration::from_secs(1)).unwrap();
        assert_eq!(reply.status.phase, Phase::Unavailable);
        assert!(reply.offer.is_none());
    }
}

#[test]
fn a_failed_observation_retires_the_offer_even_after_the_host_recovers() {
    let f = Fixture::new();
    let offer = f
        .request(ServiceRequest::Offer { schema: SCHEMA })
        .offer
        .unwrap();
    f.backend.revision.store(0, Ordering::SeqCst);
    let request = ServiceRequest::Confirm {
        schema: SCHEMA,
        consent: Consent {
            schema: SCHEMA,
            ticket: offer.ticket,
            end_game: true,
        },
    };
    let refused = f.request(request.clone());
    assert_eq!(refused.error, Some(Failure::NotReady));
    assert_eq!(refused.status.phase, Phase::Aborted);
    f.backend.revision.store(1, Ordering::SeqCst);
    assert_eq!(f.request(request).error, Some(Failure::Stale));
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);
}
