use super::*;
use crate::install::{SCHEMA, wire::*};
use std::sync::atomic::AtomicUsize;
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
fn status() -> ServiceReply {
    ServiceReply {
        schema: SCHEMA,
        error: None,
        status: Status {
            phase: Phase::Idle,
            ticket: None,
        },
        offer: None,
        aborted: None,
    }
}
#[test]
fn real_socket_authenticates_before_disclosing_request_and_bounds_response_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let _server = Server::bind(
        &path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |bytes, p| {
            assert_eq!(p.pid, std::process::id());
            ServiceRequest::from_json(bytes).unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            serde_json::to_vec(&status()).unwrap()
        }),
    )
    .unwrap();
    let request = ServiceRequest::Status { schema: SCHEMA };
    assert_eq!(
        exchange::<_, ServiceReply>(
            &path,
            uid(),
            Some(std::process::id() + 1),
            &request,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    assert_eq!(
        exchange::<_, ServiceReply>(
            &path,
            uid(),
            Some(std::process::id()),
            &request,
            Duration::from_secs(1)
        )
        .unwrap()
        .status
        .phase,
        Phase::Idle
    );
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    let other = dir.path().join("silent.sock");
    let listener = UnixListener::bind(&other).unwrap();
    let (release, released) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let _stream = listener.accept().unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let before = Instant::now();
    assert_eq!(
        exchange::<_, ServiceReply>(&other, uid(), None, &request, Duration::from_millis(50))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(before.elapsed() < Duration::from_secs(1));
    release.send(()).unwrap();
    worker.join().unwrap();
}
#[test]
fn malformed_oversize_and_unauthorized_requests_never_reach_handler() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let server = Server::bind(
        &path,
        Arc::new(|_| true),
        Arc::new(move |bytes, _| {
            if ServiceRequest::from_json(bytes).is_ok() {
                count.fetch_add(1, Ordering::SeqCst);
            }
            serde_json::to_vec(&status()).unwrap()
        }),
    )
    .unwrap();
    let mut stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = vec![b'x'; MAX_MESSAGE_BYTES + 1];
    bytes.push(b'\n');
    stream.write_all(&bytes).unwrap();
    assert!(stream.read(&mut [0]).is_err() || seen.load(Ordering::SeqCst) == 0);
    assert!(
        ServiceRequest::from_json(br#"{"command":"status","schema":1,"device":"/dev/sda"}"#)
            .is_err()
    );
    drop(server);
    let _server = Server::bind(
        &path,
        Arc::new(|_| false),
        Arc::new(|_, _| panic!("unauthorized handler")),
    )
    .unwrap();
    assert!(
        exchange::<_, ServiceReply>(
            &path,
            uid(),
            None,
            &ServiceRequest::Status { schema: SCHEMA },
            Duration::from_secs(1)
        )
        .is_err()
    );
}
#[test]
fn existing_paths_are_not_replaced_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing");
    std::fs::write(&path, "keep").unwrap();
    assert!(Server::bind(&path, Arc::new(|_| true), Arc::new(|_, _| vec![])).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
}

#[test]
fn concurrent_clients_are_bounded_and_do_not_admit_a_fifth_handler() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.sock");
    let (entered, entries) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let released = std::sync::Mutex::new(released);
    let _server = Server::bind(
        &path,
        Arc::new(|_| true),
        Arc::new(move |_, _| {
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            serde_json::to_vec(&status()).unwrap()
        }),
    )
    .unwrap();
    let mut streams = Vec::new();
    for _ in 0..4 {
        let mut stream = UnixStream::connect(&path).unwrap();
        stream.write_all(b"{}\n").unwrap();
        streams.push(stream);
        entries.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    assert!(
        exchange::<_, ServiceReply>(
            &path,
            uid(),
            None,
            &ServiceRequest::Status { schema: SCHEMA },
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert!(entries.try_recv().is_err());
    for _ in 0..4 {
        release.send(()).unwrap();
    }
}
