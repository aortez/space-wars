use super::*;
use slint::Rgb8Pixel;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use std::{
    cell::Cell,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    /// Saves the last rendered frame as a PNG when the environment names a file.
    fn screenshot(&self, variable: &str) {
        let Some(path) = std::env::var_os(variable) else {
            return;
        };
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 800, 480);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes: Vec<_> = self.pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
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
            written: None,
        },
        offer: None,
        aborted: None,
        policy: None,
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

fn external_confirmation(launcher: bool) {
    let mut h = Harness::new();
    h.window.set_launcher_visible(launcher);
    h.window.set_ingame_menu_visible(false);
    h.window.set_sound_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let mode = Arc::new(AtomicUsize::new(0));
    let progress_mode = mode.clone();
    let recovery_mode = mode.clone();
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(move |request| {
            assert!(matches!(request, ServiceRequest::Status { .. }));
            let mut reply = response(Phase::Writing);
            if progress_mode.load(Ordering::SeqCst) == 1 {
                reply.status.ticket.as_mut().unwrap().sequence += 1;
                reply.status.phase = Phase::ReadingBack;
            }
            Ok(reply)
        }),
        Rc::new(move || Some(frames.get())),
        uid(),
        Some(Arc::new(move |request| {
            Ok(ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce,
                host: request.host.clone(),
                service: ticket().service,
                result: if let Some(held) = &request.held {
                    if recovery_mode.load(Ordering::SeqCst) == 2 {
                        RecoveryDecision::Aborted {
                            ticket: held.clone(),
                        }
                    } else {
                        RecoveryDecision::Waiting {}
                    }
                } else {
                    RecoveryDecision::Clear {}
                },
            })
        })),
        None,
    );
    h.until(true, |w| !w.get_install_fenced());
    assert!(!h.window.get_updates_visible());
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
    assert!(view.available);
    assert_eq!(view.game, !launcher);
    assert!(
        !h.window.get_updates_visible(),
        "offer observation must not interrupt the app"
    );
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
    assert!(ack.acknowledgement.is_some(), "{ack:?}");
    let after = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Observe {
            schema: SCHEMA,
            nonce: 2,
        },
    )
    .view
    .unwrap();
    assert!(after.available);
    assert_eq!(after.revision, view.revision);
    assert_eq!(after.settings_revision, view.settings_revision);
    assert_eq!(after.game, view.game);
    assert!(h.window.get_updates_visible());
    assert!(h.window.get_install_fenced());
    assert!(!h.window.get_updates_can_cancel());
    h.until(true, |w| {
        w.get_updates_message().contains("Installing the update")
    });
    mode.store(1, Ordering::SeqCst);
    let until = Instant::now() + Duration::from_millis(700);
    while Instant::now() < until {
        h.pump(true);
    }
    assert!(
        h.window
            .get_updates_message()
            .contains("Installing the update")
    );
    mode.store(2, Ordering::SeqCst);
    h.until(true, |w| !w.get_install_fenced());
    h.window.invoke_updates_cancel();
    assert!(!h.window.get_updates_visible());
    assert_eq!(h.window.get_launcher_visible(), launcher);
}

#[test]
fn external_confirmation_can_open_updates_from_a_running_game() {
    external_confirmation(false);
}

#[test]
fn external_confirmation_can_open_updates_from_the_launcher() {
    external_confirmation(true);
}

#[test]
fn external_abort_retires_the_panel_ticket_despite_a_lost_status_reply() {
    let mut h = Harness::new();
    h.window.set_ingame_menu_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let frames = h.frames.clone();
    let (entered, entries) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let completed = Arc::new(AtomicUsize::new(0));
    let replies = completed.clone();
    let cancelled = Arc::new(AtomicUsize::new(0));
    let cancellations = cancelled.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(move |request| {
            // The CLI advances to another offer in the same service instance.
            // The panel never receives the previous ticket's Aborted status.
            let next = Ticket {
                sequence: 2,
                ..ticket()
            };
            let mut reply = response(Phase::Offered);
            reply.status.ticket = Some(next.clone());
            match request {
                ServiceRequest::Status { .. } if replies.load(Ordering::SeqCst) == 0 => {
                    entered.send(()).unwrap();
                    released
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    replies.fetch_add(1, Ordering::SeqCst);
                    return Err("status reply lost".into());
                }
                ServiceRequest::Status { .. } => {}
                ServiceRequest::Cancel { .. } => {
                    cancellations.fetch_add(1, Ordering::SeqCst);
                    reply.error = Some(Failure::Stale);
                }
                ServiceRequest::Offer { .. } => {
                    reply.offer = Some(Offer {
                        schema: SCHEMA,
                        ticket: next,
                        candidate: Candidate {
                            id: ticket().candidate,
                            channel: Channel::Release,
                            generation: 11,
                            build_id: "next-offer".into(),
                        },
                        ends_game: true,
                        expires_ms: transport::boot_ms().unwrap() + 300_000,
                    });
                }
                _ => panic!("external preparation cannot create local consent"),
            }
            Ok(reply)
        }),
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
        None,
    );
    h.until(true, |w| !w.get_install_fenced());
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
    assert!(ack.acknowledgement.is_some(), "{ack:?}");
    h.until(true, |_| entries.try_recv().is_ok());
    let abort = |ticket| HostRequest::Abort {
        schema: SCHEMA,
        aborted: Aborted {
            schema: SCHEMA,
            ticket,
        },
    };
    let wrong = pumping_exchange(
        &mut h,
        &socket,
        abort(Ticket {
            sequence: 2,
            ..ticket()
        }),
    );
    assert_eq!(wrong.error, Some(Failure::Stale));
    h.window.invoke_updates_cancel();
    assert!(h.window.get_install_fenced());
    assert!(h.window.get_updates_visible());
    assert!(!h.window.get_updates_can_cancel());

    let accepted = pumping_exchange(&mut h, &socket, abort(ticket()));
    assert!(accepted.error.is_none());
    assert!(!h.window.get_install_fenced());
    assert!(h.window.get_updates_can_cancel());
    let message = h.window.get_updates_message();
    assert!(message.contains("cancelled"));
    release.send(()).unwrap();
    h.until(true, |_| completed.load(Ordering::SeqCst) == 1);
    // Give the UI a poll after the old status request completes. Its failure
    // must not overwrite the accepted cancellation or restore the old ticket.
    let drained = Instant::now();
    h.until(true, |_| drained.elapsed() >= Duration::from_millis(60));
    let after_reply = h.window.get_updates_message();
    h.window.invoke_updates_cancel();
    assert!(!h.window.get_updates_visible());
    assert_eq!(cancelled.load(Ordering::SeqCst), 0);
    assert_eq!(after_reply, message);

    h.window.set_ingame_menu_visible(true);
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_can_confirm());
    assert!(h.window.get_updates_message().contains("next-offer"));
}

