use super::*;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const BOOT: &str = "11111111-2222-3333-4444-555555555555";
const NEXT_BOOT: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";

#[derive(Clone, Copy)]
enum Behavior {
    Apply,
    Reject,
    NotSubmitted,
    Timeout,
    NoChange,
    LoseReadback,
    /// Returns before readback changes, with the monitor's transition live.
    Delay,
}

struct World {
    clock: Clock,
    radio: Observation,
    activity: Activity,
    change: Option<Suspension>,
    reject_reconcile: bool,
    guard: bool,
    guard_active: bool,
    busy_on_guard: bool,
    unknown: bool,
    fail_snapshots: usize,
    behavior: Behavior,
    calls: Vec<bool>,
    barrier: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    elapsed_on_write: u64,
    expected: Option<bool>,
    on_pause: Option<PauseEvent>,
    pauses: usize,
    reconciles: usize,
}

type PauseEvent = Box<dyn FnMut(&mut World, usize) + Send>;

#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);
struct Guard(Arc<Mutex<World>>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.lock().unwrap().guard_active = false;
    }
}

impl Fake {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(World {
            clock: Clock {
                boot_id: BOOT.into(),
                ms: 0,
            },
            radio: Observation {
                schema: 1,
                bus_owner: ":1.7".into(),
                pid: 47,
                networking_enabled: true,
                wireless_enabled: true,
                wireless_hardware_enabled: true,
                startup: false,
                checkpoint_count: 0,
                state: 70,
                profile_revision: engine_ota::Sha256::digest(b"profiles"),
                profile_count: 2,
                ethernet_available: false,
                wireless_available: true,
            },
            activity: Activity {
                ssh_sessions: Some(false),
                checkpoints: Some(false),
                install_lock: Some(false),
                operations: Some(false),
            },
            change: None,
            reject_reconcile: false,
            guard: true,
            guard_active: false,
            busy_on_guard: false,
            unknown: false,
            fail_snapshots: 0,
            behavior: Behavior::Apply,
            calls: Vec::new(),
            barrier: None,
            elapsed_on_write: 0,
            expected: None,
            on_pause: None,
            pauses: 0,
            reconciles: 0,
        })))
    }
    fn edit(&self, f: impl FnOnce(&mut World)) {
        f(&mut self.0.lock().unwrap());
    }
    fn calls(&self) -> Vec<bool> {
        self.0.lock().unwrap().calls.clone()
    }
    /// Runs after each readback pause, with that pause's one-based count.
    fn on_pause(&self, event: impl FnMut(&mut World, usize) + Send + 'static) {
        self.edit(|w| w.on_pause = Some(Box::new(event)));
    }
    fn pauses(&self) -> usize {
        self.0.lock().unwrap().pauses
    }
    fn reconciles(&self) -> usize {
        self.0.lock().unwrap().reconciles
    }
}

impl Backend for Fake {
    type ShutdownGuard = Guard;
    fn clock(&mut self) -> Result<Clock> {
        Ok(self.0.lock().unwrap().clock.clone())
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        let mut world = self.0.lock().unwrap();
        if world.unknown || world.fail_snapshots != 0 {
            world.fail_snapshots = world.fail_snapshots.saturating_sub(1);
            return Err(invalid("injected observation failure"));
        }
        Ok(Snapshot {
            clock: world.clock.clone(),
            radio: world.radio.clone(),
            activity: world.activity,
            change: world.change,
            expected: world.expected,
        })
    }
    fn shutdown_guard(&mut self) -> Result<Option<Guard>> {
        let mut world = self.0.lock().unwrap();
        if !world.guard {
            return Ok(None);
        }
        if world.busy_on_guard {
            world.activity.ssh_sessions = Some(true);
        }
        assert!(!world.guard_active);
        world.guard_active = true;
        Ok(Some(Guard(self.0.clone())))
    }
    fn reconcile(&mut self) -> Result<()> {
        let mut world = self.0.lock().unwrap();
        world.reconciles += 1;
        if world.reject_reconcile {
            return Err(invalid("monitor unavailable"));
        }
        world.change = None;
        world.expected = None;
        Ok(())
    }
    fn pause(&mut self, duration: Duration) {
        let mut world = self.0.lock().unwrap();
        world.pauses += 1;
        world.clock.ms += duration.as_millis() as u64;
        let pauses = world.pauses;
        if let Some(mut event) = world.on_pause.take() {
            event(&mut world, pauses);
            world.on_pause.get_or_insert(event);
        }
    }
    fn set_wireless(&mut self, expected: &Observation, enabled: bool) -> Result<WriteResult> {
        let barrier = {
            let mut world = self.0.lock().unwrap();
            assert_eq!(*expected, world.radio);
            assert!(enabled || world.guard_active, "off must retain protection");
            world.calls.push(enabled);
            world.barrier.take()
        };
        if let Some((entered, resume)) = barrier {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let mut world = self.0.lock().unwrap();
        world.clock.ms += world.elapsed_on_write;
        match world.behavior {
            Behavior::Apply => world.radio.wireless_enabled = enabled,
            Behavior::Reject => return Ok(WriteResult::Rejected),
            Behavior::NotSubmitted => return Ok(WriteResult::NotSubmitted),
            Behavior::Timeout => return Err(invalid("injected ambiguous write")),
            Behavior::NoChange => {}
            Behavior::LoseReadback => {
                world.radio.wireless_enabled = enabled;
                world.fail_snapshots = 1;
            }
            Behavior::Delay => world.expected = Some(enabled),
        }
        Ok(WriteResult::Returned)
    }
}

fn caller() -> Caller {
    Caller {
        uid: 1000,
        pid: 73,
        process_start_ticks: 17,
    }
}
fn token() -> HoldToken {
    HoldToken::parse("a".repeat(64)).unwrap()
}
fn enabled() -> Policy {
    Policy {
        partial_offline: true,
        stay_connected: false,
    }
}
fn enable(controller: &mut Controller<Fake>) {
    let generation = controller.status().unwrap().policy.generation;
    controller.set_policy(generation, enabled()).unwrap();
}
fn ready(controller: &mut Controller<Fake>, fake: &Fake) {
    enable(controller);
    for ms in (0..110_000).step_by(1000) {
        fake.edit(|w| w.clock.ms = ms);
        assert!(controller.tick().unwrap().pending.is_none());
    }
    assert!(fake.calls().is_empty());
    fake.edit(|w| w.clock.ms = 110_000);
}

fn ssh_caller() -> Caller {
    Caller { uid: 0, ..caller() }
}

#[test]
fn transient_external_changes_persist_until_an_explicit_successful_monitor_resume() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    let before = fake.0.lock().unwrap().radio.clone();
    fake.edit(|w| w.change = Some(Suspension::ExternalRadioChange));
    let status = controller.tick().unwrap();
    assert_eq!(status.actual.as_ref(), Some(&before));
    assert_eq!(status.suspended, Some(Suspension::ExternalRadioChange));
    assert!(fake.calls().is_empty());
    drop(controller);
    fake.edit(|w| {
        w.change = None;
        w.reject_reconcile = true;
    });
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let status = controller.tick().unwrap();
    assert_eq!(status.suspended, Some(Suspension::ExternalRadioChange));
    assert!(controller.resume(status.policy.generation).is_err());
    assert_eq!(
        controller.status().unwrap().suspended,
        Some(Suspension::ExternalRadioChange)
    );
    fake.edit(|w| w.reject_reconcile = false);
    let generation = controller.status().unwrap().policy.generation;
    assert!(controller.resume(generation).unwrap().suspended.is_none());
    assert!(fake.calls().is_empty());
}

