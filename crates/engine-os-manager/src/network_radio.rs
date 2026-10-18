//! The sole NetworkManager write used by the radio coordinator. Callers must
//! hold its transition journal and operation/admission guards before calling.
use crate::Result;
use crate::network_controller::WriteResult;
use crate::network_observation::{Observation, bounded, bus_error, connection_observation};
use std::cell::Cell;
use std::time::Duration;

const NAME: &str = "org.freedesktop.NetworkManager";
const OBJECT: &str = "/org/freedesktop/NetworkManager";
const DEADLINE: Duration = Duration::from_secs(5);

/// A method reply is not availability; the coordinator must read back actual
/// state. A timeout is ambiguous even if the radio still appears unchanged.
pub fn set_wireless(expected: &Observation, enabled: bool) -> Result<WriteResult> {
    let submitted = Cell::new(false);
    let result = futures_lite::future::block_on(bounded(DEADLINE, async {
        let connection = zbus::connection::Builder::system()
            .map_err(bus_error)?
            .method_timeout(DEADLINE)
            .build()
            .await
            .map_err(bus_error)?;
        write_attempt(&connection, 0, expected, enabled, &submitted).await
    }));
    complete_attempt(result, submitted.get())
}

fn complete_attempt(result: Result<WriteResult>, submitted: bool) -> Result<WriteResult> {
    match result {
        Err(_) if !submitted => Ok(WriteResult::NotSubmitted),
        result => result,
    }
}

#[cfg(test)]
pub(crate) async fn write(
    connection: &zbus::Connection,
    uid: u32,
    expected: &Observation,
    enabled: bool,
) -> Result<WriteResult> {
    let submitted = Cell::new(false);
    let result = write_attempt(connection, uid, expected, enabled, &submitted).await;
    complete_attempt(result, submitted.get())
}

async fn write_attempt(
    connection: &zbus::Connection,
    uid: u32,
    expected: &Observation,
    enabled: bool,
    submitted: &Cell<bool>,
) -> Result<WriteResult> {
    let actual = connection_observation(connection, uid).await?;
    if actual != *expected
        || !actual.networking_enabled
        || !actual.wireless_hardware_enabled
        || actual.startup
        || actual.checkpoint_count != 0
    {
        return Ok(WriteResult::Rejected);
    }
    if actual.wireless_enabled == enabled {
        return Ok(WriteResult::Returned);
    }
    // Address the verified unique owner, never an alias that could be replaced
    // between observation and dispatch. The command has no caller-chosen path,
    // interface, property, connection ID or arbitrary NetworkManager method.
    let proxy = zbus::Proxy::new(
        connection,
        actual.bus_owner.as_str(),
        OBJECT,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(bus_error)?;
    submitted.set(true);
    let result: zbus::Result<()> = proxy
        .call(
            "Set",
            &(
                NAME,
                "WirelessEnabled",
                zbus::zvariant::Value::Bool(enabled),
            ),
        )
        .await;
    match result {
        Ok(()) => Ok(WriteResult::Returned),
        Err(zbus::Error::MethodError(name, _, _))
            if matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.AccessDenied"
                    | "org.freedesktop.NetworkManager.PermissionDenied"
            ) =>
        {
            Ok(WriteResult::Rejected)
        }
        Err(error) => Err(bus_error(error)),
    }
}
