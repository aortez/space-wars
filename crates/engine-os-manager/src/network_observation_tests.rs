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
        !self.0.unstable.load(Ordering::SeqCst) || reads.is_multiple_of(2)
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn startup(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
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
    fn enable(&self, _enabled: bool) {
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
    }
    #[zbus(property)]
    fn set_wireless_enabled(&mut self, _enabled: bool) {
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn actual_private_bus_reports_radio_and_checkpoint_state_without_mutations() {
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        let server = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name(NAME)
            .unwrap()
            .serve_at(OBJECT, Manager(fixture.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = bus.connect().await;
        let uid = unsafe { libc::geteuid() };
        let observed = bounded(DEADLINE, connection_observation(&client, uid))
            .await
            .unwrap();
        assert!(observed.networking_enabled && observed.wireless_enabled);
        assert!(!observed.wireless_hardware_enabled);
        assert_eq!(observed.checkpoint_count, 1);
        assert_eq!(observed.state, 70);
        assert_eq!(observed.bus_owner, server.unique_name().unwrap().as_str());
        assert!(
            connection_observation(&client, uid.wrapping_add(1))
                .await
                .is_err()
        );
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
