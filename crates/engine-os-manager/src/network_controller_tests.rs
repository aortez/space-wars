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
}

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
        })))
    }
    fn edit(&self, f: impl FnOnce(&mut World)) {
        f(&mut self.0.lock().unwrap());
    }
    fn calls(&self) -> Vec<bool> {
        self.0.lock().unwrap().calls.clone()
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
        if world.reject_reconcile {
            return Err(invalid("monitor unavailable"));
        }
        world.change = None;
        Ok(())
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
    assert!(network_policy::protect_ssh_without_service(&path).is_err());
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
        let result = controller.admit(&token(), caller(), HoldKind::Setup);
        assert_eq!(result.is_ok(), initially_enabled);
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
    assert!(network_policy::protect_ssh_without_service(&path).is_err());
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
fn hold_arriving_during_shutdown_waits_for_restoration_before_acknowledgment() {
    for (kind, owner, availability) in [
        (HoldKind::Setup, caller(), Availability::WirelessReady),
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
