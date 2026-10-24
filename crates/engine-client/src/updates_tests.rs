use super::*;
use slint::Rgb8Pixel;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use std::{
    cell::Cell,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

struct TestPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}
struct Harness {
    window: MainWindow,
    adapter: Rc<MinimalSoftwareWindow>,
    frames: Rc<Cell<u64>>,
    pixels: Vec<Rgb8Pixel>,
    input: input::SharedInput,
}
impl Harness {
    fn new() -> Self {
        let adapter = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(TestPlatform(adapter.clone()))).unwrap();
        let window = MainWindow::new().unwrap();
        window
            .window()
            .set_size(slint::LogicalSize::new(800., 480.));
        // A paused scenario has a real current session and doesn't need a
        // launcher draft. Its settings document still must reach disk.
        window.set_launcher_visible(false);
        window.set_ingame_menu_visible(true);
        window.set_sound_visible(true);
        window.set_scenario_instance("42".into());
        let (input, _) = input::new_shared_input();
        crate::install_ui_navigation(&window);
        crate::install_keyboard_navigation(&window, input.clone());
        window.show().unwrap();
        Self {
            window,
            adapter,
            frames: Rc::new(Cell::new(0)),
            pixels: vec![Rgb8Pixel::default(); 800 * 480],
            input,
        }
    }
    fn pump(&mut self, render: bool) {
        slint::platform::update_timers_and_animations();
        if render {
            self.adapter.draw_if_needed(|renderer| {
                renderer.render(&mut self.pixels, 800);
                self.frames.set(self.frames.get() + 1);
            });
        }
        thread::sleep(Duration::from_millis(5));
    }
    fn until(&mut self, render: bool, mut done: impl FnMut(&MainWindow) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(&self.window) {
            assert!(
                Instant::now() < deadline,
                "UI wait timed out: {}",
                self.window.get_updates_message()
            );
            self.pump(render);
        }
    }
}
fn host_identity() -> HostInstance {
    HostInstance {
        pid: std::process::id(),
        start_ticks: 10,
        invocation: "b".repeat(32),
    }
}
fn ticket() -> Ticket {
    Ticket {
        service: Id::from_bytes([1; 32]).unwrap(),
        sequence: 1,
        candidate: Id::from_bytes([2; 32]).unwrap(),
    }
}
fn uid() -> u32 {
    unsafe { nix::libc::geteuid() }
}
fn response(phase: Phase) -> ServiceReply {
    ServiceReply {
        schema: SCHEMA,
        error: None,
        status: Status {
            phase,
            ticket: Some(ticket()),
        },
        offer: None,
        aborted: None,
    }
}
fn host_request(path: &Path, request: HostRequest) -> HostReply {
    transport::exchange(
        path,
        uid(),
        Some(std::process::id()),
        &request,
        Duration::from_secs(4),
    )
    .unwrap()
}

fn pumping_exchange(h: &mut Harness, path: &Path, request: HostRequest) -> HostReply {
    let path = path.to_owned();
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = send.send(host_request(&path, request));
    });
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(reply) = receive.try_recv() {
            return reply;
        }
        assert!(Instant::now() < end);
        h.pump(true);
    }
}

