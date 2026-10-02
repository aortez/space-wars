//! Small, shared workflows for an existing application session.
//!
//! Callers own process supervision, artifacts and saved-settings verification.
//! These operations never start/restart a scenario or change preferences.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    ClockState, ControlClient, ControlClientError, ControlFailureCode, HostPauseRequest,
    UiActivateRequest, UiScreen, UiState,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const TRANSITION_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Workflow {
    Settings,
    ClockPause,
}

/// A recovery anchor, persisted before any workflow action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub initial: UiState,
    socket_device: u64,
    socket_inode: u64,
}

impl Session {
    pub fn capture(client: &ControlClient, deadline: Instant) -> Result<Self, String> {
        let (socket_device, socket_inode) = socket_identity(client)?;
        let initial = client.ui_state_before(deadline).map_err(message)?;
        let session = Self {
            initial,
            socket_device,
            socket_inode,
        };
        session.check_socket(client)?;
        if session.initial.benchmark_active
            || !matches!(
                session.initial.screen,
                UiScreen::LauncherMain | UiScreen::Gameplay | UiScreen::PauseMain
            )
        {
            return Err(
                "Start from the launcher, gameplay, or the main pause menu, outside a benchmark"
                    .into(),
            );
        }
        if session.initial.screen == UiScreen::LauncherMain {
            if session.initial.active_scenario.is_some() || session.initial.paused {
                return Err("Launcher has unexpected active-session state".into());
            }
        } else if session.initial.active_scenario.is_none()
            || session.initial.scenario_revision.is_none()
            || session.initial.paused != (session.initial.screen == UiScreen::PauseMain)
        {
            return Err("Gameplay/pause state is inconsistent".into());
        }
        Ok(session)
    }

    fn check_socket(&self, client: &ControlClient) -> Result<(), String> {
        if socket_identity(client)? != (self.socket_device, self.socket_inode) {
            return Err("Control socket changed; refusing to control a replacement app".into());
        }
        Ok(())
    }

    fn check_state(&self, state: &UiState) -> Result<(), String> {
        if state.active_scenario != self.initial.active_scenario
            || state.scenario_revision != self.initial.scenario_revision
            || state.selected_scenario != self.initial.selected_scenario
            || state.benchmark_active != self.initial.benchmark_active
        {
            return Err(
                "Scenario/session changed during the workflow; further actions refused".into(),
            );
        }
        if state.active_scenario.is_some() && state.screen != UiScreen::Gameplay && !state.paused {
            return Err("The scenario resumed while a menu was open".into());
        }
        Ok(())
    }

    pub fn restored(&self, state: &UiState) -> bool {
        self.check_state(state).is_ok()
            && state.screen == self.initial.screen
            && state.paused == self.initial.paused
    }
}

