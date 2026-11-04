use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

struct PrivateBus(Child, String);
impl PrivateBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Default)]
struct Fixture {
    unstable: AtomicBool,
    slow: AtomicBool,
    reads: AtomicU32,
    mutations: AtomicU32,
    profile_changes: AtomicBool,
    revision_reads: AtomicU32,
    controls: AtomicBool,
    radio_off: AtomicBool,
    deny_write: AtomicBool,
    delay_write: AtomicBool,
    slow_write: AtomicBool,
    write_received: AtomicBool,
    release_write: AtomicBool,
    omit_second_profile: AtomicBool,
    wireless_primary: AtomicBool,
    disconnected: AtomicBool,
}
struct Manager(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn networking_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    async fn wireless_enabled(&self) -> bool {
        if self.0.slow.load(Ordering::SeqCst) {
            async_io::Timer::after(Duration::from_secs(2)).await;
        }
        let reads = self.0.reads.fetch_add(1, Ordering::SeqCst);
        !self.0.radio_off.load(Ordering::SeqCst)
            && (!self.0.unstable.load(Ordering::SeqCst) || reads.is_multiple_of(2))
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        self.0.controls.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn startup(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.controls.load(Ordering::SeqCst) {
            return Vec::new();
        }
        vec![
            "/org/freedesktop/NetworkManager/Checkpoint/1"
                .try_into()
                .unwrap(),
        ]
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        if self.0.disconnected.load(Ordering::SeqCst) {
            40
        } else {
            70
        }
    }
    #[zbus(property)]
    fn primary_connection_type(&self) -> &str {
        if self.0.wireless_primary.load(Ordering::SeqCst) {
            "802-11-wireless"
        } else {
            "802-3-ethernet"
        }
    }
    fn enable(&self, _enabled: bool) {
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
    }
    #[zbus(property)]
    async fn set_wireless_enabled(&mut self, enabled: bool) -> zbus::fdo::Result<()> {
        if self.0.deny_write.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::AccessDenied(
                "private fixture denial".into(),
            ));
        }
        if self.0.slow_write.load(Ordering::SeqCst) {
            self.0.write_received.store(true, Ordering::SeqCst);
            bounded(Duration::from_secs(5), async {
                while !self.0.release_write.load(Ordering::SeqCst) {
                    async_io::Timer::after(Duration::from_millis(1)).await;
                }
                Ok(())
            })
            .await
            .map_err(|_| zbus::fdo::Error::Failed("fixture release deadline".into()))?;
        }
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
        // NM 1.46.6 can reply, and notify the old value, before its cached
        // rfkill state is exported. Delayed tests apply the target later.
        if !self.0.delay_write.load(Ordering::SeqCst) {
            self.0.radio_off.store(!enabled, Ordering::SeqCst);
        }
        Ok(())
    }
}

