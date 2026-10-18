//! A single request to the existing OS time provider, after durable scheduling.
//! No NTP implementation, server/configuration edits or NetworkManager restart.
use crate::Result;
use crate::network_policy::Clock;
use crate::time::ProviderIdentity;
use crate::time_schedule::Schedule;
use std::cell::Cell;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const DEADLINE: Duration = Duration::from_secs(5);
const MANAGER: &str = "org.freedesktop.systemd1";
const UNIT: &str = "/org/freedesktop/systemd1/unit/systemd_2dtimesyncd_2eservice";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// No request dispatched. Includes missing/disabled provider, changed
    /// identity, expired/cancelled attempt and an already-consumed allowance.
    Unavailable,
    /// Only the method reply is known. Success still requires accepted new time
    /// evidence; a job reply is not a successful synchronization.
    Requested,
    /// Request was dispatched but the reply is ambiguous. Never send it again.
    Uncertain,
}

/// Run on the time-check worker, never the status/observation thread. `guard`
/// checks the original attempt, current saved policy and live owned hold, and
/// returns the actual CLOCK_BOOTTIME reading. It must be fast, local and fail on
/// cancellation. The total five-second bound includes bus setup and preflight;
/// periodic guard checks also account for device sleep. The scheduler commits
/// the query allowance before dispatch, even when the method reply is lost.
pub fn request(schedule: &mut Schedule, id: u64, guard: impl Fn() -> Result<Clock>) -> Outcome {
    futures_lite::future::block_on(bounded(
        schedule,
        id,
        guard,
        async {
            zbus::connection::Builder::system()
                .map_err(|_| ())?
                .method_timeout(DEADLINE)
                .build()
                .await
                .map_err(|_| ())
        },
        1,
        0,
    ))
}

async fn bounded(
    schedule: &mut Schedule,
    id: u64,
    guard: impl Fn() -> Result<Clock>,
    connection: impl std::future::Future<Output = std::result::Result<zbus::Connection, ()>>,
    manager_pid: u32,
    manager_uid: u32,
) -> Outcome {
    let dispatched = Cell::new(false);
    let Ok(start) = guard() else {
        return Outcome::Unavailable;
    };
    let Ok(status) = schedule.status(&start) else {
        return Outcome::Unavailable;
    };
    let Some(attempt) = status
        .active
        .filter(|a| a.id == id && !a.query_requested && a.connection.is_some())
    else {
        return Outcome::Unavailable;
    };
    let Some(end) = start
        .ms
        .checked_add(DEADLINE.as_millis() as u64)
        .map(|end| end.min(attempt.deadline_ms))
    else {
        return Outcome::Unavailable;
    };
    let checked_clock = || {
        let now = guard().map_err(|_| ())?;
        if now.boot_id != start.boot_id || now.ms < start.ms || now.ms >= end {
            return Err(());
        }
        Ok(now)
    };
    let operation = async {
        checked_clock()?;
        let connection = connection.await?;
        let (manager, expected) = preflight(&connection, manager_pid, manager_uid).await?;
        let now = checked_clock()?;
        if !schedule
            .claim_query(&now, id, expected.clone())
            .map_err(|_| ())?
        {
            return Err(());
        }
        // Disk flushes and provider transitions can happen during admission.
        // Re-read both identities and provider enablement before the only send.
        let (again, provider) = preflight(&connection, manager_pid, manager_uid).await?;
        if again != manager || provider != expected {
            return Err(());
        }
        let proxy = zbus::Proxy::new(
            &connection,
            manager.as_str(),
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .await
        .map_err(|_| ())?;
        checked_clock()?;
        dispatched.set(true);
        let job: OwnedObjectPath = proxy
            .call("TryRestartUnit", &("systemd-timesyncd.service", "fail"))
            .await
            .map_err(|_| ())?;
        checked_clock()?;
        if !job.as_str().starts_with("/org/freedesktop/systemd1/job/") {
            return Err(());
        }
        Ok(())
    };
    let deadline = async {
        let timer = async {
            async_io::Timer::after(DEADLINE).await;
            Err(())
        };
        let monitor = async {
            loop {
                checked_clock()?;
                async_io::Timer::after(Duration::from_millis(100)).await;
            }
        };
        futures_lite::future::or(timer, monitor).await
    };
    match futures_lite::future::or(operation, deadline).await {
        Ok(()) => Outcome::Requested,
        Err(()) if dispatched.get() => Outcome::Uncertain,
        Err(()) => Outcome::Unavailable,
    }
}

async fn preflight(
    connection: &zbus::Connection,
    pid: u32,
    uid: u32,
) -> std::result::Result<(String, ProviderIdentity), ()> {
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|_| ())?;
    let owner = bus
        .get_name_owner(MANAGER.try_into().map_err(|_| ())?)
        .await
        .map_err(|_| ())?;
    if bus
        .get_connection_unix_process_id(owner.clone().into())
        .await
        .map_err(|_| ())?
        != pid
        || bus
            .get_connection_unix_user(owner.clone().into())
            .await
            .map_err(|_| ())?
            != uid
    {
        return Err(());
    }
    let unit = zbus::Proxy::new(
        connection,
        owner.as_str(),
        UNIT,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(|_| ())?;
    let mut values: std::collections::HashMap<String, OwnedValue> = unit
        .call("GetAll", &("org.freedesktop.systemd1.Unit",))
        .await
        .map_err(|_| ())?;
    if values.len() > 256 {
        return Err(());
    }
    let mut value = |name| values.remove(name).ok_or(())?.try_into().map_err(|_| ());
    let load: String = value("LoadState")?;
    let active: String = value("ActiveState")?;
    let enabled: String = value("UnitFileState")?;
    if load != "loaded"
        || active != "active"
        || !matches!(enabled.as_str(), "enabled" | "enabled-runtime" | "static")
    {
        return Err(());
    }
    let provider = crate::linux::identity(connection).await.map_err(|_| ())?;
    // Identity used the well-known PID-1 name; make sure it is still the exact
    // authenticated manager to which the mutation would be addressed.
    if bus
        .get_name_owner(MANAGER.try_into().map_err(|_| ())?)
        .await
        .map_err(|_| ())?
        != owner
    {
        return Err(());
    }
    Ok((owner.to_string(), provider))
}

#[cfg(test)]
#[path = "time_query_tests.rs"]
mod tests;
