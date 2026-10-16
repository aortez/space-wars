use super::*;
use crate::owned_fs::open_directory;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::{JoinHandle, JoinSet};
use tokio_rustls::{TlsAcceptor, rustls};

use crate::test_support::{EXPIRES, Fault, Fixture, descriptor, device, location, time};

#[tokio::test]
async fn service_request_receipt_is_included_in_preparation_and_session_deadlines() {
    let now = boottime_ms().unwrap();
    let age = now.min(PREPARATION_MS / 2);
    let started = now - age;
    let session = DownloadSession::new_at(TestControl::new(), started).unwrap();
    assert_eq!(
        session.http.budget.deadline(Deadline::Preparation),
        started + PREPARATION_MS
    );
    assert_eq!(
        session.http.budget.deadline(Deadline::Session),
        started + SESSION_MS
    );
    session
        .http
        .budget
        .clock_offset
        .store(PREPARATION_MS - age + 1, Ordering::SeqCst);
    assert!(
        session
            .http
            .budget
            .check_deadline(Deadline::Preparation)
            .is_err()
    );
    assert!(
        session
            .http
            .budget
            .check_deadline(Deadline::Session)
            .is_ok()
    );
    assert!(DownloadSession::new_at(TestControl::new(), now + SESSION_MS).is_err());
    if let Some(expired) = now.checked_sub(PREPARATION_MS) {
        assert!(DownloadSession::new_at(TestControl::new(), expired).is_err());
    }
}

struct TestControl {
    time: Mutex<Option<UsableTime>>,
    cancelled: AtomicBool,
    cancel_after: AtomicU64,
    expire_after: AtomicU64,
    progress: Mutex<Vec<Progress>>,
}

impl TestControl {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            time: Mutex::new(time()),
            cancelled: AtomicBool::new(false),
            cancel_after: AtomicU64::new(u64::MAX),
            expire_after: AtomicU64::new(u64::MAX),
            progress: Mutex::new(Vec::new()),
        })
    }
    fn ready(&self) -> bool {
        self.progress
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event.phase, Phase::Ready))
    }
}

impl Control for TestControl {
    fn checkpoint(&self) -> Result<Option<UsableTime>> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(Error::Download("cancelled"));
        }
        Ok(*self.time.lock().unwrap())
    }
    fn progress(&self, progress: Progress) {
        if matches!(progress.phase, Phase::Downloading) {
            if progress.bytes >= self.cancel_after.load(Ordering::SeqCst) {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            if progress.bytes >= self.expire_after.load(Ordering::SeqCst) {
                *self.time.lock().unwrap() = Some(UsableTime(EXPIRES.parse().unwrap()));
            }
        }
        self.progress.lock().unwrap().push(progress);
    }
}

#[derive(Clone)]
enum Action {
    Normal,
    IgnoreRange,
    WrongRange,
    WrongValidator,
    MissingLength,
    Encoding,
    Corrupt,
    Status(u16),
    Redirect(String),
    StallHeaders,
    StallBody,
    Cut(usize),
}

#[derive(Clone, Debug)]
struct Request {
    path: String,
    range: Option<String>,
    if_range: Option<String>,
}

struct Server {
    base: Url,
    task: JoinHandle<()>,
    requests: Arc<Mutex<Vec<Request>>>,
    actions: Arc<Mutex<BTreeMap<String, VecDeque<Action>>>>,
    active: Arc<AtomicUsize>,
}

struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn new(files: BTreeMap<String, Vec<u8>>) -> Self {
        Self::with_tls(files, false).await
    }

    async fn with_tls(files: BTreeMap<String, Vec<u8>>, tls: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = if tls {
            Url::parse(&format!(
                "https://localhost:{}/",
                listener.local_addr().unwrap().port()
            ))
            .unwrap()
        } else {
            Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap()
        };
        let acceptor = tls.then(|| {
            let cert = rustls::pki_types::CertificateDer::from(
                include_bytes!("../tests/fixtures/download-localhost-cert.der").to_vec(),
            );
            let key = rustls::pki_types::PrivatePkcs8KeyDer::from(
                include_bytes!("../tests/fixtures/download-localhost-key.pk8").to_vec(),
            );
            let config = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key.into())
                .unwrap();
            TlsAcceptor::from(Arc::new(config))
        });
        let requests = Arc::new(Mutex::new(Vec::new()));
        let actions = Arc::new(Mutex::new(BTreeMap::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let (r, a, connections) = (requests.clone(), actions.clone(), active.clone());
        let files = Arc::new(files);
        let task = tokio::spawn(async move {
            let mut workers = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.unwrap();
                        connections.fetch_add(1, Ordering::SeqCst);
                        let active = Active(connections.clone());
                        let (files, requests, actions) = (files.clone(), r.clone(), a.clone());
                        let acceptor = acceptor.clone();
                        workers.spawn(async move {
                            let _active = active;
                            if let Some(acceptor) = acceptor {
                                if let Ok(socket) = acceptor.accept(socket).await { serve(socket, files, requests, actions).await; }
                            } else { serve(socket, files, requests, actions).await; }
                        });
                    },
                    result = workers.join_next(), if !workers.is_empty() => { result.unwrap().unwrap(); },
                }
            }
        });
        Self {
            base,
            task,
            requests,
            actions,
            active,
        }
    }

    fn set(&self, path: &str, actions: impl IntoIterator<Item = Action>) {
        self.actions
            .lock()
            .unwrap()
            .insert(path.into(), actions.into_iter().collect());
    }

    fn image_requests(&self) -> Vec<Request> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.path.starts_with("/images/"))
            .cloned()
            .collect()
    }

    async fn idle(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}

