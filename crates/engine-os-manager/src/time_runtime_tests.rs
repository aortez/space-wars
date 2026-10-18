use super::*;
use crate::network_controller;
use crate::network_observation::Observation;
use crate::network_policy::{self, Activity, HoldKind, Intent, Policy};
use crate::time::{
    ClockReading, ClockReceipt, ConfirmedSync, ProviderIdentity, TimeReason, TimeRecord,
    TimeTracker,
};
use std::collections::BTreeMap;

const BOOT: &str = "11111111-2222-3333-4444-555555555555";
const UTC: u64 = 1_800_000_000_000_000;

fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
fn provider(n: u32) -> ProviderIdentity {
    ProviderIdentity {
        bus_owner: format!(":1.{n}"),
        invocation_id: format!("{n:032x}"),
        pid: n,
        uid: 991,
    }
}
fn proof(ms: u64, generation: u64) -> Evidence {
    let now = ClockReading {
        realtime_us: UTC + ms * 1000,
        monotonic_us: ms * 1000,
        boottime_us: ms * 1000,
    };
    let record = TimeRecord {
        success_generation: generation,
        last_sync: Some(ConfirmedSync {
            boot_id: BOOT.into(),
            provider: provider(2),
            receipt: ClockReceipt {
                packet_count: generation,
                realtime_us: now.realtime_us,
                monotonic_us: now.monotonic_us,
                boottime_us: now.boottime_us,
            },
        }),
        ..TimeRecord::default()
    };
    let tracker = TimeTracker::new(
        BOOT.into(),
        record,
        TimeReason::WaitingForNetworkSync,
        now,
        None,
    );
    Evidence::accepted(&tracker, now).unwrap()
}

#[derive(Clone, Copy)]
enum Query {
    Wait,
    Success,
    Unavailable,
    CycleMode,
    Stall,
}
struct World {
    ms: u64,
    epoch: u64,
    mode: u64,
    enabled: bool,
    network_visible: bool,
    connection: Option<Connection>,
    hardware: bool,
    fail_admit: bool,
    admit_delay: u64,
    query: Query,
    query_entered: bool,
    evidence: Option<Evidence>,
    grants: BTreeMap<String, u64>,
    admissions: Vec<(u64, u64)>,
    releases: usize,
    queries: usize,
    fallbacks: usize,
    offers: Vec<u64>,
}