#[test]
fn external_preparation_can_be_cancelled_before_presenting_the_updater() {
    let mut h = Harness::new();
    h.window.set_ingame_menu_visible(false);
    h.window.set_updates_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let frames = h.frames.clone();
    let cancelled = Arc::new(AtomicUsize::new(0));
    let calls = cancelled.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(move |request| match request {
            ServiceRequest::Cancel {
                ticket: selected, ..
            } => {
                assert_eq!(selected, &ticket());
                calls.fetch_add(1, Ordering::SeqCst);
                let mut reply = response(Phase::Aborted);
                reply.aborted = Some(Aborted {
                    schema: SCHEMA,
                    ticket: ticket(),
                });
                Ok(reply)
            }
            ServiceRequest::Status { .. } => Ok(response(Phase::Preparing)),
            _ => panic!("external preparation cannot create local consent"),
        }),
        Rc::new(move || Some(frames.get())),
        uid(),
        None,
        None,
    );
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
    let (send, received) = mpsc::sync_channel(1);
    thread::spawn(move || {
        send.send(host_request(
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
        ))
        .unwrap();
    });
    h.until(false, |w| w.get_install_fenced());
    assert!(
        received.try_recv().is_err(),
        "no acknowledgement without presented frames"
    );
    assert!(h.window.get_updates_can_cancel());
    h.window.invoke_updates_cancel();
    h.until(false, |w| !w.get_install_fenced());
    let result = received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(result.acknowledgement.is_none());
    assert_eq!(result.error, Some(Failure::Stale));
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    h.window.invoke_updates_cancel();
    assert!(!h.window.get_updates_visible());
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
        None,
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
        None,
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
fn restart_before_host_prepare_retires_old_offer_and_allows_back_and_reopen() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let mode = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let confirmations = Arc::new(AtomicUsize::new(0));
    let service_mode = mode.clone();
    let confirmed = confirmations.clone();
    let recovery_mode = mode.clone();
    let observed = calls.clone();
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(move |request| {
            let restarted = service_mode.load(Ordering::SeqCst) != 0;
            let mut reply = response(Phase::Preparing);
            match request {
                ServiceRequest::Offer { .. } => {
                    let mut ticket = ticket();
                    if restarted {
                        ticket.service = Id::from_bytes([8; 32]).unwrap();
                    }
                    reply.status = Status {
                        phase: Phase::Offered,
                        ticket: Some(ticket.clone()),
                        written: None,
                    };
                    reply.offer = Some(Offer {
                        schema: SCHEMA,
                        candidate: Candidate {
                            id: ticket.candidate.clone(),
                            channel: Channel::Release,
                            generation: 11,
                            build_id: "test-image".into(),
                        },
                        ticket,
                        ends_game: true,
                        expires_ms: transport::boot_ms().unwrap() + 300_000,
                    });
                }
                ServiceRequest::Confirm { .. } => {
                    confirmed.fetch_add(1, Ordering::SeqCst);
                    // Native trust/payload preflight has not sent Host::Prepare.
                }
                ServiceRequest::Cancel { .. } => {
                    assert!(restarted);
                    reply.error = Some(Failure::Stale);
                }
                ServiceRequest::Status { .. } => {}
                ServiceRequest::Policy { .. } | ServiceRequest::SavePolicy { .. } => {
                    unreachable!("the setting has an exchange of its own")
                }
            }
            if restarted && reply.offer.is_none() {
                reply.status = Status {
                    phase: Phase::Idle,
                    ticket: None,
                    written: None,
                };
            }
            Ok(reply)
        }),
        Rc::new(move || Some(frames.get())),
        uid(),
        Some(Arc::new(move |request| {
            observed.fetch_add(1, Ordering::SeqCst);
            let mode = recovery_mode.load(Ordering::SeqCst);
            Ok(ReconcileReply {
                schema: SCHEMA,
                nonce: request.nonce + u64::from(mode == 1),
                host: request.host.clone(),
                service: if mode == 0 {
                    ticket().service
                } else {
                    Id::from_bytes([8; 32]).unwrap()
                },
                result: RecoveryDecision::Clear {},
            })
        })),
        None,
    );
    h.until(true, |w| !w.get_install_fenced());
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_can_confirm());
    h.window.invoke_updates_confirm();
    h.until(true, |_| confirmations.load(Ordering::SeqCst) == 1);
    let before = calls.load(Ordering::SeqCst);
    h.until(true, |_| calls.load(Ordering::SeqCst) > before);
    for _ in 0..10 {
        h.pump(true);
    }
    assert!(h.window.get_updates_visible());
    assert!(!h.window.get_updates_message().contains("restarted"));
    assert!(!h.window.get_install_fenced());

    mode.store(1, Ordering::SeqCst);
    h.window.invoke_updates_cancel();
    h.until(true, |w| {
        w.get_updates_message().contains("expired or changed")
    });
    let before = calls.load(Ordering::SeqCst);
    h.until(true, |_| calls.load(Ordering::SeqCst) > before);
    for _ in 0..10 {
        h.pump(true);
    }
    assert!(!h.window.get_updates_message().contains("restarted"));
    mode.store(2, Ordering::SeqCst);
    h.until(true, |w| {
        w.get_updates_message().contains("service restarted")
    });
    assert!(h.window.get_updates_can_cancel());
    assert!(!h.window.get_install_fenced());
    // Reconciliation also fences out a delayed preparation from the old daemon.
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
    let late = pumping_exchange(
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
    assert_eq!(late.error, Some(Failure::Stale));
    h.window.invoke_updates_cancel();
    assert!(!h.window.get_updates_visible());
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_can_confirm());
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
        None,
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
        None,
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
        None,
    );
    h.until(true, |w| !w.get_install_fenced());
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_can_confirm());
    h.pump(true);
    h.screenshot("SPACEWARS_UPDATE_SCREENSHOT");
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
    let _ = host.request(
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
    let _ = host.request(
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
    )
    .unwrap();
    assert!(
        h.window
            .get_updates_message()
            .contains("could not be saved")
    );
    let writer = SettingsWriter::new(dir.path().join("ok.toml")).unwrap();
    let prepare = make(&host, 2);
    let (tx, rx) = mpsc::sync_channel(1);
    let _ = host.request(
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

#[test]
fn writing_status_fills_the_activity_bar_and_reports_the_percentage() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let exchange = Arc::new(move |request: &ServiceRequest| match request {
        ServiceRequest::Offer { .. } => {
            let mut reply = response(Phase::Offered);
            reply.offer = Some(Offer {
                schema: SCHEMA,
                ticket: ticket(),
                candidate: Candidate {
                    id: ticket().candidate,
                    channel: Channel::Release,
                    generation: 25,
                    build_id: "rootfs-progress".into(),
                },
                ends_game: false,
                expires_ms: transport::boot_ms().unwrap() + 300_000,
            });
            Ok(reply)
        }
        ServiceRequest::Confirm { .. } => Ok(response(Phase::Preparing)),
        ServiceRequest::Status { .. } => {
            let mut reply = response(Phase::Writing);
            reply.status.written = Some(Written {
                bytes: 300_000_000,
                total: 1_000_000_000,
            });
            Ok(reply)
        }
        _ => panic!("unexpected request"),
    });
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.path().join("host.sock"),
        exchange,
        Rc::new(|| Some(0)),
        uid(),
        None,
        None,
    );
    h.window.invoke_updates_open();
    h.until(false, |w| w.get_updates_can_confirm());
    assert!(!h.window.get_updates_busy());
    h.window.invoke_updates_confirm();
    // Confirmation alone shows the sweeping bar; the writer's counts then fill it.
    h.until(false, |w| w.get_updates_busy());
    h.until(false, |w| w.get_updates_progress() > 0.0);
    assert!((h.window.get_updates_progress() - 0.3).abs() < 0.001);
    let message = h.window.get_updates_message().to_string();
    assert!(message.contains("Installing the update"), "{message}");
    assert!(
        message.contains("300 of 1000 MB written (30%)"),
        "{message}"
    );
    assert!(!h.window.get_updates_can_confirm());
}

