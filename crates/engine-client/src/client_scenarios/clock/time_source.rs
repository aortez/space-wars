//! Where the Clock learns whether it may show the host's time (#257).
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use spacewars_control::device_time::{STATUS_SOCKET, time_usable};

/// How often the device service is asked.
const POLL: Duration = Duration::from_secs(1);
/// How long each read or write of one question may wait.
const CALL: Duration = Duration::from_secs(2);
/// The longest the first answer may hold up the Clock's first frame.
const FIRST_CALL: Duration = Duration::from_millis(250);
/// A verdict outlives the service's silence this long, so a busy moment
/// doesn't blank the face. After it, the time is unknown. The service takes
/// longer than this to restart, so a restart shows `--:--` until it answers.
const HOLD: Duration = Duration::from_secs(5);

/// Decides whether the Clock may show the host's time.
#[derive(Clone)]
pub(crate) enum TimeSource {
    /// The OS keeps a desktop's time, and the person at it sets it.
    Host,
    /// A kiosk shows a time only while its device service vouches for it.
    Device(Arc<DeviceTime>),
    /// Never trusted: the development preview of `--:--`.
    Unknown,
}

impl TimeSource {
    /// `SPACEWARS_CLOCK_TIME=unknown` previews `--:--`, and `host` trusts the
    /// host's clock even on a kiosk. Otherwise a kiosk build asks its device
    /// service and a desktop trusts its own clock.
    pub(crate) fn from_override(value: Option<&str>) -> Self {
        match value {
            Some("unknown") => Self::Unknown,
            Some("host") => Self::Host,
            // Tests never ask the real service, so none depends on the
            // machine it runs on.
            _ if cfg!(all(feature = "pi-kiosk", not(test))) => Self::Device(DeviceTime::shared()),
            _ => Self::Host,
        }
    }

    pub(crate) fn usable(&self) -> bool {
        match self {
            Self::Host => true,
            Self::Device(device) => device.usable(),
            Self::Unknown => false,
        }
    }
}

/// The device service's latest word on the time, kept fresh by a thread of
/// its own so that no frame ever waits on the service.
pub(crate) struct DeviceTime {
    /// The latest verdict and when it came.
    latest: Mutex<Option<(bool, Instant)>>,
    hold: Duration,
}

impl DeviceTime {
    /// The one watcher every Clock on this kiosk shares. It starts with the
    /// first Clock and keeps watching through pauses and other scenarios.
    pub(crate) fn shared() -> Arc<Self> {
        static SHARED: OnceLock<Arc<DeviceTime>> = OnceLock::new();
        SHARED
            .get_or_init(|| Self::watch(PathBuf::from(STATUS_SOCKET), POLL, HOLD))
            .clone()
    }

    /// Asks the service at `path` on a thread of its own, at once and then
    /// every `poll`, and waits at most `FIRST_CALL` for the first answer
    /// before returning. A kiosk whose service answers promptly opens on its
    /// time rather than on `--:--`, and a stuck service can't hold the Clock
    /// up for longer than that. Until an answer comes, and once `hold` has
    /// passed without one, the time is unknown: a kiosk never falls back to
    /// trusting the host's clock.
    pub(crate) fn watch(path: PathBuf, poll: Duration, hold: Duration) -> Arc<Self> {
        let device = Arc::new(Self {
            latest: Mutex::new(None),
            hold,
        });
        let (answered, first_answer) = mpsc::channel();
        let watcher = Arc::downgrade(&device);
        let spawned = thread::Builder::new()
            .name("clock-device-time".into())
            .spawn(move || {
                let mut answered = Some(answered);
                let mut said = None;
                loop {
                    let answer = time_usable(&path, CALL);
                    let Some(device) = watcher.upgrade() else {
                        return;
                    };
                    said = device.hear(answer, said);
                    drop(device);
                    if let Some(answered) = answered.take() {
                        let _ = answered.send(());
                    }
                    thread::sleep(poll);
                }
            });
        match spawned {
            Ok(_) => {
                let _ = first_answer.recv_timeout(FIRST_CALL);
            }
            Err(error) => {
                tracing::error!(%error, "cannot watch the device service; the time will be unknown");
            }
        }
        device
    }

    /// Keeps the service's latest verdict, and logs the one the Clock follows
    /// whenever it changes.
    fn hear(&self, answer: io::Result<bool>, said: Option<bool>) -> Option<bool> {
        if let Ok(usable) = answer {
            *self.latest.lock().unwrap() = Some((usable, Instant::now()));
        }
        let usable = self.usable();
        if said != Some(usable) {
            match answer {
                Err(error) => tracing::warn!(
                    %error,
                    usable,
                    "the device service did not say whether the time is usable"
                ),
                Ok(_) => {
                    tracing::info!(usable, "the device service says whether the time is usable")
                }
            }
        }
        Some(usable)
    }