#[test]
fn startup_waits_for_authenticated_matching_recovery_and_defers_launch_exactly_once() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let service_path = dir.path().join("reconcile.sock");
    let path = service_path.clone();
    let host_path = dir.path().join("host.sock");
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        host_path.clone(),
        Arc::new(|_| Err("unused".into())),
        Rc::new(|| Some(0)),
        uid(),
        Some(Arc::new(move |request| {
            transport::exchange(&path, uid(), None, request, Duration::from_millis(100))
                .map_err(|e| e.to_string())
        })),
    );
    let launched = Rc::new(Cell::new(0));
    let count = launched.clone();
    let _deferred = crate::updates::after_startup(&h.window, move |_| count.set(count.get() + 1));
    h.until(true, |w| {
        w.get_updates_message().contains("Waiting for the device")
    });
    assert!(h.window.get_install_fenced());
    assert_eq!(launched.get(), 0);
    let observed = pumping_exchange(
        &mut h,
        &host_path,
        HostRequest::Observe {
            schema: SCHEMA,
            nonce: 9,
        },
    )
    .view
    .unwrap();
    assert!(observed.fenced);
    assert!(!observed.available);
    let refused = pumping_exchange(
        &mut h,
        &host_path,
        HostRequest::Prepare {
            schema: SCHEMA,
            prepare: Prepare {
                schema: SCHEMA,
                ticket: ticket(),
                host: observed.host,
                host_revision: observed.revision,
                settings_revision: observed.settings_revision,
                expires_ms: transport::boot_ms().unwrap() + PREPARE_MS,
            },
        },
    );
    assert_eq!(refused.error, Some(Failure::NotReady));

    let mode = Arc::new(AtomicUsize::new(0));
    let responses = Arc::new(AtomicUsize::new(0));
    let selected = mode.clone();
    let calls = responses.clone();
    let _service = transport::Server::bind(
        &service_path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |bytes, _| {
            let request = ReconcileRequest::from_json(bytes).unwrap();
            let choice = selected.load(Ordering::SeqCst);
            let mut reply = ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce,
                host: request.host,
                service: Id::from_bytes([7; 32]).unwrap(),
                result: match choice {
                    2 => RecoveryDecision::Waiting {},
                    3 => RecoveryDecision::Blocked {},
                    _ => RecoveryDecision::Clear {},
                },
            };
            if choice == 0 {
                reply.nonce += 1;
            }
            if choice == 1 {
                reply.host.start_ticks += 1;
            }
            calls.fetch_add(1, Ordering::SeqCst);
            serde_json::to_vec(&reply).unwrap()
        }),
    )
    .unwrap();
    for choice in 0..4 {
        mode.store(choice, Ordering::SeqCst);
        let previous = responses.load(Ordering::SeqCst);
        h.until(true, |_| responses.load(Ordering::SeqCst) > previous);
        for _ in 0..5 {
            h.pump(true);
        }
        h.window.invoke_updates_cancel();
        crate::handle_ui_action(&h.window, UiAction::Start);
        assert!(h.window.get_install_fenced());
        assert!(!h.window.get_updates_can_cancel());
        assert_eq!(launched.get(), 0);
    }
    mode.store(4, Ordering::SeqCst);
    h.until(true, |_| launched.get() == 1);
    assert!(!h.window.get_install_fenced());
    assert!(!h.window.get_updates_visible());
    for _ in 0..5 {
        h.pump(true);
    }
    assert_eq!(launched.get(), 1);
    assert!(
        h.frames.get() > 0,
        "health presentation continues while startup is held"
    );
}

#[test]
fn acknowledged_host_rejects_stale_clear_and_wrong_ticket_before_new_service_abort() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let mode = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let selected = mode.clone();
    let observed = calls.clone();
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(|_| Err("lost service reply".into())),
        Rc::new(move || Some(frames.get())),
        uid(),
        Some(Arc::new(move |request| {
            let mut result = RecoveryDecision::Clear {};
            if let Some(held) = &request.held {
                observed.fetch_add(1, Ordering::SeqCst);
                result = match selected.load(Ordering::SeqCst) {
                    0 => RecoveryDecision::Waiting {},
                    1 => RecoveryDecision::Clear {},
                    2 => {
                        let mut wrong = held.clone();
                        wrong.sequence += 1;
                        RecoveryDecision::Aborted { ticket: wrong }
                    }
                    _ => RecoveryDecision::Aborted {
                        ticket: held.clone(),
                    },
                };
            }
            Ok(ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce,
                host: request.host.clone(),
                service: if request.held.is_none() && selected.load(Ordering::SeqCst) == 0 {
                    ticket().service
                } else {
                    Id::from_bytes([8; 32]).unwrap()
                },
                result,
            })
        })),
    );
    h.until(true, |w| !w.get_install_fenced());
    h.window.set_updates_visible(true);
    let view = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Observe {
            schema: SCHEMA,
            nonce: 1,
        },
    )
    .view
    .unwrap();
    let ack = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Prepare {
            schema: SCHEMA,
            prepare: Prepare {
                schema: SCHEMA,
                ticket: ticket(),
                host: view.host,
                host_revision: view.revision,
                settings_revision: view.settings_revision,
                expires_ms: transport::boot_ms().unwrap() + PREPARE_MS,
            },
        },
    );
    assert!(ack.acknowledgement.is_some());
    for choice in 0..3 {
        mode.store(choice, Ordering::SeqCst);
        let previous = calls.load(Ordering::SeqCst);
        h.until(true, |_| calls.load(Ordering::SeqCst) > previous);
        for _ in 0..5 {
            h.pump(true);
        }
        assert!(h.window.get_install_fenced());
        assert!(!h.window.get_updates_can_cancel());
    }
    mode.store(3, Ordering::SeqCst);
    h.until(true, |w| !w.get_install_fenced());
    assert!(h.window.get_updates_message().contains("did not start"));
    assert!(h.window.get_updates_can_cancel());
    let view = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Observe {
            schema: SCHEMA,
            nonce: 2,
        },
    )
    .view
    .unwrap();
    let old = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Prepare {
            schema: SCHEMA,
            prepare: Prepare {
                schema: SCHEMA,
                ticket: ticket(),
                host: view.host,
                host_revision: view.revision,
                settings_revision: view.settings_revision,
                expires_ms: transport::boot_ms().unwrap() + PREPARE_MS,
            },
        },
    );
    assert_eq!(old.error, Some(Failure::Stale));
}

