//! Automatic attempts through the real service and app sockets, with the
//! device's saved preference and attempt history on disk.
use super::*;
use crate::install_admission::{Gate, History, Moment};
use crate::install_policy::{Change, Mode, Preference};
use automatic::{Hold, IDLE_MS, Pace, Phase as Watcher, Status as Watched};

const IMAGE: &str = "test-image";

fn pace(countdown_ms: u64) -> Pace {
    Pace {
        watch: Duration::from_millis(5),
        count: Duration::from_millis(5),
        countdown: Duration::from_millis(countdown_ms),
    }
}
/// Long enough to see that nothing reached the app before it ended.
const SHORT: u64 = 250;
/// Long enough that a slow test thread still acts inside it.
const LONG: u64 = 60_000;

/// The saved preference and attempt history of one device, and its clock.
struct Device {
    dir: tempfile::TempDir,
    now: Arc<Mutex<Option<Moment>>>,
    /// The preference the last opened gate reads, as the status socket's
    /// handler holds it in the service.
    preference: Mutex<Option<Arc<Preference>>>,
}
impl Device {
    /// 03:30 local on the first night, inside the default window.
    fn new(mode: Mode) -> Self {
        let dir = tempfile::tempdir().unwrap();
        if mode == Mode::Automatic {
            Preference::open(&dir.path().join("install-policy"))
                .change(Change {
                    mode: Some(mode),
                    window: None,
                })
                .unwrap();
        }
        let device = Self {
            dir,
            now: Arc::new(Mutex::new(None)),
            preference: Mutex::new(None),
        };
        device.night(0);
        device
    }
    fn night(&self, days: i64) {
        *self.now.lock().unwrap() = Some(Moment {
            utc_s: 1_794_310_200 + days * 86_400,
            date: jiff::civil::date(2026, 11, 10)
                .checked_add(jiff::Span::new().days(days))
                .unwrap(),
            minute: 3 * 60 + 30,
        });
    }
    /// Opens the stores as a starting service does. Only one owner at a time.
    fn gate(&self) -> Arc<Gatekeeper> {
        let now = self.now.clone();
        // The previous owner's lock is released before the stores reopen.
        self.preference.lock().unwrap().take();
        let preference = Arc::new(Preference::open(&self.dir.path().join("install-policy")));
        *self.preference.lock().unwrap() = Some(preference.clone());
        Arc::new(Gatekeeper::new(
            preference,
            History::open(&self.dir.path().join("install-admission")),
            Arc::new(move || *now.lock().unwrap()),
        ))
    }
    /// The owner's `install-policy --mode` command.
    fn save(&self, mode: Mode) {
        let preference = self.preference.lock().unwrap().clone().unwrap();
        preference
            .change(Change {
                mode: Some(mode),
                window: None,
            })
            .unwrap();
    }
}

fn show(f: &Fixture, scene: Option<Scene>, idle_ms: Option<u64>) {
    *f.shown.lock().unwrap() = Shown { scene, idle_ms };
}
fn idle_launcher(f: &Fixture) {
    show(f, Some(Scene::Launcher), Some(IDLE_MS));
}
fn watched(f: &Fixture) -> Watched {
    f.service.automatic().unwrap().read().unwrap()
}
fn watching(hold: Hold) -> Watched {
    Watched {
        phase: Watcher::Watching,
        hold: Some(hold),
        countdown_ends_ms: None,
    }
}
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(2));
    }
}
/// Lets the watcher look at the app this many more times.
fn looks(f: &Fixture, count: usize) {
    let from = f.observations.load(Ordering::SeqCst);
    until("more observations", || {
        f.observations.load(Ordering::SeqCst) >= from + count
    });
}
fn untouched(gate: &Gatekeeper) {
    let status = gate.status(Some(IMAGE));
    assert_eq!(
        (status.gate, status.last_attempt, status.tried_build_id),
        (Gate::Open, None, None)
    );
}