/// A service whose availability check lasts until the test releases it, so the
/// cursor can be driven while the check is still running.
fn held_offer_exchange(
    release: Arc<AtomicUsize>,
    confirmations: Arc<AtomicUsize>,
    cancellations: Arc<AtomicUsize>,
    offer_ms: u64,
) -> Exchange {
    Arc::new(move |request: &ServiceRequest| match request {
        ServiceRequest::Offer { .. } => {
            let end = Instant::now() + Duration::from_secs(5);
            while release.load(Ordering::SeqCst) == 0 && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
            let mut reply = response(Phase::Offered);
            reply.offer = Some(Offer {
                schema: SCHEMA,
                ticket: ticket(),
                candidate: Candidate {
                    id: ticket().candidate,
                    channel: Channel::Nightly,
                    generation: 26,
                    build_id: "rootfs-focus".into(),
                },
                ends_game: false,
                expires_ms: transport::boot_ms().unwrap() + offer_ms,
            });
            Ok(reply)
        }
        ServiceRequest::Confirm { .. } => {
            confirmations.fetch_add(1, Ordering::SeqCst);
            Ok(response(Phase::Preparing))
        }
        ServiceRequest::Cancel { .. } => {
            cancellations.fetch_add(1, Ordering::SeqCst);
            Ok(response(Phase::Idle))
        }
        ServiceRequest::Status { .. } => Ok(response(Phase::Preparing)),
        ServiceRequest::Policy { .. } | ServiceRequest::SavePolicy { .. } => {
            unreachable!("the setting has an exchange of its own")
        }
    })
}

#[test]
fn the_cursor_reaches_install_only_after_the_offer_arrives_and_only_by_hand() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let release = Arc::new(AtomicUsize::new(0));
    let confirmations = Arc::new(AtomicUsize::new(0));
    let cancellations = Arc::new(AtomicUsize::new(0));
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.path().join("host.sock"),
        held_offer_exchange(
            release.clone(),
            confirmations.clone(),
            cancellations.clone(),
            300_000,
        ),
        Rc::new(|| Some(0)),
        uid(),
        None,
        None,
    );
    h.window.invoke_updates_open();
    for _ in 0..5 {
        h.pump(false);
    }
    assert!(
        h.window
            .get_updates_message()
            .contains("Checking installation availability")
    );
    assert!(!h.window.get_updates_can_confirm());
    assert_eq!(h.window.get_updates_focus(), 1);
    // Up, Down and Up again: the cursor stays on Back while the check runs.
    for code in [0, 1, 0] {
        h.window.invoke_keyboard_action(code, false);
        h.pump(false);
        assert_eq!(h.window.get_updates_focus(), 1, "key code {code}");
    }
    release.store(1, Ordering::SeqCst);
    h.until(false, |w| w.get_updates_can_confirm());
    for _ in 0..3 {
        h.pump(false);
    }
    // The offer's arrival does not move the cursor onto the live button.
    assert_eq!(h.window.get_updates_focus(), 1);
    h.window.invoke_keyboard_action(0, false);
    assert_eq!(h.window.get_updates_focus(), 0);
    h.window.invoke_keyboard_action(4, false);
    h.until(false, |_| confirmations.load(Ordering::SeqCst) == 1);
    // The press disables Install, so the cursor returns to Cancel.
    h.until(false, |w| w.get_updates_focus() == 1);
    assert!(!h.window.get_updates_can_confirm());
    assert_eq!(cancellations.load(Ordering::SeqCst), 0);
}

