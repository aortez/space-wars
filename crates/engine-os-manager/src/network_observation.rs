//! Read-only, uncached NetworkManager observations. This does not establish
//! session safety or permission to change the radio; it never scans/connects.
use crate::{Error, Result};
use engine_ota::Sha256;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NAME: &str = "org.freedesktop.NetworkManager";
const OBJECT: &str = "/org/freedesktop/NetworkManager";
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Observation {
    pub schema: u32,
    pub bus_owner: String,
    pub pid: u32,
    pub networking_enabled: bool,
    pub wireless_enabled: bool,
    pub wireless_hardware_enabled: bool,
    pub startup: bool,
    pub checkpoint_count: usize,
    /// NetworkManager's overall state, not a claim that Wi-Fi is connected.
    pub state: u32,
    /// Digest of sorted profile object paths and per-profile VersionId values.
    pub profile_revision: Sha256,
    pub profile_count: usize,
    /// An activated primary Ethernet connection; not proof of Internet access.
    pub ethernet_available: bool,
    /// An activated primary Wi-Fi connection; not proof of Internet access.
    pub wireless_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Properties {
    networking: bool,
    wireless: bool,
    hardware: bool,
    startup: bool,
    checkpoints: Vec<OwnedObjectPath>,
    state: u32,
    primary_type: String,
}

fn parse(mut values: HashMap<String, OwnedValue>) -> Result<Properties> {
    if values.len() > 128 {
        return Err(invalid("NetworkManager property count exceeds limit"));
    }
    let flag = |values: &mut HashMap<String, OwnedValue>, name: &str| -> Result<bool> {
        values
            .remove(name)
            .ok_or_else(|| invalid("NetworkManager omitted a required radio property"))?
            .try_into()
            .map_err(|_| invalid("NetworkManager radio property has the wrong type"))
    };
    let networking = flag(&mut values, "NetworkingEnabled")?;
    let wireless = flag(&mut values, "WirelessEnabled")?;
    let hardware = flag(&mut values, "WirelessHardwareEnabled")?;
    let startup = flag(&mut values, "Startup")?;
    let mut checkpoints: Vec<OwnedObjectPath> = values
        .remove("Checkpoints")
        .ok_or_else(|| invalid("NetworkManager omitted checkpoint information"))?
        .try_into()
        .map_err(|_| invalid("NetworkManager checkpoint property has the wrong type"))?;
    if checkpoints.len() > 32
        || checkpoints.iter().any(|p| {
            p.as_str().len() > 256
                || !p
                    .as_str()
                    .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
        })
    {
        return Err(invalid(
            "NetworkManager checkpoint inventory exceeds bounds",
        ));
    }
    checkpoints.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    if checkpoints.windows(2).any(|v| v[0] == v[1]) {
        return Err(invalid("duplicate NetworkManager checkpoint"));
    }
    let state: u32 = values
        .remove("State")
        .ok_or_else(|| invalid("NetworkManager omitted its state"))?
        .try_into()
        .map_err(|_| invalid("NetworkManager state has the wrong type"))?;
    if ![0, 10, 20, 30, 40, 50, 60, 70].contains(&state) {
        return Err(invalid("unsupported NetworkManager state"));
    }
    let primary_type: String = values
        .remove("PrimaryConnectionType")
        .ok_or_else(|| invalid("NetworkManager omitted its primary connection type"))?
        .try_into()
        .map_err(|_| invalid("NetworkManager primary connection type is invalid"))?;
    if primary_type.len() > 64
        || !primary_type
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid(
            "NetworkManager primary connection type exceeds bounds",
        ));
    }
    Ok(Properties {
        networking,
        wireless,
        hardware,
        startup,
        checkpoints,
        state,
        primary_type,
    })
}

async fn property(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Result<OwnedValue> {
    let proxy = zbus::Proxy::new(connection, owner, path, "org.freedesktop.DBus.Properties")
        .await
        .map_err(bus_error)?;
    let reply = proxy
        .call_method("Get", &(interface, name))
        .await
        .map_err(bus_error)?;
    if reply.data().len() > 16 * 1024 {
        return Err(invalid(
            "NetworkManager settings revision reply exceeds limit",
        ));
    }
    reply.body().deserialize().map_err(bus_error)
}

fn profile_paths(value: OwnedValue) -> Result<Vec<OwnedObjectPath>> {
    let mut paths: Vec<OwnedObjectPath> = value
        .try_into()
        .map_err(|_| invalid("NetworkManager profile inventory is unavailable"))?;
    if paths.len() > 64
        || paths.iter().any(|path| {
            path.as_str().len() > 128
                || !path
                    .as_str()
                    .strip_prefix("/org/freedesktop/NetworkManager/Settings/")
                    .is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                    })
        })
    {
        return Err(invalid("NetworkManager profile inventory exceeds bounds"));
    }
    paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    if paths.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid(
            "NetworkManager profile inventory contains duplicates",
        ));
    }
    Ok(paths)
}

