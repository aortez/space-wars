use super::*;
use crate::time_schedule::{Connection, Schedule};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

const BOOT: &str = "11111111-2222-3333-4444-555555555555";
struct Bus(Child, String);
impl Bus {
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
    async fn connection(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .method_timeout(DEADLINE)
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct State {
    active: &'static str,
    enabled: &'static str,
    load: &'static str,
    pid: u32,
    invocation_reads: usize,
    change_identity: bool,
    mode: &'static str,
    calls: Vec<(String, String)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: "active",
            enabled: "enabled",
            load: "loaded",
            pid: std::process::id(),
            invocation_reads: 0,
            change_identity: false,
            mode: "reply",
            calls: Vec::new(),
        }
    }
}
struct Unit(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        self.0.lock().unwrap().active.into()
    }
    #[zbus(property)]
    fn unit_file_state(&self) -> String {
        self.0.lock().unwrap().enabled.into()
    }
    #[zbus(property)]
    fn load_state(&self) -> String {
        self.0.lock().unwrap().load.into()
    }
    #[zbus(property, name = "InvocationID")]
    fn invocation(&self) -> Vec<u8> {
        let mut state = self.0.lock().unwrap();
        state.invocation_reads += 1;
        vec![
            if state.change_identity {
                state.invocation_reads as u8
            } else {
                42
            };
            16
        ]
    }
}
struct Service(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> u32 {
        self.0.lock().unwrap().pid
    }
}
struct Manager(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn try_restart_unit(&self, name: &str, mode: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        let behavior = {
            let mut state = self.0.lock().unwrap();
            state.calls.push((name.into(), mode.into()));
            state.mode
        };
        if behavior == "deny" {
            return Err(zbus::fdo::Error::AccessDenied("fixture denied".into()));
        }
        if behavior == "stall" {
            async_io::Timer::after(Duration::from_secs(30)).await;
        }
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }
}

async fn provider(bus: &Bus, state: Arc<Mutex<State>>) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(MANAGER)
        .unwrap()
        .name("org.freedesktop.timesync1")
        .unwrap()
        .serve_at(UNIT, Unit(state.clone()))
        .unwrap()
        .serve_at(UNIT, Service(state.clone()))
        .unwrap()
        .serve_at("/org/freedesktop/systemd1", Manager(state))
        .unwrap()
        .build()
        .await
        .unwrap()
}
fn clock(ms: u64) -> Clock {
    Clock {
        boot_id: BOOT.into(),
        ms,
    }
}
fn fixture() -> (tempfile::TempDir, Schedule, u64) {
    let temp = tempfile::tempdir().unwrap();
    let mut schedule = Schedule::open(&temp.path().join("state"), &clock(1000)).unwrap();
    let id = schedule
        .observe(&clock(1000), true, None)
        .unwrap()
        .active
        .unwrap()
        .id;
    schedule
        .connected(&clock(1000), id, Connection::Ethernet)
        .unwrap();
    (temp, schedule, id)
}
async fn send(bus: &Bus, schedule: &mut Schedule, id: u64) -> Outcome {
    bounded(
        schedule,
        id,
        || Ok(clock(1000)),
        async { Ok(bus.connection().await) },
        std::process::id(),
        unsafe { libc::geteuid() },
    )
    .await
}

#[test]
fn requests_only_the_fixed_running_provider_once_and_does_not_claim_sync_success() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State::default()));
        let _server = provider(&bus, state.clone()).await;
        let (_temp, mut schedule, id) = fixture();
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Requested);
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Unavailable);
        assert_eq!(
            state.lock().unwrap().calls,
            [("systemd-timesyncd.service".into(), "fail".into())]
        );
        let status = schedule.status(&clock(1000)).unwrap();
        assert!(status.active.unwrap().query_requested);
        assert_eq!(status.last_success_generation, None);
        assert_eq!(status.completion, None);
    });
}

#[test]
fn disabled_missing_inactive_or_mismatched_provider_is_not_started() {
    futures_lite::future::block_on(async {
        for case in ["disabled", "masked", "inactive", "not-found", "pid"] {
            let bus = Bus::new();
            let state = Arc::new(Mutex::new(State::default()));
            {
                let mut s = state.lock().unwrap();
                match case {
                    "disabled" | "masked" => s.enabled = case,
                    "inactive" => s.active = "inactive",
                    "not-found" => s.load = "not-found",
                    _ => s.pid += 1,
                }
            }
            let _server = provider(&bus, state.clone()).await;
            let (_temp, mut schedule, id) = fixture();
            assert_eq!(
                send(&bus, &mut schedule, id).await,
                Outcome::Unavailable,
                "{case}"
            );
            assert!(state.lock().unwrap().calls.is_empty());
            assert!(
                !schedule
                    .status(&clock(1000))
                    .unwrap()
                    .active
                    .unwrap()
                    .query_requested
            );
        }
        let bus = Bus::new();
        let (_temp, mut schedule, id) = fixture();
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Unavailable);
    });
}