async fn server(bus: &PrivateBus, fixture: &Arc<Fixture>) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(NAME)
        .unwrap()
        .serve_at(OBJECT, Manager(fixture.clone()))
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Settings",
            Settings(fixture.clone()),
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Settings/1",
            Profile(fixture.clone()),
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Settings/2",
            Profile(fixture.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn radio_signal(
    connection: &zbus::Connection,
    fields: HashMap<String, OwnedValue>,
    invalidated: Vec<String>,
) {
    connection
        .emit_signal(
            None::<&str>,
            OBJECT,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(NAME, fields, invalidated),
        )
        .await
        .unwrap();
}

async fn wireless_signal(connection: &zbus::Connection, enabled: bool) {
    radio_signal(
        connection,
        HashMap::from([("WirelessEnabled".into(), OwnedValue::from(enabled))]),
        vec![],
    )
    .await;
}

#[test]
fn continuous_monitor_latches_changes_reversed_between_snapshots_and_ignores_other_senders() {
    use crate::network_controller::Suspension;
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let server = server(&bus, &fixture).await;
        let uid = unsafe { libc::geteuid() };
        let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
        let (before, change) = monitor.observe().await.unwrap();
        assert!(change.is_none());
        let other = bus.connect().await;
        radio_signal(
            &other,
            HashMap::from([("WirelessEnabled".into(), OwnedValue::from(false))]),
            vec![],
        )
        .await;
        assert!(monitor.observe().await.unwrap().1.is_none());
        for enabled in [false, true] {
            radio_signal(
                &server,
                HashMap::from([("WirelessEnabled".into(), OwnedValue::from(enabled))]),
                vec![],
            )
            .await;
        }
        let (after, change) = monitor.observe().await.unwrap();
        assert_eq!(before, after);
        assert_eq!(change, Some(Suspension::ExternalRadioChange));
        assert_eq!(monitor.observe().await.unwrap().1, change);
        assert!(!monitor.prepare(&before, false).await.unwrap());
        let mut resumed = Monitor::connect(bus.connect().await, uid).await.unwrap();
        assert!(resumed.observe().await.unwrap().1.is_none());
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn continuous_monitor_attributes_one_own_change_and_keeps_later_changes_external() {
    use crate::network_controller::{Suspension, WriteResult};
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let server = server(&bus, &fixture).await;
        let uid = unsafe { libc::geteuid() };
        let client = bus.connect().await;
        let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
        let (before, _) = monitor.observe().await.unwrap();
        assert!(!monitor.prepare(&before, true).await.unwrap());
        assert!(monitor.prepare(&before, false).await.unwrap());
        assert!(!monitor.prepare(&before, false).await.unwrap());
        let result = crate::network_radio::write(&client, uid, &before, false).await;
        assert_eq!(result.as_ref().unwrap(), &WriteResult::Returned);
        monitor.finish_write(&result);
        let (after, change) = monitor.observe().await.unwrap();
        assert!(!after.wireless_enabled);
        assert!(change.is_none());
        // A later transition is not another allowance for our Set.
        radio_signal(
            &server,
            HashMap::from([("WirelessEnabled".into(), OwnedValue::from(true))]),
            vec![],
        )
        .await;
        assert_eq!(
            monitor.observe().await.unwrap().1,
            Some(Suspension::ExternalRadioChange)
        );
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn continuous_monitor_ignores_redundant_notifications_around_own_transition() {
    use crate::network_controller::{Suspension, WriteResult};
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        for enabled in [true, false] {
            let fixture = Arc::new(Fixture::default());
            fixture.controls.store(true, Ordering::SeqCst);
            fixture.radio_off.store(enabled, Ordering::SeqCst);
            let server = server(&bus, &fixture).await;
            let uid = unsafe { libc::geteuid() };
            let client = bus.connect().await;
            let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
            let (before, _) = monitor.observe().await.unwrap();
            assert!(monitor.prepare(&before, enabled).await.unwrap());
            // NM 1.46.6 reports the old exported value while its cached rfkill
            // bit catches up, then reports the requested value. Replay both.
            radio_signal(
                &server,
                HashMap::from([("WirelessEnabled".into(), OwnedValue::from(!enabled))]),
                vec![],
            )
            .await;
            let result = crate::network_radio::write(&client, uid, &before, enabled).await;
            assert_eq!(result.as_ref().unwrap(), &WriteResult::Returned);
            monitor.finish_write(&result);
            let (after, change) = monitor.observe().await.unwrap();
            assert_eq!(after.wireless_enabled, enabled);
            assert_eq!(change, None, "redundant old value before target {enabled}");
            for _ in 0..2 {
                radio_signal(
                    &server,
                    HashMap::from([("WirelessEnabled".into(), OwnedValue::from(enabled))]),
                    vec![],
                )
                .await;
            }
            assert!(monitor.observe().await.unwrap().1.is_none());
            // A real reversal remains external even if the next snapshot is
            // identical and more duplicate notifications follow it.
            for value in [!enabled, !enabled, enabled, enabled] {
                radio_signal(
                    &server,
                    HashMap::from([("WirelessEnabled".into(), OwnedValue::from(value))]),
                    vec![],
                )
                .await;
            }
            let (last, change) = monitor.observe().await.unwrap();
            assert_eq!(last, after);
            assert_eq!(change, Some(Suspension::ExternalRadioChange));
            assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
            server.release_name(NAME).await.unwrap();
        }
    });
}

#[test]
fn continuous_monitor_redundant_notifications_do_not_complete_a_missing_transition() {
    use crate::network_controller::{Suspension, WriteResult};
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        fixture.radio_off.store(true, Ordering::SeqCst);
        let server = server(&bus, &fixture).await;
        let uid = unsafe { libc::geteuid() };
        let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
        let (before, _) = monitor.observe().await.unwrap();
        assert!(monitor.prepare(&before, true).await.unwrap());
        // A returned method without the target signal is still incomplete.
        monitor.finish_write(&Ok(WriteResult::Returned));
        for delay in [Duration::ZERO, Duration::from_millis(5100)] {
            async_io::Timer::after(delay).await;
            radio_signal(
                &server,
                HashMap::from([("WirelessEnabled".into(), OwnedValue::from(false))]),
                vec![],
            )
            .await;
            let (_, change) = monitor.observe().await.unwrap();
            if delay.is_zero() {
                assert!(change.is_none());
            } else {
                assert_eq!(change, Some(Suspension::ObservationLost));
            }
        }
        assert!(!monitor.prepare(&before, true).await.unwrap());
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn continuous_monitor_keeps_a_delayed_transition_until_signal_and_readback_agree() {
    use crate::network_controller::WriteResult;
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        for enabled in [true, false] {
            for signal_first in [true, false] {
                let fixture = Arc::new(Fixture::default());
                fixture.controls.store(true, Ordering::SeqCst);
                fixture.radio_off.store(enabled, Ordering::SeqCst);
                fixture.delay_write.store(true, Ordering::SeqCst);
                let server = server(&bus, &fixture).await;
                let uid = unsafe { libc::geteuid() };
                let client = bus.connect().await;
                let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
                let (before, _) = monitor.observe().await.unwrap();
                assert!(monitor.prepare(&before, enabled).await.unwrap());
                let result = crate::network_radio::write(&client, uid, &before, enabled).await;
                assert_eq!(result.as_ref().unwrap(), &WriteResult::Returned);
                monitor.finish_write(&result);
                // The reply and old-value notifications leave readback unchanged.
                for _ in 0..2 {
                    let (after, change) = monitor.observe().await.unwrap();
                    assert_eq!(after, before);
                    assert!(change.is_none());
                    assert_eq!(monitor.expected(), Some(enabled));
                    wireless_signal(&server, !enabled).await;
                }
                if signal_first {
                    wireless_signal(&server, enabled).await;
                } else {
                    fixture.radio_off.store(!enabled, Ordering::SeqCst);
                }
                let (partial, change) = monitor.observe().await.unwrap();
                let readback = if signal_first { !enabled } else { enabled };
                assert_eq!(partial.wireless_enabled, readback);
                assert!(change.is_none());
                assert_eq!(monitor.expected(), Some(enabled));
                if signal_first {
                    fixture.radio_off.store(!enabled, Ordering::SeqCst);
                } else {
                    wireless_signal(&server, enabled).await;
                }
                let (complete, change) = monitor.observe().await.unwrap();
                assert_eq!(complete.wireless_enabled, enabled);
                assert!(change.is_none(), "enabled={enabled}, first={signal_first}");
                assert_eq!(monitor.expected(), None);
                assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
                server.release_name(NAME).await.unwrap();
            }
        }
    });
}

