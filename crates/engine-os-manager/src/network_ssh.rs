//! Admission before the socket-activated sshd authenticates a connection.
//! No shell, unit, endpoint, path or identity is accepted from the caller.
use crate::network_policy::protect_ssh_without_service;
use crate::network_service::{self, Request};
use crate::{Error, Result};
use std::fs;
use std::net::TcpStream;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(45);

pub fn admit() -> Result<()> {
    // SAFETY: geteuid has no preconditions or side effects.
    if unsafe { libc::geteuid() } != 0 {
        return Err(unavailable());
    }
    accepted_socket(libc::STDIN_FILENO, 22)?;
    let request = Request::ProtectSsh { schema: 1 };
    let deadline = Instant::now() + DEADLINE;
    admit_before(
        deadline,
        || {
            network_service::request_before(
                Path::new(network_service::SOCKET_PATH),
                &request,
                0,
                deadline,
            )
        },
        || {
            // Only offline recovery reads /data. A responsive coordinator is
            // authenticated through its root-owned socket, not USB metadata.
            match fs::symlink_metadata("/data/spacewars-system") {
                Ok(meta) if meta.is_dir() && meta.uid() == 0 && meta.mode() & 0o077 == 0 => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(unavailable()),
            }
            let boot_id = crate::linux::boot_id()?;
            protect_ssh_without_service(Path::new(network_service::STATE_DIRECTORY), &boot_id)
                .map(|_guard| ())
        },
    )
}

fn admit_before(
    deadline: Instant,
    mut coordinator: impl FnMut() -> Result<serde_json::Value>,
    mut fallback: impl FnMut() -> Result<()>,
) -> Result<()> {
    loop {
        let reply = coordinator();
        if reply.as_ref().is_ok_and(protected) {
            return Ok(());
        }
        // A stopped/uninstalled service must not unnecessarily disable SSH.
        // Taking its exclusive policy lock excludes a live coordinator. This
        // never initializes state, writes radio settings or clears uncertainty.
        let absent = matches!(&reply, Err(Error::Io(error)) if matches!(error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused));
        if absent && fallback().is_ok() {
            return Ok(());
        }
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            std::thread::sleep(remaining.min(Duration::from_millis(200)));
            if Instant::now() < deadline {
                continue;
            }
        }
        // Keep the last refusal: a request after expiry would replace it with
        // a local deadline error before contacting the coordinator.
        return Err(Error::Invalid(format!(
            "SSH network protection refused: {}; no safe offline admission",
            refusal_reason(&reply),
        )));
    }
}

// Only fixed categories reach the journal. Never format a peer's payload,
// capability, filesystem path or arbitrary transport error.
fn refusal_reason(reply: &Result<serde_json::Value>) -> &'static str {
    match reply {
        Ok(value) if value.get("schema") == Some(&serde_json::json!(1)) => {
            match value.get("error").and_then(|value| value.as_str()) {
                Some("busy") => "coordinator busy",
                Some("unavailable") => "coordinator unavailable",
                Some("rejected") => "coordinator rejected admission",
                Some("expired") => "request expired",
                Some("owner-gone") => "request owner gone",
                Some("invalid-request") => "invalid request",
                _ => "invalid admission reply",
            }
        }
        Ok(_) | Err(Error::Json(_)) => "invalid admission reply",
        Err(Error::Io(error)) => match error.kind() {
            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof => {
                "connection reset"
            }
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => "coordinator timeout",
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                "coordinator absent"
            }
            _ => "coordinator transport failure",
        },
        Err(_) => "coordinator request failed",
    }
}

fn protected(reply: &serde_json::Value) -> bool {
    reply.get("schema") == Some(&serde_json::json!(1))
        && reply.get("error").is_some_and(serde_json::Value::is_null)
        && reply.get("availability").and_then(|value| value.as_str()) == Some("session-protected")
}

fn accepted_socket(fd: RawFd, port: u16) -> Result<()> {
    // SAFETY: fcntl validates fd; only a successful duplicate is made into an
    // OwnedFd. Neither this helper nor TcpStream consumes the connection bytes.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(unavailable());
    }
    let socket = unsafe { OwnedFd::from_raw_fd(duplicate) };
    let mut kind: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&kind) as libc::socklen_t;
    // SAFETY: the live descriptor, initialized output and length are valid.
    if unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut kind as *mut libc::c_int).cast(),
            &mut length,
        )
    } != 0
        || length as usize != std::mem::size_of_val(&kind)
        || kind != libc::SOCK_STREAM
    {
        return Err(unavailable());
    }
    let stream = TcpStream::from(socket);
    let local = stream.local_addr().map_err(|_| unavailable())?;
    stream.peer_addr().map_err(|_| unavailable())?;
    if local.port() != port {
        return Err(unavailable());
    }
    Ok(())
}

