//! NetworkManager 1.46-compatible D-Bus transport. Never reads saved secrets,
//! toggles radios, restarts networking, edits existing profiles, or uses sudo.
use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

use super::worker::{Backend, CALL_TIME, ROLLBACK_SECONDS, Trial, bounded};
use super::{Inventory, Network, Security};

const SERVICE: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const PROFILE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
type Properties = HashMap<String, OwnedValue>;
type Settings = HashMap<String, Properties>;

pub(super) struct NetworkManager {
    connection: Connection,
}

// Do not display raw daemon error messages: they can quote connection settings.
fn explain(error: zbus::Error) -> String {
    let denied = matches!(&error, zbus::Error::MethodError(name, _, _)
        if name.as_str().contains("Denied") || name.as_str().contains("Permission") || name.as_str().contains("NotAuthorized"));
    if denied {
        "Wi-Fi changes are not permitted. Use the operating system's network settings or ask an administrator.".into()
    } else {
        "NetworkManager could not complete the request. Check Wi-Fi availability and retry.".into()
    }
}

fn path(value: &str) -> Result<OwnedObjectPath, String> {
    value
        .try_into()
        .map_err(|_| "Invalid NetworkManager object.".into())
}

fn missing_object(error: &zbus::Error) -> bool {
    matches!(error, zbus::Error::MethodError(name, _, _) if matches!(name.as_str(),
        "org.freedesktop.DBus.Error.UnknownObject" | "org.freedesktop.DBus.Error.UnknownMethod" |
        "org.freedesktop.NetworkManager.Settings.InvalidConnection"))
}
fn text(properties: &Properties, name: &str) -> String {
    properties
        .get(name)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or_default()
        .into()
}
fn number(properties: &Properties, name: &str) -> u32 {
    properties
        .get(name)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or_default()
}
fn boolean(properties: &Properties, name: &str) -> bool {
    properties
        .get(name)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn object(properties: &Properties, name: &str) -> String {
    properties
        .get(name)
        .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
        .map(|p| p.as_str().to_owned())
        .unwrap_or_default()
}
fn bytes(properties: &Properties, name: &str) -> Vec<u8> {
    properties
        .get(name)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<u8>::try_from(v).ok())
        .unwrap_or_default()
}

fn security(flags: u32, wpa: u32, rsn: u32) -> Security {
    let keys = wpa | rsn;
    if keys & 0x100 != 0 {
        Security::WpaPsk
    } else if rsn & 0x400 != 0 {
        Security::Sae
    } else if flags & 1 == 0 && keys == 0 {
        Security::Open
    } else {
        Security::Unsupported
    }
}

fn ssid_name(ssid: &[u8]) -> String {
    String::from_utf8_lossy(ssid)
        .chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect()
}

fn network_id(device: &str, ssid: &[u8], security: Security) -> String {
    let hex: String = ssid.iter().map(|b| format!("{b:02x}")).collect();
    format!("{device}:{hex}:{security:?}")
}

impl NetworkManager {
    pub async fn open() -> Result<Self, String> {
        bounded(CALL_TIME, async {
            Ok(Self {
                connection: Connection::system().await.map_err(explain)?,
            })
        })
        .await
    }

    async fn proxy<'a>(&'a self, object: &'a str, interface: &'a str) -> Result<Proxy<'a>, String> {
        Proxy::new(&self.connection, SERVICE, object, interface)
            .await
            .map_err(explain)
    }

    async fn properties(&self, object: &str, interface: &str) -> Result<Properties, String> {
        self.proxy(object, "org.freedesktop.DBus.Properties")
            .await?
            .call("GetAll", &(interface,))
            .await
            .map_err(explain)
    }

    async fn saved(&self) -> Result<Vec<(String, Settings)>, String> {
        let paths: Vec<OwnedObjectPath> = self
            .proxy(&format!("{ROOT}/Settings"), &format!("{SERVICE}.Settings"))
            .await?
            .call("ListConnections", &())
            .await
            .map_err(explain)?;
        let mut saved = Vec::new();
        for id in paths.into_iter().take(128) {
            // GetSettings never includes secrets. A disappearing/inaccessible
            // profile must not prevent the rest of the scan list from appearing.
            if let Ok(settings) = self
                .proxy(id.as_str(), PROFILE)
                .await?
                .call::<_, _, Settings>("GetSettings", &())
                .await
            {
                saved.push((id.to_string(), settings));
            }
        }
        saved.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(saved)
    }
}

