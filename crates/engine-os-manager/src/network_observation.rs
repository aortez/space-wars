//! Read-only, uncached NetworkManager observations. This does not establish
//! session safety or permission to change the radio; it never scans/connects.
use crate::{Error, Result};
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Properties {
    networking: bool,
    wireless: bool,
    hardware: bool,
    startup: bool,
    checkpoints: Vec<OwnedObjectPath>,
    state: u32,
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
    Ok(Properties {
        networking,
        wireless,
        hardware,
        startup,
        checkpoints,
        state,
    })
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

async fn connection_observation(
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
    let first = properties(connection, owner.as_str()).await?;
    let second = properties(connection, owner.as_str()).await?;
    if first != second
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
    })
}

async fn bounded<T>(
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
fn bus_error(_: impl std::fmt::Display) -> Error {
    // D-Bus errors can include arbitrary peer text; status must not echo it.
    invalid("NetworkManager observation is unavailable")
}

#[cfg(test)]
#[path = "network_observation_tests.rs"]
mod tests;
