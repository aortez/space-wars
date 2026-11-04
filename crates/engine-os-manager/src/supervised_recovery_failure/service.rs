//! Fixed recovery-unit observations; no unit management or watchdog operations.
use super::{Instance, require};
use crate::{Error, Result, kiosk_health, linux};
use engine_ota::{ImageIdentity, Sha256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::time::Duration;
use zbus::zvariant::OwnedValue;

const UNIT: &str = "spacewars-recovery.service";
const PATH: &str = "/org/freedesktop/systemd1/unit/spacewars_2drecovery_2eservice";
const EXECUTABLE: &str = "/usr/bin/engine-os-manager";
type Properties = HashMap<String, OwnedValue>;
type Commands = Vec<(String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32)>;
type StatusSet = (Vec<i32>, Vec<i32>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Unit {
    pub active: String,
    pub sub: String,
    pub invocation: String,
    pub pid: u32,
    pub exec_pid: u32,
    pub exec_start_us: u64,
    pub exec_code: i32,
    pub exec_status: i32,
    pub result: String,
    pub restarts: u32,
}

fn field<T>(properties: &mut Properties, key: &str) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::fmt::Display,
{
    T::try_from(
        properties
            .remove(key)
            .ok_or_else(|| Error::Invalid(format!("missing recovery property {key}")))?,
    )
    .map_err(|e| Error::Invalid(format!("invalid recovery property {key}: {e}")))
}

fn decode(mut unit: Properties, mut service: Properties) -> Result<Unit> {
    require(
        field::<String>(&mut unit, "Id")? == UNIT
            && field::<String>(&mut unit, "LoadState")? == "loaded"
            && field::<String>(&mut unit, "CollectMode")? == "inactive"
            && field::<String>(&mut service, "Type")? == "oneshot"
            && field::<String>(&mut service, "ExitType")? == "main"
            && field::<String>(&mut service, "Restart")? == "no"
            && field::<String>(&mut service, "KillMode")? == "control-group"
            && field::<bool>(&mut service, "SendSIGKILL")?
            && field::<u64>(&mut service, "TimeoutStopUSec")? == 5_000_000,
        "recovery unit policy differs from the reviewed oneshot service",
    )?;
    for name in [
        "FailureAction",
        "SuccessAction",
        "StartLimitAction",
        "JobTimeoutAction",
    ] {
        require(
            field::<String>(&mut unit, name)? == "none",
            "unexpected recovery unit action",
        )?;
    }
    for name in ["OnFailure", "OnSuccess"] {
        require(
            field::<Vec<String>>(&mut unit, name)?.is_empty(),
            "unexpected recovery unit dependency action",
        )?;
    }
    for name in ["ExecStop", "ExecStopPost"] {
        require(
            field::<Commands>(&mut service, name)?.is_empty(),
            "unexpected recovery stop command",
        )?;
    }
    for name in [
        "SuccessExitStatus",
        "RestartForceExitStatus",
        "RestartPreventExitStatus",
    ] {
        let (codes, signals) = field::<StatusSet>(&mut service, name)?;
        require(
            codes.is_empty() && signals.is_empty(),
            "unexpected recovery exit-status override",
        )?;
    }
    let invocation = field::<Vec<u8>>(&mut unit, "InvocationID")?;
    require(
        invocation.len() == 16 && invocation.iter().any(|b| *b != 0),
        "missing recovery invocation",
    )?;
    Ok(Unit {
        active: field(&mut unit, "ActiveState")?,
        sub: field(&mut unit, "SubState")?,
        invocation: invocation.iter().map(|b| format!("{b:02x}")).collect(),
        pid: field(&mut service, "MainPID")?,
        exec_pid: field(&mut service, "ExecMainPID")?,
        exec_start_us: field(&mut service, "ExecMainStartTimestampMonotonic")?,
        exec_code: field(&mut service, "ExecMainCode")?,
        exec_status: field(&mut service, "ExecMainStatus")?,
        result: field(&mut service, "Result")?,
        restarts: field(&mut service, "NRestarts")?,
    })
}

async fn read_on(connection: &zbus::Connection) -> Result<Unit> {
    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(|e| Error::Invalid(e.to_string()))?;
    let unit = proxy
        .call("GetAll", &("org.freedesktop.systemd1.Unit",))
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let service = proxy
        .call("GetAll", &("org.freedesktop.systemd1.Service",))
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;
    decode(unit, service)
}

pub(super) fn read() -> Result<Unit> {
    futures_lite::future::block_on(futures_lite::future::or(
        async {
            let connection = zbus::Connection::system()
                .await
                .map_err(|e| Error::Invalid(e.to_string()))?;
            read_on(&connection).await
        },
        async {
            async_io::Timer::after(Duration::from_secs(1)).await;
            Err(Error::Invalid("recovery unit observation timed out".into()))
        },
    ))
}

impl Unit {
    pub(super) fn settling(&self, before: &Self) -> Result<()> {
        require(
            self.invocation == before.invocation
                && self.exec_pid == before.pid
                && self.exec_start_us == before.exec_start_us
                && self.restarts == 0
                && (self.pid == 0 || self.pid == before.pid)
                && matches!(self.result.as_str(), "success" | "signal")
                && matches!(self.active.as_str(), "activating" | "deactivating"),
            "recovery failed-state observation changed instance or result",
        )
    }

    pub(super) fn running(&self) -> Result<()> {
        require(
            self.active == "activating"
                && self.sub == "start"
                && self.pid > 1
                && self.pid <= i32::MAX as u32
                && self.exec_pid == self.pid
                && self.exec_start_us > 0
                && self.exec_code == 0
                && self.exec_status == 0
                && self.result == "success"
                && self.restarts == 0,
            "recovery service is not the initial running trial observer",
        )
    }
    pub(super) fn failed(&self, before: &Self) -> Result<()> {
        before.running()?;
        require(
            self.active == "failed"
                && self.sub == "failed"
                && self.pid == 0
                && self.exec_pid == before.pid
                && self.exec_start_us == before.exec_start_us
                && self.invocation == before.invocation
                && self.restarts == before.restarts
                && self.result == "signal"
                && self.exec_code == libc::CLD_KILLED
                && self.exec_status == libc::SIGKILL,
            "selected recovery process has no matching failed SIGKILL result",
        )
    }
}

fn executable(pid: u32, image: &ImageIdentity) -> Result<()> {
    let expected = &image.binaries.os_manager;
    require(
        expected.bytes > 0 && expected.bytes <= 256 * 1024 * 1024,
        "invalid recovery executable size",
    )?;
    let mut file = File::open(format!("/proc/{pid}/exe"))?;
    let before = file.metadata()?;
    let installed = std::fs::symlink_metadata(EXECUTABLE)?;
    require(
        before.is_file()
            && installed.is_file()
            && before.uid() == 0
            && before.mode() & 0o022 == 0
            && before.len() == expected.bytes
            && before.dev() == installed.dev()
            && before.ino() == installed.ino(),
        "recovery process is not the installed immutable executable",
    )?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(expected.bytes + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    require(
        bytes.len() as u64 == expected.bytes
            && Sha256::digest(&bytes) == expected.sha256
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec(),
        "recovery executable changed or differs from the selected image",
    )?;
    let mut command = Vec::new();
    File::open(format!("/proc/{pid}/cmdline"))?
        .take(1025)
        .read_to_end(&mut command)?;
    require(
        command == b"/usr/bin/engine-os-manager\0recover\0",
        "not the fixed recovery command",
    )
}

pub(super) fn instance(image: &ImageIdentity) -> Result<Instance> {
    let unit = read()?;
    unit.running()?;
    let start_ticks = kiosk_health::process(unit.pid, 0)?;
    executable(unit.pid, image)?;
    require(
        read()? == unit && kiosk_health::process(unit.pid, 0)? == start_ticks,
        "recovery process changed during executable verification",
    )?;
    Ok(Instance {
        unit,
        start_ticks,
        boot_id: linux::boot_id()?,
    })
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
