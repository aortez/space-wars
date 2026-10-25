//! Trial-only kiosk observations. The independent service supplies expectations
//! from the verified image. Neither a control reply nor a running PID grants
//! permission to promote a slot. The recovery journal owns that decision.
use crate::{Error, Result, linux};
use futures_lite::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacewars_control::{HEALTH_SCHEMA_VERSION, HealthRequest, HealthState, KIOSK_CONTROL_SOCKET};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

mod worker;
pub use worker::KioskObserver;

const UNIT: &str = "/org/freedesktop/systemd1/unit/spacewars_2dkiosk_2eservice";
const KIOSK_UID: u32 = 1000;
const EXECUTABLE: &str = "/usr/bin/engine-client";
const MAX_RESPONSE: usize = 4096;
const DEADLINE: Duration = Duration::from_secs(5);

/// Must come from the image identity bound by the verified rootfs payload, not
/// from the kiosk or an unprivileged socket request. Revision alone is not an
/// image identity: the hash and length bind the actual running executable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskExpectation {
    pub build_id: String,
    pub build_revision: String,
    pub executable_bytes: u64,
    pub executable_sha256: [u8; 32],
}

impl KioskExpectation {
    /// The caller must have authenticated the containing rootfs. A build record
    /// alone does not establish trust in a candidate or authorize promotion.
    pub fn from_image(image: &engine_ota::ImageIdentity) -> Result<Self> {
        image
            .validate()
            .map_err(|error| Error::Invalid(error.to_string()))?;
        let expected = Self {
            build_id: image.build_id.clone(),
            build_revision: image.source.revision.clone(),
            executable_bytes: image.binaries.client.bytes,
            executable_sha256: *image.binaries.client.sha256.as_bytes(),
        };
        expected.validate()?;
        Ok(expected)
    }

    pub fn validate(&self) -> Result<()> {
        if self.build_id.is_empty()
            || self.build_id.len() > 128
            || !self
                .build_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || self.build_revision.is_empty()
            || self.build_revision.len() > 128
            || self.build_revision.chars().any(char::is_control)
            || !(1..=256 * 1024 * 1024).contains(&self.executable_bytes)
            || self.executable_sha256 == [0; 32]
        {
            return invalid("invalid expected kiosk image");
        }
        Ok(())
    }
}

/// The adapter obtains every field from OS observations or a fresh UI-thread
/// response. Successful construction means the executable and state write were
/// checked. Continuous progress is still the recovery monitor's responsibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskHealth {
    pub boot_id: String,
    pub uptime_ms: u64,
    pub process_id: u32,
    pub start_ticks: u64,
    pub invocation_id: String,
    pub build_id: String,
    pub control_response: u64,
    pub presented_frames: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct Instance {
    pid: u32,
    invocation_id: String,
}

async fn instance(connection: &zbus::Connection) -> Result<Instance> {
    use zbus::zvariant::OwnedValue;
    let properties = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        UNIT,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(dbus_error)?;
    // Explicit GetAll avoids a cached MainPID or InvocationID after a restart.
    let mut unit: std::collections::HashMap<String, OwnedValue> = properties
        .call("GetAll", &("org.freedesktop.systemd1.Unit",))
        .await
        .map_err(dbus_error)?;
    let mut service: std::collections::HashMap<String, OwnedValue> = properties
        .call("GetAll", &("org.freedesktop.systemd1.Service",))
        .await
        .map_err(dbus_error)?;
    let active: String = unit
        .remove("ActiveState")
        .ok_or_else(|| missing("ActiveState"))?
        .try_into()
        .map_err(dbus_error)?;
    let invocation: Vec<u8> = unit
        .remove("InvocationID")
        .ok_or_else(|| missing("InvocationID"))?
        .try_into()
        .map_err(dbus_error)?;
    let pid: u32 = service
        .remove("MainPID")
        .ok_or_else(|| missing("MainPID"))?
        .try_into()
        .map_err(dbus_error)?;
    if active != "active"
        || pid <= 1
        || invocation.len() != 16
        || invocation.iter().all(|b| *b == 0)
    {
        return invalid("kiosk systemd instance is not active and coherent");
    }
    Ok(Instance {
        pid,
        invocation_id: invocation.iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Fresh systemd and /proc identity for authenticated install exchanges.
pub(crate) fn install_instance() -> Result<spacewars_control::install::HostInstance> {
    futures_lite::future::block_on(futures_lite::future::or(
        async {
            let connection = zbus::Connection::system().await.map_err(dbus_error)?;
            let first = instance(&connection).await?;
            let start_ticks = process(first.pid, KIOSK_UID)?;
            let second = instance(&connection).await?;
            if first != second || process(first.pid, KIOSK_UID)? != start_ticks {
                return invalid("kiosk instance changed");
            }
            Ok(spacewars_control::install::HostInstance {
                pid: first.pid,
                start_ticks,
                invocation: first.invocation_id,
            })
        },
        async {
            async_io::Timer::after(Duration::from_secs(1)).await;
            invalid("kiosk identity observation timed out")
        },
    ))
}

fn missing(name: &str) -> Error {
    Error::Invalid(format!("systemd omitted kiosk {name}"))
}

fn dbus_error(error: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("kiosk systemd observation: {error}"))
}

fn invalid<T>(message: &str) -> Result<T> {
    Err(Error::Invalid(message.into()))
}

fn read_small(path: &Path, limit: usize) -> Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return invalid("kiosk process evidence is not a regular file");
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return invalid("kiosk process evidence exceeds its bound");
    }
    String::from_utf8(bytes).map_err(|_| Error::Invalid("non-UTF8 process evidence".into()))
}