async fn serve(
    mut socket: impl AsyncRead + AsyncWrite + Unpin,
    files: Arc<BTreeMap<String, Vec<u8>>>,
    requests: Arc<Mutex<Vec<Request>>>,
    actions: Arc<Mutex<BTreeMap<String, VecDeque<Action>>>>,
) {
    let mut buffer = Vec::new();
    loop {
        let mut part = [0u8; 1024];
        let count = socket.read(&mut part).await.unwrap();
        if count == 0 {
            return;
        }
        buffer.extend_from_slice(&part[..count]);
        assert!(buffer.len() <= 16 * 1024);
        if buffer.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&buffer).unwrap();
    let mut lines = text.split("\r\n");
    let path = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let headers: BTreeMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    assert!(!headers.contains_key("authorization"));
    assert!(!headers.contains_key("cookie"));
    assert!(!headers.contains_key("referer"));
    assert_eq!(
        headers.get("accept-encoding").map(String::as_str),
        Some("identity")
    );
    let request = Request {
        path: path.clone(),
        range: headers.get("range").cloned(),
        if_range: headers.get("if-range").cloned(),
    };
    requests.lock().unwrap().push(request.clone());
    let path = path.split('?').next().unwrap();
    let action = actions
        .lock()
        .unwrap()
        .get_mut(path)
        .and_then(VecDeque::pop_front)
        .unwrap_or(Action::Normal);
    if matches!(action, Action::StallHeaders) {
        let _ = socket.read(&mut [0u8; 1]).await;
        return;
    }
    let Some(mut bytes) = files.get(path).cloned() else {
        let _ = socket
            .write_all(b"HTTP/1.1 404 Missing\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    };
    let total = bytes.len();
    let requested = request.range.as_deref().map(|value| {
        value
            .strip_prefix("bytes=")
            .unwrap()
            .strip_suffix('-')
            .unwrap()
            .parse::<usize>()
            .unwrap()
    });
    let mut status = 200;
    let mut extra = String::new();
    if let Action::Redirect(location) = &action {
        status = 302;
        extra = format!("Location: {location}\r\n");
        bytes.clear();
    } else if let Action::Status(value) = action {
        status = value;
        bytes.clear();
    } else if path.starts_with("/images/") {
        let tag = if matches!(action, Action::WrongValidator) {
            "\"changed\""
        } else {
            "\"asset-v1\""
        };
        extra = format!("ETag: {tag}\r\n");
        if let Some(start) = requested.filter(|_| !matches!(action, Action::IgnoreRange)) {
            status = 206;
            let first = if matches!(action, Action::WrongRange) {
                start + 1
            } else {
                start
            };
            extra.push_str(&format!(
                "Content-Range: bytes {first}-{}/{total}\r\n",
                total - 1
            ));
            bytes = bytes[start..].to_vec();
        }
        if matches!(action, Action::Encoding) {
            extra.push_str("Content-Encoding: gzip\r\n");
        }
        if matches!(action, Action::Corrupt) {
            bytes[0] ^= 1;
        }
    }
    let length = if matches!(action, Action::MissingLength) {
        String::new()
    } else {
        format!("Content-Length: {}\r\n", bytes.len())
    };
    if socket
        .write_all(
            format!("HTTP/1.1 {status} Fixture\r\n{length}{extra}Connection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .is_err()
    {
        return;
    }
    if matches!(action, Action::StallBody) {
        let _ = socket.read(&mut [0u8; 1]).await;
        return;
    }
    if let Action::Cut(count) = action {
        let _ = socket.write_all(&bytes[..count]).await;
        let _ = socket.flush().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    } else {
        let _ = socket.write_all(&bytes).await;
    }
}

struct Case {
    _directory: tempfile::TempDir,
    trust: TrustStore,
    stage: StagingStore,
    image: ImageDescriptor,
    device: DeviceIdentity,
    server: Server,
    control: Arc<TestControl>,
}

impl Case {
    async fn new(channel: Channel) -> Self {
        Self::with_tls(channel, false).await
    }

    async fn with_tls(channel: Channel, tls: bool) -> Self {
        let fixture = Fixture::new().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stage");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let stage = StagingStore::under(open_directory(&path, owner).unwrap(), owner).unwrap();
        let bytes: Vec<u8> = (0..65536u32).flat_map(u32::to_le_bytes).collect();
        let compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
        let mut image = descriptor(channel, 4);
        image.expanded = crate::Blob {
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes),
        };
        image.compressed = crate::Blob {
            bytes: compressed.len() as u64,
            sha256: Sha256::digest(&compressed),
        };
        let feed = fixture
            .publish(
                1,
                std::slice::from_ref(&image),
                EXPIRES.parse().unwrap(),
                Fault::None,
            )
            .await;
        let mut files: BTreeMap<_, _> = feed
            .files
            .into_iter()
            .map(|(name, bytes)| (format!("/metadata/{name}"), bytes))
            .collect();
        files.insert(
            format!(
                "/images/{}.{}",
                image.compressed.sha256,
                image.target_name()
            ),
            compressed,
        );
        let server = if tls {
            Server::with_tls(files, true).await
        } else {
            Server::new(files).await
        };
        let trust = TrustStore::initialize(
            &directory.path().join("trust"),
            &fixture.root_bytes,
            location(),
        )
        .unwrap();
        let mut device = device();
        device.inactive_slot_bytes = 1024 * 1024;
        Self {
            _directory: directory,
            trust,
            stage,
            image,
            device,
            server,
            control: TestControl::new(),
        }
    }

    fn target(&self) -> String {
        format!(
            "/images/{}.{}",
            self.image.compressed.sha256,
            self.image.target_name()
        )
    }
    fn ready(&self) -> bool {
        self._directory
            .path()
            .join("stage")
            .join(format!("{}.ext4.zst", self.image.compressed.sha256))
            .exists()
    }
    fn session(&self) -> DownloadSession {
        let mut session = DownloadSession::new(self.control.clone()).unwrap();
        session.http.client = client(false).unwrap();
        session.http.fixture = Some(self.server.base.clone());
        session
    }
    async fn run(&mut self) -> Result<Outcome> {
        self.session()
            .run(
                &mut self.trust,
                &mut self.stage,
                self.image.channel,
                &self.device,
                &ImageHistory::default(),
            )
            .await
    }
    async fn prefix(&mut self) -> u64 {
        self.control.cancel_after.store(1, Ordering::SeqCst);
        assert!(self.run().await.is_err());
        self.control.cancel_after.store(u64::MAX, Ordering::SeqCst);
        self.control.cancelled.store(false, Ordering::SeqCst);
        let crate::staging_linux::RetainedStatus::Unverified { bytes, total, .. } =
            self.stage.status().unwrap()
        else {
            panic!("missing partial")
        };
        assert!(bytes > 0 && bytes < total);
        assert!(!self.ready());
        self.server.idle().await;
        bytes
    }
}

#[tokio::test]
async fn real_http_signed_feeds_stage_both_channels_and_reverify_completed_bytes() {
    for channel in [Channel::Release, Channel::Nightly] {
        let mut case = Case::new(channel).await;
        assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
        assert!(case.ready() && case.control.ready());
        assert_eq!(case.server.image_requests().len(), 1);
        assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
        assert_eq!(
            case.server.image_requests().len(),
            1,
            "retained complete bytes need no new image transfer"
        );
        case.server.idle().await;
    }
}

#[tokio::test]
async fn cancelled_transfer_resumes_with_the_exact_range_and_strong_validator() {
    let mut case = Case::new(Channel::Release).await;
    let offset = case.prefix().await;
    assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
    let requests = case.server.image_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].range.as_deref(),
        Some(format!("bytes={offset}-").as_str())
    );
    assert_eq!(requests[1].if_range.as_deref(), Some("\"asset-v1\""));
}

#[tokio::test]
async fn incorrect_range_or_validator_gets_one_full_restart_and_no_mixed_bytes() {
    for action in [
        Action::WrongRange,
        Action::WrongValidator,
        Action::Status(416),
    ] {
        let mut case = Case::new(Channel::Release).await;
        case.prefix().await;
        case.server.set(&case.target(), [action]);
        assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
        let requests = case.server.image_requests();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].range.is_some());
        assert!(requests[2].range.is_none() && requests[2].if_range.is_none());
    }
}

