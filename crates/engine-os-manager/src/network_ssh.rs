//! Admission before the socket-activated sshd authenticates a connection.
//! No shell, unit, endpoint, path or identity is accepted from the caller.
use crate::network_policy::{HoldKind, HoldToken, protect_ssh_without_service};
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
    let boot_id = crate::linux::boot_id()?;
    // The state parent must have the same ownership as the daemon requires.
    // A missing parent is permitted on an image that has never run the service.
    match fs::symlink_metadata("/data/spacewars-system") {
        Ok(meta) if meta.is_dir() && meta.uid() == 0 && meta.mode() & 0o077 == 0 => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(unavailable()),
    }
    let token = HoldToken::generate()?;
    let request = Request::Admit {
        schema: 1,
        token: token.expose().to_owned(),
        kind: HoldKind::Ssh,
    };
    let deadline = Instant::now() + DEADLINE;
    loop {
        let reply = network_service::request_before(
            Path::new(network_service::SOCKET_PATH),
            &request,
            0,
            deadline,
        );
        if reply.as_ref().is_ok_and(protected) {
            return Ok(());
        }
        // A stopped/uninstalled service must not unnecessarily disable SSH.
        // Taking its exclusive policy lock excludes a live coordinator. This
        // never initializes state, writes radio settings or clears uncertainty.
        if let Ok(_guard) =
            protect_ssh_without_service(Path::new(network_service::STATE_DIRECTORY), &boot_id)
        {
            return Ok(());
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(unavailable());
        };
        std::thread::sleep(remaining.min(Duration::from_millis(200)));
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
    fn only_explicit_ssh_admission_is_accepted() {
        let valid = serde_json::json!({"schema":1,"error":null,"availability":"session-protected"});
        assert!(protected(&valid));
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