fn start_ticks(stat: &str, expected_pid: u32) -> Result<u64> {
    // comm (field 2) can contain spaces, parentheses and newlines. The fields
    // after its final ')' are numeric except the single process-state byte.
    let prefix = format!("{expected_pid} (");
    let Some(end) = stat.rfind(')') else {
        return invalid("invalid process stat");
    };
    let fields = &stat[end + 1..];
    if !stat.starts_with(&prefix) || !fields.starts_with(' ') {
        return invalid("process stat identity mismatch");
    }
    let value = fields
        .split_whitespace()
        .nth(19)
        .and_then(|s| s.parse().ok());
    match value {
        Some(value) if value > 0 => Ok(value),
        _ => invalid("invalid process start time"),
    }
}

pub(crate) fn process(pid: u32, uid: u32) -> Result<u64> {
    let root = Path::new("/proc").join(pid.to_string());
    let status = read_small(&root.join("status"), 64 * 1024)?;
    let uids = status
        .lines()
        .filter_map(|line| line.strip_prefix("Uid:\t"))
        .collect::<Vec<_>>();
    if uids.len() != 1
        || uids[0]
            .split_whitespace()
            .map(|v| v.parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()
            .as_deref()
            != Some(&[uid; 4])
    {
        return invalid("kiosk process credentials mismatch");
    }
    start_ticks(&read_small(&root.join("stat"), 16 * 1024)?, pid)
}

fn executable(pid: u32, path: &Path, owner: u32, expected: &KioskExpectation) -> Result<()> {
    // /proc/PID/exe is intentionally followed: it is the kernel's reference to
    // the running image, including one superseded/unlinked since execution.
    let mut running = File::open(format!("/proc/{pid}/exe"))?;
    let actual = running.metadata()?;
    let installed = fs::symlink_metadata(path)?;
    if !actual.is_file()
        || !installed.is_file()
        || actual.uid() != owner
        || actual.mode() & 0o022 != 0
        || actual.len() != expected.executable_bytes
        || actual.dev() != installed.dev()
        || actual.ino() != installed.ino()
    {
        return invalid("running kiosk is not the installed immutable executable");
    }
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = running.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > expected.executable_bytes {
            return invalid("kiosk executable grew while hashing");
        }
        hash.update(&buffer[..count]);
    }
    let after = running.metadata()?;
    if read != expected.executable_bytes
        || <[u8; 32]>::from(hash.finalize()) != expected.executable_sha256
        || actual.ctime() != after.ctime()
        || actual.ctime_nsec() != after.ctime_nsec()
        || actual.mtime() != after.mtime()
        || actual.mtime_nsec() != after.mtime_nsec()
    {
        return invalid("running kiosk executable does not match the verified image");
    }
    Ok(())
}

#[cfg(feature = "recovery-qualification")]
pub(crate) fn qualification_instance(
    health: &KioskHealth,
    expected: &KioskExpectation,
) -> Result<()> {
    let current = install_instance()?;
    if current.pid != health.process_id
        || current.start_ticks != health.start_ticks
        || current.invocation != health.invocation_id
    {
        return invalid("selected kiosk instance changed");
    }
    executable(current.pid, Path::new(EXECUTABLE), 0, expected)?;
    // The executable hash can take time. Check the systemd process again.
    if install_instance()? != current {
        return invalid("kiosk changed while verifying its executable");
    }
    Ok(())
}

fn peer(stream: &UnixStream, pid: u32, uid: u32) -> Result<()> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
    // SAFETY: getsockopt receives an open Unix socket and correctly sized,
    // writable ucred/socklen_t buffers. The kernel supplies these credentials.
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if length as usize != std::mem::size_of_val(&credentials)
        || credentials.pid <= 1
        || credentials.pid as u32 != pid
        || credentials.uid != uid
    {
        return invalid("kiosk control peer does not match the systemd process");
    }
    Ok(())
}

async fn challenge(path: &Path, pid: u32, uid: u32, nonce: u64) -> Result<HealthState> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid || metadata.mode() & 0o007 != 0 {
        return invalid("unexpected kiosk control socket owner, type or permissions");
    }
    let mut stream = async_io::Async::<UnixStream>::connect(path).await?;
    peer(stream.get_ref(), pid, uid)?;
    let request = HealthRequest {
        schema_version: HEALTH_SCHEMA_VERSION,
        nonce,
    };
    let request = format!("health\n{}\n", serde_json::to_string(&request)?);
    stream.write_all(request.as_bytes()).await?;
    stream.get_ref().shutdown(std::net::Shutdown::Write)?;
    let mut bytes = Vec::new();
    stream
        .take(MAX_RESPONSE as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > MAX_RESPONSE {
        return invalid("oversized kiosk health response");
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Invalid("non-UTF8 kiosk health response".into()))?;
    let Some(json) = text.strip_prefix("ok ").and_then(|s| s.strip_suffix('\n')) else {
        return invalid("kiosk rejected health challenge or sent invalid framing");
    };
    HealthState::from_json(json, nonce).map_err(|error| Error::Invalid(error.to_string()))
}