#[test]
fn ssh_admission_is_root_only_and_does_not_claim_or_force_wifi_availability() {
    for unknown in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        fake.edit(|w| {
            w.radio.wireless_enabled = false;
            w.radio.ethernet_available = true;
        });
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        let before = controller.status().unwrap().policy;
        assert!(controller.admit(&token(), caller(), HoldKind::Ssh).is_err());
        assert_eq!(controller.status().unwrap().policy, before);
        fake.edit(|w| w.unknown = unknown);
        let admitted = controller
            .admit(&token(), ssh_caller(), HoldKind::Ssh)
            .unwrap();
        assert_eq!(admitted.availability, Availability::SessionProtected);
        assert!(fake.calls().is_empty());
        assert_eq!(admitted.status.actual.is_none(), unknown);
        assert!(controller.renew(&token(), caller()).is_err());
        assert!(controller.release(&token(), caller()).is_err());
    }
}

#[test]
fn ssh_admission_restores_owned_off_and_never_accepts_a_late_or_failed_off_command() {
    for behavior in [
        Behavior::Apply,
        Behavior::Reject,
        Behavior::Timeout,
        Behavior::LoseReadback,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        assert!(controller.tick().unwrap().owned_off);
        fake.edit(|w| w.behavior = behavior);
        let admitted = controller.admit(&token(), ssh_caller(), HoldKind::Ssh);
        assert_eq!(admitted.is_ok(), matches!(behavior, Behavior::Apply));
        assert_eq!(fake.calls(), [false, true]);
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    fake.edit(|w| w.behavior = Behavior::Timeout);
    assert!(controller.tick().unwrap().pending.is_some());
    drop(controller);
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    assert!(
        controller
            .admit(&token(), ssh_caller(), HoldKind::Ssh)
            .is_err()
    );
    fake.edit(|w| w.radio.wireless_enabled = false);
    assert!(
        controller
            .admit(&token(), ssh_caller(), HoldKind::Ssh)
            .is_err()
    );
    assert_eq!(fake.calls(), [false]);
    drop(controller);
    assert!(network_policy::protect_ssh_without_service(&path, BOOT).is_err());
}

#[test]
fn ssh_recovery_after_reboot_preserves_the_retained_enable_fault_until_owner_resume() {
    // Pi 5 cleanup completed an enable request but retained its old off witness
    // and external-change suspension. Restarting the service is not a reboot.
    for reboot in [false, true] {
        for unknown in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("network");
            let mut fake = Fake::new();
            fake.edit(|w| w.radio.wireless_enabled = false);
            let mut controller = Controller::open(&path, fake.clone()).unwrap();
            let before = Witness::new(&fake.clock().unwrap(), &fake.0.lock().unwrap().radio);
            let retained = RadioState {
                witness: Some(before.clone()),
                owned_off: true,
                pending: Some(Pending {
                    before,
                    enabled: true,
                    phase: Phase::Returned,
                }),
                suspended: Some(Suspension::ExternalRadioChange),
            };
            controller
                .policy
                .save_radio(retained.clone(), &fake.clock().unwrap())
                .unwrap();
            drop(controller);
            fake.edit(|w| {
                if reboot {
                    w.clock.boot_id = NEXT_BOOT.into();
                }
                w.radio.wireless_enabled = true;
                w.activity.ssh_sessions = Some(true);
                w.unknown = unknown;
            });
            let mut controller = Controller::open(&path, fake.clone()).unwrap();
            let result = controller.admit(&token(), ssh_caller(), HoldKind::Ssh);
            assert_eq!(result.is_ok(), reboot, "reboot={reboot}, unknown={unknown}");
            assert_eq!(
                controller.policy.radio(&fake.clock().unwrap()).unwrap(),
                retained
            );
            assert!(fake.calls().is_empty());
            if reboot {
                let admitted = result.unwrap();
                assert_eq!(admitted.availability, Availability::SessionProtected);
                assert!(admitted.status.owned_off && admitted.status.pending.is_some());
                assert_eq!(
                    admitted.status.suspended,
                    Some(Suspension::ExternalRadioChange)
                );
                assert_eq!(admitted.status.policy.holds[0].kind, HoldKind::Ssh);
                fake.edit(|w| w.unknown = false);
                let generation = controller.status().unwrap().policy.generation;
                let resumed = controller.resume(generation).unwrap();
                assert!(!resumed.owned_off);
                assert!(resumed.pending.is_none() && resumed.suspended.is_none());
                assert_eq!(resumed.policy.policy, Policy::default());
                assert!(fake.calls().is_empty());
            }
        }
    }
}

