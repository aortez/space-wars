//! Linux BlueZ transport. Uses a private D-Bus connection and application agent,
//! not the system default agent, adapter power, or global discoverability.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

use super::worker::{Backend, CALL_TIME, bounded};
use super::{Action, Device, Inventory};

const AGENT: &str = "/org/spacewars/ControllerPairing";
const HID: &str = "00001124-0000-1000-8000-00805f9b34fb";
const HOGP: &str = "00001812-0000-1000-8000-00805f9b34fb";

pub(super) struct Bluez {
    connection: Connection,
    selected: Arc<Mutex<Option<String>>>,
}

fn explain(error: impl std::fmt::Display) -> String {
    format!("Bluetooth: {error}. Check pairing mode and try again.")
}

impl Bluez {
    pub async fn open() -> Result<Self, String> {
        bounded(
            CALL_TIME,
            Self::open_on(zbus::connection::Builder::system().map_err(explain)?),
        )
        .await
    }

    async fn open_on(builder: zbus::connection::Builder<'_>) -> Result<Self, String> {
        let selected = Arc::new(Mutex::new(None));
        let connection = builder
            .serve_at(
                AGENT,
                PairingAgent {
                    selected: Arc::clone(&selected),
                },
            )
            .map_err(explain)?
            .build()
            .await
            .map_err(explain)?;
        let manager = Proxy::new(
            &connection,
            "org.bluez",
            "/org/bluez",
            "org.bluez.AgentManager1",
        )
        .await
        .map_err(explain)?;
        manager
            .call::<_, _, ()>(
                "RegisterAgent",
                &(OwnedObjectPath::try_from(AGENT).unwrap(), "NoInputNoOutput"),
            )
            .await
            .map_err(explain)?;
        Ok(Self {
            connection,
            selected,
        })
    }

    async fn proxy<'a>(&'a self, path: &'a str, interface: &'a str) -> Result<Proxy<'a>, String> {
        Proxy::new(&self.connection, "org.bluez", path, interface)
            .await
            .map_err(explain)
    }
}

async fn connect(proxy: &Proxy<'_>) -> Result<(), String> {
    // Connected describes the Bluetooth link, which Pair may establish before
    // HID is connected. Always ask BlueZ to connect the remaining profiles.
    // Controllers that already established HID can return AlreadyConnected.
    match proxy.call::<_, _, ()>("Connect", &()).await {
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.bluez.Error.AlreadyConnected" =>
        {
            Ok(())
        }
        result => result.map_err(explain),
    }
}

impl Backend for Bluez {
    async fn inventory(&self) -> Result<Inventory, String> {
        let manager = zbus::fdo::ObjectManagerProxy::builder(&self.connection)
            .destination("org.bluez")
            .map_err(explain)?
            .path("/")
            .map_err(explain)?
            .build()
            .await
            .map_err(explain)?;
        let objects = manager.get_managed_objects().await.map_err(explain)?;
        Ok(decode_inventory(objects))
    }

    async fn start_scan(&self, adapter: &str) -> Result<(), String> {
        let adapter = self.proxy(adapter, "org.bluez.Adapter1").await?;
        let filter: HashMap<&str, Value<'_>> = HashMap::from([
            ("Transport", Value::from("auto")),
            ("DuplicateData", Value::from(false)),
        ]);
        adapter
            .call::<_, _, ()>("SetDiscoveryFilter", &(filter,))
            .await
            .map_err(explain)?;
        adapter
            .call::<_, _, ()>("StartDiscovery", &())
            .await
            .map_err(explain)
    }

    async fn stop_scan(&self, adapter: &str) -> Result<(), String> {
        self.proxy(adapter, "org.bluez.Adapter1")
            .await?
            .call::<_, _, ()>("StopDiscovery", &())
            .await
            .map_err(explain)
    }

    async fn act(&self, action: &Action, device: &Device) -> Result<(), String> {
        let proxy = self.proxy(&device.id, "org.bluez.Device1").await?;
        match action {
            Action::Pair => {
                *self.selected.lock().unwrap() = Some(device.id.clone());
                let _guard = SelectionGuard(&self.selected);
                proxy.call::<_, _, ()>("Pair", &()).await.map_err(explain)?;
                proxy.set_property("Trusted", true).await.map_err(explain)?;
                connect(&proxy).await?;
            }
            Action::Connect => {
                proxy.set_property("Trusted", true).await.map_err(explain)?;
                connect(&proxy).await?;
            }
            Action::Disconnect => proxy
                .call::<_, _, ()>("Disconnect", &())
                .await
                .map_err(explain)?,
            Action::Forget => {
                self.proxy(&device.adapter, "org.bluez.Adapter1")
                    .await?
                    .call::<_, _, ()>(
                        "RemoveDevice",
                        &(OwnedObjectPath::try_from(device.id.as_str()).map_err(explain)?,),
                    )
                    .await
                    .map_err(explain)?;
            }
        }
        Ok(())
    }

    async fn cancel(&self, action: &Action, device: &Device) {
        // No authorization remains after the operation future is dropped.
        *self.selected.lock().unwrap() = None;
        if let Ok(proxy) = self.proxy(&device.id, "org.bluez.Device1").await {
            if matches!(action, Action::Pair) {
                let _ = bounded(CALL_TIME, async {
                    proxy
                        .call::<_, _, ()>("CancelPairing", &())
                        .await
                        .map_err(explain)
                })
                .await;
            }
            if matches!(action, Action::Pair | Action::Connect) && !device.connected {
                let _ = bounded(CALL_TIME, async {
                    proxy
                        .call::<_, _, ()>("Disconnect", &())
                        .await
                        .map_err(explain)
                })
                .await;
            }
        }
    }
}