fn unavailable() -> Error {
    Error::Invalid("SSH network protection is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, UdpSocket};
    use std::os::unix::net::UnixStream;

    fn session_protected() -> serde_json::Value {
        serde_json::json!({"schema":1,"error":null,"availability":"session-protected"})
    }

    #[test]
    fn coordinator_admission_does_not_require_fallback_boot_evidence() {
        admit_before(
            Instant::now(),
            || Ok(session_protected()),
            || panic!("a protected session must not read the fallback boot ID"),
        )
        .unwrap();
    }

    #[test]
    fn unavailable_fallback_does_not_prevent_later_coordinator_admission() {
        let mut attempts = 0;
        let mut fallbacks = 0;
        admit_before(
            Instant::now() + Duration::from_secs(2),
            || {
                attempts += 1;
                if attempts == 1 {
                    Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
                } else {
                    Ok(session_protected())
                }
            },
            || {
                fallbacks += 1;
                Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
            },
        )
        .unwrap();
        assert_eq!((attempts, fallbacks), (2, 1));
        assert!(
            admit_before(Instant::now(), || Err(unavailable()), || Err(unavailable())).is_err()
        );
    }

    #[test]
    fn helper_requires_an_accepted_tcp_socket_and_preserves_its_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut peer = TcpStream::connect(address).unwrap();
        let (mut socket, _) = listener.accept().unwrap();
        assert!(accepted_socket(listener.as_raw_fd(), address.port()).is_err());
        assert!(accepted_socket(socket.as_raw_fd(), address.port().wrapping_add(1)).is_err());
        peer.write_all(b"SSH-2.0-fixture\r\n").unwrap();
        accepted_socket(socket.as_raw_fd(), address.port()).unwrap();
        let mut bytes = [0; 17];
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"SSH-2.0-fixture\r\n");
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        assert!(accepted_socket(udp.as_raw_fd(), udp.local_addr().unwrap().port()).is_err());
        let (unix, _peer) = UnixStream::pair().unwrap();
        assert!(accepted_socket(unix.as_raw_fd(), 22).is_err());
        assert!(accepted_socket(-1, 22).is_err());
    }

    #[test]
    fn final_refusal_names_the_reason_without_echoing_peer_data() {
        for (wire, expected) in [
            ("busy", "coordinator busy"),
            ("unavailable", "coordinator unavailable"),
            ("rejected", "coordinator rejected admission"),
            ("expired", "request expired"),
            ("owner-gone", "request owner gone"),
            ("invalid-request", "invalid request"),
            ("secret capability", "invalid admission reply"),
        ] {
            let error = admit_before(
                Instant::now(),
                || Ok(serde_json::json!({"schema": 1, "error": wire})),
                || panic!("responsive coordinator refusal must not read offline storage"),
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("secret capability"));
        }
        for (kind, expected) in [
            (std::io::ErrorKind::ConnectionReset, "connection reset"),
            (std::io::ErrorKind::TimedOut, "coordinator timeout"),
        ] {
            assert!(
                admit_before(
                    Instant::now(),
                    || Err(std::io::Error::from(kind).into()),
                    || Err(unavailable())
                )
                .unwrap_err()
                .to_string()
                .contains(expected)
            );
        }
    }

    #[test]
    fn retry_deadline_preserves_the_last_coordinator_refusal() {
        for (wire, expected) in [
            ("busy", "coordinator busy"),
            ("unavailable", "coordinator unavailable"),
            ("rejected", "coordinator rejected admission"),
        ] {
            let mut attempts = 0;
            let error = admit_before(
                Instant::now() + Duration::from_millis(50),
                || {
                    attempts += 1;
                    if attempts == 1 {
                        Ok(serde_json::json!({"schema": 1, "error": wire}))
                    } else {
                        Err(Error::Invalid("network request deadline exceeded".into()))
                    }
                },
                || panic!("responsive coordinator refusal must not read offline storage"),
            )
            .unwrap_err()
            .to_string();
            assert_eq!(attempts, 1, "must not retry after sleeping to the deadline");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn only_explicit_ssh_admission_is_accepted() {
        assert!(protected(&session_protected()));
        for reply in [
            serde_json::json!({"schema":1,"error":null,"availability":"wireless-ready"}),
            serde_json::json!({"schema":1,"error":"unavailable","availability":"session-protected"}),
            serde_json::json!({"schema":2,"error":null,"availability":"session-protected"}),
            serde_json::json!({"schema":1,"availability":"session-protected"}),
        ] {
            assert!(!protected(&reply));
        }
    }
}
