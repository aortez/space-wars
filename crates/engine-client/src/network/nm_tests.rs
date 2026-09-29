use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const WIFI: &str = "/wifi";
const CHECKPOINT: &str = "/checkpoint";
const CANDIDATE: &str = "/candidate";
const ACTIVE: &str = "/active";
#[derive(Clone, Default)]
struct Mock(Arc<Mutex<Vec<String>>>, Arc<std::sync::atomic::AtomicBool>);
impl Mock {
    fn call(&self, call: &str) {
        self.0.lock().unwrap().push(call.into());
    }
}

struct Manager(Mock);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI).unwrap()]
    }
    fn get_permissions(&self) -> HashMap<String, String> {
        [
            "network-control",
            "settings.modify.system",
            "checkpoint-rollback",
            "wifi.scan",
        ]
        .into_iter()
        .map(|suffix| (format!("{SERVICE}.{suffix}"), "yes".into()))
        .collect()
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        seconds: u32,
        flags: u32,
    ) -> OwnedObjectPath {
        assert_eq!(devices, [path(WIFI).unwrap()]);
        assert_eq!(seconds, ROLLBACK_SECONDS);
        assert_eq!(flags, 0);
        self.0.call("checkpoint");
        path(CHECKPOINT).unwrap()
    }
    fn add_and_activate_connection2(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        ap: OwnedObjectPath,
        options: Properties,
    ) -> (OwnedObjectPath, OwnedObjectPath, Properties) {
        assert_eq!(device.as_str(), WIFI);
        assert_eq!(ap.as_str(), "/ap");
        assert_eq!(text(&options, "persist"), "volatile");
        assert!(!options.contains_key("bind-activation"));
        assert_eq!(
            text(&settings["802-11-wireless-security"], "psk"),
            "secret123"
        );
        assert_eq!(bytes(&settings["802-11-wireless"], "ssid"), b"Test network");
        self.0.call("activate-new");
        (
            path(CANDIDATE).unwrap(),
            path(ACTIVE).unwrap(),
            Properties::new(),
        )
    }
    fn activate_connection(
        &self,
        profile: OwnedObjectPath,
        device: OwnedObjectPath,
        ap: OwnedObjectPath,
    ) -> OwnedObjectPath {
        assert_eq!(profile.as_str(), "/saved");
        assert_eq!(device.as_str(), WIFI);
        assert_eq!(ap.as_str(), "/ap");
        self.0.call("activate-saved");
        path(ACTIVE).unwrap()
    }
    fn checkpoint_destroy(&self, checkpoint: OwnedObjectPath) {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        self.0.call("keep");
    }
    fn checkpoint_rollback(&self, checkpoint: OwnedObjectPath) -> HashMap<String, u32> {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        self.0.call("rollback");
        HashMap::from([(
            WIFI.into(),
            u32::from(self.0.1.load(std::sync::atomic::Ordering::Relaxed)),
        )])
    }
}

struct Profile(Mock);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn get_settings(&self) -> Settings {
        let network = super::super::tests::network();
        let mut settings: Settings = connection_settings(&network, Some("not-returned"))
            .into_iter()
            .map(|(key, values)| {
                (
                    key.into(),
                    values
                        .into_iter()
                        .map(|(key, value)| (key.into(), OwnedValue::try_from(value).unwrap()))
                        .collect(),
                )
            })
            .collect();
        settings
            .get_mut("802-11-wireless-security")
            .unwrap()
            .remove("psk");
        settings
    }
    fn update2(&self, settings: Settings, flags: u32, args: Properties) -> Properties {
        assert!(
            settings.is_empty(),
            "preserve NM-owned credentials without requesting secrets"
        );
        assert_eq!(flags, 1);
        assert!(args.is_empty());
        self.0.call("save");
        Properties::new()
    }
    fn delete(&self) {
        self.0.call("delete-own");
    }
}

struct Device;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn managed(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn interface(&self) -> &str {
        "wlan0"
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        100
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        path(ACTIVE).unwrap()
    }
}

struct Wireless(Mock);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path("/ap").unwrap()
    }
    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        ["/ap", "/stronger", "/open", "/wep", "/stale", "/hidden"]
            .into_iter()
            .map(|p| path(p).unwrap())
            .collect()
    }
    fn request_scan(&self, options: Properties) {
        assert!(options.is_empty());
        self.0.call("scan");
    }
}
struct AccessPoint {
    ssid: Vec<u8>,
    flags: u32,
    rsn: u32,
    strength: u8,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.ssid.clone()
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        self.flags
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        self.rsn
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.strength
    }
    #[zbus(property)]
    fn mode(&self) -> u32 {
        2
    }
}
struct Profiles;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Profiles {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path("/saved").unwrap()]
    }
}