#[test]
fn an_expired_offer_returns_the_cursor_to_back_and_keeps_it_there() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let confirmations = Arc::new(AtomicUsize::new(0));
    let cancellations = Arc::new(AtomicUsize::new(0));
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.path().join("host.sock"),
        held_offer_exchange(
            Arc::new(AtomicUsize::new(1)),
            confirmations.clone(),
            cancellations.clone(),
            400,
        ),
        Rc::new(|| Some(0)),
        uid(),
        None,
        None,
    );
    h.window.invoke_updates_open();
    h.until(false, |w| w.get_updates_can_confirm());
    h.window.invoke_keyboard_action(0, false);
    assert_eq!(h.window.get_updates_focus(), 0);
    h.until(false, |w| !w.get_updates_can_confirm());
    assert!(
        h.window
            .get_updates_message()
            .contains("expired or changed")
    );
    assert_eq!(h.window.get_updates_focus(), 1);
    // A press meant for the withdrawn Install button cannot confirm anything.
    h.window.invoke_keyboard_action(4, false);
    h.window.invoke_keyboard_action(0, false);
    for _ in 0..3 {
        h.pump(false);
    }
    assert_eq!(h.window.get_updates_focus(), 1);
    assert_eq!(confirmations.load(Ordering::SeqCst), 0);
}

fn observed(host: &mut Host, window: &MainWindow, writer: &SettingsWriter, now: u64) -> HostView {
    let (reply, receipt) = mpsc::sync_channel(1);
    let _ = host.request(
        window,
        &Settings::default(),
        writer,
        Incoming {
            request: HostRequest::Observe {
                schema: SCHEMA,
                nonce: 1,
            },
            reply,
        },
        now,
        Some(1),
    );
    receipt.recv().unwrap().view.unwrap()
}

#[test]
fn observation_names_the_scene_and_the_time_since_a_person_used_the_app() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    let mut host = Host::new(host_identity());
    // The app's start counts as use.
    assert!(host.input_ms.is_some());
    host.input_ms = Some(1_000);
    // A scenario that has not named itself is a game, paused or not.
    let view = observed(&mut host, &h.window, &writer, 601_000);
    assert_eq!(
        (view.scene, view.idle_ms),
        (Some(Scene::Game), Some(600_000))
    );
    h.window
        .set_runtime_diagnostics("scenario=spacewars\nscenario_revision=4\npaused=true".into());
    assert_eq!(
        observed(&mut host, &h.window, &writer, 601_000).scene,
        Some(Scene::Game)
    );
    h.window
        .set_runtime_diagnostics("scenario=clock\nscenario_revision=5\npaused=false".into());
    let view = observed(&mut host, &h.window, &writer, 601_000);
    // The Clock still ends when an installation starts, so it stays a game
    // for a confirmation; only the scene tells the two apart.
    assert_eq!((view.scene, view.game), (Some(Scene::Clock), true));
    h.window.set_launcher_visible(true);
    let view = observed(&mut host, &h.window, &writer, 601_000);
    assert_eq!((view.scene, view.game), (Some(Scene::Launcher), false));

    // A touch or a button still held down is use now.
    let activity = h.window.global::<crate::UserActivity>();
    activity.set_pointer_held(true);
    assert_eq!(
        observed(&mut host, &h.window, &writer, 700_000).idle_ms,
        Some(0)
    );
    activity.set_pointer_held(false);
    assert_eq!(
        observed(&mut host, &h.window, &writer, 700_500).idle_ms,
        Some(500)
    );
    activity.set_gamepad_held(true);
    assert_eq!(
        observed(&mut host, &h.window, &writer, 800_000).idle_ms,
        Some(0)
    );
    activity.set_gamepad_held(false);

    // An unreadable clock, in either direction, is never idle time.
    assert_eq!(
        observed(&mut host, &h.window, &writer, u64::MAX).idle_ms,
        None
    );
    assert_eq!(
        observed(&mut host, &h.window, &writer, 799_999).idle_ms,
        None
    );
    host.input_ms = None;
    assert_eq!(
        observed(&mut host, &h.window, &writer, 900_000).idle_ms,
        None
    );
}

#[test]
fn any_input_restarts_the_idle_time_even_inside_updates() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(|_| Err("the panel sends nothing here".into())),
        Rc::new(move || Some(frames.get())),
        uid(),
        None,
        None,
    );
    let observe = |h: &mut Harness| {
        pumping_exchange(
            h,
            &socket,
            HostRequest::Observe {
                schema: SCHEMA,
                nonce: 1,
            },
        )
        .view
        .unwrap()
    };
    let rest = |h: &mut Harness| {
        let end = Instant::now() + Duration::from_millis(120);
        while Instant::now() < end {
            h.pump(false);
        }
    };
    // The idle time counts from the press when it is no longer than the time
    // since: counted from the input before, it would be a rest longer. A slow
    // reply lengthens only the time since, so load cannot fail the check.
    let since = |pressed: Instant| u64::try_from(pressed.elapsed().as_millis()).unwrap() + 1;
    rest(&mut h);
    let idle = observe(&mut h);
    assert!(idle.idle_ms.unwrap() >= 120, "{:?}", idle.idle_ms);

    // Outside Updates, input also changes the revision an offer was made for.
    let pressed = Instant::now();
    h.window.invoke_updates_activity();
    let used = observe(&mut h);
    assert!(
        used.idle_ms.unwrap() <= since(pressed),
        "{:?}",
        used.idle_ms
    );
    assert_eq!(used.revision, idle.revision + 1);

    // Inside Updates it keeps the revision, so that moving the cursor does not
    // withdraw an offer, but it is still a person using the app. The open
    // screen reports no idle time at all, so no countdown starts behind it.
    h.window.set_updates_visible(true);
    let opened = observe(&mut h);
    assert_eq!(opened.idle_ms, None);
    h.window.invoke_updates_activity();
    let used = observe(&mut h);
    assert_eq!((used.idle_ms, used.revision), (None, opened.revision));
    // Once it closes, the idle time counts from the last input inside it.
    rest(&mut h);
    let pressed = Instant::now();
    h.window.invoke_updates_activity();
    h.window.set_updates_visible(false);
    let closed = observe(&mut h);
    assert!(
        closed.idle_ms.unwrap() <= since(pressed),
        "{:?}",
        closed.idle_ms
    );

    // Losing focus counts as well.
    rest(&mut h);
    let pressed = Instant::now();
    h.window.invoke_updates_interrupted();
    let used = observe(&mut h);
    assert!(
        used.idle_ms.unwrap() <= since(pressed),
        "{:?}",
        used.idle_ms
    );
    assert_eq!(used.revision, closed.revision + 1);
}