    pub(crate) fn usable(&self) -> bool {
        self.latest
            .lock()
            .unwrap()
            .is_some_and(|(usable, at)| usable && at.elapsed() <= self.hold)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU8, Ordering};

    const NO_ANSWER: u8 = 0;
    const USABLE: u8 = 1;
    const UNKNOWN: u8 = 2;

    /// A stand-in device service whose verdict a test can change, or silence.
    fn service(verdict: Arc<AtomicU8>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.sock");
        let listener = UnixListener::bind(&path).unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                let answer = match verdict.load(Ordering::SeqCst) {
                    USABLE => "{\"schema\":1,\"time\":{\"usable\":true}}\n",
                    UNKNOWN => "{\"schema\":1,\"time\":{\"usable\":false}}\n",
                    _ => continue,
                };
                let _ = (&stream).write_all(answer.as_bytes());
            }
        });
        (dir, path)
    }

    fn eventually(device: &DeviceTime, usable: bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while device.usable() != usable {
            assert!(Instant::now() < deadline, "never became usable={usable}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_time_is_usable_only_while_the_service_says_so() {
        let verdict = Arc::new(AtomicU8::new(UNKNOWN));
        let (_dir, path) = service(verdict.clone());
        let device = DeviceTime::watch(path, Duration::from_millis(10), Duration::from_secs(60));
        thread::sleep(Duration::from_millis(50));
        assert!(!device.usable());
        verdict.store(USABLE, Ordering::SeqCst);
        eventually(&device, true);
        verdict.store(UNKNOWN, Ordering::SeqCst);
        eventually(&device, false);
    }

    #[test]
    fn the_first_answer_is_in_before_the_first_frame() {
        let (_dir, path) = service(Arc::new(AtomicU8::new(USABLE)));
        // Polls an hour apart, so only the first question has been asked.
        let device = DeviceTime::watch(path, Duration::from_secs(3600), Duration::from_secs(60));
        assert!(device.usable());

        // A service that takes the question and never answers holds the first
        // frame up only briefly.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.sock");
        let _listener = UnixListener::bind(&path).unwrap();
        let asked = Instant::now();
        let device = DeviceTime::watch(path, Duration::from_secs(3600), Duration::from_secs(60));
        assert!(asked.elapsed() < BRIEFLY, "{:?}", asked.elapsed());
        assert!(!device.usable());
    }

    /// Well under the stuck service's 5 seconds, with room for a busy machine.
    const BRIEFLY: Duration = Duration::from_millis(FIRST_CALL.as_millis() as u64 + 750);

    #[test]
    fn a_service_that_never_finishes_its_answer_holds_the_first_frame_up_only_briefly() {
        // It sends a byte every 50 milliseconds for 5 seconds, so no single
        // read ever waits long enough to time out.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.sock");
        let listener = UnixListener::bind(&path).unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                thread::spawn(move || {
                    for _ in 0..100 {
                        if stream.write_all(b" ").is_err() {
                            return;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                });
            }
        });
        let asked = Instant::now();
        let device = DeviceTime::watch(path, Duration::from_secs(3600), Duration::from_secs(60));
        assert!(asked.elapsed() < BRIEFLY, "{:?}", asked.elapsed());
        assert!(!device.usable());
    }

    #[test]
    fn a_silent_or_missing_service_leaves_the_time_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let missing = DeviceTime::watch(
            dir.path().join("status.sock"),
            Duration::from_millis(10),
            Duration::from_secs(60),
        );
        thread::sleep(Duration::from_millis(50));
        assert!(!missing.usable());

        // A service that vouched for the time and then fell silent is
        // believed only for the hold.
        let verdict = Arc::new(AtomicU8::new(USABLE));
        let (_dir, path) = service(verdict.clone());
        let device = DeviceTime::watch(path, Duration::from_millis(10), Duration::from_secs(2));
        eventually(&device, true);
        verdict.store(NO_ANSWER, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(100));
        assert!(device.usable(), "a short silence keeps the last verdict");
        eventually(&device, false);
    }

    #[test]
    fn the_preview_the_override_and_tests_never_ask_the_service() {
        assert!(!TimeSource::from_override(Some("unknown")).usable());
        assert!(TimeSource::from_override(Some("host")).usable());
        // Not even in a kiosk build.
        assert!(TimeSource::from_override(None).usable());
    }
}
