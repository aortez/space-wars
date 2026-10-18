//! Deployment capabilities arrive on stdin, never command arguments or logs.
use crate::network_policy::{HoldKind, HoldToken};
use crate::network_service::Request;
use crate::{Error, Result};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub enum Operation {
    Admit,
    Renew,
    Release,
}

pub fn request(operation: Operation) -> Result<Request> {
    let input = std::io::stdin();
    let token = read_token(input.as_fd(), Duration::from_secs(5))?;
    Ok(match operation {
        Operation::Admit => Request::Admit {
            schema: 1,
            token: token.expose().to_owned(),
            kind: HoldKind::Deployment,
        },
        Operation::Renew => Request::Renew {
            schema: 1,
            token: token.expose().to_owned(),
        },
        Operation::Release => Request::Release {
            schema: 1,
            token: token.expose().to_owned(),
        },
    })
}

fn read_token(input: BorrowedFd<'_>, timeout: Duration) -> Result<HoldToken> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::with_capacity(64);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(invalid)?;
        let mut poll = libc::pollfd {
            fd: input.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one live descriptor and writable pollfd; no FD flags change.
        let ready = unsafe {
            libc::poll(
                &mut poll,
                1,
                remaining.as_millis().clamp(1, i32::MAX as u128) as i32,
            )
        };
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if ready <= 0 || poll.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(invalid());
        }
        let mut byte = 0_u8;
        // SAFETY: input and the one-byte output remain valid. This CLI is the
        // sole reader of its stdin; polling also permits EOF without blocking.
        let read = unsafe { libc::read(input.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) };
        if read < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if read != 1 {
            return Err(invalid());
        }
        if byte == b'\n' {
            return HoldToken::parse(String::from_utf8(bytes).map_err(|_| invalid())?);
        }
        if bytes.len() == 64 {
            return Err(invalid());
        }
        bytes.push(byte);
    }
}

fn invalid() -> Error {
    Error::Invalid("expected one 64-character deployment capability line on stdin".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    #[test]
    fn deployment_input_is_bounded_and_invalid_input_is_never_echoed() {
        let secret = "a".repeat(64);
        for (input, valid) in [
            (format!("{secret}\n"), true),
            (format!("{}\n", "b".repeat(63)), false),
            (format!("{}\n", "c".repeat(65)), false),
            (format!("{}\n", "D".repeat(64)), false),
            (secret.clone(), false),
            ("\n".into(), false),
        ] {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            writer.write_all(input.as_bytes()).unwrap();
            drop(writer);
            let result = read_token(reader.as_fd(), Duration::from_secs(1));
            assert_eq!(result.is_ok(), valid);
            assert!(!format!("{result:?}").contains(&secret));
        }
        let (reader, _writer) = UnixStream::pair().unwrap();
        let start = Instant::now();
        assert!(read_token(reader.as_fd(), Duration::from_millis(25)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