#[test]
fn restarted_app_displays_confirmed_outcome_and_keeps_it_through_cleanup() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.path().join("host.sock"),
        Arc::new(|_| Err("unused".into())),
        Rc::new(|| Some(0)),
        uid(),
        Some(Arc::new(move |request| {
            let count = observed.fetch_add(1, Ordering::SeqCst);
            Ok(ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce,
                host: request.host.clone(),
                service: Id::from_bytes([8; 32]).unwrap(),
                result: if count == 0 {
                    RecoveryDecision::Finished {
                        ticket: ticket(),
                        transaction: Id::from_bytes([4; 32]).unwrap(),
                        outcome: InstallOutcome::RolledBack,
                    }
                } else {
                    RecoveryDecision::Clear {}
                },
            })
        })),
    );
    h.until(true, |_| calls.load(Ordering::SeqCst) >= 2);
    for _ in 0..5 {
        h.pump(true);
    }
    assert!(!h.window.get_install_fenced());
    assert!(h.window.get_updates_visible());
    assert!(h.window.get_updates_message().contains("rolled back"));
    assert!(h.window.get_updates_can_cancel());
}

#[test]
fn failed_host_listener_cannot_bypass_startup_reconciliation() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    std::fs::write(&socket, "occupied").unwrap();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket,
        Arc::new(|_| Err("unused".into())),
        Rc::new(|| Some(0)),
        uid(),
        Some(Arc::new(|_| panic!("missing host cannot request release"))),
    );
    let _deferred = crate::updates::after_startup(&h.window, |_| panic!("startup must stay held"));
    for _ in 0..10 {
        h.pump(true);
    }
    h.window.invoke_updates_cancel();
    assert!(h.window.get_install_fenced());
    assert!(!h.window.get_updates_can_cancel());
}