#[tokio::test]
async fn ignored_range_uses_the_full_response_without_an_extra_request() {
    let mut case = Case::new(Channel::Release).await;
    case.prefix().await;
    case.server.set(&case.target(), [Action::IgnoreRange]);
    assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
    assert_eq!(case.server.image_requests().len(), 2);
}

#[tokio::test]
async fn invalid_full_responses_and_corruption_never_publish_or_retry() {
    for action in [
        Action::Encoding,
        Action::MissingLength,
        Action::Corrupt,
        Action::Status(503),
    ] {
        let mut case = Case::new(Channel::Release).await;
        case.server.set(&case.target(), [action]);
        assert!(case.run().await.is_err());
        assert!(!case.ready() && !case.control.ready());
        assert_eq!(case.server.image_requests().len(), 1);
        case.server.idle().await;
    }
}

#[tokio::test]
async fn abrupt_server_disconnect_retains_a_bounded_noninstallable_partial() {
    let mut case = Case::new(Channel::Release).await;
    case.server.set(&case.target(), [Action::Cut(2048)]);
    assert!(case.run().await.is_err());
    assert!(!case.ready());
    let crate::staging_linux::RetainedStatus::Unverified { bytes, .. } =
        case.stage.status().unwrap()
    else {
        panic!("missing partial")
    };
    assert_eq!(bytes, 2048);
    assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
}

#[tokio::test]
async fn unknown_or_expired_time_makes_no_ready_candidate() {
    let mut case = Case::new(Channel::Release).await;
    *case.control.time.lock().unwrap() = None;
    assert!(matches!(case.run().await, Err(Error::WaitingForTime)));
    assert!(case.server.requests.lock().unwrap().is_empty());
    *case.control.time.lock().unwrap() = time();
    case.control
        .expire_after
        .store(case.image.compressed.bytes, Ordering::SeqCst);
    assert!(case.run().await.is_err());
    assert!(!case.ready() && !case.control.ready());
}

#[tokio::test]
async fn installed_or_unselected_channels_do_not_download_and_clear_old_candidates() {
    let mut case = Case::new(Channel::Release).await;
    case.run().await.unwrap();
    let history = ImageHistory {
        installed_sha256: Some(case.image.compressed.sha256),
        ..ImageHistory::default()
    };
    let outcome = case
        .session()
        .run(
            &mut case.trust,
            &mut case.stage,
            Channel::Release,
            &case.device,
            &history,
        )
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        Outcome::Ineligible {
            reason: Eligibility::AlreadyInstalled,
            ..
        }
    ));
    assert!(!case.ready());
    let outcome = case
        .session()
        .run(
            &mut case.trust,
            &mut case.stage,
            Channel::Nightly,
            &case.device,
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    assert!(matches!(outcome, Outcome::NoCandidate));
    assert_eq!(case.server.image_requests().len(), 1);
}

#[tokio::test]
async fn redirect_loops_credentials_downgrades_and_query_errors_are_bounded_and_redacted() {
    for location in [
        "http://updates.example/metadata/timestamp.json",
        "https://user:secret@updates.example/metadata/timestamp.json",
        "/metadata/timestamp.json?private=secret",
    ] {
        let mut case = Case::new(Channel::Release).await;
        case.server.set(
            "/metadata/timestamp.json",
            (0..10).map(|_| Action::Redirect(location.into())),
        );
        let error = case.run().await.unwrap_err();
        let message = format!("{error:?} {error}");
        assert!(
            !message.contains("secret") && !message.contains("private="),
            "{message}"
        );
        assert!(case.server.requests.lock().unwrap().len() <= MAX_REDIRECTS + 3);
        assert!(case.server.image_requests().is_empty());
        case.server.idle().await;
    }
}

