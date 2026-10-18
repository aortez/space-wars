//! Session protection evidence for the socket-activated OpenSSH image.
//! Count kernel connections and service instances, not interactive terminals.
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NAME: &str = "org.freedesktop.systemd1";
const ROOT: &str = "/org/freedesktop/systemd1";
const TEMPLATE: &str = "sshd@spacewars-network-check.service";
const DEADLINE: Duration = Duration::from_secs(5);
const HOST_NAMESPACE_NAME: &str = "spacewars-host-netns";
static HOST_NAMESPACE: OnceLock<NamespaceHandoff> = OnceLock::new();
static SERVICE_HANDOFF_REQUIRED: OnceLock<()> = OnceLock::new();

enum NamespaceHandoff {
    Missing,
    Rejected(&'static str),
    Accepted(File),
}

impl NamespaceHandoff {
    fn confirmed(&self, required: bool) -> bool {
        match self {
            Self::Accepted(file) => same_namespace(file, Path::new("/proc/self/ns/net")),
            _ if required => false,
            // Only owner diagnostics may use direct PID 1 inspection. A service
            // with a failed handoff must not accidentally acquire this fallback.
            _ => fs::read_link("/proc/self/ns/net")
                .ok()
                .zip(fs::read_link("/proc/1/ns/net").ok())
                .is_some_and(|(a, b)| a == b),
        }
    }
}
type Fields = HashMap<String, OwnedValue>;
type UnitRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);
type ExecCommand = (String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Observation {
    pub schema: u32,
    pub active_tcp_connections: usize,
    pub active_ssh_connections: usize,
    pub other_tcp_listeners: usize,
    pub active_ssh_units: usize,
    pub socket_active: bool,
    pub admission_hook_loaded: bool,
    pub configuration_supported: bool,
    pub host_namespace_confirmed: bool,
    pub observation_complete: bool,
    /// None defers shutdown. True includes pre-authentication and closing work.
    pub ssh_activity: Option<bool>,
}

#[derive(Default)]
struct Tcp {
    active: usize,
    ssh: usize,
    other_listeners: usize,
}

fn namespace_descriptor(pid: &str, count: &str, names: &str) -> bool {
    pid.parse::<u32>().ok() == Some(std::process::id())
        && count == "1"
        && names == HOST_NAMESPACE_NAME
}

fn namespace_file(fd: i32) -> Option<File> {
    // Reopen our own descriptor with CLOEXEC; keep the namespace pinned so its
    // inode cannot be recycled. Do not take ownership of an unrelated caller FD.
    let file = File::open(format!("/proc/self/fd/{fd}")).ok()?;
    // SAFETY: NS_GET_NSTYPE takes no pointer and only inspects the open file.
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::NS_GET_NSTYPE) } != libc::CLONE_NEWNET {
        return None;
    }
    // systemd passes the original without CLOEXEC. It must not reach helpers.
    // SAFETY: fcntl operates on a descriptor number, with no pointer arguments.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return None;
    }
    Some(file)
}

/// Capture systemd's OpenFile before starting threads or spawning helpers.
/// A socket-activated SSH admission command must leave its socket untouched.
pub fn initialize_host_namespace() {
    HOST_NAMESPACE.get_or_init(|| {
        namespace_handoff(
            ["LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"].map(std::env::var),
            3,
        )
    });
}

fn namespace_handoff(
    metadata: [std::result::Result<String, std::env::VarError>; 3],
    fd: i32,
) -> NamespaceHandoff {
    if metadata
        .iter()
        .all(|value| matches!(value, Err(std::env::VarError::NotPresent)))
    {
        return NamespaceHandoff::Missing;
    }
    if let [Ok(pid), Ok(count), Ok(names)] = &metadata
        && namespace_descriptor(pid, count, names)
    {
        return namespace_file(fd).map_or(
            NamespaceHandoff::Rejected("descriptor is unavailable or not a network namespace"),
            NamespaceHandoff::Accepted,
        );
    }
    NamespaceHandoff::Rejected("activation metadata does not identify the expected descriptor")
}

/// Called once by serve, after logging initialization and before worker startup.
/// Missing evidence disables idle shutdown, not the independent device service.
pub fn require_service_namespace() {
    SERVICE_HANDOFF_REQUIRED.get_or_init(|| {
        match HOST_NAMESPACE.get().unwrap_or(&NamespaceHandoff::Missing) {
            NamespaceHandoff::Accepted(file)
                if same_namespace(file, Path::new("/proc/self/ns/net")) =>
            {
                tracing::info!("host network namespace handoff accepted");
            }
            NamespaceHandoff::Accepted(_) => tracing::warn!(
                "host network namespace handoff differs from service namespace; idle shutdown disabled"
            ),
            NamespaceHandoff::Missing => tracing::warn!(
                "host network namespace handoff missing; idle shutdown disabled"
            ),
            NamespaceHandoff::Rejected(reason) => tracing::warn!(
                reason,
                "host network namespace handoff rejected; idle shutdown disabled"
            ),
        }
    });
}