fn counted(
    host: &mut Host,
    window: &MainWindow,
    writer: &SettingsWriter,
    now: u64,
    ends_ms: u64,
) -> HostView {
    let (reply, receipt) = mpsc::sync_channel(1);
    let _ = host.request(
        window,
        &Settings::default(),
        writer,
        Incoming {
            request: HostRequest::Countdown {
                schema: SCHEMA,
                nonce: 1,
                generation: 27,
                ends_ms,
            },
            reply,
        },
        now,
        Some(1),
    );
    receipt.recv().unwrap().view.unwrap()
}
fn counting(seconds: u64) -> String {
    format!("Generation 27 installs in {seconds} s · Press any button or touch here to postpone")
}

#[test]
fn a_countdown_shows_until_it_ends_goes_quiet_or_somebody_uses_the_app() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let writer = SettingsWriter::new(dir.path().join("settings.toml")).unwrap();
    let mut host = Host::new(host_identity());
    host.input_ms = Some(0);
    assert_eq!(host.notice(&h.window, 9_000), "");

    // The announcement answers like an observation and starts the notice.
    let view = counted(&mut host, &h.window, &writer, 10_000, 70_000);
    assert_eq!(
        (view.scene, view.idle_ms),
        (Some(Scene::Game), Some(10_000))
    );
    assert_eq!(host.notice(&h.window, 10_000), counting(60));
    assert_eq!(host.notice(&h.window, 10_001), counting(60));
    assert_eq!(host.notice(&h.window, 11_000), counting(59));
    // The service announces it at every look. Once it goes quiet, the
    // countdown is over for a reason the app cannot see.
    assert_eq!(host.notice(&h.window, 12_500), counting(58));
    assert_eq!(host.notice(&h.window, 12_501), "");
    counted(&mut host, &h.window, &writer, 13_000, 70_000);
    assert_eq!(host.notice(&h.window, 13_000), counting(57));
    // It ends at its end, when Updates takes over.
    counted(&mut host, &h.window, &writer, 69_500, 70_000);
    assert_eq!(host.notice(&h.window, 69_999), counting(1));
    assert_eq!(host.notice(&h.window, 70_000), "");

    // Somebody uses the app during the next one.
    counted(&mut host, &h.window, &writer, 80_000, 140_000);
    assert_eq!(host.notice(&h.window, 80_500), counting(60));
    host.used(Some(81_000));
    assert_eq!(host.notice(&h.window, 81_000), "Update postponed.");
    // The service has not looked yet and announces the same countdown again.
    let view = counted(&mut host, &h.window, &writer, 81_500, 140_000);
    assert_eq!(view.idle_ms, Some(500));
    assert_eq!(host.notice(&h.window, 81_500), "Update postponed.");
    assert_eq!(host.notice(&h.window, 84_999), "Update postponed.");
    assert_eq!(host.notice(&h.window, 85_000), "");
    // Another night's countdown shows again.
    counted(&mut host, &h.window, &writer, 90_000, 150_000);
    assert_eq!(host.notice(&h.window, 90_000), counting(60));

    // A touch that is still held down when the service looks is use as well.
    h.window
        .global::<crate::UserActivity>()
        .set_pointer_held(true);
    counted(&mut host, &h.window, &writer, 91_000, 150_000);
    assert_eq!(host.notice(&h.window, 91_000), "Update postponed.");
    h.window
        .global::<crate::UserActivity>()
        .set_pointer_held(false);

    // Updates replaces the notice once the installation itself begins, and an
    // unreadable clock shows nothing.
    counted(&mut host, &h.window, &writer, 100_000, 160_000);
    assert_eq!(host.notice(&h.window, 100_000), counting(60));
    assert_eq!(host.notice(&h.window, u64::MAX), "");
    counted(&mut host, &h.window, &writer, 101_000, 160_000);
    h.window.set_updates_visible(true);
    assert_eq!(host.notice(&h.window, 101_000), "");
    h.window.set_updates_visible(false);
    assert_eq!(host.notice(&h.window, 101_000), "");
}

#[test]
fn the_announced_countdown_is_drawn_and_any_input_postpones_it() {
    let mut h = Harness::new();
    h.window.set_ingame_menu_visible(false);
    h.window.set_sound_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        socket.clone(),
        Arc::new(|_| Err("the panel sends nothing here".into())),
        Rc::new(move || Some(frames.get())),
        uid(),
        None,
        None,
    );
    h.pump(true);
    assert_eq!(h.window.get_updates_notice(), "");
    let announce = |h: &mut Harness, ends_ms: u64| {
        pumping_exchange(
            h,
            &socket,
            HostRequest::Countdown {
                schema: SCHEMA,
                nonce: 1,
                generation: 27,
                ends_ms,
            },
        )
        .view
        .unwrap()
    };
    let ends_ms = transport::boot_ms().unwrap() + 60_000;
    let before = announce(&mut h, ends_ms);
    h.until(true, |w| {
        w.get_updates_notice()
            .starts_with("Generation 27 installs in ")
    });
    // Showing it is not use of the app and does not change what an offer
    // was made for.
    let after = announce(&mut h, ends_ms);
    assert_eq!(after.revision, before.revision);
    assert!(after.idle_ms >= before.idle_ms);
    assert!(!h.window.get_updates_visible());

    h.window.invoke_updates_activity();
    h.until(true, |w| w.get_updates_notice() == "Update postponed.");
    // The service's next look reads the use and keeps the notice gone.
    let used = announce(&mut h, ends_ms);
    assert!(used.idle_ms.unwrap() < before.idle_ms.unwrap().max(1_000));
    h.pump(true);
    assert_eq!(h.window.get_updates_notice(), "Update postponed.");
}

