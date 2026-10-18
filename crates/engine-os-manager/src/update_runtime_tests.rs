use super::*;
use crate::network_controller;
use crate::network_observation::Observation;
use crate::network_policy::{self, Activity, HoldKind, Intent, Policy};
use engine_ota::Sha256;
use std::collections::BTreeMap;

const BOOT: &str = "11111111-2222-3333-4444-555555555555";

#[test]
fn native_network_read_accepts_publication_after_the_callers_clock_sample() {
    use crate::linux;
    use crate::service::ObservedState;
    use crate::time::{TimeReason, TimeRecord, TimeTracker};
    let boot_id = linux::boot_id().unwrap();
    let reading = linux::clock_reading().unwrap();
    let observed = Clock {
        boot_id: boot_id.clone(),
        ms: reading.boottime_us / 1000,
    };
    let earlier = Clock {
        boot_id: boot_id.clone(),
        ms: observed.ms.checked_sub(1).unwrap(),
    };
    let watch = Watch::default();
    let mut snapshot = Fake::new().network(&clock(1000)).unwrap().status;
    snapshot.observed = observed.clone();
    watch.publish(Some(snapshot));
    let native = native::Native::new(
        Arc::new(Mutex::new(ObservedState {
            tracker: TimeTracker::new(
                boot_id.clone(),
                TimeRecord::default(),
                TimeReason::WaitingForNetworkSync,
                reading,
                None,
            ),
            boot_id: boot_id.clone(),
            provider_error: None,
            persistence_error: None,
            rtc_error: None,
            observed_at: Instant::now(),
        })),
        watch,
        boot_id,
    );
    assert_eq!(native.network(&earlier).unwrap().status.observed, observed);
}

fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
#[derive(Clone, Copy)]
enum Work {
    Finish,
    Fail,
    Wait,
}
struct World {
    ms: u64,
    epoch: u64,
    mode: u64,
    partial: bool,
    route: bool,
    time: bool,
    work: Work,
    fail_prepare: bool,
    busy_prepare: bool,
    fail_admit: bool,
    fail_renew: bool,
    admit_delay: u64,
    running: bool,
    sessions: usize,
    prepares: usize,
    renewals: usize,
    holds: BTreeMap<String, u64>,
    trace: Vec<&'static str>,
}
#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);
impl Fake {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(World {
            ms: 1000,
            epoch: 1,
            mode: 1,
            partial: true,
            route: true,
            time: true,
            work: Work::Finish,
            fail_prepare: false,
            busy_prepare: false,
            fail_admit: false,
            fail_renew: false,
            admit_delay: 0,
            running: false,
            sessions: 0,
            prepares: 0,
            renewals: 0,
            holds: BTreeMap::from([("other-owner".into(), u64::MAX)]),
            trace: Vec::new(),
        })))
    }
    fn edit(&self, change: impl FnOnce(&mut World)) {
        change(&mut self.0.lock().unwrap());
    }
}
impl Inputs for Fake {
    fn clock(&self) -> Result<Clock> {
        Ok(clock(self.0.lock().unwrap().ms))
    }
    fn time(&self) -> Option<UsableTime> {
        let w = self.0.lock().unwrap();
        w.time.then(|| {
            UsableTime::from_unix_microseconds(1_800_000_000_000_000 + w.ms * 1000).unwrap()
        })
    }
    fn network(&self, _clock: &Clock) -> Option<Snapshot> {
        let w = self.0.lock().unwrap();
        Some(Snapshot {
            epoch: w.epoch,
            status: network_controller::Status {
                observed: clock(w.ms),
                policy: network_policy::Status {
                    generation: 1,
                    mode_generation: w.mode,
                    policy: Policy {
                        partial_offline: w.partial,
                        stay_connected: false,
                    },
                    intent: Intent::Available,
                    holds: w
                        .holds
                        .values()
                        .map(|end| network_policy::HoldStatus {
                            kind: HoldKind::Download,
                            expires_in_ms: end.saturating_sub(w.ms),
                            reconnect_grace: false,
                            wireless_required: false,
                        })
                        .collect(),
                },
                actual: Some(Observation {
                    schema: 1,
                    bus_owner: ":1.1".into(),
                    pid: 11,
                    networking_enabled: true,
                    wireless_enabled: true,
                    wireless_hardware_enabled: true,
                    startup: false,
                    checkpoint_count: 0,
                    state: 70,
                    profile_revision: Sha256::digest(b"fixture"),
                    profile_count: 1,
                    ethernet_available: w.route,
                    wireless_available: false,
                }),
                activity: Some(Activity {
                    ssh_sessions: Some(false),
                    checkpoints: Some(false),
                    install_lock: Some(false),
                    operations: Some(false),
                }),
                owned_off: false,
                pending: None,
                suspended: None,
                deferred: None,
            },
        })
    }
}
struct Running(Fake);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.edit(|w| {
            w.running = false;
            w.trace.push("io-finished");
        });
    }
}
impl Backend for Fake {
    type Prepared = Self;
    type Ready = ();
    fn prepare(
        &mut self,
        _active: &Active,
        _channel: Channel,
        _control: Arc<Control>,
    ) -> std::result::Result<Self, PreparationError> {
        let mut w = self.0.lock().unwrap();
        w.prepares += 1;
        w.trace.push("prepared");
        if w.busy_prepare {
            Err(PreparationError::MaintenanceBusy)
        } else if w.fail_prepare {
            Err(PreparationError::failed("fixture", unavailable()))
        } else {
            Ok(self.clone())
        }
    }
    fn run(fake: Self, control: Arc<Control>) -> engine_ota::Result<Finished<()>> {
        fake.edit(|w| {
            w.running = true;
            w.trace.push("spawned");
        });
        let _running = Running(fake.clone());
        control.wait_ready()?;
        let work = {
            let mut w = fake.0.lock().unwrap();
            w.sessions += 1;
            w.trace.push("network-started");
            w.work
        };
        use download::Control as _;
        control.progress(Progress {
            phase: download::Phase::Checking,
            candidate_id: None,
            bytes: 0,
            total: 0,
        });
        match work {
            Work::Fail => Err(interrupted()),
            Work::Finish => Ok(Finished {
                outcome: download::Outcome::NoCandidate,
                proof: None,
                time: control.checkpoint()?.unwrap(),
            }),
            Work::Wait => {
                control.progress(Progress {
                    phase: download::Phase::Downloading,
                    candidate_id: None,
                    bytes: 1,
                    total: 10,
                });
                while control.checkpoint().is_ok() {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(interrupted())
            }
        }
    }
    fn ready_valid(&mut self, _proof: &()) -> bool {
        true
    }
    fn admit(&mut self, token: &HoldToken, request: &DownloadReservation) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        if let Some(parent) = &request.time_hold {
            assert!(w.holds.contains_key(parent.expose()));
        }
        w.holds.insert(token.expose().into(), request.deadline.ms);
        w.trace.push("admitted");
        w.ms += w.admit_delay;
        if w.fail_admit {
            Err(unavailable())
        } else {
            Ok(())
        }
    }
    fn renew(&mut self, token: &HoldToken, deadline_ms: u64) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        assert_eq!(w.holds.get(token.expose()), Some(&deadline_ms));
        w.renewals += 1;
        if w.fail_renew {
            Err(unavailable())
        } else {
            Ok(())
        }
    }
    fn release(&mut self, token: &HoldToken) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        assert!(!w.running, "network IO must end before releasing its hold");
        w.holds.remove(token.expose());
        w.trace.push("released");
        Ok(())
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    runner: Runner<Fake>,
    fake: Fake,
    handle: Handle,
    requests: mpsc::Receiver<Command>,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut runner =
            Runner::open(&temp.path().join("updates"), fake.clone(), &clock(1000)).unwrap();
        let (tx, requests) = mpsc::sync_channel(8);
        let handle = Handle {
            inputs: Arc::new(fake.clone()),
            requests: tx,
            stop: Arc::new(AtomicBool::new(false)),
            shared: Arc::new(Mutex::new(Shared {
                status: Status {
                    phase: Phase::Idle,
                    reason: None,
                    schedule: None,
                    progress: None,
                    ready: None,
                    manual_pending: false,
                },
                observed: clock(1000),
                manual_pending: None,
                selections_pending: 0,
                control: Weak::new(),
                ready: None,
            })),
        };
        runner.publish(&handle).unwrap();
        Self {
            _temp: temp,
            runner,
            fake,
            handle,
            requests,
        }
    }
    fn step(&mut self) {
        self.runner.step(&self.requests, &self.handle).unwrap();
    }
    fn until(&mut self, condition: impl Fn(&Self) -> bool) {
        let end = Instant::now() + Duration::from_secs(3);
        while !condition(self) {
            assert!(Instant::now() < end, "worker condition timed out");
            self.step();
            thread::sleep(Duration::from_millis(2));
        }
    }
}

