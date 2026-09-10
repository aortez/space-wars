//! Observable launcher preparation, with durable writes and cartridge I/O off
//! the UI thread. Only one launch can be in flight; settings are committed to
//! memory after saving succeeds, so a failed save remains retryable.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock, mpsc};
use std::time::{Duration, Instant};

use engine_common::Settings;
use slint::{ComponentHandle, Timer, TimerMode};

use crate::client_scenarios::ScenarioAsset;
use crate::{EffectiveLaunch, MainWindow, SharedNesRomCatalog, host, input};

const POLL_INTERVAL: Duration = Duration::from_millis(16);
type SaveSettings = Arc<dyn Fn(&Settings) -> Result<(), String> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Preparing,
    Saving,
    Loading,
    Starting,
}

impl Stage {
    fn id(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Saving => "saving_settings",
            Self::Loading => "loading_cartridge",
            Self::Starting => "starting_scenario",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing launch…",
            Self::Saving => "Saving settings to storage…",
            Self::Loading => "Loading cartridge…",
            Self::Starting => "Starting scenario…",
        }
    }
}

enum PreparationMessage {
    Stage(Stage),
    SaveFinished(Result<(), String>, Duration),
    Ready(Result<ScenarioAsset, String>, Duration),
}

#[derive(Default)]
struct Timings {
    save: Duration,
    load: Duration,
    start: Duration,
}

struct PendingLaunch {
    receiver: mpsc::Receiver<PreparationMessage>,
    launch: EffectiveLaunch,
    settings: Settings,
    benchmark: bool,
    started_at: Instant,
    stage: Stage,
    timings: Timings,
    ready: Option<ScenarioAsset>,
}

impl PendingLaunch {
    fn publish(&self, window: &MainWindow, now: Instant, outcome: &str) {
        let elapsed = now.saturating_duration_since(self.started_at);
        window.set_launcher_busy_stage(self.stage.id().into());
        window.set_launcher_busy_detail(self.stage.label().into());
        window.set_launcher_busy_elapsed(format!("{:.1} s", elapsed.as_secs_f64()).into());
        window.set_launcher_busy_progress((elapsed.as_secs_f32() / 1.5).fract());
        window.set_launcher_diagnostics(
            format!(
                "launch_state={outcome}\nlaunch_scenario={}\nlaunch_stage={}\nlaunch_elapsed_ms={}\nlaunch_save_ms={}\nlaunch_asset_ms={}\nlaunch_start_ms={}",
                self.launch.scenario,
                self.stage.id(),
                elapsed.as_millis(),
                self.timings.save.as_millis(),
                self.timings.load.as_millis(),
                self.timings.start.as_millis(),
            )
            .into(),
        );
    }
}

pub(crate) struct Launcher {
    window: slint::Weak<MainWindow>,
    render_timer: Rc<RefCell<Option<Timer>>>,
    controls: host::SharedScenarioControls,
    input: input::SharedInput,
    settings: Arc<RwLock<Settings>>,
    catalog: SharedNesRomCatalog,
    writer: SaveSettings,
    pending: RefCell<Option<PendingLaunch>>,
    timer: Timer,
}

impl Launcher {
    pub(crate) fn new(
        window: &MainWindow,
        render_timer: Rc<RefCell<Option<Timer>>>,
        controls: host::SharedScenarioControls,
        input: input::SharedInput,
        settings: Arc<RwLock<Settings>>,
        catalog: SharedNesRomCatalog,
        settings_writer: crate::settings_writer::SettingsWriter,
    ) -> Rc<Self> {
        Rc::new(Self {
            window: window.as_weak(),
            render_timer,
            controls,
            input,
            settings,
            catalog,
            writer: Arc::new(move |settings| settings_writer.save_blocking(settings.clone())),
            pending: RefCell::new(None),
            timer: Timer::default(),
        })
    }