fn same_namespace(file: &File, current: &Path) -> bool {
    file.metadata()
        .ok()
        .zip(fs::metadata(current).ok())
        .is_some_and(|(host, current)| host.dev() == current.dev() && host.ino() == current.ino())
}

fn host_namespace_confirmed() -> bool {
    HOST_NAMESPACE
        .get()
        .unwrap_or(&NamespaceHandoff::Missing)
        .confirmed(SERVICE_HANDOFF_REQUIRED.get().is_some())
}

fn endpoint(value: &str, ipv6: bool) -> Result<(IpAddr, u16)> {
    let (address, port) = value.split_once(':').ok_or_else(unavailable)?;
    if address.len() != if ipv6 { 32 } else { 8 } || port.len() != 4 {
        return Err(unavailable());
    }
    let port = u16::from_str_radix(port, 16).map_err(|_| unavailable())?;
    let mut bytes = Vec::with_capacity(16);
    for word in address.as_bytes().chunks_exact(8) {
        let word = std::str::from_utf8(word).map_err(|_| unavailable())?;
        bytes.extend(
            u32::from_str_radix(word, 16)
                .map_err(|_| unavailable())?
                .to_ne_bytes(),
        );
    }
    let address = if ipv6 {
        IpAddr::V6(Ipv6Addr::from(
            <[u8; 16]>::try_from(bytes).map_err(|_| unavailable())?,
        ))
    } else {
        IpAddr::V4(Ipv4Addr::from(
            <[u8; 4]>::try_from(bytes).map_err(|_| unavailable())?,
        ))
    };
    Ok((address, port))
}

fn loopback(address: IpAddr) -> bool {
    address.is_loopback()
        || matches!(address, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback()))
}

fn tcp(text: &str, ipv6: bool) -> Result<Tcp> {
    if text.len() > 512 * 1024 {
        return Err(unavailable());
    }
    let mut lines = text.lines();
    let header: Vec<_> = lines
        .next()
        .ok_or_else(unavailable)?
        .split_whitespace()
        .collect();
    if header.len() < 12
        || header[0] != "sl"
        || header[1] != "local_address"
        || !matches!(header[2], "rem_address" | "remote_address")
        || header[3] != "st"
    {
        return Err(unavailable());
    }
    let mut result = Tcp::default();
    for (index, line) in lines.enumerate() {
        if index >= 2048 || line.len() > 2048 {
            return Err(unavailable());
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        // TIME_WAIT entries legitimately omit fields after the common prefix.
        if fields.len() < 4 || !fields[0].ends_with(':') || fields[3].len() != 2 {
            return Err(unavailable());
        }
        let (local, port) = endpoint(fields[1], ipv6)?;
        endpoint(fields[2], ipv6)?;
        let state = u8::from_str_radix(fields[3], 16).map_err(|_| unavailable())?;
        if !(1..=12).contains(&state)
            || (state != 6 && fields.len() < 10)
            || fields[0]
                .strip_suffix(':')
                .and_then(|value| value.parse::<u32>().ok())
                .is_none()
        {
            return Err(unavailable());
        }
        if state == 10 {
            if !loopback(local) && port != 22 {
                result.other_listeners += 1;
            }
        } else if !matches!(state, 6 | 7) {
            if port == 22 {
                result.ssh += 1;
            }
            if !loopback(local) {
                result.active += 1;
            }
        }
    }
    Ok(result)
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(unavailable());
    }
    Ok(bytes)
}

fn defaults(path: &Path) -> bool {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
        _ => return false,
    };
    match file.metadata() {
        Ok(meta)
            if meta.is_file()
                && meta.uid() == 0
                && meta.nlink() == 1
                && meta.mode() & 0o022 == 0 => {}
        _ => return false,
    }
    let mut bytes = Vec::new();
    if file.take(4097).read_to_end(&mut bytes).is_err() || bytes.len() > 4096 {
        return false;
    }
    String::from_utf8(bytes).ok().is_some_and(|text| {
        let mut assignments = 0;
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .all(|line| {
                assignments += 1;
                assignments == 1 && matches!(line, "SSHD_OPTS=" | "SSHD_OPTS=\"\"" | "SSHD_OPTS=''")
            })
    })
}