#[test]
fn ssh_recovery_after_reboot_does_not_clear_a_timed_out_off_command() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let mut fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    fake.edit(|w| w.behavior = Behavior::Timeout);
    assert!(controller.tick().unwrap().pending.is_some());
    drop(controller);
    fake.edit(|w| {
        w.clock = Clock {
            boot_id: NEXT_BOOT.into(),
            ms: 0,
        };
        w.behavior = Behavior::Apply;
        w.radio.wireless_enabled = false;
        w.radio.wireless_available = false;
        w.radio.ethernet_available = true;
        w.activity.ssh_sessions = Some(true);
    });
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let retained = controller.policy.radio(&fake.clock().unwrap()).unwrap();
    let admitted = controller
        .admit(&token(), ssh_caller(), HoldKind::Ssh)
        .unwrap();
    assert_eq!(admitted.availability, Availability::SessionProtected);
    assert_eq!(
        controller.policy.radio(&fake.clock().unwrap()).unwrap(),
        retained
    );
    assert_eq!(fake.calls(), [false]);
    // Only explicit owner reconciliation may retire the uncertain command.
    let resumed = controller
        .resume(admitted.status.policy.generation)
        .unwrap();
    assert!(resumed.pending.is_none() && resumed.suspended.is_none());
    assert!(resumed.actual.unwrap().wireless_enabled);
    assert_eq!(fake.calls(), [false, true]);
}

#[test]
fn expired_ssh_admission_is_kept_until_live_sessions_end_and_quiet_is_observed() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller
        .admit(&token(), ssh_caller(), HoldKind::Ssh)
        .unwrap();
    fake.edit(|w| {
        w.clock.ms += 120_001;
        w.activity.ssh_sessions = Some(true);
    });
    let busy = controller.tick().unwrap();
    assert_eq!(busy.policy.holds.len(), 1);
    assert_eq!(busy.policy.holds[0].expires_in_ms, 0);
    fake.edit(|w| w.activity.ssh_sessions = Some(false));
    for _ in 0..10 {
        fake.edit(|w| w.clock.ms += 1000);
        assert_eq!(controller.tick().unwrap().policy.holds.len(), 1);
    }
    assert!(fake.calls().is_empty());
    fake.edit(|w| w.clock.ms += 1000);
    let idle = controller.tick().unwrap();
    assert!(idle.policy.holds.is_empty());
    assert!(idle.owned_off);
    assert_eq!(fake.calls(), [false]);
}

#[test]
fn off_is_inert_and_only_policy_owned_shutdown_is_restored() {
    for initially_enabled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        fake.edit(|w| w.radio.wireless_enabled = initially_enabled);
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        let generation = controller.status().unwrap().policy.generation;
        controller
            .set_policy(
                generation,
                Policy {
                    partial_offline: false,
                    stay_connected: true,
                },
            )
            .unwrap();
        let expected = if initially_enabled {
            Availability::WirelessReady
        } else {
            Availability::SetupProtected
        };
        let result = controller
            .admit(&token(), caller(), HoldKind::Setup)
            .unwrap();
        assert_eq!(result.availability, expected);
        assert_eq!(
            result.status.actual.unwrap().wireless_enabled,
            initially_enabled
        );
        assert_eq!(
            controller.renew(&token(), caller()).unwrap().availability,
            expected
        );
        controller.release(&token(), caller()).unwrap();
        assert!(fake.calls().is_empty());
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    let status = controller.tick().unwrap();
    assert!(status.owned_off);
    assert_eq!(fake.calls(), [false]);
    drop(controller);
    assert!(network_policy::protect_ssh_without_service(&path, BOOT).is_err());
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let generation = controller.status().unwrap().policy.generation;
    let status = controller
        .set_policy(generation, Policy::default())
        .unwrap();
    assert!(!status.owned_off);
    assert!(status.actual.unwrap().wireless_enabled);
    assert_eq!(fake.calls(), [false, true]);
}