#[test]
fn continuous_monitor_judges_a_delayed_transition_at_its_original_deadline() {
    use crate::network_controller::{Suspension, WriteResult};
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        for case in ["duplicates", "queued", "late", "signalled"] {
            let fixture = Arc::new(Fixture::default());
            fixture.controls.store(true, Ordering::SeqCst);
            fixture.radio_off.store(true, Ordering::SeqCst);
            fixture.delay_write.store(true, Ordering::SeqCst);
            let server = server(&bus, &fixture).await;
            let uid = unsafe { libc::geteuid() };
            let client = bus.connect().await;
            let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
            let (before, _) = monitor.observe().await.unwrap();
            assert!(monitor.prepare(&before, true).await.unwrap());
            let result = crate::network_radio::write(&client, uid, &before, true).await;
            assert_eq!(result.as_ref().unwrap(), &WriteResult::Returned);
            monitor.finish_write(&result);
            match case {
                // Old-value notifications do not extend the original deadline.
                "duplicates" => {
                    for _ in 0..3 {
                        wireless_signal(&server, false).await;
                    }
                    assert!(monitor.observe().await.unwrap().1.is_none());
                    wireless_signal(&server, false).await;
                }
                // Queued before the deadline, but drained after it.
                "queued" => wireless_signal(&server, true).await,
                "signalled" => {
                    wireless_signal(&server, true).await;
                    assert!(monitor.observe().await.unwrap().1.is_none());
                    assert_eq!(monitor.expected(), Some(true));
                }
                _ => {}
            }
            monitor.reach_deadline();
            let (_, change) = monitor.observe().await.unwrap();
            assert_eq!(monitor.expected(), None, "{case}");
            if case == "signalled" {
                // The coordinator rejects the unchanged readback, not the monitor.
                assert!(change.is_none());
            } else {
                assert_eq!(change, Some(Suspension::ObservationLost), "{case}");
                // Late readback and notification cannot clear or renew it.
                fixture.radio_off.store(false, Ordering::SeqCst);
                wireless_signal(&server, true).await;
                let (after, change) = monitor.observe().await.unwrap();
                assert!(after.wireless_enabled);
                assert_eq!(change, Some(Suspension::ObservationLost), "{case}");
                assert!(!monitor.prepare(&after, false).await.unwrap());
            }
            assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
            server.release_name(NAME).await.unwrap();
        }
    });
}