async fn fields(
    connection: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
) -> Result<Fields> {
    let proxy = zbus::Proxy::new(connection, owner, path, "org.freedesktop.DBus.Properties")
        .await
        .map_err(|_| unavailable())?;
    let reply = proxy
        .call_method("GetAll", &(interface,))
        .await
        .map_err(|_| unavailable())?;
    if reply.data().len() > 1024 * 1024 {
        return Err(unavailable());
    }
    let fields: Fields = reply.body().deserialize().map_err(|_| unavailable())?;
    if fields.len() > 512 {
        return Err(unavailable());
    }
    Ok(fields)
}

fn flag(fields: &Fields, key: &str) -> Option<bool> {
    fields.get(key)?.try_clone().ok()?.try_into().ok()
}
fn string(fields: &Fields, key: &str) -> Option<String> {
    fields.get(key)?.try_clone().ok()?.try_into().ok()
}
fn strings(fields: &Fields, key: &str) -> Option<Vec<String>> {
    fields.get(key)?.try_clone().ok()?.try_into().ok()
}
fn commands(fields: &Fields, key: &str) -> Option<Vec<ExecCommand>> {
    fields.get(key)?.try_clone().ok()?.try_into().ok()
}

fn host_context(unit: &Fields, service: &Fields) -> bool {
    flag(unit, "NeedDaemonReload") == Some(false)
        && string(unit, "LoadState").as_deref() == Some("loaded")
        && strings(unit, "JoinsNamespaceOf").is_some_and(|list| list.is_empty())
        && flag(service, "PrivateNetwork") == Some(false)
        && ["NetworkNamespacePath", "RootDirectory", "RootImage"]
            .iter()
            .all(|key| string(service, key).as_deref() == Some(""))
        && ["User", "Group"]
            .iter()
            .all(|key| matches!(string(service, key).as_deref(), Some("" | "root")))
}

fn guarded(service: &Fields) -> bool {
    commands(service, "ExecStartPre").is_some_and(|commands| {
        commands.len() == 1
            && commands[0].0 == "/usr/bin/engine-os-manager"
            && commands[0].1 == ["/usr/bin/engine-os-manager", "ssh-network-admit"]
            && !commands[0].2
    })
}

fn supported(unit: &Fields, service: &Fields) -> bool {
    host_context(unit, service)
        && string(service, "StandardInput").as_deref() == Some("socket")
        && commands(service, "ExecStart").is_some_and(|commands| {
            commands.len() == 1
                && commands[0].0 == "/usr/sbin/sshd"
                && commands[0].1 == ["/usr/sbin/sshd", "-i", "$SSHD_OPTS"]
        })
        && ["ExecStartPost", "ExecCondition"]
            .iter()
            .all(|key| commands(service, key).is_some_and(|list| list.is_empty()))
        && strings(service, "Environment")
            .is_some_and(|values| values.is_empty() || values == ["SSHD_OPTS="])
        && ["PassEnvironment", "UnsetEnvironment"]
            .iter()
            .all(|key| strings(service, key).is_some_and(|list| list.is_empty()))
        && service
            .get("EnvironmentFiles")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| <Vec<(String, bool)>>::try_from(value).ok())
            .is_some_and(|values| {
                values.is_empty() || values == [("/etc/default/ssh".into(), true)]
            })
}

async fn units(manager: &zbus::Proxy<'_>) -> Result<BTreeMap<String, String>> {
    let reply = manager
        .call_method("ListUnits", &())
        .await
        .map_err(|_| unavailable())?;
    if reply.data().len() > 1024 * 1024 {
        return Err(unavailable());
    }
    let rows: Vec<UnitRow> = reply.body().deserialize().map_err(|_| unavailable())?;
    if rows.len() > 4096 {
        return Err(unavailable());
    }
    let mut result = BTreeMap::new();
    for row in rows {
        if row.0.len() > 512 {
            return Err(unavailable());
        }
        if (row.0 == "sshd.service" || (row.0.starts_with("sshd@") && row.0.ends_with(".service")))
            && (!matches!(
                row.3.as_str(),
                "active" | "reloading" | "activating" | "deactivating" | "inactive" | "failed"
            ) || result.insert(row.0, row.3).is_some())
        {
            return Err(unavailable());
        }
    }
    Ok(result)
}