#[test]
fn runtime_boot_maintenance_contention_preserves_the_original_attempt() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.busy_prepare = true);
    f.step();
    let waiting = f.handle.status().unwrap().schedule.unwrap();
    let active = waiting
        .active
        .expect("busy boot recovery must not consume the startup check");
    assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
    assert_eq!(f.handle.status().unwrap().phase, Phase::Preparing);
    assert_eq!(
        f.handle.status().unwrap().reason,
        Some(Reason::MaintenanceBusy)
    );
    assert!(f.handle.check_now().unwrap().coalesced);
    for ms in [1100, 1200, 1999] {
        f.fake.edit(|w| w.ms = ms);
        f.step();
    }
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
    f.fake.edit(|w| {
        w.ms = 2000;
        w.busy_prepare = false;
        w.work = Work::Wait;
    });
    f.step();
    f.until(|f| f.fake.0.lock().unwrap().sessions == 1);
    let resumed = &f.runner.job.as_ref().unwrap().active;
    assert_eq!(resumed.id, active.id);
    assert_eq!(resumed.started_ms, active.started_ms);
    assert_eq!(resumed.admission_deadline_ms, active.admission_deadline_ms);
    assert_eq!(
        resumed.preparation_deadline_ms,
        active.preparation_deadline_ms
    );
    assert_eq!(resumed.deadline_ms, active.deadline_ms);
    assert_eq!(f.fake.0.lock().unwrap().prepares, 2);
    assert_eq!(f.handle.status().unwrap().reason, None);
}

#[test]
fn runtime_busy_maintenance_expires_without_renewing_manual_or_daily_allowances() {
    for manual in [false, true] {
        let mut f = Fixture::new();
        f.fake.edit(|w| w.busy_prepare = true);
        if manual {
            f.handle.check_now().unwrap();
        }
        f.step();
        let started = f.handle.status().unwrap().schedule.unwrap();
        let active = started.active.unwrap();
        for ms in [2000, active.admission_deadline_ms - 1] {
            f.fake.edit(|w| w.ms = ms);
            f.step();
            assert!(f.handle.check_now().unwrap().coalesced);
            assert_eq!(
                f.handle
                    .status()
                    .unwrap()
                    .schedule
                    .unwrap()
                    .active
                    .unwrap()
                    .id,
                active.id
            );
        }
        // Releasing the lock exactly at the original deadline is too late.
        f.fake.edit(|w| {
            w.ms = active.admission_deadline_ms;
            w.busy_prepare = false;
        });
        f.step();
        let expired = f.handle.status().unwrap().schedule.unwrap();
        assert!(expired.active.is_none());
        assert_eq!(expired.completion, Some(Completion::AdmissionTimeout));
        let prepares = f.fake.0.lock().unwrap().prepares;
        for ms in [900_000, 3_600_000, 21_600_000] {
            f.fake.edit(|w| w.ms = ms);
            f.step();
        }
        assert_eq!(f.fake.0.lock().unwrap().prepares, prepares);
        assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
        assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
        // An explicit new request can still start a separate bounded attempt.
        assert!(!f.handle.check_now().unwrap().coalesced);
        f.step();
        f.until(|f| f.runner.job.is_none());
        assert_eq!(f.fake.0.lock().unwrap().sessions, 1);
    }
}