#[test]
fn stay_connected_and_overlapping_holds_protect_availability() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    assert_eq!(
        controller
            .admit(&token(), caller(), HoldKind::Setup)
            .unwrap()
            .availability,
        Availability::WirelessReady
    );
    let other = HoldToken::generate().unwrap();
    controller
        .admit(&other, caller(), HoldKind::Download)
        .unwrap();
    controller.release(&token(), caller()).unwrap();
    for ms in (111_000..=121_000).step_by(1000) {
        fake.edit(|w| w.clock.ms = ms);
        controller.tick().unwrap();
    }
    assert_eq!(fake.calls(), [false, true]);
    let generation = controller.status().unwrap().policy.generation;
    controller
        .set_policy(
            generation,
            Policy {
                stay_connected: true,
                ..enabled()
            },
        )
        .unwrap();
    controller.release(&other, caller()).unwrap();
    for ms in (122_000..=132_000).step_by(1000) {
        fake.edit(|w| w.clock.ms = ms);
        controller.tick().unwrap();
    }
    assert_eq!(fake.calls(), [false, true]);
    assert!(controller.status().unwrap().policy.holds.is_empty());
}

#[test]
fn time_and_download_can_use_ethernet_but_setup_requires_wireless() {
    for kind in [HoldKind::TimeCheck, HoldKind::Download] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        controller.tick().unwrap();
        fake.edit(|w| w.radio.ethernet_available = true);
        let admission = controller.admit(&token(), caller(), kind).unwrap();
        assert_eq!(admission.availability, Availability::EthernetReady);
        assert_eq!(fake.calls(), [false]);
        let other = HoldToken::generate().unwrap();
        controller.admit(&other, caller(), HoldKind::Setup).unwrap();
        assert_eq!(fake.calls(), [false, true]);
    }
}

#[test]
fn time_check_can_fall_back_from_ethernet_without_a_second_hold_or_longer_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    fake.edit(|w| w.radio.ethernet_available = true);
    assert_eq!(
        controller
            .admit(&token(), caller(), HoldKind::TimeCheck)
            .unwrap()
            .availability,
        Availability::EthernetReady
    );
    fake.edit(|w| w.clock.ms += 15_000);
    let reply = controller.require_wireless(&token(), caller()).unwrap();
    assert_eq!(reply.availability, Availability::WirelessReady);
    assert_eq!(reply.status.policy.holds.len(), 1);
    assert_eq!(reply.status.policy.holds[0].expires_in_ms, 105_000);
    assert_eq!(fake.calls(), [false, true]);
    controller.require_wireless(&token(), caller()).unwrap();
    assert_eq!(fake.calls(), [false, true]);
    fake.edit(|w| w.clock.ms = 230_000);
    assert!(controller.require_wireless(&token(), caller()).is_err());
}

#[test]
fn download_handoff_keeps_connectivity_after_time_release_and_ethernet_does_not_wake_wifi() {
    for ethernet in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        controller.tick().unwrap();
        fake.edit(|w| {
            w.radio.ethernet_available = ethernet;
            w.radio.wireless_available = !ethernet;
        });
        let root = Caller { uid: 0, ..caller() };
        let parent = token();
        controller
            .admit(&parent, root, HoldKind::TimeCheck)
            .unwrap();
        let download = HoldToken::parse("b".repeat(64)).unwrap();
        let status = controller.status().unwrap();
        let request = DownloadReservation {
            deadline: Clock {
                boot_id: BOOT.into(),
                ms: 110_000 + 1_800_000,
            },
            admission_deadline_ms: 230_000,
            mode_generation: Some(status.policy.mode_generation),
            time_hold: Some(parent.clone()),
            handoff_deadline_ms: Some(200_000),
        };
        let admitted = controller
            .admit_download(&download, root, &request)
            .unwrap();
        assert_eq!(admitted.status.policy.holds.len(), 2);
        let released = controller.release(&parent, root).unwrap();
        assert_eq!(released.policy.holds.len(), 1);
        assert_eq!(released.policy.holds[0].kind, HoldKind::Download);
        fake.edit(|w| w.clock.ms += 20_000);
        controller.renew(&download, root).unwrap();
        assert_eq!(
            fake.calls(),
            if ethernet {
                vec![false]
            } else {
                vec![false, true]
            }
        );
        // The old parent no longer authorizes another handoff.
        assert!(
            controller
                .admit_download(&HoldToken::parse("c".repeat(64)).unwrap(), root, &request)
                .is_err()
        );
    }
}

#[test]
fn stale_connected_download_opportunity_does_not_wake_an_off_radio() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    fake.edit(|w| w.radio.wireless_available = false);
    let root = Caller { uid: 0, ..caller() };
    let request = DownloadReservation {
        deadline: Clock {
            boot_id: BOOT.into(),
            ms: 110_000 + 1_800_000,
        },
        admission_deadline_ms: 230_000,
        mode_generation: Some(controller.status().unwrap().policy.mode_generation),
        time_hold: None,
        handoff_deadline_ms: None,
    };
    assert!(controller.admit_download(&token(), root, &request).is_err());
    assert_eq!(fake.calls(), [false]);
    assert!(controller.status().unwrap().policy.holds.is_empty());
}

#[test]
fn time_fallback_honors_radio_conflicts_hardware_block_and_mode_off() {
    for case in ["conflict", "hardware", "off"] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        if case == "off" {
            fake.edit(|w| w.radio.wireless_enabled = false);
        } else {
            ready(&mut controller, &fake);
            controller.tick().unwrap();
        }
        fake.edit(|w| w.radio.ethernet_available = true);
        controller
            .admit(&token(), caller(), HoldKind::TimeCheck)
            .unwrap();
        fake.edit(|w| {
            w.clock.ms += 15_000;
            if case == "conflict" {
                w.change = Some(Suspension::ExternalRadioChange);
            }
            if case == "hardware" {
                w.radio.wireless_hardware_enabled = false;
            }
        });
        let before = fake.calls();
        assert!(controller.require_wireless(&token(), caller()).is_err());
        assert_eq!(fake.calls(), before);
    }
}

