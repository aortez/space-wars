use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

struct TestBackend {
    path: std::path::PathBuf,
    host: HostInstance,
    ready: Ready,
    accepted: AtomicUsize,
    revision: Arc<AtomicU64>,
    delay: Mutex<Option<Delay>>,
    parked: Mutex<Option<std::sync::Weak<()>>>,
    safe_shutdown: AtomicBool,
}
struct Delay {
    after_consumption: bool,
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}
impl Delay {
    fn wait(&self) {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(3)).unwrap();
    }
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
        admission: &Admission,
        ack: &Acknowledgement,
        peer: &HostInstance,
    ) -> Result<(), Failure> {
        let delay = self.delay.lock().unwrap().take();
        if let Some(delay) = &delay
            && !delay.after_consumption
        {
            delay.wait();
        }
        let context = self.context()?;
        let authorization = admission.consume(ack, peer, &context)?;
        assert_eq!(authorization.selection_generation(), 4);
        self.accepted.fetch_add(1, Ordering::SeqCst);
        if let Some(delay) = &delay
            && delay.after_consumption
        {
            admission.phase(Phase::Writing);
            delay.wait();
        }
        if let Some(recovery) = self.parked.lock().unwrap().take() {
            admission.phase(Phase::RebootRequested);
            while !admission.stopped() {
                thread::sleep(Duration::from_millis(1));
            }
            self.safe_shutdown
                .store(recovery.upgrade().is_none(), Ordering::SeqCst);
        }
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
            delay: Mutex::new(None),
            parked: Mutex::new(None),
            safe_shutdown: AtomicBool::new(false),
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
fn disabled_backend_exposes_no_install_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.sock");
    let _runtime =
        Runtime::bind(&path, "boot".into(), uid(), Arc::new(Unavailable), false).unwrap();
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
fn slow_reconciliation_does_not_block_live_install_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let _service =
        Runtime::bind(&path, "boot".into(), uid(), Arc::new(Unavailable), false).unwrap();
    let recovery_path = dir.path().join("reconcile.sock");
    let (entered, entries) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let released = Mutex::new(released);
    let _recovery = super::super::reconciliation::Runtime::bind(
        &recovery_path,
        uid(),
        Id::from_bytes([3; 32]).unwrap(),
        Arc::new(move |_| {
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            Ok(RecoveryDecision::Clear {})
        }),
    )
    .unwrap();
    let request = ReconcileRequest {
        schema: SCHEMA,
        nonce: 1,
        host: HostInstance {
            pid: std::process::id(),
            start_ticks: 12,
            invocation: "a".repeat(32),
        },
        held: None,
    };
    let pending = thread::spawn(move || {
        transport::exchange::<_, ReconcileReply>(
            &recovery_path,
            uid(),
            None,
            &request,
            Duration::from_secs(4),
        )
        .unwrap()
    });
    entries.recv_timeout(Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    let response: ServiceReply = transport::exchange(
        &path,
        uid(),
        None,
        &ServiceRequest::Status { schema: SCHEMA },
        Duration::from_millis(500),
    )
    .unwrap();
    assert_eq!(response.status.phase, Phase::Unavailable);
    assert!(start.elapsed() < Duration::from_millis(500));
    release.send(()).unwrap();
    assert_eq!(pending.join().unwrap().result, RecoveryDecision::Clear {});
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

struct Unavailable;
impl Backend for Unavailable {
    fn context(&self) -> Result<Context, Failure> {
        panic!("disabled backend was called")
    }
    fn prepare(&self, _: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure> {
        panic!("disabled backend was called")
    }
    fn abort(&self, _: &Aborted) {
        panic!("disabled backend was called")
    }
    fn accept(&self, _: &Admission, _: &Acknowledgement, _: &HostInstance) -> Result<(), Failure> {
        panic!("disabled backend was called")
    }
}

#[test]
fn shutdown_stops_reconciliation_before_releasing_a_queued_reboot() {
    let mut f = Fixture::new();
    let alive = Arc::new(());
    *f.backend.parked.lock().unwrap() = Some(Arc::downgrade(&alive));
    f.service.recovery = Some(
        super::super::reconciliation::Runtime::bind(
            &f._dir.path().join("recovery.sock"),
            uid(),
            Id::from_bytes([8; 32]).unwrap(),
            Arc::new(move |_| {
                assert_eq!(Arc::strong_count(&alive), 1);
                Ok(RecoveryDecision::Waiting {})
            }),
        )
        .unwrap(),
    );
    f.prepare();
    f.release.send(()).unwrap();
    f.wait(Phase::RebootRequested);
    drop(f.service);
    assert!(f.backend.safe_shutdown.load(Ordering::SeqCst));
}

#[test]
fn final_admission_and_writing_do_not_block_status_or_cancellation() {
    for after_consumption in [false, true] {
        let f = Fixture::new();
        let (entered, entries) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        *f.backend.delay.lock().unwrap() = Some(Delay {
            after_consumption,
            entered,
            release: released,
        });
        let offer = f.prepare();
        f.release.send(()).unwrap();
        entries.recv_timeout(Duration::from_secs(2)).unwrap();
        let before = Instant::now();
        let status = f.request(ServiceRequest::Status { schema: SCHEMA });
        assert_eq!(
            status.status.phase,
            if after_consumption {
                Phase::Writing
            } else {
                Phase::Preparing
            }
        );
        assert_eq!(status.status.ticket.as_ref(), Some(&offer.ticket));
        let cancel = f.request(ServiceRequest::Cancel {
            schema: SCHEMA,
            ticket: offer.ticket,
        });
        assert!(before.elapsed() < Duration::from_secs(1));
        assert_eq!(cancel.error, after_consumption.then_some(Failure::TooLate));
        release.send(()).unwrap();
        if !after_consumption {
            f.wait(Phase::Aborted);
        }
        assert_eq!(
            f.backend.accepted.load(Ordering::SeqCst),
            usize::from(after_consumption)
        );
    }
}