impl Backend for NetworkManager {
    async fn inventory(&self) -> Result<Inventory, String> {
        let manager = self.proxy(ROOT, SERVICE).await?;
        let manager_props = self.properties(ROOT, SERVICE).await?;
        if !boolean(&manager_props, "WirelessEnabled")
            || !boolean(&manager_props, "WirelessHardwareEnabled")
        {
            return Ok(Inventory { summary: "Wi-Fi is disabled or hardware-blocked. Enable it in the operating system, then Refresh.".into(), ..Inventory::default() });
        }
        let permissions: HashMap<String, String> =
            manager.call("GetPermissions", &()).await.map_err(explain)?;
        let permitted = |suffix: &str| {
            permissions
                .get(&format!("{SERVICE}.{suffix}"))
                .is_some_and(|v| v == "yes")
        };
        let mut inventory = Inventory {
            can_connect: [
                "network-control",
                "settings.modify.system",
                "checkpoint-rollback",
            ]
            .into_iter()
            .all(permitted),
            can_scan: permitted("wifi.scan"),
            ..Inventory::default()
        };
        let saved = self.saved().await?;
        let devices: Vec<OwnedObjectPath> =
            manager.call("GetDevices", &()).await.map_err(explain)?;
        let mut networks: HashMap<String, Network> = HashMap::new();
        for device in devices.into_iter().take(16) {
            let props = self.properties(device.as_str(), DEVICE).await?;
            if number(&props, "DeviceType") != 2 || !boolean(&props, "Managed") {
                continue;
            }
            if inventory.devices.len() >= 4 {
                break;
            }
            let interface = text(&props, "Interface");
            let device_connected = number(&props, "State") == 100;
            inventory.devices.push(device.to_string());
            let wifi = self.proxy(device.as_str(), WIRELESS).await?;
            let wifi_props = self.properties(device.as_str(), WIRELESS).await?;
            let active = object(&wifi_props, "ActiveAccessPoint");
            let aps: Vec<OwnedObjectPath> = wifi
                .call("GetAllAccessPoints", &())
                .await
                .map_err(explain)?;
            for ap in aps.into_iter().take(256) {
                let Ok(props) = self.properties(ap.as_str(), AP).await else {
                    continue;
                };
                let ssid = bytes(&props, "Ssid");
                if ssid.is_empty() || ssid.len() > 32 || number(&props, "Mode") != 2 {
                    continue;
                }
                let security = security(
                    number(&props, "Flags"),
                    number(&props, "WpaFlags"),
                    number(&props, "RsnFlags"),
                );
                let id = network_id(device.as_str(), &ssid, security);
                let saved = saved
                    .iter()
                    .find(|(_, settings)| matches_profile(settings, &ssid, security, &interface))
                    .map(|(id, _)| id.clone());
                let network = Network {
                    id: id.clone(),
                    device: device.to_string(),
                    interface: interface.clone(),
                    ap: ap.to_string(),
                    name: ssid_name(&ssid),
                    ssid,
                    security,
                    strength: props
                        .get("Strength")
                        .and_then(|v| u8::try_from(v).ok())
                        .unwrap_or(0),
                    connected: ap.as_str() == active && device_connected,
                    saved,
                };
                let replace = networks.get(&id).is_none_or(|old| {
                    (network.connected, network.strength) > (old.connected, old.strength)
                });
                if replace {
                    networks.insert(id, network);
                }
            }
        }
        inventory.networks = networks.into_values().collect();
        inventory.networks.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(b.saved.is_some().cmp(&a.saved.is_some()))
                .then(b.strength.cmp(&a.strength))
                .then(a.id.cmp(&b.id))
        });
        inventory.networks.truncate(64);
        inventory.summary = if inventory.devices.is_empty() {
            "No managed Wi-Fi adapter found.".into()
        } else if let Some(current) = inventory.networks.iter().find(|n| n.connected) {
            format!("Connected: {} · {}", current.name, current.interface)
        } else {
            "Wi-Fi is not connected. Scan, then choose a network.".into()
        };
        if !inventory.can_connect {
            inventory.summary.push_str(
                "\nNetwork changes need operating system permission; use system settings.",
            );
        }
        Ok(inventory)
    }

    async fn scan(&self, devices: &[String]) -> Result<(), String> {
        for device in devices {
            self.proxy(device, WIRELESS)
                .await?
                .call::<_, _, ()>("RequestScan", &(HashMap::<String, Value<'_>>::new(),))
                .await
                .map_err(explain)?;
        }
        Ok(())
    }

    async fn checkpoint(&self, device: &str) -> Result<String, String> {
        // Never destroy someone else's checkpoint or include wired interfaces.
        let cp: OwnedObjectPath = self
            .proxy(ROOT, SERVICE)
            .await?
            .call(
                "CheckpointCreate",
                &(vec![path(device)?], ROLLBACK_SECONDS, 0_u32),
            )
            .await
            .map_err(explain)?;
        Ok(cp.to_string())
    }

    async fn activate(
        &self,
        network: &Network,
        password: Option<&str>,
        trial: &mut Trial,
    ) -> Result<(), String> {
        let manager = self.proxy(ROOT, SERVICE).await?;
        let device = path(&network.device)?;
        let ap = path(&network.ap)?;
        if let Some(saved) = network.saved.as_deref().filter(|_| password.is_none()) {
            let active: OwnedObjectPath = manager
                .call("ActivateConnection", &(path(saved)?, device, ap))
                .await
                .map_err(explain)?;
            trial.active = active.to_string();
        } else {
            let settings = connection_settings(network, password);
            let options = HashMap::from([("persist", Value::from("volatile"))]);
            let (profile, active, _): (OwnedObjectPath, OwnedObjectPath, Properties) = manager
                .call(
                    "AddAndActivateConnection2",
                    &(settings, device, ap, options),
                )
                .await
                .map_err(explain)?;
            trial.created_profile = Some(profile.to_string());
            trial.active = active.to_string();
        }
        Ok(())
    }

    async fn ready(&self, trial: &Trial) -> Result<bool, String> {
        let props = self
            .properties(&trial.active, &format!("{SERVICE}.Connection.Active"))
            .await?;
        match number(&props, "State") {
            2 => {
                let device = self.properties(&trial.device, DEVICE).await?;
                Ok(number(&device, "State") == 100
                    && object(&device, "ActiveConnection") == trial.active)
            }
            3 | 4 => Err("Connection failed. Check the password and signal strength.".into()),
            _ => Ok(false),
        }
    }

    async fn keep(&self, trial: &Trial) -> Result<(), String> {
        if let Some(profile) = &trial.created_profile {
            // Empty settings means keep the daemon's existing settings/secrets.
            // Persisting clears the volatile flag; no GetSecrets round trip.
            let _: Properties = self
                .proxy(profile, PROFILE)
                .await?
                .call("Update2", &(Settings::new(), 1_u32, Properties::new()))
                .await
                .map_err(explain)?;
        }
        self.proxy(ROOT, SERVICE)
            .await?
            .call::<_, _, ()>("CheckpointDestroy", &(path(&trial.checkpoint)?,))
            .await
            .map_err(explain)
    }

    async fn rollback(&self, trial: &Trial) -> Result<(), String> {
        let manager = self.proxy(ROOT, SERVICE).await?;
        let result: Result<HashMap<String, u32>, _> = manager
            .call("CheckpointRollback", &(path(&trial.checkpoint)?,))
            .await;
        let result = result.map_err(explain)?;
        if result.get(&trial.device) != Some(&0) {
            // An unknown rollback outcome may follow a successfully committed
            // Keep whose reply was lost. Never delete that active connection.
            return Err("NetworkManager could not restore the Wi-Fi adapter.".into());
        }
        // Delete only the profile we created, including if a Save reply timed
        // out. Never delete another client's new connections or saved profiles.
        if let Some(profile) = &trial.created_profile {
            bounded(CALL_TIME, async {
                match self
                    .proxy(profile, PROFILE)
                    .await?
                    .call::<_, _, ()>("Delete", &())
                    .await
                {
                    Ok(()) => Ok(()),
                    Err(error) if missing_object(&error) => Ok(()),
                    Err(error) => Err(explain(error)),
                }
            })
            .await?;
        }
        // Tolerate daemon versions which already remove a rolled-back
        // checkpoint. Never destroy any other transaction's checkpoint.
        match manager
            .call::<_, _, ()>("CheckpointDestroy", &(path(&trial.checkpoint)?,))
            .await
        {
            Ok(()) => {}
            Err(error) if missing_object(&error) => {}
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.freedesktop.NetworkManager.InvalidArguments" => {}
            Err(error) => return Err(explain(error)),
        }
        // Rollback restores synchronously but does not promise association/DHCP.
        Ok(())
    }
}

fn matches_profile(settings: &Settings, ssid: &[u8], security: Security, interface: &str) -> bool {
    let Some(connection) = settings.get("connection") else {
        return false;
    };
    let Some(wifi) = settings.get("802-11-wireless") else {
        return false;
    };
    if text(connection, "type") != "802-11-wireless" || bytes(wifi, "ssid") != ssid {
        return false;
    }
    let binding = text(connection, "interface-name");
    if !binding.is_empty() && binding != interface {
        return false;
    }
    let key = settings
        .get("802-11-wireless-security")
        .map(|s| text(s, "key-mgmt"));
    matches!(
        (security, key.as_deref()),
        (Security::Open, None) | (Security::WpaPsk, Some("wpa-psk")) | (Security::Sae, Some("sae"))
    )
}

fn connection_settings<'a>(
    network: &'a Network,
    password: Option<&'a str>,
) -> HashMap<&'static str, HashMap<&'static str, Value<'a>>> {
    let mut settings = HashMap::from([
        (
            "connection",
            HashMap::from([
                ("id", Value::from(format!("Space-Wars {}", network.name))),
                ("type", Value::from("802-11-wireless")),
                ("autoconnect", Value::from(true)),
            ]),
        ),
        (
            "802-11-wireless",
            HashMap::from([
                ("ssid", Value::from(network.ssid.clone())),
                ("mode", Value::from("infrastructure")),
            ]),
        ),
        ("ipv4", HashMap::from([("method", Value::from("auto"))])),
        ("ipv6", HashMap::from([("method", Value::from("auto"))])),
    ]);
    if network.security != Security::Open {
        settings.insert(
            "802-11-wireless-security",
            HashMap::from([
                (
                    "key-mgmt",
                    Value::from(if network.security == Security::Sae {
                        "sae"
                    } else {
                        "wpa-psk"
                    }),
                ),
                ("psk", Value::from(password.unwrap_or_default())),
                ("psk-flags", Value::from(0_u32)),
            ]),
        );
    }
    settings
}

#[cfg(test)]
#[path = "nm_tests.rs"]
mod tests;
