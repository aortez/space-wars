//! Whether the device can vouch for its time. The independent device service
//! decides that from a confirmed network sync or a retained RTC (#256), and
//! answers on its status socket. The Clock shows `--:--` without it (#257).
use std::io;
#[cfg(unix)]
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

/// The device service's status socket, open to root and the kiosk user.
pub const STATUS_SOCKET: &str = "/run/spacewars-system/status.sock";

/// The service never sends a status longer than this.
#[cfg(unix)]
const MAX_STATUS_BYTES: u64 = 16 * 1024;

/// Asks the device service once whether the time is usable, waiting at most
/// `timeout` for each read or write. Only the `time.usable` verdict of the
/// service's first status schema counts: any other answer is an error, never
/// a usable time.
pub fn time_usable(path: &Path, timeout: Duration) -> io::Result<bool> {
    verdict(&status(path, timeout)?)
}

/// The service's one-line status answer, without its newline.
#[cfg(unix)]
fn status(path: &Path, timeout: Duration) -> io::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"{\"schema\":1,\"command\":\"status\"}\n")?;
    let mut answer = Vec::new();
    BufReader::new(stream.take(MAX_STATUS_BYTES + 1)).read_until(b'\n', &mut answer)?;
    if answer.pop() != Some(b'\n') {
        return Err(invalid("the status answer was cut short or too long"));
    }
    Ok(answer)
}

/// Only a unix device runs the service, so elsewhere nothing vouches for the
/// time.
#[cfg(not(unix))]
fn status(_: &Path, _: Duration) -> io::Result<Vec<u8>> {
    Err(io::ErrorKind::Unsupported.into())
}

fn verdict(answer: &[u8]) -> io::Result<bool> {
    let status: Value =
        serde_json::from_slice(answer).map_err(|_| invalid("the status answer is not JSON"))?;
    if status.get("schema").and_then(Value::as_u64) != Some(1) {
        return Err(invalid("the status answer has an unsupported schema"));
    }
    status
        .get("time")
        .and_then(|time| time.get("usable"))
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("the status answer has no time verdict"))
}

fn invalid(why: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;

    /// Asks a one-answer stand-in for the device service, which checks the
    /// request it was sent.
    fn ask(answer: &[u8]) -> io::Result<bool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let answer = answer.to_vec();
        let service = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            let _ = (&stream).write_all(&answer);
            request
        });
        let verdict = time_usable(&path, Duration::from_secs(2));
        assert_eq!(
            service.join().unwrap(),
            "{\"schema\":1,\"command\":\"status\"}\n"
        );
        verdict
    }

    #[test]
    fn the_service_vouches_for_the_time_or_says_it_cannot() {
        assert!(
            ask(b"{\"schema\":1,\"time\":{\"usable\":true,\"source\":\"network-sync\"}}\n")
                .unwrap()
        );
        assert!(
            !ask(b"{\"schema\":1,\"time\":{\"usable\":false,\"reason\":\"rtc-absent\"}}\n")
                .unwrap()
        );
    }

    #[test]
    fn anything_but_a_verdict_is_an_error_and_never_a_usable_time() {
        for answer in [
            &b"{\"schema\":2,\"time\":{\"usable\":true}}\n"[..],
            b"{\"time\":{\"usable\":true}}\n",
            b"{\"schema\":1,\"time\":{\"usable\":\"yes\"}}\n",
            b"{\"schema\":1,\"usable\":true}\n",
            b"{\"schema\":1,\"error\":\"unavailable\"}\n",
            b"not json\n",
            // Cut short before the line ends.
            b"{\"schema\":1,\"time\":{\"usable\":true}}",
            b"",
        ] {
            let error = ask(answer).unwrap_err();
            assert_eq!(
                error.kind(),
                io::ErrorKind::InvalidData,
                "{}",
                String::from_utf8_lossy(answer)
            );
        }
    }

    #[test]
    fn an_answer_longer_than_the_service_ever_sends_is_refused() {
        let padding = "x".repeat(MAX_STATUS_BYTES as usize);
        let answer =
            format!("{{\"schema\":1,\"time\":{{\"usable\":true}},\"pad\":\"{padding}\"}}\n");
        let error = ask(answer.as_bytes()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_missing_service_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(time_usable(&dir.path().join("status.sock"), Duration::from_secs(1)).is_err());
    }

    #[test]
    fn a_silent_service_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let held = thread::spawn(move || listener.accept().unwrap());
        let error = time_usable(&path, Duration::from_millis(100)).unwrap_err();
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "{error:?}"
        );
        drop(held.join());
    }
}