#[test]
fn delayed_scheduler_admission_cannot_wake_radio_after_admission_or_attempt_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    let mode = controller.status().unwrap().policy.mode_generation;
    let deadline = Clock {
        boot_id: BOOT.into(),
        ms: 230_000,
    };
    fake.edit(|w| w.clock.ms = 140_000);
    assert!(
        controller
            .admit_time(
                &token(),
                ssh_caller(),
                &deadline,
                140_000,
                Some(mode),
                false
            )
            .is_err()
    );
    assert_eq!(fake.calls(), [false]);
    fake.edit(|w| w.clock.ms = 230_000);
    assert!(
        controller
            .admit_time(
                &token(),
                ssh_caller(),
                &deadline,
                230_000,
                Some(mode),
                false
            )
            .is_err()
    );
    assert_eq!(fake.calls(), [false]);
    assert!(controller.status().unwrap().policy.holds.is_empty());
}

#[test]
fn expired_retained_holds_protect_existing_activity_without_waking_an_off_radio() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    fake.edit(|w| w.radio.ethernet_available = true);
    controller
        .admit(&token(), caller(), HoldKind::TimeCheck)
        .unwrap();
    fake.edit(|w| {
        w.clock.ms = 230_000;
        w.radio.ethernet_available = false;
        w.activity.operations = None;
    });
    let status = controller.tick().unwrap();
    assert_eq!(status.policy.holds.len(), 1);
    assert_eq!(status.policy.holds[0].expires_in_ms, 0);
    assert_eq!(fake.calls(), [false]);
}

#[test]
fn incomplete_or_new_activity_and_busy_guards_prevent_shutdown() {
    for case in [
        "unknown",
        "ssh",
        "install",
        "operation",
        "checkpoint",
        "guard",
        "new-ssh",
        "hardware",
        "startup",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| match case {
            "unknown" => w.activity.ssh_sessions = None,
            "ssh" => w.activity.ssh_sessions = Some(true),
            "install" => w.activity.install_lock = Some(true),
            "operation" => w.activity.operations = Some(true),
            "checkpoint" => w.radio.checkpoint_count = 1,
            "guard" => w.guard = false,
            "new-ssh" => w.busy_on_guard = true,
            "hardware" => w.radio.wireless_hardware_enabled = false,
            "startup" => w.radio.startup = true,
            _ => unreachable!(),
        });
        controller.tick().unwrap();
        assert!(fake.calls().is_empty(), "{case}");
        assert!(!fake.0.lock().unwrap().guard_active);
    }
}

#[test]
fn external_changes_suspend_across_service_restart_until_explicit_resume() {
    for case in ["radio", "profiles", "manager", "networking"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let fake = Fake::new();
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| match case {
            "radio" => w.radio.wireless_enabled = false,
            "profiles" => w.radio.profile_revision = engine_ota::Sha256::digest(b"changed"),
            "manager" => w.radio.bus_owner = ":1.8".into(),
            "networking" => w.radio.networking_enabled = false,
            _ => unreachable!(),
        });
        let status = controller.tick().unwrap();
        assert!(status.suspended.is_some());
        assert!(fake.calls().is_empty());
        drop(controller);
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        let status = controller.tick().unwrap();
        assert!(status.suspended.is_some());
        assert!(controller.resume(status.policy.generation - 1).is_err());
        let status = controller.resume(status.policy.generation).unwrap();
        assert!(status.suspended.is_none());
        assert!(fake.calls().is_empty());
    }
}

#[test]
fn timeout_never_acknowledges_a_hold_or_disappears_on_service_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    fake.edit(|w| w.behavior = Behavior::Timeout);
    let status = controller.tick().unwrap();
    assert_eq!(status.deferred, Some(Deferred::CommandUncertain));
    assert!(!status.pending.unwrap().method_returned);
    assert!(
        controller
            .admit(&token(), caller(), HoldKind::Setup)
            .is_err()
    );
    // The timed-out request may still execute after our caller returns.
    fake.edit(|w| w.radio.wireless_enabled = false);
    drop(controller);
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let status = controller.tick().unwrap();
    assert!(status.pending.is_some());
    assert!(controller.resume(status.policy.generation).is_err());
    assert_eq!(fake.calls(), [false]);
    drop(controller);
    fake.edit(|w| {
        w.clock = Clock {
            boot_id: NEXT_BOOT.into(),
            ms: 0,
        };
        w.behavior = Behavior::Apply;
    });
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let generation = controller.status().unwrap().policy.generation;
    controller.resume(generation).unwrap();
    controller
        .admit(&token(), caller(), HoldKind::Setup)
        .unwrap();
    assert_eq!(fake.calls(), [false, true]);
}