fn socket_identity(client: &ControlClient) -> Result<(u64, u64), String> {
    let metadata = fs::metadata(client.socket_path()).map_err(message)?;
    if !metadata.file_type().is_socket() {
        return Err("Control path is not a Unix socket".into());
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<UiState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

/// The observer may save artifacts, subject to the supplied deadline.
pub trait Observer: FnMut(Observation, Instant) -> Result<(), String> {}
impl<T: FnMut(Observation, Instant) -> Result<(), String>> Observer for T {}

pub fn run(
    client: &ControlClient,
    session: &Session,
    workflow: Workflow,
    deadline: Instant,
    observer: impl Observer,
) -> Result<UiState, String> {
    let mut runner = Runner {
        client,
        session,
        deadline,
        observer,
    };
    let state = runner.state()?;
    if !session.restored(&state) {
        return Err("The initial screen or pause state changed before the workflow began".into());
    }
    match workflow {
        Workflow::Settings => runner.settings(),
        Workflow::ClockPause => runner.clock_pause(),
    }
}

/// Bounded menu cleanup, never process/service restart. A failed run stays failed
/// even if this succeeds. A changed socket or scenario invalidates the anchor.
pub fn restore(
    client: &ControlClient,
    session: &Session,
    deadline: Instant,
    observer: impl Observer,
) -> Result<UiState, String> {
    let mut runner = Runner {
        client,
        session,
        deadline,
        observer,
    };
    for _ in 0..8 {
        let state = runner.state()?;
        if session.restored(&state) {
            return Ok(state);
        }
        let control = match state.screen {
            UiScreen::LauncherInfo | UiScreen::PauseInfo => "info.back",
            UiScreen::LauncherNetwork | UiScreen::PauseNetwork => "network.back",
            UiScreen::LauncherSound | UiScreen::PauseSound => "sound.back",
            UiScreen::PauseMain if session.initial.screen == UiScreen::Gameplay => "pause.resume",
            _ => {
                return Err(format!(
                    "Cannot restore the original session from {}",
                    state.screen
                ));
            }
        };
        runner.activate(control, state.screen)?;
        if control == "pause.resume" {
            runner.wait(UiScreen::Gameplay, |state| !state.paused)?;
        }
    }
    Err("Menu cleanup exceeded its action limit".into())
}

struct Runner<'a, F> {
    client: &'a ControlClient,
    session: &'a Session,
    deadline: Instant,
    observer: F,
}

impl<F: Observer> Runner<'_, F> {
    fn deadline(&self) -> Result<Instant, String> {
        if Instant::now() >= self.deadline {
            return Err("Workflow deadline elapsed".into());
        }
        self.session.check_socket(self.client)?;
        Ok(self.deadline.min(Instant::now() + REQUEST_TIMEOUT))
    }

    fn record(
        &mut self,
        command: &str,
        state: Option<UiState>,
        checkpoint: Option<&str>,
    ) -> Result<(), String> {
        let deadline = self.deadline()?;
        (self.observer)(
            Observation {
                command: command.into(),
                state,
                checkpoint: checkpoint.map(str::to_owned),
            },
            deadline,
        )
    }

    fn state(&mut self) -> Result<UiState, String> {
        self.record("ui state", None, None)?;
        let state = self
            .client
            .ui_state_before(self.deadline()?)
            .map_err(message)?;
        self.session.check_socket(self.client)?;
        self.session.check_state(&state)?;
        self.record("ui state", Some(state.clone()), None)?;
        Ok(state)
    }

    fn checkpoint(&mut self, name: &str) -> Result<(), String> {
        let state = self.state()?;
        self.record("checkpoint", Some(state), Some(name))
    }

    fn wait(
        &mut self,
        screen: UiScreen,
        predicate: impl Fn(&UiState) -> bool,
    ) -> Result<UiState, String> {
        let until = self.deadline.min(Instant::now() + TRANSITION_TIMEOUT);
        loop {
            let state = self.state()?;
            if state.screen == screen && predicate(&state) {
                return Ok(state);
            }
            if Instant::now() >= until {
                return Err(format!("Timed out waiting for {screen}"));
            }
            thread::sleep(POLL_INTERVAL.min(until.saturating_duration_since(Instant::now())));
        }
    }

    fn activate(&mut self, id: &str, screen: UiScreen) -> Result<UiState, String> {
        // Telemetry panels may advance revisions between read and activation.
        // Retry only an explicit rejection that guarantees no action occurred.
        for _ in 0..3 {
            let state = self.state()?;
            if state.screen != screen || !state.controls.iter().any(|c| c.id == id && c.enabled) {
                return Err(format!(
                    "Control {id} is not available on the expected screen {screen}"
                ));
            }
            let mut request = UiActivateRequest::new(id);
            request.expected_screen = Some(screen);
            request.expected_revision = Some(state.revision);
            self.record(&format!("ui activate {id}"), None, None)?;
            match self.client.ui_activate_before(&request, self.deadline()?) {
                Ok(next) => {
                    self.session.check_socket(self.client)?;
                    self.session.check_state(&next)?;
                    self.record(&format!("ui activate {id}"), Some(next.clone()), None)?;
                    return Ok(next);
                }
                Err(ControlClientError::Failure(f))
                    if f.code == ControlFailureCode::StaleRevision => {}
                Err(error) => return Err(message(error)),
            }
        }
        Err(format!("Repeated stale revisions activating {id}"))
    }

    fn pause(&mut self) -> Result<UiState, String> {
        let state = self.state()?;
        if state.screen == UiScreen::PauseMain && state.paused {
            return Ok(state);
        }
        if state.screen != UiScreen::Gameplay {
            return Err("Expected gameplay before pausing".into());
        }
        let mut request = HostPauseRequest::new();
        request.expected_revision = Some(state.revision);
        self.record("host pause", None, None)?;
        let next = self
            .client
            .host_pause_before(&request, self.deadline()?)
            .map_err(message)?;
        self.session.check_state(&next)?;
        self.record("host pause", Some(next), None)?;
        self.wait(UiScreen::PauseMain, |state| state.paused)
    }

    fn settings(&mut self) -> Result<UiState, String> {
        let launcher = self.session.initial.screen == UiScreen::LauncherMain;
        let (root, settings, info, network) = if launcher {
            (
                UiScreen::LauncherMain,
                UiScreen::LauncherSound,
                UiScreen::LauncherInfo,
                UiScreen::LauncherNetwork,
            )
        } else {
            self.pause()?;
            (
                UiScreen::PauseMain,
                UiScreen::PauseSound,
                UiScreen::PauseInfo,
                UiScreen::PauseNetwork,
            )
        };
        self.activate(
            if launcher {
                "launcher.sound"
            } else {
                "pause.sound"
            },
            root,
        )?;
        self.wait(settings, |_| true)?;
        self.checkpoint("settings")?;
        self.inspect_info(settings, info)?;
        self.activate("settings.network", settings)?;
        self.wait(network, |s| {
            s.controls
                .iter()
                .any(|c| c.id == "network.saved" && c.enabled)
        })?;
        self.activate("network.saved", network)?;
        self.wait(network, |s| {
            s.controls
                .iter()
                .any(|c| c.id == "network.refresh" && c.enabled)
        })?;
        self.checkpoint("saved-networks")?;
        self.activate("network.back", network)?;
        self.activate("network.back", network)?;
        self.wait(settings, |_| true)?;
        self.activate("sound.back", settings)?;
        self.wait(root, |_| true)
    }

    fn inspect_info(&mut self, settings: UiScreen, info: UiScreen) -> Result<(), String> {
        self.activate("settings.device-info", settings)?;
        let state = self.wait(info, |s| value(s, "info.status") == Some("ready"))?;
        for id in ["info.version", "info.hostname"] {
            if value(&state, id).is_none_or(str::is_empty) {
                return Err(format!("Device Info is missing {id}"));
            }
        }
        self.checkpoint("device-info")?;
        self.activate("info.back", info)?;
        self.wait(settings, |_| true)?;
        Ok(())
    }

    fn clock(&mut self) -> Result<ClockState, String> {
        self.state()?;
        self.record("clock state", None, None)?;
        let state = self
            .client
            .clock_state_before(self.deadline()?)
            .map_err(message)?;
        if Some(state.scenario_revision) != self.session.initial.scenario_revision {
            return Err("Clock instance changed during the workflow".into());
        }
        Ok(state)
    }

    fn clock_pause(&mut self) -> Result<UiState, String> {
        if self.session.initial.active_scenario.as_deref() != Some("clock") {
            return Err("clock-pause requires an already running or paused Clock".into());
        }
        let initial = self.clock()?;
        self.pause()?;
        self.activate("pause.sound", UiScreen::PauseMain)?;
        self.wait(UiScreen::PauseSound, |_| true)?;
        self.inspect_info(UiScreen::PauseSound, UiScreen::PauseInfo)?;
        self.activate("sound.back", UiScreen::PauseSound)?;
        self.wait(UiScreen::PauseMain, |s| s.paused)?;
        self.checkpoint("paused")?;
        let paused = self.clock()?;
        if !paused.paused || paused.settings != initial.settings {
            return Err("Clock pause/settings verification failed".into());
        }
        // Observe a stable paused interval, independent of animation phases.
        let until = Instant::now() + Duration::from_millis(200);
        loop {
            let current = self.clock()?;
            if !current.paused
                || current.simulation_tick != paused.simulation_tick
                || current.settings != initial.settings
            {
                return Err("Clock advanced or preferences changed while paused".into());
            }
            if Instant::now() >= until {
                break;
            }
            thread::sleep(POLL_INTERVAL);
        }
        if self.session.initial.paused {
            return self.state();
        }
        self.activate("pause.resume", UiScreen::PauseMain)?;
        self.wait(UiScreen::Gameplay, |s| !s.paused)?;
        let until = self.deadline.min(Instant::now() + TRANSITION_TIMEOUT);
        loop {
            let current = self.clock()?;
            if current.paused || current.settings != initial.settings {
                return Err("Clock did not resume with unchanged preferences".into());
            }
            if current.simulation_tick > paused.simulation_tick {
                break;
            }
            if Instant::now() >= until {
                return Err("Clock did not advance after resume".into());
            }
            thread::sleep(POLL_INTERVAL);
        }
        self.checkpoint("resumed")?;
        self.state()
    }
}

fn value<'a>(state: &'a UiState, id: &str) -> Option<&'a str> {
    state.controls.iter().find(|c| c.id == id)?.value.as_deref()
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