#[derive(Clone, Copy)]
enum Step {
    Wait,
    Duplicate,
    Complete,
    Deadline,
}

/// Runs the coordinator against the real monitor and private bus, with the
/// native adapter's prepare, write and finish sequence.
struct Bus {
    monitor: crate::network_monitor::Monitor,
    client: zbus::Connection,
    server: zbus::Connection,
    fixture: Arc<Fixture>,
    uid: u32,
    ms: u64,
    steps: std::collections::VecDeque<Step>,
    pauses: Arc<AtomicU32>,
}

impl crate::network_controller::Backend for Bus {
    type ShutdownGuard = ();
    fn clock(&mut self) -> Result<crate::network_policy::Clock> {
        Ok(crate::network_policy::Clock {
            boot_id: "11111111-2222-3333-4444-555555555555".into(),
            ms: self.ms,
        })
    }
    fn snapshot(&mut self) -> Result<crate::network_controller::Snapshot> {
        let (radio, change) = futures_lite::future::block_on(self.monitor.observe())?;
        Ok(crate::network_controller::Snapshot {
            clock: self.clock()?,
            radio,
            activity: crate::network_policy::Activity {
                ssh_sessions: Some(false),
                checkpoints: None,
                install_lock: Some(false),
                operations: Some(false),
            },
            change,
            expected: self.monitor.expected(),
        })
    }
    fn shutdown_guard(&mut self) -> Result<Option<()>> {
        Ok(Some(()))
    }
    fn set_wireless(
        &mut self,
        expected: &Observation,
        enabled: bool,
    ) -> Result<crate::network_controller::WriteResult> {
        if !futures_lite::future::block_on(self.monitor.prepare(expected, enabled))
            .is_ok_and(|ready| ready)
        {
            return Ok(crate::network_controller::WriteResult::NotSubmitted);
        }
        let result = futures_lite::future::block_on(crate::network_radio::write(
            &self.client,
            self.uid,
            expected,
            enabled,
        ));
        self.monitor.finish_write(&result);
        result
    }
    fn pause(&mut self, duration: Duration) {
        self.ms += duration.as_millis() as u64;
        self.pauses.fetch_add(1, Ordering::SeqCst);
        match self.steps.pop_front().unwrap_or(Step::Wait) {
            Step::Wait => {}
            Step::Duplicate => futures_lite::future::block_on(wireless_signal(&self.server, false)),
            Step::Complete => {
                self.fixture.radio_off.store(false, Ordering::SeqCst);
                futures_lite::future::block_on(wireless_signal(&self.server, true));
            }
            Step::Deadline => self.monitor.reach_deadline(),
        }
    }
}