#[test]
fn returned_commands_require_matching_readback_and_survive_lost_observation() {
    for behavior in [Behavior::Reject, Behavior::NoChange, Behavior::LoseReadback] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let fake = Fake::new();
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| w.behavior = behavior);
        let status = controller.tick().unwrap();
        assert!(!status.owned_off);
        assert!(status.deferred.is_some());
        drop(controller);
        fake.edit(|w| w.behavior = Behavior::Apply);
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        let status = controller.tick().unwrap();
        if matches!(behavior, Behavior::LoseReadback) {
            assert!(status.owned_off && status.pending.is_none());
            controller
                .admit(&token(), caller(), HoldKind::Setup)
                .unwrap();
            assert_eq!(fake.calls(), [false, true]);
        } else {
            assert!(status.suspended.is_some());
            assert!(
                controller
                    .admit(&token(), caller(), HoldKind::Setup)
                    .is_err()
            );
            assert_eq!(fake.calls(), [false]);
        }
    }
}

#[test]
fn delayed_readback_completes_within_the_original_transition_without_another_write() {
    for (restore, complete_at) in [(false, 1), (false, 3), (true, 2)] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        if restore {
            assert!(controller.tick().unwrap().owned_off);
        }
        fake.edit(|w| w.behavior = Behavior::Delay);
        fake.on_pause(move |w, pause| {
            if pause == complete_at {
                w.radio.wireless_enabled = restore;
            }
        });
        let status = if restore {
            let admission = controller
                .admit(&token(), caller(), HoldKind::Setup)
                .unwrap();
            assert_eq!(admission.availability, Availability::WirelessReady);
            admission.status
        } else {
            controller.tick().unwrap()
        };
        assert_eq!(status.owned_off, !restore);
        assert!(status.pending.is_none() && status.suspended.is_none());
        assert!(status.deferred.is_none());
        assert_eq!(fake.pauses(), complete_at);
        let calls = if restore {
            vec![false, true]
        } else {
            vec![false]
        };
        assert_eq!(fake.calls(), calls);
    }
    // A wait longer than one call's poll cap is still the same transition.
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    fake.edit(|w| w.behavior = Behavior::Delay);
    fake.on_pause(|w, pause| {
        if pause > READBACK_POLLS {
            w.radio.wireless_enabled = false;
        }
    });
    let status = controller.tick().unwrap();
    assert_eq!(status.deferred, Some(Deferred::ReadbackPending));
    assert_eq!(
        status.pending,
        Some(Transition {
            target_enabled: false,
            method_returned: true,
        })
    );
    assert!(status.suspended.is_none() && !status.owned_off);
    assert_eq!(fake.pauses(), READBACK_POLLS);
    // Status only reports the wait; the next coordinator tick resumes it.
    let status = controller.status().unwrap();
    assert_eq!(status.deferred, Some(Deferred::ReadbackPending));
    assert_eq!(fake.pauses(), READBACK_POLLS);
    let status = controller.tick().unwrap();
    assert!(status.owned_off && status.pending.is_none() && status.deferred.is_none());
    assert_eq!(fake.pauses(), READBACK_POLLS + 1);
    assert_eq!(fake.calls(), [false]);
}

#[test]
fn delayed_readback_expiry_remains_a_fault_after_late_completion_and_restart() {
    for signalled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let fake = Fake::new();
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| w.behavior = Behavior::Delay);
        // At its deadline the monitor drops the transition. Without the target
        // notification it also latches lost observation.
        fake.on_pause(move |w, pause| {
            if pause == 2 {
                w.expected = None;
                if !signalled {
                    w.change = Some(Suspension::ObservationLost);
                }
            }
        });
        let fault = if signalled {
            Suspension::ReadbackMismatch
        } else {
            Suspension::ObservationLost
        };
        let status = controller.tick().unwrap();
        assert_eq!(status.suspended, Some(fault));
        assert!(
            status
                .pending
                .is_some_and(|p| p.method_returned && !p.target_enabled)
        );
        assert!(!status.owned_off);
        // Neither a late readback nor a reported transition completes it.
        fake.edit(|w| {
            w.radio.wireless_enabled = false;
            w.expected = Some(false);
        });
        assert_eq!(controller.tick().unwrap().suspended, Some(fault));
        drop(controller);
        fake.edit(|w| w.change = None);
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        let status = controller.tick().unwrap();
        assert_eq!(status.suspended, Some(fault));
        assert!(status.pending.is_some() && !status.owned_off);
        assert!(
            controller
                .admit(&token(), caller(), HoldKind::Setup)
                .is_err()
        );
        assert_eq!(fake.calls(), [false]);
        assert_eq!(fake.pauses(), 2);
        assert_eq!(fake.reconciles(), 0);
    }
}

#[test]
fn restart_during_a_deferred_readback_does_not_reconstruct_its_allowance() {
    for caught_up in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let fake = Fake::new();
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| w.behavior = Behavior::Delay);
        assert_eq!(
            controller.tick().unwrap().deferred,
            Some(Deferred::ReadbackPending)
        );
        drop(controller);
        // A reopened service has a new monitor without the old transition.
        fake.edit(|w| {
            w.expected = None;
            w.radio.wireless_enabled = !caught_up;
        });
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        let status = controller.tick().unwrap();
        if caught_up {
            // A matching readback still confirms a returned command.
            assert!(status.owned_off && status.pending.is_none());
            assert!(status.suspended.is_none());
        } else {
            assert_eq!(status.suspended, Some(Suspension::ReadbackMismatch));
            assert!(status.pending.is_some() && !status.owned_off);
        }
        assert_eq!(fake.calls(), [false]);
        assert_eq!(fake.pauses(), READBACK_POLLS);
    }
}