#[test]
fn actual_socket_waits_for_exact_save_and_two_rendered_frames_and_keeps_lost_ack_fence() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let settings_path = dir.path().join("settings.toml");
    let settings = Arc::new(RwLock::new(Settings::default()));
    settings.write().unwrap().audio.muted = true;
    let (entered, entries) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let released = Mutex::new(released);
    let save_path = settings_path.clone();
    let writer = SettingsWriter::with_writer(Arc::new(move |snapshot| {
        entered.send(()).unwrap();
        released
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        crate::settings::save_settings(snapshot, &save_path).map_err(|e| e.to_string())
    }))
    .unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let progress = Arc::new(Mutex::new(None::<Phase>));
    let reported = progress.clone();
    let host_path = socket.clone();
    let exchange = Arc::new(
        move |request: &ServiceRequest| -> Result<ServiceReply, String> {
            match request {
                ServiceRequest::Offer { .. } => {
                    let mut reply = response(Phase::Offered);
                    reply.offer = Some(Offer {
                        schema: SCHEMA,
                        ticket: ticket(),
                        candidate: Candidate {
                            id: ticket().candidate,
                            channel: Channel::Release,
                            generation: 11,
                            build_id: "test-image".into(),
                        },
                        ends_game: true,
                        expires_ms: transport::boot_ms().unwrap() + 300_000,
                    });
                    Ok(reply)
                }
                ServiceRequest::Confirm { consent, .. } => {
                    assert!(consent.end_game);
                    assert_eq!(consent.ticket, ticket());
                    let view = host_request(
                        &host_path,
                        HostRequest::Observe {
                            schema: SCHEMA,
                            nonce: 7,
                        },
                    )
                    .view
                    .unwrap();
                    assert!(view.available);
                    assert!(view.game);
                    let ack = host_request(
                        &host_path,
                        HostRequest::Prepare {
                            schema: SCHEMA,
                            prepare: Prepare {
                                schema: SCHEMA,
                                ticket: ticket(),
                                host: view.host,
                                host_revision: view.revision,
                                settings_revision: view.settings_revision,
                                expires_ms: transport::boot_ms().unwrap() + PREPARE_MS,
                            },
                        },
                    );
                    assert!(ack.error.is_none());
                    assert!(ack.acknowledgement.is_some());
                    count.fetch_add(1, Ordering::SeqCst);
                    // Simulate a lost service reply after the app has acknowledged.
                    Err("lost reply".into())
                }
                ServiceRequest::Status { .. } => reported
                    .lock()
                    .unwrap()
                    .map(response)
                    .ok_or("service offline".into()),
                _ => Err("service offline".into()),
            }
        },
    );
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        settings,
        writer,
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        exchange,
        Rc::new(move || Some(frames.get())),
        uid(),
        Some(Arc::new(|request| {
            Ok(ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce,
                host: request.host.clone(),
                service: ticket().service,
                result: if request.held.is_some() {
                    RecoveryDecision::Waiting {}
                } else {
                    RecoveryDecision::Clear {}
                },
            })
        })),
    );
    h.until(true, |w| !w.get_install_fenced());
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_can_confirm());
    h.pump(true);
    if let Some(path) = std::env::var_os("SPACEWARS_UPDATE_SCREENSHOT") {
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 800, 480);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes: Vec<_> = h.pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
    }
    assert!(h.window.get_updates_confirm_label().contains("End game"));
    crate::ui_activation::activate(&h.window, "updates.confirm");
    h.until(false, |w| w.get_install_fenced());
    entries.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(!crate::ui_activation::activate(&h.window, "pause.resume"));
    assert!(!crate::ui_activation::activate(
        &h.window,
        "launcher.start-game"
    ));
    for _ in 0..8 {
        h.pump(true);
    }
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        0,
        "rendering cannot acknowledge an unfinished save"
    );
    release.send(()).unwrap();
    h.until(true, |w| !w.get_updates_can_cancel());
    assert!(
        crate::settings::load_settings(&settings_path)
            .unwrap()
            .settings
            .audio
            .muted
    );
    h.until(true, |_| accepted.load(Ordering::SeqCst) == 1);
    for _ in 0..55 {
        h.pump(true);
    }
    assert!(
        h.window.get_install_fenced(),
        "lost reply never releases the fence"
    );
    assert!(!h.window.get_updates_can_cancel());
    let path = socket.clone();
    for phase in [
        Phase::Writing,
        Phase::Flushing,
        Phase::ReadingBack,
        Phase::CheckingFilesystem,
        Phase::RebootRequested,
        Phase::RecoveryRequired,
    ] {
        *progress.lock().unwrap() = Some(phase);
        let message = progress_message(phase).unwrap();
        h.until(true, |w| w.get_updates_message() == message);
        // Reconciliation continues independently while writing owns the lock.
        for _ in 0..105 {
            h.pump(true);
        }
        assert_eq!(h.window.get_updates_message(), message);
        assert!(h.window.get_install_fenced());
        assert!(!h.window.get_updates_can_cancel());
    }
    let worker = thread::spawn(move || {
        host_request(
            &path,
            HostRequest::Abort {
                schema: SCHEMA,
                aborted: Aborted {
                    schema: SCHEMA,
                    ticket: ticket(),
                },
            },
        )
    });
    h.until(true, |w| !w.get_install_fenced());
    assert!(worker.join().unwrap().error.is_none());
    assert!(h.window.get_updates_can_cancel());
    // Rendered evidence comes from the real Slint panel.
    assert!(h.pixels.iter().any(|p| p.r > 150 && p.g > 150));
}