#[test]
fn networkmanager_scan_list_deduplicates_and_matches_saved_profiles_without_secrets() {
    let mut bus = Bus(Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap());
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    async_io::block_on(async {
        bounded(Duration::from_secs(10), async {
            let state = Mock::default();
            let mut server = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .name(SERVICE)
                .unwrap()
                .serve_at(ROOT, Manager(state.clone()))
                .unwrap()
                .serve_at(format!("{ROOT}/Settings"), Profiles)
                .unwrap()
                .serve_at("/saved", Profile(state.clone()))
                .unwrap()
                .serve_at(WIFI, Device)
                .unwrap()
                .serve_at(WIFI, Wireless(state.clone()))
                .unwrap();
            for (path, name, flags, rsn, strength) in [
                ("/ap", "Test network", 1, 0x100, 40),
                ("/stronger", "Test network", 1, 0x100, 90),
                ("/open", "Open cafe", 0, 0, 65),
                ("/wep", "Legacy network", 1, 0, 50),
                ("/hidden", "", 1, 0x100, 50),
            ] {
                server = server
                    .serve_at(
                        path,
                        AccessPoint {
                            ssid: name.as_bytes().to_vec(),
                            flags,
                            rsn,
                            strength,
                        },
                    )
                    .unwrap();
            }
            let _server = server.build().await.unwrap();
            let client = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap();
            let backend = NetworkManager { connection: client };
            let list = backend.inventory().await.unwrap();
            assert!(list.can_connect && list.can_scan);
            assert_eq!(list.networks.len(), 3);
            assert_eq!(
                list.networks[0].ap, "/ap",
                "prefer the active AP over a stronger duplicate"
            );
            assert!(list.networks[0].connected);
            assert_eq!(list.networks[0].saved.as_deref(), Some("/saved"));
            assert_eq!(list.networks[1].security, Security::Open);
            assert_eq!(list.networks[2].security, Security::Unsupported);
            backend.scan(&list.devices).await.unwrap();
            assert_eq!(*state.0.lock().unwrap(), ["scan"]);
            Ok(())
        })
        .await
        .unwrap();
    });
}
struct Active;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
}

#[test]
fn networkmanager_wire_protocol_uses_volatile_trials_and_targeted_rollback() {
    let mut bus = Bus(Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("dbus-daemon is required for isolated D-Bus tests"));
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    async_io::block_on(async {
        bounded(Duration::from_secs(10), async {
            let state = Mock::default();
            let _server = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .name(SERVICE)
                .unwrap()
                .serve_at(ROOT, Manager(state.clone()))
                .unwrap()
                .serve_at(CANDIDATE, Profile(state.clone()))
                .unwrap()
                .serve_at(WIFI, Device)
                .unwrap()
                .serve_at(ACTIVE, Active)
                .unwrap()
                .build()
                .await
                .unwrap();
            let client = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap();
            let backend = NetworkManager { connection: client };
            let mut network = super::super::tests::network();
            let mut trial = Trial {
                checkpoint: backend.checkpoint(WIFI).await.unwrap(),
                device: WIFI.into(),
                ..Trial::default()
            };
            backend
                .activate(&network, Some("secret123"), &mut trial)
                .await
                .unwrap();
            assert!(backend.ready(&trial).await.unwrap());
            backend.keep(&trial).await.unwrap();
            assert_eq!(
                *state.0.lock().unwrap(),
                ["checkpoint", "activate-new", "save", "keep"]
            );
            state.0.lock().unwrap().clear();
            backend.rollback(&trial).await.unwrap();
            assert_eq!(*state.0.lock().unwrap(), ["rollback", "delete-own", "keep"]);
            state.0.lock().unwrap().clear();
            state.1.store(true, std::sync::atomic::Ordering::Relaxed);
            assert!(backend.rollback(&trial).await.is_err());
            assert_eq!(
                *state.0.lock().unwrap(),
                ["rollback"],
                "an uncertain outcome must not delete a possibly kept connection"
            );
            state.1.store(false, std::sync::atomic::Ordering::Relaxed);
            state.0.lock().unwrap().clear();
            network.saved = Some("/saved".into());
            trial.created_profile = None;
            backend.activate(&network, None, &mut trial).await.unwrap();
            backend.keep(&trial).await.unwrap();
            backend.rollback(&trial).await.unwrap();
            assert_eq!(
                *state.0.lock().unwrap(),
                ["activate-saved", "keep", "rollback", "keep"]
            );
            Ok(())
        })
        .await
        .unwrap();
    });
}

#[test]
fn access_point_identity_uses_raw_bytes_and_security_not_display_name() {
    assert_eq!(security(0, 0, 0), Security::Open);
    assert_eq!(security(1, 0, 0x100), Security::WpaPsk);
    assert_eq!(security(1, 0, 0x400), Security::Sae);
    assert_eq!(security(1, 0, 0x500), Security::WpaPsk);
    for flags in [0, 0x200, 0x800] {
        assert_eq!(security(1, 0, flags), Security::Unsupported);
    }
    assert_eq!(ssid_name(b"a\nb"), "a�b");
    assert_ne!(
        network_id(WIFI, &[0xff], Security::Open),
        network_id(WIFI, &[0xfe], Security::Open)
    );
    assert_ne!(
        network_id(WIFI, b"abc", Security::Open),
        network_id(WIFI, b"abc", Security::WpaPsk)
    );
}

#[test]
fn transport_errors_do_not_echo_daemon_supplied_secrets() {
    assert!(!explain(zbus::Error::Failure("secret123".into())).contains("secret123"));
}