#[test]
fn readback_deferral_never_accepts_witness_changes_or_monitor_conflicts() {
    for case in [
        "manager",
        "pid",
        "profiles",
        "networking",
        "hardware",
        "external",
        "lost",
        "target",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        fake.edit(|w| w.behavior = Behavior::Delay);
        fake.on_pause(move |w, pause| {
            if pause != 1 {
                return;
            }
            match case {
                "manager" => w.radio.bus_owner = ":1.8".into(),
                "pid" => w.radio.pid = 48,
                "profiles" => w.radio.profile_revision = engine_ota::Sha256::digest(b"changed"),
                "networking" => w.radio.networking_enabled = false,
                "hardware" => w.radio.wireless_hardware_enabled = false,
                "external" => w.change = Some(Suspension::ExternalRadioChange),
                "lost" => w.change = Some(Suspension::ObservationLost),
                "target" => w.expected = Some(true),
                _ => unreachable!(),
            }
        });
        let fault = match case {
            "external" => Suspension::ExternalRadioChange,
            "lost" => Suspension::ObservationLost,
            _ => Suspension::ReadbackMismatch,
        };
        let status = controller.tick().unwrap();
        assert_eq!(status.suspended, Some(fault), "{case}");
        assert!(status.pending.is_some() && !status.owned_off, "{case}");
        // The original target arriving afterward does not clear the conflict.
        fake.edit(|w| w.radio.wireless_enabled = false);
        assert_eq!(controller.tick().unwrap().suspended, Some(fault), "{case}");
        assert!(
            controller
                .admit(&token(), caller(), HoldKind::Setup)
                .is_err()
        );
        assert_eq!(fake.calls(), [false], "{case}");
        assert_eq!(fake.pauses(), 1, "{case}");
    }
}

#[test]
fn ssh_admission_waits_for_delayed_restoration_and_failure_never_rewrites_or_resumes() {
    for outcome in ["complete", "expire", "never"] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        assert!(controller.tick().unwrap().owned_off);
        fake.edit(|w| w.behavior = Behavior::Delay);
        fake.on_pause(move |w, pause| match (outcome, pause) {
            ("complete", 3) => w.radio.wireless_enabled = true,
            ("expire", 3) => {
                w.expected = None;
                w.change = Some(Suspension::ObservationLost);
            }
            _ => {}
        });
        let admitted = controller.admit(&token(), ssh_caller(), HoldKind::Ssh);
        let pauses = if outcome == "never" {
            READBACK_POLLS
        } else {
            3
        };
        assert_eq!(fake.pauses(), pauses, "{outcome}");
        if outcome == "complete" {
            let admitted = admitted.unwrap();
            assert_eq!(admitted.availability, Availability::SessionProtected);
            assert!(!admitted.status.owned_off && admitted.status.pending.is_none());
        } else {
            assert!(admitted.is_err(), "{outcome}");
            // The helper's bounded retries only observe the unresolved write.
            for _ in 0..3 {
                assert!(
                    controller
                        .admit(&token(), ssh_caller(), HoldKind::Ssh)
                        .is_err()
                );
            }
            let status = controller.status().unwrap();
            assert!(status.owned_off && status.pending.is_some(), "{outcome}");
        }
        assert_eq!(fake.calls(), [false, true], "{outcome}");
        assert_eq!(fake.reconciles(), 0, "{outcome}");
    }
}

#[test]
fn hold_arriving_during_shutdown_waits_for_restoration_before_acknowledgment() {
    for (kind, owner, availability) in [
        (HoldKind::Setup, caller(), Availability::WirelessReady),
        (HoldKind::Deployment, caller(), Availability::WirelessReady),
        (HoldKind::Ssh, ssh_caller(), Availability::SessionProtected),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        fake.edit(|w| w.barrier = Some((entered_tx, resume_rx)));
        let controller = Arc::new(Mutex::new(controller));
        let off = {
            let controller = controller.clone();
            std::thread::spawn(move || controller.lock().unwrap().tick().unwrap())
        };
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (queued_tx, queued_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let admit = {
            let controller = controller.clone();
            std::thread::spawn(move || {
                queued_tx.send(()).unwrap();
                let result = controller.lock().unwrap().admit(&token(), owner, kind);
                ack_tx.send(result).unwrap();
            })
        };
        queued_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            ack_rx.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        resume_tx.send(()).unwrap();
        off.join().unwrap();
        let admission = ack_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(admission.availability, availability);
        assert!(admission.status.actual.unwrap().wireless_enabled);
        assert_eq!(fake.calls(), [false, true]);
        admit.join().unwrap();
    }
}

#[test]
fn failed_restore_and_lost_observation_cannot_acknowledge_availability() {
    for behavior in [
        Behavior::Reject,
        Behavior::Timeout,
        Behavior::NoChange,
        Behavior::LoseReadback,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        ready(&mut controller, &fake);
        controller.tick().unwrap();
        fake.edit(|w| w.behavior = behavior);
        assert!(
            controller
                .admit(&token(), caller(), HoldKind::Setup)
                .is_err()
        );
        assert_eq!(fake.calls(), [false, true]);
        assert_eq!(controller.status().unwrap().policy.holds.len(), 1);
    }
}

#[test]
fn observation_failure_resets_quiet_grace_and_publishes_no_stale_actual_state() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    fake.edit(|w| w.unknown = true);
    let status = controller.tick().unwrap();
    assert!(status.actual.is_none());
    assert_eq!(status.deferred, Some(Deferred::ObservationUnavailable));
    fake.edit(|w| w.unknown = false);
    for ms in (111_000..=120_000).step_by(1000) {
        fake.edit(|w| w.clock.ms = ms);
        controller.tick().unwrap();
    }
    assert!(fake.calls().is_empty());
    fake.edit(|w| w.clock.ms = 121_000);
    assert!(controller.tick().unwrap().owned_off);
}

