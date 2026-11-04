use super::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::thread;

fn uid() -> u32 {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

#[test]
fn setup_capability_is_reused_across_admission_renewal_and_release() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let mut capability = None;
        for operation in ["admit", "renew", "release"] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["command"], operation);
            assert_eq!(request["schema"], 1);
            let token = request["token"].as_str().unwrap();
            assert_eq!(token.len(), 64);
            assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
            assert_eq!(capability.get_or_insert_with(|| token.to_owned()), token);
            if operation == "admit" {
                assert_eq!(request["kind"], "setup");
            } else {
                assert!(request.get("kind").is_none());
            }
            writeln!(
                stream,
                "{}",
                json!({
                    "schema":1, "error":null, "status":{},
                    "availability": match operation {
                        "release" => Value::Null,
                        "renew" => json!("setup-protected"),
                        _ => json!("wireless-ready"),
                    },
                })
            )
            .unwrap();
        }
    });
    let hold = SetupHold::at(&path).unwrap();
    futures_lite::future::block_on(async {
        for operation in ["admit", "renew", "release"] {
            hold.operation_from(operation, Duration::from_secs(2), uid())
                .await
                .unwrap();
        }
    });
    server.join().unwrap();
}

#[test]
fn setup_rejects_unconfirmed_or_unbounded_replies() {
    for reply in [
        json!({"schema":1,"error":null,"status":{},"availability":"ethernet-ready"}).to_string(),
        json!({"schema":1,"error":"rejected","status":{},"availability":"wireless-ready"})
            .to_string(),
        json!({"schema":1,"error":null,"availability":"wireless-ready"}).to_string(),
        json!({"schema":2,"error":null,"status":{},"availability":"wireless-ready"}).to_string(),
        "x".repeat(REPLY_LIMIT + 1),
        "not json".into(),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let _ = writeln!(stream, "{reply}");
        });
        let hold = SetupHold::at(&path).unwrap();
        assert!(
            futures_lite::future::block_on(hold.operation_from(
                "admit",
                Duration::from_secs(2),
                uid(),
            ))
            .is_err()
        );
        server.join().unwrap();
    }
}

#[test]
fn unexpected_peer_receives_no_capability_and_a_stalled_peer_times_out() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("network.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert!(bytes.is_empty());
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // Wait for the client deadline and EOF, without supplying a reply.
        stream.read_to_end(&mut bytes).unwrap();
        assert!(!bytes.is_empty());
    });
    let hold = SetupHold::at(&path).unwrap();
    futures_lite::future::block_on(async {
        assert!(
            hold.operation_from("admit", Duration::from_secs(2), uid().wrapping_add(1))
                .await
                .is_err()
        );
        let error = hold
            .operation_from("admit", Duration::from_millis(50), uid())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    });
    server.join().unwrap();
}