#[test]
fn runtime_busy_time_handoff_never_becomes_an_independent_offline_wake() {
    let mut f = Fixture::new();
    let parent = HoldToken::parse("d".repeat(64)).unwrap();
    f.fake.edit(|w| {
        w.route = false;
        w.busy_prepare = true;
        w.holds.insert(parent.expose().into(), 121_000);
    });
    let (reply, answer) = mpsc::sync_channel(1);
    f.handle
        .requests
        .try_send(Command::Offer(Offer {
            parent: parent.clone(),
            until: clock(6000),
            cancelled: Arc::new(AtomicBool::new(false)),
            reply,
        }))
        .unwrap();
    f.step();
    assert!(!answer.recv().unwrap());
    let original = f.handle.status().unwrap().schedule.unwrap().active.unwrap();
    f.fake.edit(|w| {
        w.holds.remove(parent.expose());
        w.busy_prepare = false;
    });
    for ms in [2000, 6000, 7000] {
        f.fake.edit(|w| w.ms = ms);
        f.step();
    }
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
    assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
    // Fresh existing connectivity can finish the same attempt; the lost parent
    // capability is neither retained nor supplied to admission.
    f.fake.edit(|w| {
        w.route = true;
        w.work = Work::Wait;
    });
    f.step();
    f.until(|f| f.fake.0.lock().unwrap().sessions == 1);
    assert_eq!(f.runner.job.as_ref().unwrap().active.id, original.id);
    assert_eq!(
        f.runner.job.as_ref().unwrap().active.deadline_ms,
        original.deadline_ms
    );
}

#[test]
fn runtime_pending_maintenance_cancels_on_channel_mode_or_coordinator_change() {
    for change in ["channel", "mode", "coordinator"] {
        let mut f = Fixture::new();
        f.fake.edit(|w| w.busy_prepare = true);
        f.step();
        match change {
            "channel" => {
                f.handle.select_channel(Channel::Nightly).unwrap();
            }
            "mode" => f.fake.edit(|w| w.mode += 2),
            _ => f.fake.edit(|w| w.epoch += 1),
        }
        f.fake.edit(|w| {
            w.ms = 2000;
            w.busy_prepare = false;
            w.route = false;
        });
        f.step();
        assert!(f.runner.preparation_retry.is_none(), "{change}");
        assert!(
            f.handle
                .status()
                .unwrap()
                .schedule
                .unwrap()
                .active
                .is_none(),
            "{change}"
        );
        assert_eq!(f.fake.0.lock().unwrap().prepares, 1, "{change}");
        assert_eq!(f.fake.0.lock().unwrap().sessions, 0, "{change}");
        assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1, "{change}");
    }
}

#[test]
fn runtime_restart_while_waiting_for_maintenance_does_not_replay_the_attempt() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.busy_prepare = true);
    f.step();
    drop(f.runner);
    f.fake.edit(|w| {
        w.ms = 2000;
        w.busy_prepare = false;
    });
    f.runner = Runner::open(
        &f._temp.path().join("updates"),
        f.fake.clone(),
        &clock(2000),
    )
    .unwrap();
    f.step();
    let restarted = f.handle.status().unwrap().schedule.unwrap();
    assert!(restarted.active.is_none());
    assert_eq!(restarted.completion, Some(Completion::Interrupted));
    for ms in [5000, 900_000, 3_600_000] {
        f.fake.edit(|w| w.ms = ms);
        f.step();
    }
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
    assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
}

#[test]
fn runtime_automatic_download_finishes_and_releases_only_its_hold_once_per_day() {
    let mut f = Fixture::new();
    f.step();
    f.until(|f| f.runner.job.is_none());
    let world = f.fake.0.lock().unwrap();
    assert_eq!(world.sessions, 1);
    assert_eq!(
        world.holds.keys().collect::<Vec<_>>(),
        [&"other-owner".to_owned()]
    );
    assert_eq!(
        world.trace,
        [
            "prepared",
            "admitted",
            "spawned",
            "network-started",
            "io-finished",
            "released"
        ]
    );
    drop(world);
    for ms in [2000, 900_000, 3_600_000, 21_600_000] {
        f.fake.edit(|w| w.ms = ms);
        f.step();
    }
    assert_eq!(f.fake.0.lock().unwrap().sessions, 1);
    assert_eq!(
        f.handle.status().unwrap().schedule.unwrap().completion,
        Some(Completion::NoCandidate)
    );
}

