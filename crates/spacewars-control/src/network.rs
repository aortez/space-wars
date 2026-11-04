//! Kiosk setup protection through the independent root-owned device service.
//! This client never changes the radio or reads saved credentials itself.
use async_io::Async;
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use serde_json::{Value, json};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const NETWORK_SOCKET: &str = "/run/spacewars-system/network.sock";
pub const SETUP_RENEW_INTERVAL: Duration = Duration::from_secs(30);
const REQUEST_TIME: Duration = Duration::from_secs(42);
const REPLY_LIMIT: usize = 16 * 1024;

/// One capability for the entire panel/operation lifetime, including retries.
/// No Debug implementation: the capability must not enter UI or diagnostics.
pub struct SetupHold {
    path: PathBuf,
    token: String,
}

impl SetupHold {
    pub fn new() -> io::Result<Self> {
        Self::at(Path::new(NETWORK_SOCKET))
    }

    /// Select an endpoint for fixtures/tools; root peer authentication remains
    /// mandatory regardless of the socket's location or filesystem owner.
    pub fn at(path: &Path) -> io::Result<Self> {
        let mut bytes = [0; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self {
            path: path.into(),
            token: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }

    pub async fn admit(&self) -> io::Result<()> {
        self.operation("admit", REQUEST_TIME).await
    }

    pub async fn renew(&self) -> io::Result<()> {
        self.operation("renew", REQUEST_TIME).await
    }

    /// Only call after connection cleanup has finished. A lost release leaves
    /// the bounded lease and daemon checkpoint/quiet grace in place.
    pub async fn release(&self) -> io::Result<()> {
        self.operation("release", Duration::from_secs(5)).await
    }

    async fn operation(&self, operation: &str, duration: Duration) -> io::Result<()> {
        self.operation_from(operation, duration, 0).await
    }

    async fn operation_from(
        &self,
        operation: &str,
        duration: Duration,
        server_uid: u32,
    ) -> io::Result<()> {
        let mut request = json!({"schema":1,"command":operation,"token":self.token});
        if operation == "admit" {
            request["kind"] = json!("setup");
        }
        let reply = exchange(&self.path, server_uid, request, duration).await?;
        if reply.get("schema") != Some(&json!(1))
            || !reply.get("error").is_some_and(Value::is_null)
            || !reply.get("status").is_some_and(Value::is_object)
            || (operation != "release"
                && !matches!(
                    reply.get("availability").and_then(Value::as_str),
                    Some("wireless-ready" | "setup-protected")
                ))
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

async fn exchange(
    path: &Path,
    server_uid: u32,
    request: Value,
    duration: Duration,
) -> io::Result<Value> {
    let started = boot_time()?;
    let result = futures_lite::future::or(
        async {
            let mut stream = Async::<UnixStream>::connect(path).await?;
            let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
            let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
            // SAFETY: the stream and correctly sized credentials buffer remain
            // live. Authenticate before writing any capability bytes.
            let result = unsafe {
                libc::getsockopt(
                    stream.get_ref().as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut credentials as *mut libc::ucred).cast(),
                    &mut length,
                )
            };
            if result != 0
                || length as usize != std::mem::size_of_val(&credentials)
                || credentials.uid != server_uid
                || credentials.pid <= 0
            {
                return Err(unavailable());
            }
            let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
            if bytes.len() > 1024 {
                return Err(unavailable());
            }
            bytes.push(b'\n');
            stream.write_all(&bytes).await?;
            let mut reply = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let count = stream.read(&mut buffer).await?;
                if count == 0 || reply.len() + count > REPLY_LIMIT {
                    return Err(unavailable());
                }
                if let Some(end) = buffer[..count].iter().position(|byte| *byte == b'\n') {
                    reply.extend_from_slice(&buffer[..end]);
                    return serde_json::from_slice(&reply).map_err(|_| unavailable());
                }
                reply.extend_from_slice(&buffer[..count]);
            }
        },
        async {
            async_io::Timer::after(duration).await;
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "network protection request timed out",
            ))
        },
    )
    .await;
    // Async timers use monotonic time, which excludes suspend on Linux. A
    // response from before a long sleep must not revive an expired setup lease.
    if boot_time()?
        .checked_sub(started)
        .is_none_or(|elapsed| elapsed >= duration)
    {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "network protection response expired",
        ));
    }
    result
}

fn boot_time() -> io::Result<Duration> {
    let mut now: libc::timespec = unsafe { std::mem::zeroed() };
    // SAFETY: now is a valid writable timespec and CLOCK_BOOTTIME is read-only.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Duration::new(
        now.tv_sec.try_into().map_err(|_| unavailable())?,
        now.tv_nsec.try_into().map_err(|_| unavailable())?,
    ))
}

fn unavailable() -> io::Error {
    io::Error::other("network setup protection is unavailable")
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