#[tokio::test]
async fn preparation_deadline_cancels_a_stalled_metadata_request() {
    let mut case = Case::new(Channel::Release).await;
    case.server
        .set("/metadata/timestamp.json", [Action::StallHeaders]);
    let mut session = case.session();
    Arc::get_mut(&mut session.http.budget)
        .unwrap()
        .limits
        .prepare = 150;
    let error = session
        .run(
            &mut case.trust,
            &mut case.stage,
            Channel::Release,
            &case.device,
            &ImageHistory::default(),
        )
        .await
        .unwrap_err();
    assert!(format!("{error}").contains("preparation deadline"));
    assert!(case.server.image_requests().is_empty());
    case.server.idle().await;
}

#[tokio::test]
async fn payload_no_progress_deadline_closes_the_connection_and_keeps_no_ready_image() {
    let mut case = Case::new(Channel::Release).await;
    case.server.set(&case.target(), [Action::StallBody]);
    let mut session = case.session();
    Arc::get_mut(&mut session.http.budget)
        .unwrap()
        .limits
        .progress = 100;
    let error = session
        .run(
            &mut case.trust,
            &mut case.stage,
            Channel::Release,
            &case.device,
            &ImageHistory::default(),
        )
        .await
        .unwrap_err();
    assert!(format!("{error}").contains("no progress"));
    assert!(!case.ready() && !case.control.ready());
    case.server.idle().await;
}

#[tokio::test]
async fn session_clock_includes_suspend_and_cancellation_does_not_wait_for_server_bytes() {
    for cancel in [true, false] {
        let mut case = Case::new(Channel::Release).await;
        case.server.set(&case.target(), [Action::StallBody]);
        let session = case.session();
        let clock = session.http.budget.clock_offset.clone();
        let requests = case.server.requests.clone();
        let control = case.control.clone();
        let interrupt = async move {
            while !requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.path.starts_with("/images/"))
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            if cancel {
                control.cancelled.store(true, Ordering::SeqCst);
            } else {
                clock.store(SESSION_MS + 1, Ordering::SeqCst);
            }
        };
        let history = ImageHistory::default();
        let (result, ()) = tokio::join!(
            session.run(
                &mut case.trust,
                &mut case.stage,
                Channel::Release,
                &case.device,
                &history
            ),
            interrupt
        );
        let message = format!("{}", result.unwrap_err());
        assert!(
            message.contains(if cancel {
                "cancelled"
            } else {
                "session deadline"
            }),
            "{message}"
        );
        assert!(!case.ready());
        case.server.idle().await;
    }
}