#[test]
fn saved_settings_without_a_presented_updater_never_acknowledge_and_expire() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    h.window.set_updates_visible(true);
    let mut host = Host::new(host_identity());
    let settings = Settings::default();
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    host.observe(&h.window, &settings);
    let prepare = Prepare {
        schema: SCHEMA,
        ticket: ticket(),
        host: host.identity.clone(),
        host_revision: host.revision,
        settings_revision: host.settings_revision,
        expires_ms: 1000,
    };
    let (tx, rx) = mpsc::sync_channel(1);
    host.request(
        &h.window,
        &settings,
        &writer,
        Incoming {
            request: HostRequest::Prepare {
                schema: SCHEMA,
                prepare,
            },
            reply: tx,
        },
        0,
        Some(10),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while writer.status().pending {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    host.tick(&h.window, &settings, &writer, 5, Some(10));
    assert!(rx.try_recv().is_err());
    host.tick(&h.window, &settings, &writer, 6, Some(11));
    assert!(rx.try_recv().is_err());
    host.tick(&h.window, &settings, &writer, 1000, Some(11));
    assert_eq!(rx.recv().unwrap().error, Some(Failure::Expired));
    assert!(!h.window.get_install_fenced());
    h.pump(true);
}

#[test]
fn save_failure_and_changed_settings_revoke_preparation() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    h.window.set_updates_visible(true);
    // Opening a directory as a settings file fails even under root.
    let writer = SettingsWriter::new(dir.path().into()).unwrap();
    let mut settings = Settings::default();
    let mut host = Host::new(host_identity());
    host.observe(&h.window, &settings);
    let make = |host: &Host, sequence| Prepare {
        schema: SCHEMA,
        ticket: Ticket {
            sequence,
            ..ticket()
        },
        host: host.identity.clone(),
        host_revision: host.revision,
        settings_revision: host.settings_revision,
        expires_ms: 1000,
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let prepare = make(&host, 1);
    host.request(
        &h.window,
        &settings,
        &writer,
        Incoming {
            request: HostRequest::Prepare {
                schema: SCHEMA,
                prepare,
            },
            reply: tx,
        },
        0,
        Some(1),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while writer.status().pending {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    host.tick(&h.window, &settings, &writer, 1, Some(3));
    assert_eq!(rx.recv().unwrap().error, Some(Failure::SaveFailed));
    assert!(!h.window.get_install_fenced());
    host.abort(
        &h.window,
        &Aborted {
            schema: SCHEMA,
            ticket: ticket(),
        },
    );
    assert!(
        h.window
            .get_updates_message()
            .contains("could not be saved")
    );
    let writer = SettingsWriter::new(dir.path().join("ok.toml")).unwrap();
    let prepare = make(&host, 2);
    let (tx, rx) = mpsc::sync_channel(1);
    host.request(
        &h.window,
        &settings,
        &writer,
        Incoming {
            request: HostRequest::Prepare {
                schema: SCHEMA,
                prepare,
            },
            reply: tx,
        },
        2,
        Some(3),
    );
    settings.audio.muted = true;
    host.tick(&h.window, &settings, &writer, 3, Some(5));
    assert_eq!(rx.recv().unwrap().error, Some(Failure::Stale));
    assert!(!h.window.get_install_fenced());
}

#[test]
#[cfg(not(feature = "pi-kiosk"))]
fn desktop_updates_are_visible_but_installation_is_unavailable() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let settings = Arc::new(RwLock::new(Settings::default()));
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    crate::sound_controls::install(
        &h.window,
        crate::host::new_scenario_controls(),
        settings.clone(),
        writer.clone(),
    );
    let _runtime = install(&h.window, settings, writer, h.input.clone());
    assert!(crate::ui_activation::activate(
        &h.window,
        "settings.updates"
    ));
    h.until(true, |w| w.get_updates_message().contains("not available"));
    assert!(h.window.get_updates_visible());
    assert!(!h.window.get_updates_can_confirm());
    assert!(!h.window.get_install_fenced());
    assert!(!crate::ui_activation::activate(
        &h.window,
        "updates.confirm"
    ));
    h.window.invoke_keyboard_action(5, false);
    assert!(!h.window.get_updates_visible());
}

#[test]
fn reopening_updates_with_a_full_queue_reports_failure_and_can_recover() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let (entered, entries) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let completed = Arc::new(AtomicUsize::new(0));
    let finished = completed.clone();
    let exchange = Arc::new(move |request: &ServiceRequest| {
        assert!(matches!(request, ServiceRequest::Offer { .. }));
        entered.send(()).unwrap();
        released
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let sequence = finished.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        let mut reply = response(Phase::Offered);
        let offered_ticket = Ticket {
            sequence,
            ..ticket()
        };
        reply.status.ticket = Some(offered_ticket.clone());
        reply.offer = Some(Offer {
            schema: SCHEMA,
            ticket: offered_ticket,
            candidate: Candidate {
                id: ticket().candidate,
                channel: Channel::Release,
                generation: 11,
                build_id: format!("offer-{sequence}"),
            },
            ends_game: true,
            expires_ms: transport::boot_ms().unwrap() + 300_000,
        });
        Ok(reply)
    });
    let settings = Arc::new(RwLock::new(Settings::default()));
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    let _runtime = start(
        &h.window,
        settings,
        writer,
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.path().join("host.sock"),
        exchange,
        Rc::new(|| Some(0)),
        uid(),
        None,
    );
    h.window.invoke_updates_open();
    entries.recv_timeout(Duration::from_secs(2)).unwrap();
    // The first exchange is blocked. The second opening fills the only queue
    // slot; the third opening must report that its request was not accepted.
    for _ in 0..2 {
        h.window.invoke_updates_cancel();
        assert!(!h.window.get_updates_visible());
        h.window.invoke_updates_open();
    }
    let saturated_message = h.window.get_updates_message().to_string();
    release.send(()).unwrap();
    entries.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    h.until(false, |_| completed.load(Ordering::SeqCst) == 2);
    // Wait through a UI poll after both stale exchanges have completed.
    let drained = Instant::now();
    h.until(false, |_| drained.elapsed() >= Duration::from_millis(60));
    assert!(saturated_message.contains("busy"), "{saturated_message}");
    assert_eq!(h.window.get_updates_message().as_str(), saturated_message);
    assert!(!h.window.get_updates_can_confirm());
    assert!(h.window.get_updates_can_cancel());

    h.window.invoke_updates_cancel();
    h.window.invoke_updates_open();
    entries.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    h.until(false, |w| w.get_updates_can_confirm());
    assert!(h.window.get_updates_message().contains("offer-3"));
}

#[test]
fn shipped_kiosk_launch_uses_runtime_identity_without_enabling_direct_launch() {
    use clap::Parser;
    let unit = include_str!(
        "../../../yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-kiosk.service"
    );
    let command = unit
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))
        .unwrap();
    let args = crate::Args::try_parse_from(command.split_whitespace()).unwrap();
    assert!(args.fullscreen);
    assert!(!args.kiosk);
    assert!(
        !crate::should_launch_directly(&args),
        "the shipped launcher/autostart policy must be preserved"
    );

    assert!(runtime_identity(None).is_none());
    assert!(runtime_identity(Some("invalid".into())).is_none());
    let identity = runtime_identity(Some("b".repeat(32)));
    assert_eq!(identity.is_some(), cfg!(feature = "pi-kiosk"));
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let settings = Arc::new(RwLock::new(Settings::default()));
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    let _runtime = start(
        &h.window,
        settings,
        writer,
        h.input.clone(),
        identity.clone().map(Host::new),
        socket.clone(),
        Arc::new(|_| Err("installation unavailable".into())),
        Rc::new(|| Some(0)),
        uid(),
        None,
    );
    assert_eq!(socket.exists(), cfg!(feature = "pi-kiosk"));
    if let Some(identity) = identity {
        h.window.invoke_updates_open();
        let worker = thread::spawn(move || {
            host_request(
                &socket,
                HostRequest::Observe {
                    schema: SCHEMA,
                    nonce: 42,
                },
            )
        });
        h.until(false, |_| worker.is_finished());
        let view = worker.join().unwrap().view.unwrap();
        assert_eq!(view.host, identity);
        assert_eq!(view.host.pid, std::process::id());
        assert!(view.host.start_ticks > 0);
        assert_eq!(view.nonce, 42);
        assert!(view.available);
    }
}
