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
    slow_write: AtomicBool,
    write_received: AtomicBool,
    release_write: AtomicBool,
    omit_second_profile: AtomicBool,
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
        70
    }
    #[zbus(property)]
    fn primary_connection_type(&self) -> &str {
        "802-3-ethernet"
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
        self.0.radio_off.store(!enabled, Ordering::SeqCst);
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
        // A second matching notification is not another allowance for our Set.
        radio_signal(
            &server,
            HashMap::from([("WirelessEnabled".into(), OwnedValue::from(false))]),
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