#[test]
fn runtime_status_does_not_wake_offline_network_and_manual_check_coalesces() {
    let mut f = Fixture::new();
    f.fake.edit(|w| {
        w.route = false;
        w.work = Work::Wait;
    });
    for _ in 0..5 {
        f.step();
        assert_eq!(f.handle.status().unwrap().phase, Phase::Idle);
    }
    assert_eq!(f.fake.0.lock().unwrap().prepares, 0);
    assert!(!f.handle.check_now().unwrap().coalesced);
    assert!(f.handle.check_now().unwrap().coalesced);
    f.step();
    assert!(f.handle.check_now().unwrap().coalesced);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
    let deadline = f.runner.job.as_ref().unwrap().active.deadline_ms;
    f.fake.edit(|w| {
        w.ms = 35_000;
        w.route = true;
    });
    f.until(|f| f.fake.0.lock().unwrap().sessions == 1);
    assert_eq!(f.runner.job.as_ref().unwrap().active.deadline_ms, deadline);
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
}

#[test]
fn runtime_time_handoff_is_registered_before_the_offer_returns() {
    let mut f = Fixture::new();
    let parent = HoldToken::parse("b".repeat(64)).unwrap();
    f.fake.edit(|w| {
        w.route = false;
        w.time = false;
        w.holds.insert(parent.expose().into(), 121_000);
    });
    let handle = f.handle.clone();
    let offered = parent.clone();
    let call = thread::spawn(move || handle.offer_time(&offered, &clock(121_000)));
    f.until(|f| f.runner.job.is_some());
    call.join().unwrap();
    assert_eq!(f.fake.0.lock().unwrap().holds.len(), 3);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
    // A very fast NTP completion can now release its own token safely.
    f.fake.edit(|w| {
        w.holds.remove(parent.expose());
        w.time = true;
        w.route = true;
    });
    f.until(|f| f.runner.job.is_none());
    assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 1);
}

#[test]
fn runtime_late_queued_or_late_admitted_manual_request_never_starts_http() {
    for delayed_reply in [false, true] {
        let mut f = Fixture::new();
        f.fake.edit(|w| w.route = false);
        f.handle.check_now().unwrap();
        f.fake.edit(|w| {
            if delayed_reply {
                w.admit_delay = 30_000;
            } else {
                w.ms += 30_000;
            }
        });
        f.step();
        assert!(f.runner.job.is_none());
        assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
        assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
        assert_eq!(
            f.handle.status().unwrap().schedule.unwrap().completion,
            Some(Completion::AdmissionTimeout)
        );
    }
}

#[test]
fn runtime_expired_time_offer_does_not_create_an_independent_automatic_wake() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.route = false);
    let (reply, answer) = mpsc::sync_channel(1);
    f.handle
        .requests
        .try_send(Command::Offer(Offer {
            parent: HoldToken::parse("c".repeat(64)).unwrap(),
            until: clock(1000),
            cancelled: Arc::new(AtomicBool::new(false)),
            reply,
        }))
        .unwrap();
    f.step();
    assert!(!answer.recv().unwrap());
    assert_eq!(f.fake.0.lock().unwrap().prepares, 0);
}

#[test]
fn runtime_waits_for_accepted_time_and_releases_after_cancellation() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.time = false);
    f.step();
    f.until(|f| f.fake.0.lock().unwrap().running);
    assert_eq!(f.handle.status().unwrap().phase, Phase::WaitingForTime);
    assert_eq!(f.fake.0.lock().unwrap().sessions, 0);
    f.fake.edit(|w| w.time = true);
    f.until(|f| f.runner.job.is_none());
    assert_eq!(f.fake.0.lock().unwrap().sessions, 1);
}

#[test]
fn runtime_channel_change_and_network_epoch_loss_cancel_before_releasing() {
    for change in ["channel", "mode", "coordinator", "renew"] {
        let mut f = Fixture::new();
        f.fake.edit(|w| w.work = Work::Wait);
        f.step();
        f.until(|f| f.fake.0.lock().unwrap().sessions == 1);
        match change {
            "channel" => {
                f.handle.select_channel(Channel::Nightly).unwrap();
                f.fake.edit(|w| w.route = false);
            }
            "mode" => f.fake.edit(|w| w.mode += 2),
            "coordinator" => f.fake.edit(|w| w.epoch += 1),
            _ => f.fake.edit(|w| {
                w.ms += 30_000;
                w.fail_renew = true;
            }),
        }
        f.until(|f| f.runner.job.is_none());
        assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1, "{change}");
        assert_eq!(f.fake.0.lock().unwrap().sessions, 1, "{change}");
    }
}