struct ProbePaths<'a> {
    socket: &'a Path,
    executable: &'a Path,
    uid: u32,
    executable_owner: u32,
}

async fn observe_connection(
    connection: &zbus::Connection,
    paths: &ProbePaths<'_>,
    expected: &KioskExpectation,
    write_state: impl FnOnce() -> Result<()>,
) -> Result<KioskHealth> {
    expected.validate()?;
    let before = instance(connection).await?;
    let start = process(before.pid, paths.uid)?;
    executable(
        before.pid,
        paths.executable,
        paths.executable_owner,
        expected,
    )?;
    let mut random = [0u8; 8];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let response = challenge(
        paths.socket,
        before.pid,
        paths.uid,
        u64::from_ne_bytes(random),
    )
    .await?;
    if response.process_id != before.pid || response.build_revision != expected.build_revision {
        return invalid("kiosk response identity does not match the verified image");
    }
    let Some(presented_frames) = response.presented_frames else {
        return invalid("kiosk backend supplies no display presentation evidence");
    };
    write_state()?;
    // Recheck after the request and state write. A restart/exec or unit change
    // during observation must not create a sample for the previous process.
    if instance(connection).await? != before || process(before.pid, paths.uid)? != start {
        return invalid("kiosk process changed during health observation");
    }
    executable(
        before.pid,
        paths.executable,
        paths.executable_owner,
        expected,
    )?;
    Ok(KioskHealth {
        boot_id: linux::boot_id()?,
        uptime_ms: linux::clock_reading()?.boottime_us / 1000,
        process_id: before.pid,
        start_ticks: start,
        invocation_id: before.invocation_id,
        build_id: expected.build_id.clone(),
        control_response: response.ui_responses,
        presented_frames,
    })
}

/// Called only in the isolated worker, with an outer process deadline that
/// also covers filesystem I/O and the unprivileged state probe.
#[doc(hidden)]
pub fn observe_worker(expected: &KioskExpectation) -> Result<KioskHealth> {
    let stores = engine_ota::persistence_linux::PersistentStores::open()?;
    futures_lite::future::block_on(async {
        let connection = zbus::connection::Builder::system()
            .map_err(dbus_error)?
            .method_timeout(DEADLINE)
            .build()
            .await
            .map_err(dbus_error)?;
        observe_connection(
            &connection,
            &ProbePaths {
                socket: Path::new(KIOSK_CONTROL_SOCKET),
                executable: Path::new(EXECUTABLE),
                uid: KIOSK_UID,
                executable_owner: 0,
            },
            expected,
            || {
                // Fixed command/path, empty environment and no supplemental groups.
                // Writing as root would incorrectly bless kiosk-inaccessible state.
                let status = std::process::Command::new("/usr/bin/setpriv")
                    .args([
                        "--reuid=1000",
                        "--regid=1000",
                        "--clear-groups",
                        "--no-new-privs",
                        "/usr/bin/engine-os-manager",
                        "kiosk-state-worker",
                    ])
                    .env_clear()
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                if !status.success() {
                    return invalid("kiosk required state write failed");
                }
                stores.check()?;
                Ok(())
            },
        )
        .await
    })
}

/// A fixed, temporary write as the kiosk user. It never edits user settings,
/// saved games or ROMs, and runs in the health worker's bounded process group.
#[doc(hidden)]
pub fn state_worker() -> Result<()> {
    if unsafe { libc::geteuid() } != KIOSK_UID || unsafe { libc::getegid() } != KIOSK_UID {
        return invalid("kiosk state probe must run as the kiosk account");
    }
    let directory = Path::new("/data/spacewars/config");
    if fs::canonicalize("/var/lib/spacewars")? != directory
        || fs::metadata(directory)?.dev() != fs::metadata("/data")?.dev()
    {
        return invalid("kiosk configuration is not on the expected data path");
    }
    write_state_probe(directory)
}

fn write_state_probe(directory: &Path) -> Result<()> {
    let mut file = tempfile::Builder::new()
        .prefix(".ota-health-")
        .tempfile_in(directory)?;
    file.write_all(b"spacewars-kiosk-state-probe-v1\n")?;
    file.as_file().sync_all()?;
    let mut readback = Vec::new();
    file.reopen()?.take(64).read_to_end(&mut readback)?;
    if readback != b"spacewars-kiosk-state-probe-v1\n" {
        return invalid("kiosk state readback mismatch");
    }
    file.close()?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "kiosk_health_tests.rs"]
mod tests;
