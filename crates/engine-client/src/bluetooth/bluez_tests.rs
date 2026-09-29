//! A private bus and fake BlueZ exercise the real wire protocol without a
//! Bluetooth adapter, the host's system bus, or any physical controller.
use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

const ADAPTER: &str = "/org/bluez/hci0";
const PAD: &str = "/org/bluez/hci0/dev_00_11_22_33_44_55";

struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Default)]
struct State {
    calls: Vec<String>,
    paired: bool,
    link_connected: bool,
    input_connected: bool,
    connect_fails: bool,
    trusted: bool,
}
#[derive(Clone, Default)]
struct Mock(Arc<Mutex<State>>);

struct Adapter(Mock);
#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        true
    }
    fn set_discovery_filter(&self, filter: HashMap<String, OwnedValue>) {
        assert_eq!(text(&filter, "Transport"), "auto");
        assert!(!boolean(&filter, "DuplicateData"));
        self.0.0.lock().unwrap().calls.push("filter".into());
    }
    fn start_discovery(&self) {
        self.0.0.lock().unwrap().calls.push("scan".into());
    }
    fn stop_discovery(&self) {
        self.0.0.lock().unwrap().calls.push("stop".into());
    }
    fn remove_device(&self, device: OwnedObjectPath) {
        assert_eq!(device.as_str(), PAD);
        self.0.0.lock().unwrap().calls.push("forget".into());
    }
}

struct Pad(Mock);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum ConnectError {
    AlreadyConnected(String),
    Failed(String),
}

#[zbus::interface(name = "org.bluez.Device1")]
impl Pad {
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        ADAPTER.try_into().unwrap()
    }
    #[zbus(property)]
    fn alias(&self) -> &str {
        "Test\n pad"
    }
    #[zbus(property)]
    fn address(&self) -> &str {
        "00:11:22:33:44:55"
    }
    #[zbus(property)]
    fn icon(&self) -> &str {
        "input-gaming"
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        self.0.0.lock().unwrap().paired
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.0.lock().unwrap().link_connected
    }
    #[zbus(property)]
    fn trusted(&self) -> bool {
        self.0.0.lock().unwrap().trusted
    }
    #[zbus(property)]
    fn set_trusted(&mut self, value: bool) {
        let mut state = self.0.0.lock().unwrap();
        state.trusted = value;
        state.calls.push("trust".into());
    }
    fn pair(&self) {
        let mut state = self.0.0.lock().unwrap();
        state.paired = true;
        // Pairing establishes the Bluetooth link, not necessarily HID. Model
        // the intermediate state that must not be mistaken for usable input.
        state.link_connected = true;
        state.calls.push("pair".into());
    }
    fn connect(&self) -> Result<(), ConnectError> {
        let mut state = self.0.0.lock().unwrap();
        state.calls.push("connect".into());
        if state.connect_fails {
            return Err(ConnectError::Failed("input profile unavailable".into()));
        }
        if state.input_connected {
            return Err(ConnectError::AlreadyConnected("HID is connected".into()));
        }
        state.link_connected = true;
        state.input_connected = true;
        Ok(())
    }
    fn disconnect(&self) {
        let mut state = self.0.0.lock().unwrap();
        state.link_connected = false;
        state.input_connected = false;
        state.calls.push("disconnect".into());
    }
    fn cancel_pairing(&self) {
        self.0.0.lock().unwrap().calls.push("cancel".into());
    }
}

struct Manager(Mock);
#[zbus::interface(name = "org.bluez.AgentManager1")]
impl Manager {
    fn register_agent(&self, path: OwnedObjectPath, capability: &str) {
        assert_eq!(path.as_str(), AGENT);
        assert_eq!(capability, "NoInputNoOutput");
        self.0.0.lock().unwrap().calls.push("agent".into());
    }
}