#[test]
fn runtime_renewals_never_extend_the_thirty_minute_total() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.work = Work::Wait);
    f.step();
    f.until(|f| f.fake.0.lock().unwrap().sessions == 1);
    // Make sure payload receipt is consumed before jumping past preparation.
    f.until(|f| {
        f.runner
            .job
            .as_ref()
            .unwrap()
            .control
            .live
            .lock()
            .unwrap()
            .payload_at
            .is_some()
    });
    f.step();
    let end = f.runner.job.as_ref().unwrap().active.deadline_ms;
    for ms in (61_000..end).step_by(60_000) {
        f.fake.edit(|w| w.ms = ms);
        f.step();
    }
    assert!(f.fake.0.lock().unwrap().renewals > 1);
    assert_eq!(f.runner.job.as_ref().unwrap().active.deadline_ms, end);
    f.fake.edit(|w| w.ms = end);
    f.until(|f| f.runner.job.is_none());
    assert_eq!(
        f.handle.status().unwrap().schedule.unwrap().completion,
        Some(Completion::SessionTimeout)
    );
}

#[test]
fn runtime_preparation_admission_and_network_failures_wait_for_next_daily_opportunity() {
    for failure in ["prepare", "admit", "http"] {
        let mut f = Fixture::new();
        f.fake.edit(|w| match failure {
            "prepare" => w.fail_prepare = true,
            "admit" => w.fail_admit = true,
            _ => w.work = Work::Fail,
        });
        f.step();
        f.until(|f| f.runner.job.is_none());
        for ms in [5000, 900_000, 3_600_000] {
            f.fake.edit(|w| w.ms = ms);
            f.step();
        }
        assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
        assert_eq!(f.fake.0.lock().unwrap().holds.len(), 1);
        assert!(f.handle.status().unwrap().reason.is_some());
    }
}

#[test]
fn queued_channel_round_trip_invalidates_old_manual_request_and_keeps_pending_accounting() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.route = false);
    assert!(f.handle.select_channel(Channel::Release).unwrap().coalesced);
    f.handle.check_now().unwrap();
    f.handle.select_channel(Channel::Nightly).unwrap();
    assert!(f.handle.check_now().is_err());
    f.handle.select_channel(Channel::Release).unwrap();
    f.step();
    let status = f.handle.status().unwrap();
    assert!(!status.manual_pending);
    assert_eq!(status.schedule.unwrap().selection_generation, 2);
    assert_eq!(f.fake.0.lock().unwrap().prepares, 0);
    let first = f.handle.check_now().unwrap();
    f.fake.edit(|w| w.ms += 10);
    let joined = f.handle.check_now().unwrap();
    assert!(joined.coalesced);
    assert_eq!(joined.received_ms, first.received_ms);
    f.step();
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
}

#[test]
fn rejected_channel_write_does_not_leave_a_phantom_queued_selection() {
    let mut f = Fixture::new();
    f.fake.edit(|w| w.route = false);
    let state = f._temp.path().join("updates/state.json");
    let before = std::fs::read(&state).unwrap();
    f.handle.select_channel(Channel::Nightly).unwrap();
    std::fs::write(&state, b"{}").unwrap();
    assert!(f.runner.step(&f.requests, &f.handle).is_err());
    assert_eq!(f.handle.shared.lock().unwrap().selections_pending, 0);
    std::fs::write(&state, before).unwrap();
    f.handle.check_now().unwrap();
    f.step();
    assert_eq!(f.fake.0.lock().unwrap().prepares, 1);
}

#[test]
fn actual_worker_shutdown_stops_io_and_restart_does_not_replay_it() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    fake.edit(|w| w.work = Work::Wait);
    let runtime = spawn_with(temp.path().join("updates"), fake.clone()).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while fake.0.lock().unwrap().sessions == 0 {
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    }
    let handle = runtime.handle();
    assert!(handle.status().unwrap().schedule.unwrap().active.is_some());
    drop(runtime);
    assert_eq!(fake.0.lock().unwrap().holds.len(), 1);
    let runtime = spawn_with(temp.path().join("updates"), fake.clone()).unwrap();
    let handle = runtime.handle();
    while handle.status().unwrap().schedule.is_none() {
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(handle.status().unwrap().schedule.unwrap().active.is_none());
    assert_eq!(fake.0.lock().unwrap().sessions, 1);
}
