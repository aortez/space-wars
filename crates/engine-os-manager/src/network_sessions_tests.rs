use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const HEADER: &str =
    " sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n";
const UNIT_PATH: &str = "/org/freedesktop/systemd1/unit/fixture_2eservice";
const SOCKET_PATH: &str = "/org/freedesktop/systemd1/unit/sshd_2esocket";

fn row(local: &str, remote: &str, state: &str) -> String {
    format!("0: {local} {remote} {state} 00000000:00000000 00:00000000 00000000 1000 0 1\n")
}

#[test]
fn kernel_tcp_evidence_covers_preauth_established_closing_ipv6_and_other_ports() {
    let mut input = HEADER.to_owned();
    input.push_str(&row("00000000:0016", "00000000:0000", "0A"));
    input.push_str(&row("0100007F:1234", "0100007F:1235", "01"));
    input.push_str(&row("0101A8C0:0016", "0201A8C0:1235", "01"));
    input.push_str(&row("0101A8C0:0016", "0201A8C0:1236", "03"));
    input.push_str(&row("0101A8C0:0016", "0201A8C0:1237", "08"));
    input.push_str(&row("0101A8C0:0016", "0201A8C0:1238", "06"));
    input.push_str(&row("0101A8C0:2345", "0201A8C0:01BB", "01"));
    input.push_str(&row("00000000:1F90", "00000000:0000", "0A"));
    let evidence = tcp(&input, false).unwrap();
    assert_eq!(
        (evidence.active, evidence.ssh, evidence.other_listeners),
        (4, 3, 1)
    );
    let mut input = HEADER.to_owned();
    input.push_str(&row(
        "00000000000000000000000000000000:0016",
        "00000000000000000000000000000000:0000",
        "0A",
    ));
    input.push_str(&row(
        "00000000000000000000000001000000:1234",
        "00000000000000000000000001000000:1235",
        "01",
    ));
    input.push_str(&row(
        "0000000000000000FFFF00000100007F:1234",
        "0000000000000000FFFF00000100007F:1235",
        "01",
    ));
    input.push_str(&row(
        "0000000000000000FFFF00000101A8C0:0016",
        "0000000000000000FFFF00000201A8C0:1235",
        "01",
    ));
    let evidence = tcp(&input, true).unwrap();
    assert_eq!(
        (evidence.active, evidence.ssh, evidence.other_listeners),
        (1, 1, 0)
    );
    assert_eq!(tcp(HEADER, false).unwrap().active, 0);
}