fn text(properties: &HashMap<String, OwnedValue>, key: &str) -> String {
    properties
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect()
}

fn boolean(properties: &HashMap<String, OwnedValue>, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn game_controller(properties: &HashMap<String, OwnedValue>) -> bool {
    let class = properties
        .get("Class")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    let appearance = properties
        .get("Appearance")
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0);
    text(properties, "Icon") == "input-gaming"
        || matches!(class & 0x1ffc, 0x0504 | 0x0508)
        || matches!(appearance, 0x03c3 | 0x03c4)
}

fn decode_inventory(objects: zbus::fdo::ManagedObjects) -> Inventory {
    let mut adapters = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            interfaces
                .get("org.bluez.Adapter1")
                .filter(|properties| boolean(properties, "Powered"))
                .map(|_| path.to_string())
        })
        .collect::<Vec<_>>();
    adapters.sort();
    let mut devices = Vec::new();
    for (path, interfaces) in &objects {
        let Some(properties) = interfaces.get("org.bluez.Device1") else {
            continue;
        };
        if !game_controller(properties) {
            continue;
        }
        let Some(adapter) = properties
            .get("Adapter")
            .and_then(|v| <&zbus::zvariant::ObjectPath<'_>>::try_from(v).ok())
        else {
            continue;
        };
        if !adapters.iter().any(|path| path == adapter.as_str()) {
            continue;
        }
        let name = text(properties, "Alias");
        devices.push(Device {
            id: path.to_string(),
            adapter: adapter.to_string(),
            name: if name.is_empty() {
                "Game controller".into()
            } else {
                name
            },
            address: text(properties, "Address"),
            paired: boolean(properties, "Paired"),
            connected: boolean(properties, "Connected"),
        });
    }
    Inventory {
        adapter: adapters.into_iter().next(),
        devices,
    }
}

struct SelectionGuard<'a>(&'a Mutex<Option<String>>);
impl Drop for SelectionGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}

struct PairingAgent {
    selected: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    Rejected(String),
}

impl PairingAgent {
    fn authorize(&self, device: &OwnedObjectPath) -> Result<(), AgentError> {
        if self.selected.lock().unwrap().as_deref() == Some(device.as_str()) {
            Ok(())
        } else {
            Err(AgentError::Rejected(
                "Only the selected controller may pair.".into(),
            ))
        }
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl PairingAgent {
    fn release(&self) {
        *self.selected.lock().unwrap() = None;
    }
    fn cancel(&self) {
        *self.selected.lock().unwrap() = None;
    }
    fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        self.authorize(&device)
    }
    fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        _passkey: u32,
    ) -> Result<(), AgentError> {
        self.authorize(&device)
    }
    fn authorize_service(&self, device: OwnedObjectPath, uuid: &str) -> Result<(), AgentError> {
        self.authorize(&device)?;
        if matches!(uuid, HID | HOGP) {
            Ok(())
        } else {
            Err(AgentError::Rejected(
                "Only controller HID services are authorized.".into(),
            ))
        }
    }
    fn request_pin_code(&self, _device: OwnedObjectPath) -> Result<String, AgentError> {
        Err(AgentError::Rejected(
            "PIN-entry controllers are not supported by this setup flow.".into(),
        ))
    }
    fn request_passkey(&self, _device: OwnedObjectPath) -> Result<u32, AgentError> {
        Err(AgentError::Rejected(
            "Passkey-entry controllers are not supported by this setup flow.".into(),
        ))
    }
}

#[cfg(test)]
#[path = "bluez_tests.rs"]
mod tests;