#[test]
fn a_hold_expiring_during_restoration_is_not_acknowledged() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    fake.edit(|w| w.elapsed_on_write = 120_000);
    assert!(
        controller
            .admit(&token(), caller(), HoldKind::TimeCheck)
            .is_err()
    );
    assert_eq!(fake.calls(), [false, true]);
}

#[test]
fn failed_durability_never_sends_a_radio_command() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network");
        let fake = Fake::new();
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        ready(&mut controller, &fake);
        controller.policy.fail_save_at(point);
        assert!(controller.tick().is_err());
        assert!(controller.tick().is_err());
        assert!(fake.calls().is_empty());
        drop(controller);
        let mut controller = Controller::open(&path, fake.clone()).unwrap();
        let status = controller.tick().unwrap();
        let published = matches!(point, "renamed" | "directory-synced");
        assert_eq!(status.pending.is_some(), published);
        if published {
            assert_eq!(status.suspended, Some(Suspension::CommandUncertain));
        }
        assert!(fake.calls().is_empty());
    }
}

#[test]
fn a_clean_reboot_keeps_owned_off_and_persistent_stay_connected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    drop(controller);
    fake.edit(|w| {
        w.clock = Clock {
            boot_id: NEXT_BOOT.into(),
            ms: 0,
        };
        w.radio.profile_revision = engine_ota::Sha256::digest(b"new boot");
        w.radio.pid = 87;
        w.radio.bus_owner = ":1.5".into();
    });
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let status = controller.tick().unwrap();
    assert!(status.owned_off);
    assert!(status.suspended.is_none());
    controller
        .set_policy(
            status.policy.generation,
            Policy {
                stay_connected: true,
                ..enabled()
            },
        )
        .unwrap();
    drop(controller);
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    let status = controller.tick().unwrap();
    assert!(status.policy.policy.stay_connected);
    assert!(status.actual.unwrap().wireless_enabled);
    assert_eq!(fake.calls(), [false, true]);
}

#[test]
fn a_known_unsent_command_can_retry_without_reboot_or_false_admission() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network");
    let fake = Fake::new();
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    ready(&mut controller, &fake);
    controller.tick().unwrap();
    fake.edit(|w| w.behavior = Behavior::NotSubmitted);
    assert!(
        controller
            .admit(&token(), caller(), HoldKind::Setup)
            .is_err()
    );
    let status = controller.status().unwrap();
    assert!(status.pending.is_none() && status.suspended.is_none());
    assert!(!status.actual.unwrap().wireless_enabled);
    drop(controller);
    let mut controller = Controller::open(&path, fake.clone()).unwrap();
    fake.edit(|w| w.behavior = Behavior::Apply);
    controller
        .admit(&token(), caller(), HoldKind::Setup)
        .unwrap();
    assert_eq!(fake.calls(), [false, true, true]);
}

#[test]
fn setup_can_repair_a_profile_conflict_without_resuming_automatic_radio_control() {
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    enable(&mut controller);
    fake.edit(|w| w.radio.profile_revision = engine_ota::Sha256::digest(b"changed-profile"));
    let admission = controller
        .admit(&token(), caller(), HoldKind::Setup)
        .unwrap();
    assert_eq!(admission.availability, Availability::WirelessReady);
    assert!(admission.status.suspended.is_some());
    assert!(
        controller
            .renew(&token(), caller())
            .unwrap()
            .status
            .suspended
            .is_some()
    );
    assert!(
        controller
            .release(&token(), caller())
            .unwrap()
            .suspended
            .is_some()
    );
    assert!(fake.calls().is_empty());
    fake.edit(|w| w.radio.wireless_enabled = false);
    assert!(
        controller
            .admit(&token(), caller(), HoldKind::Setup)
            .is_err()
    );
    assert!(fake.calls().is_empty());
}

#[test]
fn unmanaged_deployments_preserve_ethernet_and_block_on_an_unresolved_radio_command() {
    for unknown in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        fake.edit(|w| {
            w.radio.wireless_enabled = false;
            w.radio.ethernet_available = true;
            w.unknown = unknown;
        });
        let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
        let admission = controller
            .admit(&token(), caller(), HoldKind::Deployment)
            .unwrap();
        assert_eq!(admission.availability, Availability::DeploymentProtected);
        assert!(fake.calls().is_empty());
        fake.edit(|w| w.unknown = false);
        enable(&mut controller);
        assert_eq!(
            controller.renew(&token(), caller()).unwrap().availability,
            Availability::WirelessReady
        );
        assert_eq!(fake.calls(), [true]);
    }
    let temp = tempfile::tempdir().unwrap();
    let fake = Fake::new();
    let mut controller = Controller::open(&temp.path().join("network"), fake.clone()).unwrap();
    ready(&mut controller, &fake);
    assert!(controller.tick().unwrap().owned_off);
    fake.edit(|w| w.behavior = Behavior::Timeout);
    let generation = controller.status().unwrap().policy.generation;
    let status = controller
        .set_policy(generation, Policy::default())
        .unwrap();
    assert!(status.pending.is_some());
    assert!(
        controller
            .admit(&token(), caller(), HoldKind::Deployment)
            .is_err()
    );
}