#[tokio::test]
async fn request_budget_is_shared_across_metadata_and_redirects() {
    let mut case = Case::new(Channel::Release).await;
    let session = case.session();
    session
        .http
        .budget
        .requests
        .store(MAX_HTTP_REQUESTS - 1, Ordering::SeqCst);
    assert!(
        session
            .run(
                &mut case.trust,
                &mut case.stage,
                Channel::Release,
                &case.device,
                &ImageHistory::default()
            )
            .await
            .is_err()
    );
    assert_eq!(case.server.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn signed_redirected_image_can_finish_without_exposing_redirect_query_parameters() {
    let mut case = Case::new(Channel::Release).await;
    case.server.set(
        &case.target(),
        [Action::Redirect(format!(
            "{}?fixture-secret=value",
            case.target()
        ))],
    );
    assert!(matches!(case.run().await.unwrap(), Outcome::Ready { .. }));
    assert_eq!(case.server.image_requests().len(), 2);
    let status = serde_json::to_string(&*case.control.progress.lock().unwrap()).unwrap();
    assert!(!status.contains("fixture-secret") && !status.contains("https://"));
}

#[tokio::test]
async fn https_checks_certificate_trust_and_hostname_before_any_metadata_is_read() {
    // Public fixture key/certificate: trusted only by this test client. The
    // production constructor has no caller-supplied CA or verification bypass.
    for mode in ["trusted", "unknown-ca", "wrong-hostname"] {
        let mut case = Case::with_tls(Channel::Release, true).await;
        let mut session = case.session();
        let mut builder = client_builder(true);
        if mode != "unknown-ca" {
            builder = builder.tls_certs_merge([reqwest::Certificate::from_der(include_bytes!(
                "../tests/fixtures/download-localhost-cert.der"
            ))
            .unwrap()]);
        }
        session.http.client = builder.build().unwrap();
        if mode == "wrong-hostname" {
            session
                .http
                .fixture
                .as_mut()
                .unwrap()
                .set_host(Some("127.0.0.1"))
                .unwrap();
        }
        let result = session
            .run(
                &mut case.trust,
                &mut case.stage,
                Channel::Release,
                &case.device,
                &ImageHistory::default(),
            )
            .await;
        if mode == "trusted" {
            assert!(matches!(result.unwrap(), Outcome::Ready { .. }));
            assert!(case.ready());
        } else {
            assert!(result.is_err(), "{mode}");
            assert!(
                case.server.requests.lock().unwrap().is_empty(),
                "TLS rejection precedes HTTP"
            );
            assert!(!case.ready());
        }
        case.server.idle().await;
    }
}

#[tokio::test]
async fn tuf_errors_do_not_render_a_private_configured_base_url() {
    let mut case = Case::new(Channel::Release).await;
    case.trust.state.location.metadata =
        Url::parse("https://updates.example/private-base-token/").unwrap();
    let error = case.run().await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains("private-base-token"));
    assert!(case.server.image_requests().is_empty());
}

#[tokio::test]
async fn losing_usable_time_during_a_metadata_request_remains_waiting_for_time() {
    let mut case = Case::new(Channel::Release).await;
    case.server
        .set("/metadata/timestamp.json", [Action::StallHeaders]);
    let requests = case.server.requests.clone();
    let control = case.control.clone();
    let session = case.session();
    let history = ImageHistory::default();
    let lose_time = async move {
        while !requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.path == "/metadata/timestamp.json")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        *control.time.lock().unwrap() = None;
    };
    let (result, ()) = tokio::join!(
        session.run(
            &mut case.trust,
            &mut case.stage,
            Channel::Release,
            &case.device,
            &history
        ),
        lose_time
    );
    assert!(matches!(result, Err(Error::WaitingForTime)), "{result:?}");
    case.server.idle().await;
}

#[tokio::test]
async fn refusal_and_rate_limiting_preserve_a_valid_prefix_without_an_extra_request() {
    for status in [403, 404, 429, 503] {
        let mut case = Case::new(Channel::Release).await;
        let offset = case.prefix().await;
        case.server.set(&case.target(), [Action::Status(status)]);
        assert!(case.run().await.is_err());
        let crate::staging_linux::RetainedStatus::Unverified { bytes, .. } =
            case.stage.status().unwrap()
        else {
            panic!("missing partial")
        };
        assert_eq!(bytes, offset);
        assert_eq!(case.server.image_requests().len(), 2);
        assert!(!case.ready());
    }
}

#[tokio::test]
async fn network_failure_retains_later_usable_time_against_clock_rollback() {
    let mut case = Case::new(Channel::Release).await;
    case.server.set(&case.target(), [Action::StallBody]);
    let session = case.session();
    let budget = session.http.budget.clone();
    let control = case.control.clone();
    let requests = case.server.requests.clone();
    let expire = UsableTime(EXPIRES.parse().unwrap());
    let interrupt = async move {
        while !requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.path.starts_with("/images/"))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        *control.time.lock().unwrap() = Some(expire);
        while budget
            .latest_time
            .lock()
            .unwrap()
            .is_none_or(|time| time.0 < expire.0)
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        control.cancelled.store(true, Ordering::SeqCst);
    };
    let history = ImageHistory::default();
    let (result, ()) = tokio::join!(
        session.run(
            &mut case.trust,
            &mut case.stage,
            Channel::Release,
            &case.device,
            &history
        ),
        interrupt
    );
    assert!(matches!(result, Err(Error::Download("cancelled"))));
    assert!(matches!(
        case.trust
            .cached(time(), Channel::Release, &case.device, &history)
            .await,
        Err(Error::TimeMovedBackwards)
    ));
    assert!(!case.ready());
    case.server.idle().await;
}