#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);
impl Fake {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(World {
            ms: 1000,
            epoch: 1,
            mode: 1,
            enabled: true,
            network_visible: true,
            connection: Some(Connection::Ethernet),
            hardware: true,
            fail_admit: false,
            admit_delay: 0,
            query: Query::Wait,
            query_entered: false,
            evidence: None,
            grants: BTreeMap::from([("unrelated-deployment".into(), u64::MAX)]),
            admissions: Vec::new(),
            releases: 0,
            queries: 0,
            fallbacks: 0,
            offers: Vec::new(),
        })))
    }
    fn edit(&self, change: impl FnOnce(&mut World)) {
        change(&mut self.0.lock().unwrap());
    }
    fn reader(&self) -> ReadClock {
        let fake = self.clone();
        Arc::new(move || fake.clock())
    }
}
impl Backend for Fake {
    fn clock(&self) -> Result<Clock> {
        Ok(clock(self.0.lock().unwrap().ms))
    }
    fn network(&self, _now: &Clock) -> Option<Snapshot> {
        let w = self.0.lock().unwrap();
        if !w.network_visible {
            return None;
        }
        Some(Snapshot {
            epoch: w.epoch,
            status: network_controller::Status {
                observed: clock(w.ms),
                policy: network_policy::Status {
                    generation: 10,
                    mode_generation: w.mode,
                    policy: Policy {
                        partial_offline: w.enabled,
                        stay_connected: false,
                    },
                    intent: Intent::Available,
                    holds: vec![network_policy::HoldStatus {
                        kind: HoldKind::Deployment,
                        expires_in_ms: u64::MAX,
                        reconnect_grace: false,
                        wireless_required: false,
                    }],
                },
                actual: Some(Observation {
                    schema: 1,
                    bus_owner: ":1.7".into(),
                    pid: 7,
                    networking_enabled: true,
                    wireless_enabled: true,
                    wireless_hardware_enabled: w.hardware,
                    startup: false,
                    checkpoint_count: 0,
                    state: if w.connection.is_some() { 70 } else { 40 },
                    profile_revision: engine_ota::Sha256::digest(b"fixture"),
                    profile_count: 1,
                    ethernet_available: w.connection == Some(Connection::Ethernet),
                    wireless_available: w.connection == Some(Connection::Wireless),
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
    fn evidence(&self, _now: &Clock) -> Option<Evidence> {
        self.0.lock().unwrap().evidence.clone()
    }
    fn admit(&mut self, token: &HoldToken, active: &Active, _mode: u64) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        w.admissions.push((active.id, active.deadline_ms));
        w.ms += w.admit_delay;
        if w.fail_admit {
            return Err(unavailable());
        }
        w.grants.insert(token.expose().into(), active.deadline_ms);
        Ok(())
    }
    fn fallback(&mut self, _token: &HoldToken, active: &Active, _mode: u64) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        assert!(w.ms < active.deadline_ms);
        w.fallbacks += 1;
        if !w.hardware {
            return Err(unavailable());
        }
        Ok(())
    }
    fn release(&mut self, token: &HoldToken) -> Result<()> {
        let mut w = self.0.lock().unwrap();
        w.releases += 1;
        w.grants.remove(token.expose());
        Ok(())
    }
    fn offer_download(&mut self, token: &HoldToken, active: &Active) {
        let mut w = self.0.lock().unwrap();
        assert!(w.grants.contains_key(token.expose()));
        assert!(w.ms < active.deadline_ms);
        w.offers.push(active.id);
    }
    fn query(
        &mut self,
        schedule: &mut Schedule,
        context: &Context,
        stop: Arc<AtomicBool>,
    ) -> Outcome {
        let now = self.clock().unwrap();
        assert!(self.0.lock().unwrap().offers.contains(&context.active.id));
        assert!(context.valid(&now, &self.network(&now).unwrap()));
        assert!(
            schedule
                .claim_query(&now, context.active.id, provider(1))
                .unwrap()
        );
        let behavior = {
            let mut w = self.0.lock().unwrap();
            w.query_entered = true;
            w.queries += 1;
            w.query
        };
        match behavior {
            Query::Wait => Outcome::Requested,
            Query::Success => {
                self.edit(|w| {
                    w.ms += 1;
                    w.evidence = Some(proof(w.ms, 1));
                });
                Outcome::Requested
            }
            Query::Unavailable => Outcome::Unavailable,
            Query::CycleMode => {
                self.edit(|w| w.mode += 2);
                Outcome::Unavailable
            }
            Query::Stall => {
                let end = Instant::now() + Duration::from_secs(5);
                while !stop.load(Ordering::Relaxed) && Instant::now() < end {
                    thread::sleep(Duration::from_millis(5));
                }
                Outcome::Uncertain
            }
        }
    }
}

fn fixture() -> (tempfile::TempDir, Runner<Fake>, Handle, Fake) {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let handle = Handle::new(fake.reader());
    let runner = Runner::open(
        &temp.path().join("schedule"),
        fake.clone(),
        &fake.clock().unwrap(),
    )
    .unwrap();
    (temp, runner, handle, fake)
}

#[test]
fn runtime_boot_query_success_releases_only_its_hold_and_waits_until_daily_due() {
    let (_temp, mut runner, handle, fake) = fixture();
    fake.edit(|w| w.query = Query::Success);
    runner.step(None, &handle).unwrap();
    let status = handle.status().unwrap();
    assert_eq!(status.phase, Phase::Idle);
    let saved = status.schedule.unwrap();
    assert_eq!(saved.completion, Some(Completion::Synchronized));
    assert_eq!(saved.next_ms, Some(1001 + 86_400_000));
    for ms in [1002, 2000, 1_000_000] {
        fake.edit(|w| w.ms = ms);
        runner.step(None, &handle).unwrap();
    }
    let w = fake.0.lock().unwrap();
    assert_eq!(w.queries, 1);
    assert_eq!(w.admissions, [(1, 121_000)]);
    assert_eq!(w.releases, 1);
    assert_eq!(
        w.grants.keys().collect::<Vec<_>>(),
        [&"unrelated-deployment".to_string()]
    );
}

#[test]
fn recent_success_and_read_only_status_need_no_hold_or_provider_request() {
    let (_temp, mut runner, handle, fake) = fixture();
    fake.edit(|w| w.evidence = Some(proof(900, 1)));
    runner.step(None, &handle).unwrap();
    for _ in 0..10 {
        handle.status().unwrap();
        runner.step(None, &handle).unwrap();
    }
    assert_eq!(
        handle.status().unwrap().schedule.unwrap().completion,
        Some(Completion::RecentSuccess)
    );
    let w = fake.0.lock().unwrap();
    assert!(w.admissions.is_empty());
    assert_eq!(w.queries, 0);
    assert_eq!(w.releases, 0);
}

#[test]
fn manual_queue_coalesces_and_association_can_finish_after_admission_deadline() {
    let (_temp, mut runner, handle, fake) = fixture();
    fake.edit(|w| {
        w.enabled = false;
        w.mode = 0;
        w.connection = None;
    });
    runner.step(None, &handle).unwrap();
    let first = handle.sync_now().unwrap();
    fake.edit(|w| w.ms = 10_000);
    assert!(handle.sync_now().unwrap().coalesced);
    assert_eq!(handle.sync_now().unwrap().received_ms, first.received_ms);
    runner
        .step(handle.take_request().as_ref(), &handle)
        .unwrap();
    assert_eq!(handle.status().unwrap().phase, Phase::Connecting);
    assert!(
        handle
            .status()
            .unwrap()
            .schedule
            .unwrap()
            .active
            .unwrap()
            .admitted
    );
    fake.edit(|w| {
        w.ms = 40_000;
        w.connection = Some(Connection::Wireless);
    });
    runner.step(None, &handle).unwrap();
    let active = handle.status().unwrap().schedule.unwrap().active.unwrap();
    assert_eq!(active.deadline_ms, 121_000);
    assert_eq!(active.admission_deadline_ms, 31_000);
    assert!(handle.sync_now().unwrap().coalesced);
    assert_eq!(fake.0.lock().unwrap().queries, 1);
}

#[test]
fn a_late_manual_queue_or_admission_reply_cannot_start_provider_work() {
    for late_reply in [false, true] {
        let (_temp, mut runner, handle, fake) = fixture();
        fake.edit(|w| {
            w.enabled = false;
            w.mode = 0;
        });
        handle.sync_now().unwrap();
        if late_reply {
            fake.edit(|w| w.admit_delay = 30_000);
        } else {
            fake.edit(|w| w.ms = 60_000);
        }
        runner
            .step(handle.take_request().as_ref(), &handle)
            .unwrap();
        let schedule = handle.status().unwrap().schedule.unwrap();
        assert!(schedule.active.is_none());
        assert_eq!(schedule.completion, Some(Completion::AdmissionTimeout));
        let w = fake.0.lock().unwrap();
        assert_eq!(w.queries, 0);
        assert_eq!(w.grants.len(), 1);
    }
}

#[test]
fn timeout_releases_the_hold_and_keeps_ntp_backoff_independent_of_manual_retries() {
    let (_temp, mut runner, handle, fake) = fixture();
    runner.step(None, &handle).unwrap();
    fake.edit(|w| w.ms = 121_000);
    runner.step(None, &handle).unwrap();
    let schedule = handle.status().unwrap().schedule.unwrap();
    assert_eq!(schedule.next_ms, Some(1_021_000));
    assert_eq!(schedule.failure_stage, 1);
    assert_eq!(fake.0.lock().unwrap().grants.len(), 1);
    fake.edit(|w| w.ms = 122_000);
    handle.sync_now().unwrap();
    runner
        .step(handle.take_request().as_ref(), &handle)
        .unwrap();
    let schedule = handle.status().unwrap().schedule.unwrap();
    assert_eq!(schedule.failure_stage, 1);
    assert_eq!(schedule.active.unwrap().trigger, Trigger::Manual);
    assert_eq!(fake.0.lock().unwrap().queries, 2);
}

#[test]
fn ethernet_fallback_uses_one_reservation_and_one_query() {
    let (_temp, mut runner, handle, fake) = fixture();
    runner.step(None, &handle).unwrap();
    fake.edit(|w| w.ms = 16_000);
    runner.step(None, &handle).unwrap();
    runner.step(None, &handle).unwrap();
    let w = fake.0.lock().unwrap();
    assert_eq!(w.queries, 1);
    assert_eq!(w.fallbacks, 1);
    assert_eq!(w.admissions.len(), 1);
    drop(w);
    assert_eq!(
        handle
            .status()
            .unwrap()
            .schedule
            .unwrap()
            .active
            .unwrap()
            .deadline_ms,
        121_000
    );
}

#[test]
fn service_and_coordinator_restarts_preserve_the_attempt_budget_and_query_allowance() {
    let (temp, mut runner, handle, fake) = fixture();
    runner.step(None, &handle).unwrap();
    drop(runner);
    fake.edit(|w| {
        w.ms = 20_000;
        w.epoch += 1;
    });
    let mut runner =
        Runner::open(&temp.path().join("schedule"), fake.clone(), &clock(20_000)).unwrap();
    runner.step(None, &handle).unwrap();
    let schedule = handle.status().unwrap().schedule.unwrap();
    let active = schedule.active.unwrap();
    assert_eq!(active.deadline_ms, 121_000);
    assert!(active.query_requested);
    fake.edit(|w| {
        w.ms = 30_000;
        w.epoch += 1;
        w.grants.retain(|k, _| k == "unrelated-deployment");
    });
    runner.step(None, &handle).unwrap();
    let w = fake.0.lock().unwrap();
    assert_eq!(w.queries, 1);
    assert_eq!(w.admissions, [(1, 121_000), (1, 121_000), (1, 121_000)]);
}

#[test]
fn mode_off_cancels_automatic_work_and_does_not_release_another_operations_hold() {
    let (_temp, mut runner, handle, fake) = fixture();
    runner.step(None, &handle).unwrap();
    fake.edit(|w| {
        w.ms = 2000;
        w.enabled = false;
        w.mode += 1;
    });
    runner.step(None, &handle).unwrap();
    let schedule = handle.status().unwrap().schedule.unwrap();
    assert_eq!(schedule.completion, Some(Completion::Cancelled));
    assert!(schedule.active.is_none());
    assert_eq!(fake.0.lock().unwrap().grants.len(), 1);
    handle.sync_now().unwrap();
    runner
        .step(handle.take_request().as_ref(), &handle)
        .unwrap();
    assert_eq!(
        handle
            .status()
            .unwrap()
            .schedule
            .unwrap()
            .active
            .unwrap()
            .trigger,
        Trigger::Manual
    );
}

#[test]
fn off_on_race_during_provider_work_invalidates_old_attempt_even_if_enabled_again() {
    let (_temp, mut runner, handle, fake) = fixture();
    fake.edit(|w| w.query = Query::CycleMode);
    runner.step(None, &handle).unwrap();
    let active = handle.status().unwrap().schedule.unwrap().active.unwrap();
    assert_eq!(active.id, 2);
    assert!(!active.query_requested);
    assert_eq!(fake.0.lock().unwrap().releases, 1);
    assert_eq!(fake.0.lock().unwrap().grants.len(), 1);
}

#[test]
fn missing_network_and_failed_provider_terminate_with_visible_durable_reasons() {
    for no_network in [false, true] {
        let (_temp, mut runner, handle, fake) = fixture();
        fake.edit(|w| {
            w.query = Query::Unavailable;
            w.fail_admit = no_network;
        });
        runner.step(None, &handle).unwrap();
        if no_network {
            assert_eq!(
                handle.status().unwrap().reason,
                Some(Reason::NetworkUnavailable)
            );
            fake.edit(|w| w.ms = 121_000);
            runner.step(None, &handle).unwrap();
        }
        let schedule = handle.status().unwrap().schedule.unwrap();
        assert!(schedule.active.is_none());
        assert_eq!(
            schedule.completion,
            Some(if no_network {
                Completion::AttemptTimeout
            } else {
                Completion::ProviderUnavailable
            })
        );
        assert_eq!(fake.0.lock().unwrap().grants.len(), 1);
    }
}

#[test]
fn cached_network_view_expires_on_sleep_and_coordinator_restart() {
    let fake = Fake::new();
    let watch = Watch::default();
    watch.reset();
    watch.publish(Some(fake.network(&clock(1000)).unwrap().status));
    let first = watch.read(&clock(1000)).unwrap();
    assert!(watch.read(&clock(6001)).is_none());
    assert!(watch.read(&clock(999)).is_none());
    watch.reset();
    assert!(watch.read(&clock(1000)).is_none());
    watch.publish(Some(fake.network(&clock(1000)).unwrap().status));
    assert_ne!(watch.read(&clock(1000)).unwrap().epoch, first.epoch);
}

#[test]
fn independent_runtime_keeps_status_responsive_and_cancels_query_before_releasing_on_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    fake.edit(|w| w.query = Query::Stall);
    let runtime = spawn_with(temp.path().join("schedule"), fake.clone(), fake.reader());
    let handle = runtime.handle();
    let end = Instant::now() + Duration::from_secs(3);
    while !fake.0.lock().unwrap().query_entered {
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(10));
    }
    let started = Instant::now();
    for _ in 0..10 {
        assert_eq!(handle.status().unwrap().phase, Phase::Querying);
        assert!(handle.sync_now().unwrap().coalesced);
    }
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(runtime);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(fake.0.lock().unwrap().grants.len(), 1);
    assert_eq!(handle.status().unwrap().phase, Phase::Unavailable);
    assert!(handle.sync_now().is_err());
}