/// The device service's saved setting as the panel's own exchange reaches it:
/// it applies each save and records every request.
#[derive(Default)]
struct SettingService {
    asked: Mutex<Vec<ServiceRequest>>,
    /// While set, a save waits.
    hold: AtomicBool,
    /// While set, a save fails.
    fail: AtomicBool,
}
impl SettingService {
    fn exchange(self: &Arc<Self>) -> Exchange {
        let service = self.clone();
        let saved = Mutex::new(InstallPolicy {
            mode: InstallMode::Ask,
            window: InstallWindow {
                start_minute: 180,
                end_minute: 300,
            },
            local_minute: Some(14 * 60 + 5),
        });
        Arc::new(move |request: &ServiceRequest| {
            service.asked.lock().unwrap().push(request.clone());
            let mut reply = response(Phase::Idle);
            reply.status.ticket = None;
            match request {
                ServiceRequest::Policy { .. } => {}
                ServiceRequest::SavePolicy { change, .. } => {
                    let end = Instant::now() + Duration::from_secs(5);
                    while service.hold.load(Ordering::SeqCst) && Instant::now() < end {
                        thread::sleep(Duration::from_millis(2));
                    }
                    if service.fail.load(Ordering::SeqCst) {
                        reply.error = Some(Failure::SaveFailed);
                        return Ok(reply);
                    }
                    let mut saved = saved.lock().unwrap();
                    saved.mode = change.mode.unwrap_or(saved.mode);
                    saved.window = change.window.unwrap_or(saved.window);
                }
                _ => unreachable!("installation requests have an exchange of their own"),
            }
            reply.policy = Some(*saved.lock().unwrap());
            Ok(reply)
        })
    }
    fn saves(&self) -> Vec<PolicyChange> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter_map(|request| match request {
                ServiceRequest::SavePolicy { change, .. } => Some(*change),
                _ => None,
            })
            .collect()
    }
}

/// A service with no update to offer.
fn nothing_ready() -> Exchange {
    Arc::new(|request: &ServiceRequest| {
        assert!(matches!(request, ServiceRequest::Offer { .. }));
        let mut reply = response(Phase::Idle);
        reply.status.ticket = None;
        reply.error = Some(Failure::NotReady);
        Ok(reply)
    })
}

fn with_setting(h: &Harness, dir: &Path, setting: Exchange) -> Runtime {
    start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(Host::new(host_identity())),
        dir.join("host.sock"),
        nothing_ready(),
        Rc::new(|| Some(0)),
        uid(),
        None,
        Some(setting),
    )
}

fn window(start_minute: u16, end_minute: u16) -> Option<InstallWindow> {
    Some(InstallWindow {
        start_minute,
        end_minute,
    })
}

#[test]
fn the_setting_page_saves_each_change_and_only_the_row_it_changed() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let service = Arc::new(SettingService::default());
    let _runtime = with_setting(&h, dir.path(), service.exchange());
    h.window.invoke_updates_open();
    h.until(true, |w| w.get_updates_automatic_available());
    h.pump(true);
    h.screenshot("SPACEWARS_UPDATE_SETTING_BUTTON_SCREENSHOT");
    assert_eq!(
        h.window.get_updates_automatic_label(),
        "Automatic install: Off"
    );
    assert_eq!(h.window.get_updates_focus(), 1);
    // Left from Back reaches the setting's button, and A opens its page.
    h.window.invoke_keyboard_action(2, false);
    assert_eq!(
        crate::updates::selected(&h.window).as_deref(),
        Some("updates.automatic")
    );
    h.window.invoke_keyboard_action(4, false);
    assert!(h.window.get_updates_automatic_visible());
    assert_eq!(
        crate::updates::selected(&h.window).as_deref(),
        Some("updates.automatic.mode")
    );
    h.until(true, |w| {
        w.get_updates_automatic_status() == "Saved · Device time 14:05"
    });
    h.pump(true);
    h.screenshot("SPACEWARS_UPDATE_SETTING_SCREENSHOT");
    assert_eq!(
        (
            h.window.get_updates_automatic_mode().as_str(),
            h.window.get_updates_automatic_window().as_str()
        ),
        ("Ask first", "03:00–05:00")
    );
    // Right on Mode saves Automatic and leaves the window alone.
    h.window.invoke_keyboard_action(3, false);
    assert_eq!(h.window.get_updates_automatic_mode(), "Automatic");
    h.until(false, |w| {
        w.get_updates_automatic_status().starts_with("Saved")
    });
    // Down to the window, and Left moves it an hour earlier with its length.
    h.window.invoke_keyboard_action(1, false);
    h.window.invoke_keyboard_action(2, false);
    assert_eq!(h.window.get_updates_automatic_window(), "02:00–04:00");
    h.until(false, |w| {
        w.get_updates_automatic_status().starts_with("Saved")
    });
    // A UI-control client presses the same controls.
    for _ in 0..2 {
        assert!(crate::ui_activation::activate(
            &h.window,
            "updates.automatic.window.next"
        ));
    }
    h.until(false, |w| {
        w.get_updates_automatic_status().starts_with("Saved")
            && w.get_updates_automatic_window() == "04:00–06:00"
    });
    assert_eq!(
        service.saves(),
        [
            PolicyChange {
                mode: Some(InstallMode::Automatic),
                window: None,
            },
            PolicyChange {
                mode: None,
                window: window(120, 240),
            },
            PolicyChange {
                mode: None,
                window: window(180, 300),
            },
            PolicyChange {
                mode: None,
                window: window(240, 360),
            },
        ]
    );
    // B returns to the button, which names the saved window.
    h.window.invoke_keyboard_action(5, false);
    assert!(!h.window.get_updates_automatic_visible());
    assert_eq!(h.window.get_updates_focus(), 2);
    assert_eq!(
        h.window.get_updates_automatic_label(),
        "Automatic install: 04:00–06:00"
    );
    // Right returns to Back, and Up stays there with no update to install.
    h.window.invoke_keyboard_action(3, false);
    h.window.invoke_keyboard_action(0, false);
    assert_eq!(h.window.get_updates_focus(), 1);
    h.window.invoke_keyboard_action(5, false);
    assert!(!h.window.get_updates_visible());
    // Opening Updates again reads the setting again, and the button waits
    // for that read.
    h.window.invoke_updates_open();
    assert!(!h.window.get_updates_automatic_available());
    assert!(!crate::ui_activation::activate(
        &h.window,
        "updates.automatic"
    ));
    h.until(false, |w| w.get_updates_automatic_available());
    let reads = service
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|request| matches!(request, ServiceRequest::Policy { .. }))
        .count();
    assert_eq!(reads, 2);
}