    pub(crate) fn start(self: &Rc<Self>, benchmark: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if window.get_launcher_busy() || !window.get_launcher_visible() {
            return;
        }
        window.set_launcher_error_text("".into());
        let mut candidate = self.settings.read().unwrap().clone();
        let selections = match crate::launcher_selections_from_window(&window, &candidate) {
            Ok(selections) => selections,
            Err(error) => {
                window.set_launcher_error_text(error.into());
                return;
            }
        };
        if benchmark
            && !host::scenario_registration(&selections.launch.scenario)
                .is_some_and(|registration| registration.capabilities.benchmark)
        {
            window.set_launcher_error_text(
                "The selected scenario does not support benchmark mode.".into(),
            );
            return;
        }

        let save_needed = crate::apply_launcher_selections(&mut candidate, &selections);
        let launch = selections.launch;
        let catalog = self.catalog.borrow().clone();
        let (sender, receiver) = mpsc::channel();
        let started_at = Instant::now();
        let pending = PendingLaunch {
            receiver,
            launch: launch.clone(),
            settings: candidate.clone(),
            benchmark,
            started_at,
            stage: Stage::Preparing,
            timings: Timings::default(),
            ready: None,
        };
        self.input.borrow_mut().clear();
        window.set_launcher_busy(true);
        window.set_launcher_busy_title(format!("Opening {}", launch.scenario).into());
        pending.publish(&window, started_at, "busy");
        *self.pending.borrow_mut() = Some(pending);

        let writer = Arc::clone(&self.writer);
        // The worker owns snapshots, never UI objects or the shared settings
        // lock. Dropping a window must not join a thread stuck in filesystem I/O.
        if let Err(error) = std::thread::Builder::new()
            .name("spacewars-launch-io".into())
            .spawn(move || {
                prepare(launch, candidate, save_needed, catalog, writer, sender);
            })
        {
            let pending = self.pending.borrow_mut().take().unwrap();
            self.finish(
                &window,
                pending,
                Err(format!("Could not prepare launch: {error}")),
            );
            return;
        }

        let weak = Rc::downgrade(self);
        self.timer
            .start(TimerMode::Repeated, POLL_INTERVAL, move || {
                if let Some(launcher) = weak.upgrade() {
                    launcher.poll(Instant::now());
                }
            });
    }