#[test]
fn a_fake_manager_owner_cannot_receive_a_restart_request() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State::default()));
        let _server = provider(&bus, state.clone()).await;
        for (pid, uid) in [
            (std::process::id() + 1, unsafe { libc::geteuid() }),
            (std::process::id(), unsafe { libc::geteuid() } + 1),
        ] {
            let (_temp, mut schedule, id) = fixture();
            assert_eq!(
                bounded(
                    &mut schedule,
                    id,
                    || Ok(clock(1000)),
                    async { Ok(bus.connection().await) },
                    pid,
                    uid
                )
                .await,
                Outcome::Unavailable
            );
        }
        assert!(state.lock().unwrap().calls.is_empty());
    });
}

#[test]
fn changed_provider_after_durable_claim_is_not_restarted_and_allowance_stays_consumed() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State {
            change_identity: true,
            ..State::default()
        }));
        let _server = provider(&bus, state.clone()).await;
        let (temp, mut schedule, id) = fixture();
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Unavailable);
        assert!(state.lock().unwrap().calls.is_empty());
        assert!(
            schedule
                .status(&clock(1000))
                .unwrap()
                .active
                .unwrap()
                .query_requested
        );
        drop(schedule);
        let mut schedule = Schedule::open(&temp.path().join("state"), &clock(1000)).unwrap();
        state.lock().unwrap().change_identity = false;
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Unavailable);
        assert!(state.lock().unwrap().calls.is_empty());
    });
}

#[test]
fn rejected_method_reply_cannot_replenish_the_provider_allowance() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State {
            mode: "deny",
            ..State::default()
        }));
        let _server = provider(&bus, state.clone()).await;
        let (temp, mut schedule, id) = fixture();
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Uncertain);
        drop(schedule);
        let mut schedule = Schedule::open(&temp.path().join("state"), &clock(1000)).unwrap();
        state.lock().unwrap().mode = "reply";
        assert_eq!(send(&bus, &mut schedule, id).await, Outcome::Unavailable);
        assert_eq!(state.lock().unwrap().calls.len(), 1);
    });
}

#[test]
fn stalled_bus_call_is_cancelled_on_boot_clock_expiry_and_is_never_reissued() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State {
            mode: "stall",
            ..State::default()
        }));
        let _server = provider(&bus, state.clone()).await;
        let (_temp, mut schedule, id) = fixture();
        let now = Arc::new(AtomicU64::new(1000));
        let task = bounded(
            &mut schedule,
            id,
            || Ok(clock(now.load(Ordering::SeqCst))),
            async { Ok(bus.connection().await) },
            std::process::id(),
            unsafe { libc::geteuid() },
        );
        let advance = async {
            for _ in 0..100 {
                if !state.lock().unwrap().calls.is_empty() {
                    break;
                }
                async_io::Timer::after(Duration::from_millis(10)).await;
            }
            assert_eq!(state.lock().unwrap().calls.len(), 1);
            now.store(100_000, Ordering::SeqCst); // sleep counts even with no wall-clock changes
        };
        let started = std::time::Instant::now();
        let (result, ()) = futures_lite::future::zip(task, advance).await;
        assert_eq!(result, Outcome::Uncertain);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(
            schedule
                .status(&clock(100_000))
                .unwrap()
                .active
                .unwrap()
                .query_requested
        );
    });
}

#[test]
fn an_expired_or_cancelled_attempt_dispatches_nothing() {
    futures_lite::future::block_on(async {
        let bus = Bus::new();
        let state = Arc::new(Mutex::new(State::default()));
        let _server = provider(&bus, state.clone()).await;
        let (_temp, mut schedule, id) = fixture();
        assert_eq!(
            bounded(
                &mut schedule,
                id,
                || Ok(clock(121_000)),
                async { Ok(bus.connection().await) },
                std::process::id(),
                unsafe { libc::geteuid() }
            )
            .await,
            Outcome::Unavailable
        );
        schedule.observe(&clock(121_000), false, None).unwrap();
        assert_eq!(
            bounded(
                &mut schedule,
                id,
                || Ok(clock(121_000)),
                async { Ok(bus.connection().await) },
                std::process::id(),
                unsafe { libc::geteuid() }
            )
            .await,
            Outcome::Unavailable
        );
        assert!(state.lock().unwrap().calls.is_empty());
    });
}