#[test]
fn changes_made_during_a_save_follow_it_and_a_failed_save_shows_what_is_saved() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let service = Arc::new(SettingService::default());
    let _runtime = with_setting(&h, dir.path(), service.exchange());
    h.window.invoke_updates_open();
    h.until(false, |w| w.get_updates_automatic_available());
    assert!(crate::ui_activation::activate(
        &h.window,
        "updates.automatic"
    ));
    service.hold.store(true, Ordering::SeqCst);
    for _ in 0..3 {
        assert!(crate::ui_activation::activate(
            &h.window,
            "updates.automatic.window.next"
        ));
    }
    for _ in 0..5 {
        h.pump(false);
    }
    assert_eq!(h.window.get_updates_automatic_window(), "06:00–08:00");
    assert_eq!(h.window.get_updates_automatic_status(), "Saving…");
    // The first press is with the service; the other two wait and go as one.
    assert_eq!(service.saves().len(), 1);
    service.hold.store(false, Ordering::SeqCst);
    h.until(false, |w| {
        w.get_updates_automatic_status().starts_with("Saved")
    });
    assert_eq!(
        service.saves(),
        [
            PolicyChange {
                mode: None,
                window: window(240, 360),
            },
            PolicyChange {
                mode: None,
                window: window(360, 480),
            },
        ]
    );
    // A failed save says so and shows the saved setting again.
    service.fail.store(true, Ordering::SeqCst);
    assert!(crate::ui_activation::activate(
        &h.window,
        "updates.automatic.mode.next"
    ));
    assert_eq!(h.window.get_updates_automatic_mode(), "Automatic");
    h.until(false, |w| w.get_updates_automatic_failed());
    assert_eq!(
        (
            h.window.get_updates_automatic_status().as_str(),
            h.window.get_updates_automatic_mode().as_str(),
            h.window.get_updates_automatic_window().as_str(),
        ),
        ("Could not save. Try again.", "Ask first", "06:00–08:00")
    );
    // The next press tries again.
    service.fail.store(false, Ordering::SeqCst);
    assert!(crate::ui_activation::activate(
        &h.window,
        "updates.automatic.mode.previous"
    ));
    h.until(false, |w| {
        w.get_updates_automatic_status().starts_with("Saved")
    });
    assert_eq!(h.window.get_updates_automatic_mode(), "Automatic");
    assert!(!h.window.get_updates_automatic_failed());
}

#[test]
fn the_setting_button_stays_disabled_without_its_service() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let _runtime = with_setting(
        &h,
        dir.path(),
        Arc::new(|_: &ServiceRequest| Err("service offline".into())),
    );
    h.window.invoke_updates_open();
    h.until(false, |w| {
        w.get_updates_automatic_label() == "Automatic install: unavailable"
    });
    for _ in 0..3 {
        h.pump(false);
    }
    assert!(!h.window.get_updates_automatic_available());
    h.window.invoke_keyboard_action(2, false);
    assert_eq!(h.window.get_updates_focus(), 1);
    assert!(!crate::ui_activation::activate(
        &h.window,
        "updates.automatic"
    ));
    assert!(!h.window.get_updates_automatic_visible());
}

/// An installation that takes the app over closes the page and removes its
/// button.
#[test]
fn the_setting_steps_aside_for_an_installation() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let service = Arc::new(SettingService::default());
    let _runtime = with_setting(&h, dir.path(), service.exchange());
    h.window.invoke_updates_open();
    h.until(false, |w| w.get_updates_automatic_available());
    assert!(crate::ui_activation::activate(
        &h.window,
        "updates.automatic"
    ));
    assert!(h.window.get_updates_automatic_visible());
    h.window.set_install_fenced(true);
    h.until(false, |w| !w.get_updates_automatic_visible());
    assert!(!h.window.get_updates_automatic_available());
    let ids: Vec<_> = crate::updates::inventory(&h.window)
        .into_iter()
        .map(|control| control.id)
        .collect();
    assert_eq!(ids, ["updates.confirm", "updates.cancel"]);
    assert_eq!(h.window.get_updates_focus(), 1);
}

/// What the device service does once an installation is confirmed: it asks the
/// app what it shows, then hands it the preparation. Runs off the UI thread.
fn hand_over(path: &Path) -> HostReply {
    let view = host_request(
        path,
        HostRequest::Observe {
            schema: SCHEMA,
            nonce: 7,
        },
    )
    .view
    .unwrap();
    host_request(
        path,
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
    )
}

/// The installation the app noted as nobody's press, if any.
fn noted(dir: &Path) -> Option<Ticket> {
    let bytes = std::fs::read(dir.join("unattended-install.json")).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap())
}