#[test]
fn an_idle_launcher_counts_down_then_installs_without_a_confirmation() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    // An app that says nothing about itself is watched, and nothing is spent
    // or announced.
    until("the watcher", || watched(&f) == watching(Hold::Game));
    looks(&f, 5);
    untouched(&gate);
    assert!(f.countdowns.lock().unwrap().is_empty());

    idle_launcher(&f);
    until("the countdown", || watched(&f).phase == Watcher::Countdown);
    let counting = Instant::now();
    let ends_ms = watched(&f).countdown_ends_ms.unwrap();
    // The window's attempt is spent before the countdown, and the app has not
    // been asked to prepare anything yet.
    let spent = gate.status(Some(IMAGE));
    assert_eq!(spent.gate, Gate::WindowUsed);
    assert!(spent.last_attempt.is_some());
    assert_eq!(spent.tried_build_id, None);
    assert!(f.entered.try_recv().is_err());
    assert!(!f.fenced.load(Ordering::SeqCst));

    f.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(counting.elapsed() >= Duration::from_millis(SHORT / 2));
    // Every look during the countdown told the app which generation installs
    // and when, the same end that the status reports.
    let announced = f.countdowns.lock().unwrap().clone();
    assert!(announced.len() >= 2, "{announced:?}");
    assert!(announced.iter().all(|told| *told == (11, ends_ms)));
    until("the handoff", || watched(&f).phase == Watcher::Started);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);
    f.release.send(()).unwrap();
    f.wait(Phase::Acknowledged);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 1);
    // Whatever its installation does next, this image is never tried again.
    let tried = gate.status(Some(IMAGE));
    assert_eq!(tried.gate, Gate::ImageTried);
    assert_eq!(tried.tried_build_id.as_deref(), Some(IMAGE));
    // How the countdown ended stays readable while the gate is closed.
    thread::sleep(Duration::from_millis(100));
    assert_eq!(watched(&f).phase, Watcher::Started);
}

#[test]
fn the_clock_is_ended_and_a_game_never_is() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    // Nobody has touched this game for a day. It is still a game.
    show(&f, Some(Scene::Game), Some(24 * 60 * 60_000));
    until("the watcher", || watched(&f) == watching(Hold::Game));
    looks(&f, 20);
    assert_eq!(watched(&f), watching(Hold::Game));
    untouched(&gate);
    assert!(f.entered.try_recv().is_err());

    // The fixture's app reports the Clock as a game to a confirmation, as the
    // real one does, so this attempt had to say that it ends one.
    show(&f, Some(Scene::Clock), Some(IDLE_MS));
    f.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    f.release.send(()).unwrap();
    f.wait(Phase::Acknowledged);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 1);
}

#[test]
fn use_of_the_app_during_the_countdown_postpones_to_the_next_window() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(LONG))));
    idle_launcher(&f);
    until("the countdown", || watched(&f).phase == Watcher::Countdown);
    // Somebody presses a button.
    show(&f, Some(Scene::Launcher), Some(0));
    let postponed = Watched {
        phase: Watcher::Postponed,
        hold: Some(Hold::InUse),
        countdown_ends_ms: None,
    };
    until("the postponement", || watched(&f) == postponed);
    let announced = f.countdowns.lock().unwrap().len();
    // They leave again, in the same window. It had its attempt, and the app
    // is told about no countdown.
    idle_launcher(&f);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(watched(&f), postponed);
    assert_eq!(f.countdowns.lock().unwrap().len(), announced);
    assert_eq!(gate.status(Some(IMAGE)).gate, Gate::WindowUsed);
    assert!(f.entered.try_recv().is_err());
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);

    // The next night's window opens another countdown.
    device.night(1);
    until("the next countdown", || {
        watched(&f).phase == Watcher::Countdown
    });
}

#[test]
fn saving_ask_during_the_countdown_stops_it() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(LONG))));
    idle_launcher(&f);
    until("the countdown", || watched(&f).phase == Watcher::Countdown);
    // Nobody touches the app; the owner changes the mode from a shell.
    device.save(Mode::Ask);
    let stopped = Watched {
        phase: Watcher::Postponed,
        hold: Some(Hold::NotAutomatic),
        countdown_ends_ms: None,
    };
    until("the countdown to stop", || watched(&f) == stopped);
    // Automatic again in the same window: it had its attempt.
    device.save(Mode::Automatic);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(watched(&f), stopped);
    assert_eq!(gate.status(Some(IMAGE)).gate, Gate::WindowUsed);
    assert!(f.entered.try_recv().is_err());
}

