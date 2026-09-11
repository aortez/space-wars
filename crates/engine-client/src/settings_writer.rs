//! One ordered background writer for all runtime settings changes. Bursts
//! coalesce to their newest complete snapshot; filesystem I/O never holds the
//! status or application-settings locks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use engine_common::Settings;
use slint::{ComponentHandle, Timer, TimerMode};

pub(crate) type SaveReceipt = mpsc::Receiver<Result<(), String>>;
type WriteSettings = Arc<dyn Fn(&Settings) -> Result<(), String> + Send + Sync>;

#[derive(Clone, Debug, Default)]
pub(crate) struct SaveStatus {
    revision: u64,
    pub pending: bool,
    pub error: Option<String>,
}

struct SaveRequest {
    revision: u64,
    settings: Settings,
    response: mpsc::Sender<Result<(), String>>,
}

#[derive(Clone)]
pub(crate) struct SettingsWriter {
    sender: mpsc::Sender<SaveRequest>,
    status: Arc<Mutex<SaveStatus>>,
}

impl SettingsWriter {
    pub(crate) fn new(path: PathBuf) -> std::io::Result<Self> {
        Self::with_writer(Arc::new(move |settings| {
            crate::settings::save_settings(settings, &path).map_err(|error| error.to_string())
        }))
    }

    fn with_writer(write: WriteSettings) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<SaveRequest>();
        let status = Arc::new(Mutex::new(SaveStatus::default()));
        let worker_status = Arc::clone(&status);
        std::thread::Builder::new()
            .name("spacewars-settings".into())
            .spawn(move || {
                while let Ok(mut request) = receiver.recv() {
                    let mut responses = Vec::new();
                    // Only snapshots queued before this write can be coalesced.
                    // A later request is handled after the in-flight write ends.
                    while let Ok(newer) = receiver.try_recv() {
                        responses.push(request.response);
                        request = newer;
                    }
                    let started = std::time::Instant::now();
                    let result = write(&request.settings);
                    let elapsed_ms = started.elapsed().as_millis() as u64;
                    tracing::debug!(
                        save_revision = request.revision,
                        elapsed_ms,
                        "runtime settings write finished."
                    );
                    if let Err(error) = &result {
                        tracing::error!(%error, elapsed_ms, "could not save runtime settings.");
                    }
                    {
                        let mut status = worker_status.lock().unwrap();
                        if status.revision == request.revision {
                            status.pending = false;
                            status.error = result.as_ref().err().cloned();
                        }
                    }
                    responses.push(request.response);
                    for response in responses {
                        let _ = response.send(result.clone());
                    }
                }
            })?;
        Ok(Self { sender, status })
    }

    pub(crate) fn save(&self, settings: Settings) -> SaveReceipt {
        let (response, receipt) = mpsc::channel();
        let mut status = self.status.lock().unwrap();
        status.revision += 1;
        status.pending = true;
        status.error = None;
        let request = SaveRequest {
            revision: status.revision,
            settings,
            response,
        };
        if self.sender.send(request).is_err() {
            status.pending = false;
            status.error = Some("Settings worker is unavailable.".into());
        }
        receipt
    }

    pub(crate) fn status(&self) -> SaveStatus {
        self.status.lock().unwrap().clone()
    }

    pub(crate) fn save_blocking(&self, settings: Settings) -> Result<(), String> {
        self.save(settings)
            .recv()
            .unwrap_or_else(|_| Err("Settings worker stopped unexpectedly.".into()))
    }
}

pub(crate) fn install_status(window: &crate::MainWindow, writer: SettingsWriter) -> Timer {
    let weak = window.as_weak();
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let Some(window) = weak.upgrade() else { return };
            let status = writer.status();
            window.set_settings_save_pending(status.pending);
            let error = status
                .error
                .map(|error| format!("Applied for this session; could not save settings: {error}"));
            window.set_settings_save_error(error.as_deref().unwrap_or("").into());
            // Clock and Sound persist the same complete settings document.
            window.set_clock_settings_error(error.as_deref().unwrap_or("").into());
        },
    );
    timer
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const DEADLOCK_GUARD: Duration = Duration::from_secs(5);

    #[test]
    fn blocked_write_leaves_callers_free_and_coalesces_newer_complete_snapshots() {
        let (entered, entries) = mpsc::channel();
        let (release, releases) = mpsc::channel::<()>();
        let releases = Mutex::new(releases);
        let writer = SettingsWriter::with_writer(Arc::new(move |settings| {
            entered.send(settings.clone()).unwrap();
            releases
                .lock()
                .unwrap()
                .recv_timeout(DEADLOCK_GUARD)
                .unwrap();
            Ok(())
        }))
        .unwrap();

        let mut snapshot = Settings::default();
        let first = writer.save(snapshot.clone());
        entries.recv_timeout(DEADLOCK_GUARD).unwrap();
        // The first write is held explicitly, not simulated with a slow sleep.
        snapshot.audio.master_volume = 0.35;
        let second = writer.save(snapshot.clone());
        snapshot.audio.muted = true;
        snapshot.clock.time_format = engine_common::ClockTimeFormat::TwelveHour;
        snapshot.launch.scenario = "falling".into();
        let third = writer.save(snapshot.clone());
        assert!(writer.status().pending);
        assert!(matches!(second.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release.send(()).unwrap();
        first.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap();
        let written = entries.recv_timeout(DEADLOCK_GUARD).unwrap();
        assert_eq!(written.audio, snapshot.audio);
        assert_eq!(written.clock, snapshot.clock);
        assert_eq!(written.launch.scenario, "falling");
        assert!(
            writer.status().pending,
            "an old completion cannot acknowledge a newer write"
        );
        assert!(matches!(third.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release.send(()).unwrap();
        second.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap();
        third.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap();
        assert!(!writer.status().pending);
        assert!(writer.status().error.is_none());
        assert!(matches!(entries.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn failed_write_reports_error_and_a_new_request_can_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        // An owned directory at the destination fails even when run as root.
        std::fs::create_dir(&path).unwrap();
        let writer = SettingsWriter::new(path.clone()).unwrap();
        let mut snapshot = Settings::default();
        snapshot.audio.muted = true;
        assert!(writer.save_blocking(snapshot.clone()).is_err());
        assert!(!writer.status().pending);
        assert!(writer.status().error.is_some());
        std::fs::remove_dir(&path).unwrap();
        writer.save_blocking(snapshot.clone()).unwrap();
        assert!(writer.status().error.is_none());
        assert_eq!(
            crate::settings::load_settings(&path)
                .unwrap()
                .settings
                .audio,
            snapshot.audio
        );
    }
}