#[test]
fn truncated_malformed_or_oversized_tcp_tables_cannot_report_idle() {
    for input in [
        "".into(),
        "garbage\n".into(),
        format!("{HEADER}0: 0101A8C0:0016 0201A8C0:1235 0A\n"),
        format!("{HEADER}{}", row("not-an-address", "0201A8C0:1235", "01")),
        format!("{HEADER}{}", row("0101A8C0:0016", "0201A8C0:1235", "FF")),
        format!(
            "{HEADER}{}",
            row("00000000:0016", "00000000:0000", "0A").repeat(2049)
        ),
        " ".repeat(512 * 1024 + 1),
    ] {
        assert!(tcp(&input, false).is_err());
    }
}

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
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Default)]
struct Fixture {
    hook: AtomicBool,
    session: AtomicBool,
    daemon: AtomicBool,
    unstable: AtomicBool,
    stale: AtomicBool,
    slow: AtomicBool,
    reads: AtomicU32,
    starts: AtomicU32,
}
struct Manager(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn load_unit(&self, name: &str) -> OwnedObjectPath {
        assert_eq!(name, TEMPLATE);
        UNIT_PATH.try_into().unwrap()
    }
    fn get_unit(&self, name: &str) -> OwnedObjectPath {
        assert_eq!(name, "sshd.socket");
        SOCKET_PATH.try_into().unwrap()
    }
    async fn list_units(&self) -> Vec<UnitRow> {
        if self.0.slow.load(Ordering::SeqCst) {
            async_io::Timer::after(Duration::from_secs(2)).await;
        }
        let read = self.0.reads.fetch_add(1, Ordering::SeqCst);
        let active = self.0.session.load(Ordering::SeqCst)
            || (self.0.unstable.load(Ordering::SeqCst) && read.is_multiple_of(2));
        let row = |name: &str, active: bool| {
            (
                name.into(),
                "fixture".into(),
                "loaded".into(),
                if active { "active" } else { "inactive" }.into(),
                "running".into(),
                "".into(),
                UNIT_PATH.try_into().unwrap(),
                0,
                "".into(),
                "/".try_into().unwrap(),
            )
        };
        vec![
            row("sshd@fixture.service", active),
            row("sshd.service", self.0.daemon.load(Ordering::SeqCst)),
        ]
    }
    fn start_unit(&self, _unit: &str, _mode: &str) -> OwnedObjectPath {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        "/job/1".try_into().unwrap()
    }
}
struct Unit(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn load_state(&self) -> &str {
        "loaded"
    }
    #[zbus(property)]
    fn active_state(&self) -> &str {
        "active"
    }
    #[zbus(property)]
    fn need_daemon_reload(&self) -> bool {
        self.0.stale.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn joins_namespace_of(&self) -> Vec<String> {
        vec![]
    }
}
fn command(path: &str, args: &[&str], ignore: bool) -> ExecCommand {
    (
        path.into(),
        args.iter().map(|value| (*value).into()).collect(),
        ignore,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
}
struct Service(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property)]
    fn private_network(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn network_namespace_path(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn root_directory(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn root_image(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn user(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn group(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn standard_input(&self) -> &str {
        "socket"
    }
    #[zbus(property)]
    fn environment(&self) -> Vec<String> {
        vec!["SSHD_OPTS=".into()]
    }
    #[zbus(property)]
    fn environment_files(&self) -> Vec<(String, bool)> {
        vec![("/etc/default/ssh".into(), true)]
    }
    #[zbus(property)]
    fn pass_environment(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn unset_environment(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn exec_start(&self) -> Vec<ExecCommand> {
        vec![command(
            "/usr/sbin/sshd",
            &["/usr/sbin/sshd", "-i", "$SSHD_OPTS"],
            true,
        )]
    }
    #[zbus(property)]
    fn exec_start_pre(&self) -> Vec<ExecCommand> {
        if self.0.hook.load(Ordering::SeqCst) {
            vec![command(
                "/usr/bin/engine-os-manager",
                &["/usr/bin/engine-os-manager", "ssh-network-admit"],
                false,
            )]
        } else {
            vec![]
        }
    }
    #[zbus(property)]
    fn exec_start_post(&self) -> Vec<ExecCommand> {
        vec![]
    }
    #[zbus(property)]
    fn exec_condition(&self) -> Vec<ExecCommand> {
        vec![]
    }
}
struct Socket;
#[zbus::interface(name = "org.freedesktop.systemd1.Socket")]
impl Socket {
    #[zbus(property)]
    fn accept(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn listen(&self) -> Vec<(String, String)> {
        vec![("Stream".into(), "[::]:22".into())]
    }
    #[zbus(property)]
    fn private_network(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn network_namespace_path(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn root_directory(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn root_image(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn user(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn group(&self) -> &str {
        ""
    }
}

#[test]
fn private_systemd_requires_complete_supported_protection_and_never_starts_ssh() {
    let bus = PrivateBus::new();
    futures_lite::future::block_on(async {
        let fixture = Arc::new(Fixture::default());
        let server = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name(NAME)
            .unwrap()
            .serve_at(ROOT, Manager(fixture.clone()))
            .unwrap()
            .serve_at(UNIT_PATH, Unit(fixture.clone()))
            .unwrap()
            .serve_at(UNIT_PATH, Service(fixture.clone()))
            .unwrap()
            .serve_at(SOCKET_PATH, Unit(fixture.clone()))
            .unwrap()
            .serve_at(SOCKET_PATH, Socket)
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let uid = unsafe { libc::geteuid() };
        let pid = std::process::id();
        let get = |namespace, defaults_valid| {
            connection_observation(
                &client,
                uid,
                pid,
                Tcp::default(),
                Tcp::default(),
                namespace,
                defaults_valid,
            )
        };
        let legacy = get(true, true).await.unwrap();
        assert!(legacy.configuration_supported && legacy.socket_active);
        assert!(!legacy.observation_complete && legacy.ssh_activity.is_none());
        fixture.hook.store(true, Ordering::SeqCst);
        let ready = get(true, true).await.unwrap();
        assert!(ready.observation_complete);
        assert_eq!(ready.ssh_activity, Some(false));
        for (namespace, defaults_valid) in [(false, true), (true, false)] {
            assert_eq!(
                get(namespace, defaults_valid).await.unwrap().ssh_activity,
                None
            );
        }
        fixture.stale.store(true, Ordering::SeqCst);
        assert_eq!(get(true, true).await.unwrap().ssh_activity, None);
        fixture.stale.store(false, Ordering::SeqCst);
        fixture.hook.store(false, Ordering::SeqCst);
        fixture.session.store(true, Ordering::SeqCst);
        assert_eq!(get(true, true).await.unwrap().ssh_activity, Some(true));
        fixture.session.store(false, Ordering::SeqCst);
        fixture.daemon.store(true, Ordering::SeqCst);
        let daemon = get(true, true).await.unwrap();
        assert!(!daemon.configuration_supported);
        assert_eq!(daemon.ssh_activity, Some(true));
        fixture.daemon.store(false, Ordering::SeqCst);
        let tcp_active = connection_observation(
            &client,
            uid,
            pid,
            Tcp {
                active: 1,
                ..Default::default()
            },
            Tcp::default(),
            true,
            true,
        )
        .await
        .unwrap();
        assert_eq!(tcp_active.ssh_activity, Some(true));
        fixture.hook.store(true, Ordering::SeqCst);
        let foreign = connection_observation(
            &client,
            uid,
            pid,
            Tcp {
                other_listeners: 1,
                ..Default::default()
            },
            Tcp::default(),
            true,
            true,
        )
        .await
        .unwrap();
        assert_eq!(foreign.ssh_activity, None);
        assert!(
            connection_observation(
                &client,
                uid.wrapping_add(1),
                pid,
                Tcp::default(),
                Tcp::default(),
                true,
                true
            )
            .await
            .is_err()
        );
        assert!(
            connection_observation(
                &client,
                uid,
                pid + 1,
                Tcp::default(),
                Tcp::default(),
                true,
                true
            )
            .await
            .is_err()
        );
        fixture.unstable.store(true, Ordering::SeqCst);
        assert!(get(true, true).await.is_err());
        fixture.unstable.store(false, Ordering::SeqCst);
        fixture.slow.store(true, Ordering::SeqCst);
        assert!(
            crate::network_observation::bounded(Duration::from_millis(30), get(true, true))
                .await
                .is_err()
        );
        assert_eq!(fixture.starts.load(Ordering::SeqCst), 0);
        drop(server);
    });
}