#[test]
fn a_restart_during_the_countdown_does_not_count_down_again() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(LONG))));
    idle_launcher(&f);
    until("the countdown", || watched(&f).phase == Watcher::Countdown);
    drop(f);
    drop(gate);

    // The restarted service reads what the countdown saved before it began.
    let gate = device.gate();
    assert_eq!(gate.status(Some(IMAGE)).gate, Gate::WindowUsed);
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    idle_launcher(&f);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(watched(&f).phase, Watcher::Waiting);
    // A closed gate does not even ask the app what it shows.
    assert_eq!(f.observations.load(Ordering::SeqCst), 0);
    assert!(f.entered.try_recv().is_err());
}

#[test]
fn an_app_in_use_or_a_device_that_refuses_is_watched_without_spending_the_window() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    for (scene, idle_ms, hold) in [
        (Some(Scene::Launcher), Some(IDLE_MS - 1), Hold::InUse),
        (Some(Scene::Clock), None, Hold::InUse),
        (None, Some(IDLE_MS), Hold::Game),
    ] {
        show(&f, scene, idle_ms);
        until("the watcher", || watched(&f) == watching(hold));
    }
    // An app that another handoff holds.
    idle_launcher(&f);
    f.fenced.store(true, Ordering::SeqCst);
    until("the watcher", || {
        watched(&f) == watching(Hold::AppUnavailable)
    });
    f.fenced.store(false, Ordering::SeqCst);
    // A device on which a person's offer would be refused too, such as one
    // waiting for the normal restart after its last update.
    *f.backend.refusal.lock().unwrap() = Some(Failure::NormalBootRequired);
    until("the watcher", || watched(&f) == watching(Hold::Refused));
    looks(&f, 20);
    assert_eq!(watched(&f), watching(Hold::Refused));
    untouched(&gate);
    assert!(f.entered.try_recv().is_err());

    // Once the device allows it, the same window still has its attempt.
    *f.backend.refusal.lock().unwrap() = None;
    until("the countdown", || watched(&f).phase == Watcher::Countdown);
}

#[test]
fn the_watchers_questions_do_not_repeat_an_old_abort_to_the_app() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    // A person's offer is made and withdrawn, and the app hears of it.
    let offer = f
        .request(ServiceRequest::Offer { schema: SCHEMA })
        .offer
        .unwrap();
    let cancelled = f.request(ServiceRequest::Cancel {
        schema: SCHEMA,
        ticket: offer.ticket,
    });
    assert!(cancelled.aborted.is_some());
    until("the abort", || f.aborts.load(Ordering::SeqCst) >= 1);
    thread::sleep(Duration::from_millis(50));
    let heard = f.aborts.load(Ordering::SeqCst);

    // The watcher now asks the worker again and again, on a device that keeps
    // refusing. The withdrawn offer is still the coordinator's last word.
    *f.backend.refusal.lock().unwrap() = Some(Failure::NormalBootRequired);
    idle_launcher(&f);
    until("the watcher", || watched(&f) == watching(Hold::Refused));
    looks(&f, 20);
    assert!(
        f.request(ServiceRequest::Status { schema: SCHEMA })
            .aborted
            .is_some()
    );
    assert_eq!(f.aborts.load(Ordering::SeqCst), heard);
    untouched(&gate);
}

#[test]
fn an_image_that_cannot_be_remembered_is_not_installed() {
    let device = Device::new(Mode::Automatic);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    idle_launcher(&f);
    f.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    // The history's storage fails between the countdown and the consent.
    std::fs::remove_dir_all(device.dir.path().join("install-admission")).unwrap();
    f.release.send(()).unwrap();
    // The consent is spent, so the handoff cannot simply be cancelled; the
    // backend was never given an authorization to write with.
    f.wait(Phase::RecoveryRequired);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 0);
}

