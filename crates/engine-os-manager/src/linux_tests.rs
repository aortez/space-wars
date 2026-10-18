use super::*;
use crate::time::{TimeReason, TimeRecord, TimeTracker};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

const UTC: u64 = 1_800_000_000_000_000;

struct PrivateBus(Child, String);
impl PrivateBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("private provider tests require dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .method_timeout(DEADLINE)
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

#[derive(Clone)]
struct Fixture {
    packet: u64,
    accepted: u64,
    spike: bool,
    invocation: u8,
    main_pid: u32,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            packet: 1,
            accepted: 1,
            spike: false,
            invocation: 42,
            main_pid: std::process::id(),
        }
    }
}
struct Provider(Arc<Mutex<Fixture>>);
#[zbus::interface(name = "org.freedesktop.timesync1.Manager")]
impl Provider {
    #[zbus(property, name = "SpacewarsLastSuccessfulSync")]
    fn receipt(&self) -> (u64, u64, u64, u64) {
        (
            self.0.lock().unwrap().accepted,
            UTC,
            100_000_000,
            100_000_000,
        )
    }
    #[zbus(property, name = "NTPMessage")]
    fn ntp(&self) -> NtpMessage {
        let fixture = self.0.lock().unwrap();
        (
            0,
            4,
            4,
            1,
            -20,
            0,
            100,
            vec![71, 79, 79, 71],
            UTC - 1,
            UTC,
            UTC,
            UTC + 1,
            fixture.spike,
            fixture.packet,
            100,
        )
    }
}
struct Unit(Arc<Mutex<Fixture>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> &str {
        "active"
    }
    #[zbus(property, name = "InvocationID")]
    fn invocation(&self) -> Vec<u8> {
        vec![self.0.lock().unwrap().invocation; 16]
    }
}
struct Service(Arc<Mutex<Fixture>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> u32 {
        self.0.lock().unwrap().main_pid
    }
}

async fn provider(bus: &PrivateBus, fixture: &Arc<Mutex<Fixture>>) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(PROVIDER)
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .serve_at(OBJECT, Provider(fixture.clone()))
        .unwrap()
        .serve_at(UNIT, Unit(fixture.clone()))
        .unwrap()
        .serve_at(UNIT, Service(fixture.clone()))
        .unwrap()
        .build()
        .await
        .unwrap()
}

fn clock(_: &ProviderIdentity) -> std::result::Result<(Option<u64>, bool, ClockReading), String> {
    Ok((
        Some(UTC + 100),
        true,
        ClockReading {
            realtime_us: UTC + 1_000_000,
            monotonic_us: 101_000_000,
            boottime_us: 101_000_000,
        },
    ))
}

fn tracker() -> TimeTracker {
    TimeTracker::new(
        "private-provider-boot".into(),
        TimeRecord::default(),
        TimeReason::MissingState,
        ClockReading {
            realtime_us: UTC,
            monotonic_us: 100_000_000,
            boottime_us: 100_000_000,
        },
        None,
    )
}

#[test]
fn private_provider_decodes_the_real_dbus_signatures_and_correlates_identity() {
    futures_lite::future::block_on(async {
        let bus = PrivateBus::new();
        let fixture = Arc::new(Mutex::new(Fixture::default()));
        let server = provider(&bus, &fixture).await;
        let client = bus.connect().await;
        let observation = observe_connection(&client, clock).await.unwrap();
        assert_eq!(
            observation.provider.bus_owner,
            server.unique_name().unwrap().as_str()
        );
        assert_eq!(observation.provider.pid, std::process::id());
        assert_eq!(observation.provider.uid, unsafe { libc::geteuid() });
        assert_eq!(observation.provider.invocation_id, "2a".repeat(16));
        assert!(tracker().observe(&observation));
        fixture.lock().unwrap().main_pid += 1;
        assert!(
            observe_connection(&client, clock)
                .await
                .unwrap_err()
                .contains("identity")
        );
    });
}

#[test]
fn private_provider_failed_adjustment_and_spike_cannot_confirm_sync() {
    futures_lite::future::block_on(async {
        let bus = PrivateBus::new();
        let fixture = Arc::new(Mutex::new(Fixture::default()));
        let _server = provider(&bus, &fixture).await;
        let client = bus.connect().await;
        fixture.lock().unwrap().packet = 2; // receipt remains at the accepted packet
        assert!(!tracker().observe(&observe_connection(&client, clock).await.unwrap()));
        {
            let mut fixture = fixture.lock().unwrap();
            fixture.accepted = 2;
            fixture.spike = true;
        }
        assert!(!tracker().observe(&observe_connection(&client, clock).await.unwrap()));
        fixture.lock().unwrap().spike = false;
        let rejected_kernel = observe_connection(&client, |provider| {
            let (marker, _, now) = clock(provider)?;
            Ok((marker, false, now))
        })
        .await
        .unwrap();
        assert!(!tracker().observe(&rejected_kernel));
    });
}

#[test]
fn private_provider_restart_or_new_packet_during_read_is_retried() {
    futures_lite::future::block_on(async {
        let bus = PrivateBus::new();
        let fixture = Arc::new(Mutex::new(Fixture::default()));
        let _server = provider(&bus, &fixture).await;
        let client = bus.connect().await;
        let changed = observe_connection(&client, |provider| {
            fixture.lock().unwrap().invocation += 1;
            clock(provider)
        })
        .await;
        assert!(changed.unwrap_err().contains("changed during observation"));
        let changed = observe_connection(&client, |provider| {
            fixture.lock().unwrap().packet += 1;
            clock(provider)
        })
        .await;
        assert!(changed.unwrap_err().contains("changed during observation"));
    });
}