/// An installation that ends without a restart. Only the outcome of one
/// nobody pressed Install for closes Updates by itself; either outcome reads
/// the setting again for its button.
fn outcome_without_a_restart(pressed: bool) {
    let mut h = Harness::new();
    h.window.set_launcher_visible(true);
    h.window.set_ingame_menu_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("host.sock");
    let handed = Arc::new(AtomicBool::new(false));
    let confirmed = handed.clone();
    let host_path = socket.clone();
    let service = Arc::new(SettingService::default());
    let mut host = Host::new(host_identity());
    host.unattended.close_ms = 300;
    let frames = h.frames.clone();
    let _runtime = start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.path().join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(host),
        socket.clone(),
        Arc::new(move |request| {
            let mut reply = response(Phase::Preparing);
            match request {
                ServiceRequest::Offer { .. } => {
                    reply.status.phase = Phase::Offered;
                    reply.offer = Some(Offer {
                        schema: SCHEMA,
                        ticket: ticket(),
                        candidate: Candidate {
                            id: ticket().candidate,
                            channel: Channel::Nightly,
                            generation: 29,
                            build_id: "test-image".into(),
                        },
                        ends_game: false,
                        expires_ms: transport::boot_ms().unwrap() + 300_000,
                    });
                }
                ServiceRequest::Confirm { .. } => {
                    assert!(hand_over(&host_path).acknowledgement.is_some());
                    confirmed.store(true, Ordering::SeqCst);
                }
                ServiceRequest::Status { .. } => {}
                _ => unreachable!("the setting has an exchange of its own"),
            }
            Ok(reply)
        }),
        Rc::new(move || Some(frames.get())),
        uid(),
        None,
        Some(service.exchange()),
    );
    if pressed {
        h.window.invoke_updates_open();
        h.until(true, |w| w.get_updates_can_confirm());
        h.window.invoke_updates_confirm();
    } else {
        // The Automatic setting or a command confirmed it elsewhere.
        let path = socket.clone();
        let done = handed.clone();
        thread::spawn(move || {
            assert!(hand_over(&path).acknowledgement.is_some());
            done.store(true, Ordering::SeqCst);
        });
    }
    h.until(true, |_| handed.load(Ordering::SeqCst));
    assert!(h.window.get_updates_visible());
    assert!(h.window.get_install_fenced());
    assert!(!h.window.get_updates_automatic_available());
    assert_eq!(noted(dir.path()), (!pressed).then(ticket));
    let aborted = pumping_exchange(
        &mut h,
        &socket,
        HostRequest::Abort {
            schema: SCHEMA,
            aborted: Aborted {
                schema: SCHEMA,
                ticket: ticket(),
            },
        },
    );
    assert!(aborted.error.is_none(), "{aborted:?}");
    let shown = Instant::now();
    assert!(h.window.get_updates_message().contains("cancelled"));
    assert!(!h.window.get_install_fenced());
    h.until(true, |w| w.get_updates_automatic_available());
    if pressed {
        while shown.elapsed() < Duration::from_millis(700) {
            h.pump(true);
        }
        assert!(
            h.window.get_updates_visible(),
            "the outcome of a press waits for Back"
        );
    } else {
        h.until(true, |w| !w.get_updates_visible());
        assert!(shown.elapsed() >= Duration::from_millis(250));
        assert!(h.window.get_launcher_visible());
    }
}

#[test]
fn the_outcome_of_an_installation_confirmed_elsewhere_closes_by_itself() {
    outcome_without_a_restart(false);
}

#[test]
fn the_outcome_of_a_pressed_installation_waits_for_back() {
    outcome_without_a_restart(true);
}

/// Startup reconciliation that finds the outcome of `ticket()`'s installation,
/// then nothing once the app has shown it.
fn finished(outcome: InstallOutcome) -> reconciliation::Exchange {
    let calls = AtomicUsize::new(0);
    Arc::new(move |request| {
        Ok(ReconcileReply {
            schema: SCHEMA,
            nonce: request.nonce,
            host: request.host.clone(),
            service: Id::from_bytes([8; 32]).unwrap(),
            result: if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                RecoveryDecision::Finished {
                    ticket: ticket(),
                    transaction: Id::from_bytes([4; 32]).unwrap(),
                    outcome,
                }
            } else {
                RecoveryDecision::Clear {}
            },
        })
    })
}

/// The app after the restart that finished an installation, with the note it
/// made before, if any.
fn restarted(h: &Harness, dir: &Path, note: Option<Ticket>, setting: Option<Exchange>) -> Runtime {
    if let Some(note) = note {
        std::fs::write(
            dir.join("unattended-install.json"),
            serde_json::to_vec(&note).unwrap(),
        )
        .unwrap();
    }
    let mut host = Host::new(host_identity());
    host.unattended.close_ms = 300;
    start(
        &h.window,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(dir.join("settings.toml")).unwrap(),
        h.input.clone(),
        Some(host),
        dir.join("host.sock"),
        Arc::new(|_| Err("unused".into())),
        Rc::new(|| Some(0)),
        uid(),
        Some(finished(InstallOutcome::Committed)),
        setting,
    )
}

/// The outcome the restart found names the setting on its button, and waits
/// for Back unless the app noted its installation as nobody's press.
#[test]
fn the_outcome_after_a_restart_reads_the_setting_and_waits_for_back() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let service = Arc::new(SettingService::default());
    // A note left by an earlier installation does not name this one.
    let earlier = Ticket {
        sequence: 2,
        ..ticket()
    };
    let _runtime = restarted(&h, dir.path(), Some(earlier), Some(service.exchange()));
    h.until(true, |w| !w.get_install_fenced());
    assert!(h.window.get_updates_visible());
    assert!(h.window.get_updates_message().contains("installed"));
    h.until(true, |w| w.get_updates_automatic_available());
    assert_eq!(
        h.window.get_updates_automatic_label(),
        "Automatic install: Off"
    );
    assert_eq!(noted(dir.path()), None);
    let shown = Instant::now();
    while shown.elapsed() < Duration::from_millis(700) {
        h.pump(true);
    }
    assert!(h.window.get_updates_visible());
}

/// After a restart, the outcome of an installation nobody pressed Install for
/// closes Updates once it has gone a while without use.
#[test]
fn the_outcome_after_a_restart_closes_by_itself_once_unused() {
    let mut h = Harness::new();
    h.window.set_launcher_visible(true);
    h.window.set_ingame_menu_visible(false);
    let dir = tempfile::tempdir().unwrap();
    let _runtime = restarted(&h, dir.path(), Some(ticket()), None);
    h.until(true, |w| !w.get_install_fenced());
    assert!(h.window.get_updates_visible());
    assert!(h.window.get_updates_message().contains("installed"));
    // The note stays until the device service has retired the outcome.
    assert_eq!(noted(dir.path()), Some(ticket()));
    let used = Instant::now();
    while used.elapsed() < Duration::from_millis(700) {
        h.window.invoke_updates_activity();
        h.pump(true);
    }
    assert!(h.window.get_updates_visible(), "use keeps the outcome open");
    h.until(true, |_| noted(dir.path()).is_none());
    let quiet = Instant::now();
    h.until(true, |w| !w.get_updates_visible());
    assert!(quiet.elapsed() >= Duration::from_millis(250));
    assert!(h.window.get_launcher_visible());
}