#[test]
fn ask_mode_never_looks_at_the_app_and_a_confirmation_is_not_an_attempt() {
    let device = Device::new(Mode::Ask);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(SHORT))));
    idle_launcher(&f);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(f.observations.load(Ordering::SeqCst), 0);
    assert_eq!(watched(&f).phase, Watcher::Waiting);
    assert_eq!(gate.status(Some(IMAGE)).gate, Gate::NotAutomatic);

    // A person's confirmation on the same service leaves no automatic trace.
    f.prepare();
    f.release.send(()).unwrap();
    f.wait(Phase::Acknowledged);
    assert_eq!(f.backend.accepted.load(Ordering::SeqCst), 1);
    let status = gate.status(Some(IMAGE));
    assert_eq!((status.last_attempt, status.tried_build_id), (None, None));
}

fn read(f: &Fixture) -> ServiceReply {
    f.request(ServiceRequest::Policy { schema: SCHEMA })
}
fn save(f: &Fixture, mode: Option<InstallMode>, window: Option<InstallWindow>) -> ServiceReply {
    f.request(ServiceRequest::SavePolicy {
        schema: SCHEMA,
        change: PolicyChange { mode, window },
    })
}

#[test]
fn the_panel_reads_and_saves_the_setting_that_the_watcher_follows() {
    let device = Device::new(Mode::Ask);
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(LONG))));
    let ask = InstallPolicy {
        mode: InstallMode::Ask,
        window: InstallWindow {
            start_minute: 180,
            end_minute: 300,
        },
        local_minute: Some(3 * 60 + 30),
    };
    assert_eq!(read(&f).policy, Some(ask));
    // Saving the mode keeps the window, and the watcher starts watching.
    let saved = save(&f, Some(InstallMode::Automatic), None);
    assert_eq!(saved.error, None);
    assert_eq!(
        saved.policy,
        Some(InstallPolicy {
            mode: InstallMode::Automatic,
            ..ask
        })
    );
    until("the watcher", || watched(&f) == watching(Hold::Game));
    // Saving the window keeps the mode.
    let afternoon = InstallWindow {
        start_minute: 13 * 60,
        end_minute: 15 * 60,
    };
    let saved = save(&f, None, Some(afternoon));
    assert_eq!(
        saved.policy,
        Some(InstallPolicy {
            mode: InstallMode::Automatic,
            window: afternoon,
            ..ask
        })
    );
    let status = gate.policy().unwrap();
    assert_eq!(
        (status.generation, status.mode, status.window.start_minute),
        (2, Mode::Automatic, 13 * 60)
    );
    // Without usable time the reply leaves the minute out.
    *device.now.lock().unwrap() = None;
    assert_eq!(read(&f).policy.unwrap().local_minute, None);
}

#[test]
fn an_unreadable_setting_is_unavailable_to_the_panel_and_refuses_its_saves() {
    // Automatic is saved, then the record is edited outside its owner.
    let device = Device::new(Mode::Automatic);
    let state = device.dir.path().join("install-policy/state.json");
    let edited = std::fs::read_to_string(&state)
        .unwrap()
        .replace("\"automatic\"", "\"ask\"");
    std::fs::write(&state, edited).unwrap();
    let gate = device.gate();
    let f = Fixture::with(Some((gate.clone(), pace(LONG))));
    let reply = read(&f);
    assert_eq!(
        (reply.error, reply.policy),
        (Some(Failure::Unavailable), None)
    );
    let reply = save(&f, Some(InstallMode::Ask), None);
    assert_eq!(
        (reply.error, reply.policy),
        (Some(Failure::SaveFailed), None)
    );
    assert!(!gate.automatic());
}

#[test]
fn a_service_without_the_gate_has_no_setting_for_the_panel() {
    let f = Fixture::new();
    for reply in [read(&f), save(&f, Some(InstallMode::Automatic), None)] {
        assert_eq!(
            (reply.error, reply.policy),
            (Some(Failure::Unavailable), None)
        );
    }
}