async fn profile_revision(connection: &zbus::Connection, owner: &str) -> Result<(Sha256, usize)> {
    let paths = profile_paths(
        property(
            connection,
            owner,
            "/org/freedesktop/NetworkManager/Settings",
            "org.freedesktop.NetworkManager.Settings",
            "Connections",
        )
        .await?,
    )?;
    let mut versions = Vec::with_capacity(paths.len());
    for path in paths {
        // NM 1.46.6 implements VersionId on Settings.Connection, despite its
        // generated introspection docs listing it on the Settings interface.
        let version: u64 = property(
            connection,
            owner,
            path.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
            "VersionId",
        )
        .await?
        .try_into()
        .map_err(|_| invalid("NetworkManager profile revision is unavailable"))?;
        versions.push((path, version));
    }
    Ok((
        Sha256::digest(&serde_json::to_vec(&versions)?),
        versions.len(),
    ))
}

async fn properties(connection: &zbus::Connection, owner: &str) -> Result<Properties> {
    let proxy = zbus::Proxy::new(connection, owner, OBJECT, "org.freedesktop.DBus.Properties")
        .await
        .map_err(bus_error)?;
    let reply = proxy
        .call_method("GetAll", &(NAME,))
        .await
        .map_err(bus_error)?;
    if reply.data().len() > 128 * 1024 {
        return Err(invalid("NetworkManager property reply exceeds limit"));
    }
    parse(reply.body().deserialize().map_err(bus_error)?)
}

pub(crate) async fn connection_observation(
    connection: &zbus::Connection,
    expected_uid: u32,
) -> Result<Observation> {
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(bus_error)?;
    let owner = bus
        .get_name_owner(NAME.try_into().map_err(bus_error)?)
        .await
        .map_err(bus_error)?;
    let uid = bus
        .get_connection_unix_user(owner.clone().into())
        .await
        .map_err(bus_error)?;
    let pid = bus
        .get_connection_unix_process_id(owner.clone().into())
        .await
        .map_err(bus_error)?;
    if uid != expected_uid || pid == 0 {
        return Err(invalid("NetworkManager has an unexpected owner"));
    }
    let revision = profile_revision(connection, owner.as_str()).await?;
    let first = properties(connection, owner.as_str()).await?;
    let second = properties(connection, owner.as_str()).await?;
    if first != second
        || revision != profile_revision(connection, owner.as_str()).await?
        || bus
            .get_name_owner(NAME.try_into().map_err(bus_error)?)
            .await
            .map_err(bus_error)?
            != owner
    {
        return Err(invalid("NetworkManager changed during observation"));
    }
    Ok(Observation {
        schema: 1,
        bus_owner: owner.to_string(),
        pid,
        networking_enabled: first.networking,
        wireless_enabled: first.wireless,
        wireless_hardware_enabled: first.hardware,
        startup: first.startup,
        checkpoint_count: first.checkpoints.len(),
        state: first.state,
        profile_revision: revision.0,
        profile_count: revision.1,
        ethernet_available: first.networking
            && first.state >= 50
            && first.primary_type == "802-3-ethernet",
        wireless_available: first.networking
            && first.wireless
            && first.hardware
            && !first.startup
            && first.state >= 50
            && first.primary_type == "802-11-wireless",
    })
}

pub(crate) async fn bounded<T>(
    duration: Duration,
    task: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    futures_lite::future::or(task, async {
        async_io::Timer::after(duration).await;
        Err(invalid("NetworkManager observation exceeded its deadline"))
    })
    .await
}

pub fn observe() -> Result<Observation> {
    futures_lite::future::block_on(bounded(DEADLINE, async {
        let connection = zbus::connection::Builder::system()
            .map_err(bus_error)?
            .method_timeout(DEADLINE)
            .build()
            .await
            .map_err(bus_error)?;
        connection_observation(&connection, 0).await
    }))
}

fn invalid(value: &str) -> Error {
    Error::Invalid(value.into())
}
pub(crate) fn bus_error(_: impl std::fmt::Display) -> Error {
    // D-Bus errors can include arbitrary peer text; status must not echo it.
    invalid("NetworkManager observation is unavailable")
}

#[cfg(test)]
#[path = "network_observation_tests.rs"]
mod tests;