async fn connection_observation(
    connection: &zbus::Connection,
    uid: u32,
    pid: u32,
    tcp4: Tcp,
    tcp6: Tcp,
    namespace: bool,
    defaults_valid: bool,
) -> Result<Observation> {
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|_| unavailable())?;
    let owner = bus
        .get_name_owner(NAME.try_into().map_err(|_| unavailable())?)
        .await
        .map_err(|_| unavailable())?;
    if bus
        .get_connection_unix_user(owner.clone().into())
        .await
        .map_err(|_| unavailable())?
        != uid
        || bus
            .get_connection_unix_process_id(owner.clone().into())
            .await
            .map_err(|_| unavailable())?
            != pid
    {
        return Err(unavailable());
    }
    let manager = zbus::Proxy::new(
        connection,
        owner.as_str(),
        ROOT,
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|_| unavailable())?;
    // Load the fixed template instance for inspection only; never start a job.
    let template: OwnedObjectPath = manager
        .call("LoadUnit", &(TEMPLATE,))
        .await
        .map_err(|_| unavailable())?;
    let socket: OwnedObjectPath = manager
        .call("GetUnit", &("sshd.socket",))
        .await
        .map_err(|_| unavailable())?;
    let first = units(&manager).await?;
    let unit = fields(
        connection,
        owner.as_str(),
        template.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await?;
    let service = fields(
        connection,
        owner.as_str(),
        template.as_str(),
        "org.freedesktop.systemd1.Service",
    )
    .await?;
    let socket_unit = fields(
        connection,
        owner.as_str(),
        socket.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await?;
    let socket_fields = fields(
        connection,
        owner.as_str(),
        socket.as_str(),
        "org.freedesktop.systemd1.Socket",
    )
    .await?;
    let second = units(&manager).await?;
    if first != second
        || bus
            .get_name_owner(NAME.try_into().map_err(|_| unavailable())?)
            .await
            .map_err(|_| unavailable())?
            != owner
    {
        return Err(unavailable());
    }
    let busy = |state: &str| !matches!(state, "inactive" | "failed");
    let active_units = first.values().filter(|state| busy(state)).count();
    let socket_active = string(&socket_unit, "ActiveState").as_deref() == Some("active")
        && flag(&socket_fields, "Accept") == Some(true);
    let listen = socket_fields
        .get("Listen")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| <Vec<(String, String)>>::try_from(value).ok())
        .is_some_and(|values| {
            !values.is_empty()
                && values.len() <= 2
                && values.iter().all(|(kind, address)| {
                    kind == "Stream" && matches!(address.as_str(), "[::]:22" | "0.0.0.0:22" | "22")
                })
        });
    let configuration = defaults_valid
        && supported(&unit, &service)
        && host_context(&socket_unit, &socket_fields)
        && listen
        && !first.get("sshd.service").is_some_and(|state| busy(state));
    let hook = guarded(&service);
    let active = tcp4.active + tcp6.active;
    let ssh = tcp4.ssh + tcp6.ssh;
    let others = tcp4.other_listeners + tcp6.other_listeners;
    let complete = configuration && hook && socket_active && namespace && others == 0;
    Ok(Observation {
        schema: 1,
        active_tcp_connections: active,
        active_ssh_connections: ssh,
        other_tcp_listeners: others,
        active_ssh_units: active_units,
        socket_active,
        admission_hook_loaded: hook,
        configuration_supported: configuration,
        host_namespace_confirmed: namespace,
        observation_complete: complete,
        ssh_activity: if active != 0 || ssh != 0 || active_units != 0 {
            Some(true)
        } else if complete {
            Some(false)
        } else {
            None
        },
    })
}

pub fn observe() -> Result<Observation> {
    futures_lite::future::block_on(crate::network_observation::bounded(DEADLINE, async {
        let tcp4 = tcp(
            std::str::from_utf8(&read(Path::new("/proc/net/tcp"), 512 * 1024)?)
                .map_err(|_| unavailable())?,
            false,
        )?;
        let tcp6 = tcp(
            std::str::from_utf8(&read(Path::new("/proc/net/tcp6"), 512 * 1024)?)
                .map_err(|_| unavailable())?,
            true,
        )?;
        let namespace = host_namespace_confirmed();
        let defaults_valid = defaults(Path::new("/etc/default/ssh"));
        let connection = zbus::connection::Builder::system()
            .map_err(|_| unavailable())?
            .method_timeout(DEADLINE)
            .build()
            .await
            .map_err(|_| unavailable())?;
        connection_observation(&connection, 0, 1, tcp4, tcp6, namespace, defaults_valid).await
    }))
}

fn unavailable() -> Error {
    Error::Invalid("SSH protection observation is unavailable".into())
}

#[cfg(test)]
#[path = "network_sessions_tests.rs"]
mod tests;