#[test]
fn coordinator_waits_for_a_delayed_readback_from_the_real_monitor() {
    use crate::network_controller::{Availability, Controller, Suspension};
    use crate::network_monitor::Monitor;
    use crate::network_policy::{Caller, HoldKind, HoldToken, Policy};
    for (steps, completes) in [
        (vec![Step::Wait, Step::Complete], true),
        (vec![Step::Duplicate, Step::Duplicate, Step::Complete], true),
        (vec![Step::Duplicate, Step::Deadline], false),
    ] {
        let bus = PrivateBus::new();
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        fixture.radio_off.store(true, Ordering::SeqCst);
        fixture.delay_write.store(true, Ordering::SeqCst);
        let uid = unsafe { libc::geteuid() };
        let (server, monitor, client) = futures_lite::future::block_on(async {
            let server = server(&bus, &fixture).await;
            let monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
            (server, monitor, bus.connect().await)
        });
        let pauses = Arc::new(AtomicU32::new(0));
        let backend = Bus {
            monitor,
            client,
            server: server.clone(),
            fixture: fixture.clone(),
            uid,
            ms: 0,
            steps: steps.iter().copied().collect(),
            pauses: pauses.clone(),
        };
        let temp = tempfile::tempdir().unwrap();
        let mut controller = Controller::open(&temp.path().join("network"), backend).unwrap();
        let generation = controller.status().unwrap().policy.generation;
        let policy = Policy {
            partial_offline: true,
            stay_connected: false,
        };
        controller.set_policy(generation, policy).unwrap();
        let token = HoldToken::parse("a".repeat(64)).unwrap();
        let caller = Caller {
            uid: 1000,
            pid: 73,
            process_start_ticks: 17,
        };
        // The setup hold restores Wi-Fi; NM replies before exporting it.
        let admitted = controller.admit(&token, caller, HoldKind::Setup);
        assert_eq!(pauses.load(Ordering::SeqCst) as usize, steps.len());
        if completes {
            let admitted = admitted.unwrap();
            assert_eq!(admitted.availability, Availability::WirelessReady);
            assert!(admitted.status.pending.is_none());
            assert!(admitted.status.suspended.is_none());
        } else {
            assert!(admitted.is_err());
            // A late completion is visible but does not complete the write.
            fixture.radio_off.store(false, Ordering::SeqCst);
            futures_lite::future::block_on(wireless_signal(&server, true));
            let status = controller.tick().unwrap();
            assert!(status.actual.unwrap().wireless_enabled);
            assert_eq!(status.suspended, Some(Suspension::ObservationLost));
            assert!(
                status
                    .pending
                    .is_some_and(|p| p.method_returned && p.target_enabled)
            );
            assert!(controller.admit(&token, caller, HoldKind::Setup).is_err());
            assert_eq!(pauses.load(Ordering::SeqCst) as usize, steps.len());
        }
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn continuous_monitor_expected_wireless_transition_keeps_other_radio_controls_external() {
    use crate::network_controller::Suspension;
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let server = server(&bus, &fixture).await;
        let uid = unsafe { libc::geteuid() };
        for name in ["NetworkingEnabled", "WirelessHardwareEnabled"] {
            let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
            let (before, _) = monitor.observe().await.unwrap();
            assert!(monitor.prepare(&before, false).await.unwrap());
            radio_signal(
                &server,
                HashMap::from([
                    ("WirelessEnabled".into(), OwnedValue::from(false)),
                    (name.into(), OwnedValue::from(false)),
                ]),
                vec![],
            )
            .await;
            assert_eq!(
                monitor.observe().await.unwrap().1,
                Some(Suspension::ExternalRadioChange),
                "{name}"
            );
        }
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn continuous_monitor_reports_invalidated_evidence_and_manager_replacement() {
    use crate::network_controller::Suspension;
    use crate::network_monitor::Monitor;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let first = server(&bus, &fixture).await;
        let uid = unsafe { libc::geteuid() };
        assert!(
            Monitor::connect(bus.connect().await, uid.wrapping_add(1))
                .await
                .is_err()
        );
        let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
        radio_signal(&first, HashMap::new(), vec!["WirelessEnabled".into()]).await;
        assert_eq!(
            monitor.observe().await.unwrap().1,
            Some(Suspension::ObservationLost)
        );
        let mut monitor = Monitor::connect(bus.connect().await, uid).await.unwrap();
        first.release_name(NAME).await.unwrap();
        let _second = server(&bus, &fixture).await;
        assert_eq!(
            monitor.observe().await.unwrap().1,
            Some(Suspension::ManagerChanged)
        );
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn wireless_only_command_checks_owner_revision_hardware_and_permission() {
    use crate::network_controller::WriteResult;
    use crate::network_radio::write;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let _server = server(&bus, &fixture).await;
        let client = bus.connect().await;
        let uid = unsafe { libc::geteuid() };
        let before = connection_observation(&client, uid).await.unwrap();
        for field in ["owner", "revision", "pid"] {
            let mut stale = before.clone();
            match field {
                "owner" => stale.bus_owner = ":1.999999".into(),
                "revision" => stale.profile_revision = Sha256::digest(b"changed"),
                "pid" => stale.pid += 1,
                _ => unreachable!(),
            }
            assert_eq!(
                write(&client, uid, &stale, false).await.unwrap(),
                WriteResult::Rejected
            );
        }
        assert_eq!(
            write(&client, uid.wrapping_add(1), &before, false)
                .await
                .unwrap(),
            WriteResult::NotSubmitted
        );
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
        assert_eq!(
            write(&client, uid, &before, false).await.unwrap(),
            WriteResult::Returned
        );
        let off = connection_observation(&client, uid).await.unwrap();
        assert!(!off.wireless_enabled);
        assert!(off.networking_enabled && off.ethernet_available);
        assert_eq!(off.profile_revision, before.profile_revision);
        assert_eq!(
            write(&client, uid, &before, true).await.unwrap(),
            WriteResult::Rejected
        );
        assert_eq!(
            write(&client, uid, &off, true).await.unwrap(),
            WriteResult::Returned
        );
        let on = connection_observation(&client, uid).await.unwrap();
        assert!(on.wireless_enabled);
        fixture.deny_write.store(true, Ordering::SeqCst);
        assert_eq!(
            write(&client, uid, &on, false).await.unwrap(),
            WriteResult::Rejected
        );
        assert!(
            connection_observation(&client, uid)
                .await
                .unwrap()
                .wireless_enabled
        );
        fixture.controls.store(false, Ordering::SeqCst);
        let blocked = connection_observation(&client, uid).await.unwrap();
        assert_eq!(
            write(&client, uid, &blocked, false).await.unwrap(),
            WriteResult::Rejected
        );
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn timed_out_dbus_write_can_still_execute_and_is_not_a_rejection() {
    use crate::network_radio::write;
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        let _server = server(&bus, &fixture).await;
        let client = bus.connect().await;
        let uid = unsafe { libc::geteuid() };
        let before = connection_observation(&client, uid).await.unwrap();
        fixture.slow_write.store(true, Ordering::SeqCst);
        // Establish dispatch explicitly: a short deadline around the whole
        // preflight can expire before Set reaches a slower target's provider.
        // Dropping the losing future uses the same cancellation as bounded().
        let timed_out = bounded(
            Duration::from_secs(5),
            futures_lite::future::or(
                async { write(&client, uid, &before, false).await.map(|_| false) },
                async {
                    while !fixture.write_received.load(Ordering::SeqCst) {
                        async_io::Timer::after(Duration::from_millis(1)).await;
                    }
                    async_io::Timer::after(Duration::from_millis(30)).await;
                    Ok(true)
                },
            ),
        )
        .await
        .unwrap();
        assert!(timed_out);
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
        assert!(!fixture.radio_off.load(Ordering::SeqCst));
        fixture.release_write.store(true, Ordering::SeqCst);
        let after = bounded(Duration::from_secs(3), connection_observation(&client, uid))
            .await
            .unwrap();
        assert!(!after.wireless_enabled);
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
    });
}

struct Settings(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
    #[zbus(property)]
    fn connections(&self) -> Vec<OwnedObjectPath> {
        let mut paths = vec![
            "/org/freedesktop/NetworkManager/Settings/1"
                .try_into()
                .unwrap(),
        ];
        if !self.0.omit_second_profile.load(Ordering::SeqCst) {
            paths.push(
                "/org/freedesktop/NetworkManager/Settings/2"
                    .try_into()
                    .unwrap(),
            );
        }
        paths
    }
}

struct Profile(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    #[zbus(property, name = "VersionId")]
    fn version_id(&self) -> u64 {
        let count = self.0.revision_reads.fetch_add(1, Ordering::SeqCst);
        7 + if self.0.profile_changes.load(Ordering::SeqCst) {
            u64::from(count)
        } else {
            0
        }
    }
}

#[test]
fn actual_private_bus_reports_radio_and_checkpoint_state_without_mutations() {
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        let server = server(&bus, &fixture).await;
        let client = bus.connect().await;
        let uid = unsafe { libc::geteuid() };
        let observed = bounded(DEADLINE, connection_observation(&client, uid))
            .await
            .unwrap();
        assert!(observed.networking_enabled && observed.wireless_enabled);
        assert!(!observed.wireless_hardware_enabled);
        assert_eq!(observed.checkpoint_count, 1);
        assert_eq!(observed.state, 70);
        assert!(observed.ethernet_available);
        assert_eq!(observed.profile_count, 2);
        assert_eq!(observed.bus_owner, server.unique_name().unwrap().as_str());
        fixture.omit_second_profile.store(true, Ordering::SeqCst);
        let fewer = connection_observation(&client, uid).await.unwrap();
        assert_eq!(fewer.profile_count, 1);
        assert_ne!(fewer.profile_revision, observed.profile_revision);
        fixture.omit_second_profile.store(false, Ordering::SeqCst);
        assert!(
            connection_observation(&client, uid.wrapping_add(1))
                .await
                .is_err()
        );
        fixture.profile_changes.store(true, Ordering::SeqCst);
        assert!(connection_observation(&client, uid).await.is_err());
        fixture.profile_changes.store(false, Ordering::SeqCst);
        fixture.unstable.store(true, Ordering::SeqCst);
        assert!(connection_observation(&client, uid).await.is_err());
        fixture.slow.store(true, Ordering::SeqCst);
        let started = std::time::Instant::now();
        assert!(
            bounded(
                Duration::from_millis(30),
                connection_observation(&client, uid)
            )
            .await
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn primary_wireless_observation_requires_association_and_available_radio() {
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        fixture.controls.store(true, Ordering::SeqCst);
        fixture.wireless_primary.store(true, Ordering::SeqCst);
        let _server = server(&bus, &fixture).await;
        let client = bus.connect().await;
        let uid = unsafe { libc::geteuid() };
        let ready = connection_observation(&client, uid).await.unwrap();
        assert!(ready.wireless_available);
        assert!(!ready.ethernet_available);
        for unavailable in [&fixture.radio_off, &fixture.disconnected] {
            unavailable.store(true, Ordering::SeqCst);
            assert!(
                !connection_observation(&client, uid)
                    .await
                    .unwrap()
                    .wireless_available
            );
            unavailable.store(false, Ordering::SeqCst);
        }
        fixture.controls.store(false, Ordering::SeqCst);
        assert!(
            !connection_observation(&client, uid)
                .await
                .unwrap()
                .wireless_available
        );
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    });
}

fn values() -> HashMap<String, OwnedValue> {
    let mut values = HashMap::from([
        ("NetworkingEnabled".into(), true.into()),
        ("WirelessEnabled".into(), false.into()),
        ("WirelessHardwareEnabled".into(), true.into()),
        ("Startup".into(), false.into()),
        ("State".into(), 30u32.into()),
        (
            "PrimaryConnectionType".into(),
            zbus::zvariant::Str::from("802-11-wireless").into(),
        ),
    ]);
    values.insert(
        "Checkpoints".into(),
        OwnedValue::try_from(zbus::zvariant::Value::from(Vec::<OwnedObjectPath>::new())).unwrap(),
    );
    values
}

#[test]
fn missing_malformed_or_unbounded_properties_cannot_imply_an_idle_radio() {
    assert!(!parse(values()).unwrap().wireless);
    for name in [
        "NetworkingEnabled",
        "WirelessEnabled",
        "WirelessHardwareEnabled",
        "Startup",
        "State",
        "PrimaryConnectionType",
        "Checkpoints",
    ] {
        let mut input = values();
        input.remove(name);
        assert!(parse(input).is_err());
        let mut input = values();
        input.insert(name.into(), 99u64.into());
        assert!(parse(input).is_err());
    }
    let mut input = values();
    input.insert("State".into(), 999u32.into());
    assert!(parse(input).is_err());
    for value in ["x".repeat(65), "ethernet\n".into(), "../device".into()] {
        let mut input = values();
        input.insert(
            "PrimaryConnectionType".into(),
            zbus::zvariant::Str::from(value.as_str()).into(),
        );
        assert!(parse(input).is_err());
    }
    for paths in [
        vec!["/foreign"],
        vec!["/org/freedesktop/NetworkManager/Checkpoint/1"; 2],
        vec!["/org/freedesktop/NetworkManager/Checkpoint/1"; 33],
    ] {
        let paths: Vec<OwnedObjectPath> =
            paths.into_iter().map(|p| p.try_into().unwrap()).collect();
        let mut input = values();
        input.insert(
            "Checkpoints".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(paths)).unwrap(),
        );
        assert!(parse(input).is_err());
    }
}

#[test]
fn profile_inventory_is_bounded_unique_and_limited_to_connection_objects() {
    assert!(profile_paths(true.into()).is_err());
    for paths in [
        vec!["/org/freedesktop/NetworkManager/Settings/1".to_string(); 2],
        vec!["/org/freedesktop/NetworkManager/Settings".into()],
        vec!["/org/freedesktop/NetworkManager/Devices/1".into()],
        (1..=65)
            .map(|n| format!("/org/freedesktop/NetworkManager/Settings/{n}"))
            .collect(),
    ] {
        let paths: Vec<OwnedObjectPath> = paths
            .into_iter()
            .map(|path| path.try_into().unwrap())
            .collect();
        let value = OwnedValue::try_from(zbus::zvariant::Value::from(paths)).unwrap();
        assert!(profile_paths(value).is_err());
    }
    let paths: Vec<OwnedObjectPath> = [
        "/org/freedesktop/NetworkManager/Settings/2",
        "/org/freedesktop/NetworkManager/Settings/1",
    ]
    .into_iter()
    .map(|path| path.try_into().unwrap())
    .collect();
    let value = OwnedValue::try_from(zbus::zvariant::Value::from(paths)).unwrap();
    assert_eq!(
        profile_paths(value).unwrap()[0].as_str(),
        "/org/freedesktop/NetworkManager/Settings/1"
    );
}