    fn poll(&self, now: Instant) {
        let Some(window) = self.window.upgrade() else {
            self.timer.stop();
            self.pending.borrow_mut().take();
            return;
        };
        let Some(mut pending) = self.pending.borrow_mut().take() else {
            return;
        };
        if let Some(asset) = pending.ready.take() {
            // Preparation yielded through the event loop with the Starting
            // stage visible. Scenario construction still requires the UI
            // thread; report its duration separately from storage work.
            let start = Instant::now();
            let result = crate::start_scenario_from_launch(
                &window,
                &pending.launch,
                pending.benchmark,
                host::BenchmarkConfiguration::default(),
                Rc::clone(&self.controls),
                Rc::clone(&self.input),
                pending.settings.clone(),
                asset,
            )
            .map_err(|error| error.to_string());
            pending.timings.start = start.elapsed();
            self.finish(&window, pending, result);
            return;
        }

        loop {
            match pending.receiver.try_recv() {
                Ok(PreparationMessage::Stage(stage)) => pending.stage = stage,
                Ok(PreparationMessage::SaveFinished(result, duration)) => {
                    pending.timings.save = duration;
                    if let Err(error) = result {
                        self.finish(
                            &window,
                            pending,
                            Err(format!("Could not save settings: {error}")),
                        );
                        return;
                    }
                    // A successful save stays committed even if the selected
                    // cartridge subsequently fails to load.
                    *self.settings.write().unwrap() = pending.settings.clone();
                }
                Ok(PreparationMessage::Ready(result, duration)) => {
                    pending.timings.load = duration;
                    match result {
                        Ok(asset) => {
                            pending.stage = Stage::Starting;
                            pending.ready = Some(asset);
                            break;
                        }
                        Err(error) => {
                            self.finish(&window, pending, Err(error));
                            return;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finish(
                        &window,
                        pending,
                        Err("Launch worker stopped unexpectedly.".into()),
                    );
                    return;
                }
            }
        }
        pending.publish(&window, now, "busy");
        *self.pending.borrow_mut() = Some(pending);
    }

    fn finish(&self, window: &MainWindow, pending: PendingLaunch, result: Result<Timer, String>) {
        self.timer.stop();
        self.input.borrow_mut().clear();
        let outcome = if result.is_ok() { "complete" } else { "failed" };
        pending.publish(window, Instant::now(), outcome);
        match result {
            Ok(timer) => {
                let mut slot = self.render_timer.borrow_mut();
                if let Some(previous) = slot.take() {
                    previous.stop();
                }
                self.controls.borrow_mut().clear();
                *slot = Some(timer);
                crate::hide_launcher_surfaces(window);
            }
            Err(error) => {
                crate::clear_runtime_diagnostics(window);
                window.set_launcher_error_text(error.into());
            }
        }
        window.set_launcher_busy(false);
        tracing::info!(
            scenario = pending.launch.scenario,
            outcome,
            elapsed_ms = pending.started_at.elapsed().as_millis() as u64,
            save_ms = pending.timings.save.as_millis() as u64,
            asset_ms = pending.timings.load.as_millis() as u64,
            start_ms = pending.timings.start.as_millis() as u64,
            "launcher operation finished."
        );
    }
}

fn prepare(
    launch: EffectiveLaunch,
    settings: Settings,
    save_needed: bool,
    catalog: crate::nes_roms::NesRomCatalog,
    writer: SaveSettings,
    sender: mpsc::Sender<PreparationMessage>,
) {
    if save_needed {
        if sender
            .send(PreparationMessage::Stage(Stage::Saving))
            .is_err()
        {
            return;
        }
        let started = Instant::now();
        let result = writer(&settings);
        let failed = result.is_err();
        if sender
            .send(PreparationMessage::SaveFinished(result, started.elapsed()))
            .is_err()
            || failed
        {
            return;
        }
    }
    if launch.scenario == "nes"
        && sender
            .send(PreparationMessage::Stage(Stage::Loading))
            .is_err()
    {
        return;
    }
    let started = Instant::now();
    let result = crate::resolve_launch_asset(&launch, None, &settings, &catalog);
    let _ = sender.send(PreparationMessage::Ready(result, started.elapsed()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Key, Platform, PlatformError, WindowAdapter, WindowEvent};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestPlatform;

    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
        }
    }

    struct Harness {
        window: MainWindow,
        launcher: Rc<Launcher>,
        _directory: tempfile::TempDir,
    }

    impl Harness {
        fn new(writer: SaveSettings) -> Self {
            slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
            let window = MainWindow::new().unwrap();
            window
                .window()
                .set_size(slint::LogicalSize::new(800.0, 480.0));
            let directory = tempfile::tempdir().unwrap();
            let settings = Arc::new(RwLock::new(Settings::default()));
            let catalog = Rc::new(RefCell::new(crate::nes_roms::NesRomCatalog::new(
                directory.path(),
            )));
            let (input, _) = input::new_shared_input();
            crate::install_ui_navigation(&window);
            crate::install_keyboard_navigation(&window, Rc::clone(&input));
            let mut launcher = Launcher::new(
                &window,
                Rc::new(RefCell::new(None)),
                host::new_scenario_controls(),
                input,
                settings,
                catalog,
                crate::settings_writer::SettingsWriter::new(directory.path().join("settings.toml"))
                    .unwrap(),
            );
            Rc::get_mut(&mut launcher).unwrap().writer = writer;
            let start = Rc::clone(&launcher);
            window.on_launcher_start_game(move || start.start(false));
            let harness = Self {
                window,
                launcher,
                _directory: directory,
            };
            harness.show_menu();
            harness.window.set_launcher_scenario("falling".into());
            harness.window.show().unwrap();
            harness
        }

        fn show_menu(&self) {
            let settings = self.launcher.settings.read().unwrap();
            crate::show_launcher(
                &self.window,
                &crate::launch_from_settings(&settings),
                &settings,
                &self.launcher.catalog,
            );
        }

        fn wait_for(&self, condition: impl Fn() -> bool) {
            // This is only a deadlock guard. Tests hold/release the worker with
            // channels and assert states, never storage speed or timer jitter.
            let deadline = Instant::now() + Duration::from_secs(10);
            while !condition() {
                assert!(
                    Instant::now() < deadline,
                    "launch failed to reach expected state: {}",
                    self.window.get_launcher_diagnostics()
                );
                self.launcher.poll(Instant::now());
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn blocked_save_keeps_ui_drawable_and_ignores_duplicate_keyboard_touch_and_menu_input() {
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let calls = Arc::new(AtomicUsize::new(0));
        let writes = Arc::clone(&calls);
        let harness = Harness::new(Arc::new(move |_| {
            writes.fetch_add(1, Ordering::SeqCst);
            entered.send(()).unwrap();
            gate.lock()
                .unwrap()
                .recv()
                .map_err(|error| error.to_string())
        }));
        let window = &harness.window;
        window.set_launcher_focus_index(1);
        window.invoke_launcher_start_game();
        assert!(window.get_launcher_busy());
        entry.recv_timeout(Duration::from_secs(10)).unwrap();
        harness.launcher.poll(Instant::now());
        assert_eq!(window.get_launcher_busy_stage(), "saving_settings");
        assert_ne!(
            harness.launcher.settings.read().unwrap().launch.scenario,
            "falling"
        );
        // A blocked worker must not hold the shared settings lock either.
        assert!(harness.launcher.settings.try_write().is_ok());

        window.invoke_launcher_start_game();
        harness.launcher.start(true);
        window.invoke_ui_action(0); // Up: would change the focused menu row.
        window.invoke_keyboard_action(1, false); // Down.
        for text in [Key::DownArrow, Key::Return, Key::Escape] {
            window
                .window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
            window
                .window()
                .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        }
        // The Start Game button is underneath the modal.
        let position = slint::LogicalPosition::new(400.0, 278.0);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            });
        assert_eq!(window.get_launcher_focus_index(), 1);
        assert!(!harness.launcher.input.borrow_mut().take_pause_requested());
        assert!(!harness.launcher.input.borrow_mut().take_back_requested());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let started = harness
            .launcher
            .pending
            .borrow()
            .as_ref()
            .unwrap()
            .started_at;
        harness
            .launcher
            .poll(started + Duration::from_millis(12_400));
        assert_eq!(window.get_launcher_busy_elapsed(), "12.4 s");
        let progress = window.get_launcher_busy_progress();
        let first = window.window().take_snapshot().unwrap();
        harness
            .launcher
            .poll(started + Duration::from_millis(12_700));
        assert_ne!(window.get_launcher_busy_progress(), progress);
        let second = window.window().take_snapshot().unwrap();
        assert_eq!((second.width(), second.height()), (800, 480));
        assert_ne!(
            first.as_bytes(),
            second.as_bytes(),
            "busy indicator must visibly update"
        );
        assert!(
            window
                .get_launcher_diagnostics()
                .contains("launch_elapsed_ms=12700")
        );
        if let Some(path) = std::env::var_os("SPACEWARS_TEST_BUSY_SCREENSHOT") {
            let file = std::fs::File::create(path).unwrap();
            let mut encoder = png::Encoder::new(file, second.width(), second.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(second.as_bytes())
                .unwrap();
        }

        release.send(()).unwrap();
        harness.wait_for(|| window.get_launcher_busy_stage() == "starting_scenario");
        assert!(
            window.get_launcher_busy(),
            "Starting stage must get an event-loop turn"
        );
        assert_eq!(
            harness.launcher.settings.read().unwrap().launch.scenario,
            "falling"
        );
        assert!(harness.launcher.render_timer.borrow().is_none());
        harness.wait_for(|| !window.get_launcher_busy());
        assert!(!window.get_launcher_visible());
        assert!(harness.launcher.render_timer.borrow().is_some());
        assert!(
            window
                .get_launcher_diagnostics()
                .contains("launch_state=complete")
        );
        assert!(
            window
                .get_runtime_diagnostics()
                .contains("scenario=falling")
        );
    }

    #[test]
    fn failed_save_leaves_settings_unchanged_and_retry_really_saves() {
        let calls = Arc::new(AtomicUsize::new(0));
        let writes = Arc::clone(&calls);
        let harness = Harness::new(Arc::new(move |_| {
            if writes.fetch_add(1, Ordering::SeqCst) == 0 {
                Err("test storage failure".into())
            } else {
                Ok(())
            }
        }));
        let window = &harness.window;
        let original = harness
            .launcher
            .settings
            .read()
            .unwrap()
            .launch
            .scenario
            .clone();
        window.invoke_launcher_start_game();
        harness.wait_for(|| !window.get_launcher_busy());
        assert!(window.get_launcher_visible());
        assert_eq!(
            window.get_launcher_error_text(),
            "Could not save settings: test storage failure"
        );
        assert_eq!(
            harness.launcher.settings.read().unwrap().launch.scenario,
            original
        );
        assert!(
            window
                .get_launcher_diagnostics()
                .contains("launch_state=failed")
        );
        assert!(harness.launcher.render_timer.borrow().is_none());

        window.invoke_launcher_start_game();
        assert!(window.get_launcher_error_text().is_empty());
        harness.wait_for(|| !window.get_launcher_busy());
        assert!(!window.get_launcher_visible());
        assert_eq!(
            harness.launcher.settings.read().unwrap().launch.scenario,
            "falling"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // Returning and opening unchanged settings does not rewrite storage.
        harness
            .launcher
            .render_timer
            .borrow_mut()
            .take()
            .unwrap()
            .stop();
        harness.show_menu();
        window.invoke_launcher_start_game();
        assert!(window.get_launcher_busy());
        harness.wait_for(|| !window.get_launcher_busy());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            window
                .get_launcher_diagnostics()
                .contains("launch_save_ms=0")
        );
    }

    #[test]
    fn asset_failure_returns_to_menu_after_committing_successful_save() {
        let harness = Harness::new(Arc::new(|_| Ok(())));
        let window = &harness.window;
        window.set_launcher_scenario("nes".into());
        window.invoke_launcher_start_game();
        harness.wait_for(|| !window.get_launcher_busy());
        assert!(window.get_launcher_visible());
        assert!(!window.get_launcher_error_text().is_empty());
        assert_eq!(
            harness.launcher.settings.read().unwrap().launch.scenario,
            "nes"
        );
        assert_eq!(window.get_launcher_busy_stage(), "loading_cartridge");
        assert!(
            window
                .get_launcher_diagnostics()
                .contains("launch_state=failed")
        );
        assert!(harness.launcher.render_timer.borrow().is_none());
    }
}