#[test]
fn native_transport_uses_private_agent_selected_device_and_local_discovery() {
    let child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("install dbus-daemon to run Bluetooth protocol tests");
    let mut bus = Bus(child);
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let address = address.trim();
    async_io::block_on(async {
        bounded(std::time::Duration::from_secs(15), async {
            let state = Mock::default();
            let _server = zbus::connection::Builder::address(address)
                .unwrap()
                .name("org.bluez")
                .unwrap()
                .serve_at("/", zbus::fdo::ObjectManager)
                .unwrap()
                .serve_at("/org/bluez", Manager(state.clone()))
                .unwrap()
                .serve_at(ADAPTER, Adapter(state.clone()))
                .unwrap()
                .serve_at(PAD, Pad(state.clone()))
                .unwrap()
                .build()
                .await
                .unwrap();
            let backend =
                Bluez::open_on(zbus::connection::Builder::address(address).unwrap()).await?;
            let inventory = backend.inventory().await?;
            assert_eq!(inventory.adapter.as_deref(), Some(ADAPTER));
            assert_eq!(inventory.devices.len(), 1);
            let device = &inventory.devices[0];
            assert_eq!(device.id, PAD);
            assert_eq!(device.name, "Test pad");
            assert!(!device.paired);
            backend.start_scan(ADAPTER).await?;
            backend.stop_scan(ADAPTER).await?;
            backend.act(&Action::Pair, device).await?;
            assert!(backend.selected.lock().unwrap().is_none());
            assert!(
                state.0.lock().unwrap().input_connected,
                "Pair must connect the input profile even when the Bluetooth link is already connected"
            );
            let connected = backend.inventory().await?;
            assert!(connected.devices[0].paired && connected.devices[0].connected);
            // Connect must reach BlueZ even with Connected=true. Its exact
            // AlreadyConnected reply is success, not a reason to reconnect.
            backend.act(&Action::Connect, &connected.devices[0]).await?;
            assert_eq!(
                state
                    .0
                    .lock()
                    .unwrap()
                    .calls
                    .iter()
                    .filter(|c| c.as_str() == "connect")
                    .count(),
                2,
                "BlueZ must decide whether profiles still need connecting"
            );
            {
                let mut state = state.0.lock().unwrap();
                state.input_connected = false;
                state.connect_fails = true;
            }
            let error = backend
                .act(&Action::Connect, &connected.devices[0])
                .await
                .expect_err("a connected link must not mask a failed input connection");
            assert!(error.contains("org.bluez.Error.Failed"), "{error}");
            assert!(!state.0.lock().unwrap().input_connected);
            backend.act(&Action::Disconnect, device).await?;
            backend.cancel(&Action::Pair, device).await;
            backend.act(&Action::Forget, device).await?;
            assert_eq!(
                state.0.lock().unwrap().calls,
                [
                    "agent",
                    "filter",
                    "scan",
                    "stop",
                    "pair",
                    "trust",
                    "connect",
                    "trust",
                    "connect",
                    "trust",
                    "connect",
                    "disconnect",
                    "cancel",
                    "disconnect",
                    "forget",
                ]
            );
            // The fake exposes no power/discoverability/default-agent setters:
            // any accidental global radio mutation fails this protocol test.
            Ok(())
        })
        .await
        .unwrap();
    });
}

#[test]
fn agent_authorizes_only_selected_controller_hid_and_revokes_on_drop() {
    let selected = Arc::new(Mutex::new(None));
    let agent = PairingAgent {
        selected: Arc::clone(&selected),
    };
    let pad: OwnedObjectPath = PAD.try_into().unwrap();
    assert!(agent.request_authorization(pad.clone()).is_err());
    *selected.lock().unwrap() = Some(PAD.into());
    {
        let _guard = SelectionGuard(&selected);
        assert!(agent.request_authorization(pad.clone()).is_ok());
        assert!(agent.request_confirmation(pad.clone(), 123456).is_ok());
        assert!(agent.authorize_service(pad.clone(), HID).is_ok());
        assert!(agent.authorize_service(pad.clone(), HOGP).is_ok());
        assert!(
            agent
                .authorize_service(pad.clone(), "unrelated-service")
                .is_err()
        );
        assert!(
            agent
                .request_authorization("/another/controller".try_into().unwrap())
                .is_err()
        );
        assert!(agent.request_pin_code(pad.clone()).is_err());
        assert!(agent.request_passkey(pad.clone()).is_err());
    }
    assert!(agent.request_authorization(pad).is_err());
}

#[test]
fn discovery_recognizes_classic_and_ble_gamepads_but_not_keyboards_or_audio() {
    let mut props = HashMap::new();
    assert!(!game_controller(&props));
    props.insert("Class".into(), OwnedValue::from(0x2508u32));
    assert!(game_controller(&props));
    props.insert("Class".into(), OwnedValue::from(0x0540u32));
    assert!(!game_controller(&props));
    props.insert("Appearance".into(), OwnedValue::from(0x03c4u16));
    assert!(game_controller(&props));
    props.insert("Appearance".into(), OwnedValue::from(0x03c1u16));
    props.insert("Class".into(), OwnedValue::from(0x240404u32));
    assert!(!game_controller(&props));
}
