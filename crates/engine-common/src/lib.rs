//! Shared types and traits across engine crates and scenarios.
//!
//! Stable contracts live here: the [`Scenario`] trait, input / observation
//! types, render primitives, and user [`Settings`].

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

// -- Scenario trait -----------------------------------------------------------

/// A scenario is a runnable world hosted by the client or agent.
///
/// Implementors live under `scenarios/`. The host calls [`Scenario::step`] at
/// the cadence declared by [`Scenario::tick_model`] and reads observation /
/// render data between steps.
pub trait Scenario {
    type State;
    type Config;

    fn init(config: Self::Config, seed: u64) -> Self::State;
    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult;
    fn observe(state: &Self::State) -> Observation;
    fn render_frame(state: &Self::State) -> RenderFrame;

    /// Declared up front; the host's game loop honors it.
    fn tick_model() -> TickModel;
}

/// Tick model a scenario declares to the host.
#[derive(Debug, Clone, Copy)]
pub enum TickModel {
    /// Step at a fixed rate; host calls step() at this cadence.
    FixedTimestep { hz: u32 },
    /// Step called with whatever dt the host has accumulated.
    Variable,
    /// The scenario runs its own clock (e.g., NES emulator at 60Hz NTSC).
    EmulatorClock,
}

/// Result of a scenario step.
#[derive(Debug, Clone, Default)]
pub struct StepResult {
    pub terminated: bool,
}

// -- Actions & observations ---------------------------------------------------

/// A player or agent action. Per-scenario schemas extend this via the `kind`
/// discriminant and scenario-local wrappers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub kind: u32,
    pub payload: Vec<u8>,
}

/// What a scenario hands to an agent each tick. Shape is per-scenario; the
/// container/transport is shared.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub payload: Vec<u8>,
}

// -- Render frame -------------------------------------------------------------

/// Draw list emitted by a scenario's `render_frame`. The client translates to
/// Slint draw calls.
#[derive(Debug, Clone, Default)]
pub struct RenderFrame {
    pub layers: Vec<RenderLayer>,
}

/// Ordered 2D layer within a [`RenderFrame`].
#[derive(Debug, Clone, Default)]
pub struct RenderLayer {
    pub z: i32,
    // Primitives (sprites, shapes, text) land here once defined.
}

// -- Errors -------------------------------------------------------------------

/// Expected simulation failures. Invariant violations should `panic!`, not
/// return this.
#[derive(Debug)]
pub enum SimError {
    InvalidAction,
}

impl core::fmt::Display for SimError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SimError::InvalidAction => write!(f, "invalid action for current state"),
        }
    }
}

impl std::error::Error for SimError {}

// -- Settings -----------------------------------------------------------------

/// User-persisted app settings. Loaded/saved by `engine-client`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub controls: ControlBindings,
    pub runtime: RuntimeSettings,
    pub last_scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            vsync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlBindings {
    // Keymap lands here once the input schema is defined.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub crash_behavior: CrashBehavior,
    pub log_level: String,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            crash_behavior: CrashBehavior::default_for_platform(),
            log_level: "info".into(),
        }
    }
}

/// What happens when the client panics.
///
/// On Pi we want the kiosk process to die and let systemd restart it; on the
/// desktop we want to freeze and show a debug overlay.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CrashBehavior {
    /// Panic propagates up, process exits, systemd restarts. Pi default.
    Reboot,
    /// Top-level handler catches unwind, shows debug overlay, waits for the
    /// user. Desktop default.
    Freeze,
}

impl CrashBehavior {
    /// Default per target. Pi builds override this via their bundled
    /// `settings.toml`.
    pub const fn default_for_platform() -> Self {
        Self::Freeze
    }
}

impl Default for CrashBehavior {
    fn default() -> Self {
        Self::default_for_platform()
    }
}

// -- Settings load/save -------------------------------------------------------

/// Environment variable that overrides the platform-default config directory.
///
/// Used by the Pi kiosk build (wants `/var/lib/spacewars/`) and by tests.
pub const CONFIG_DIR_ENV: &str = "SPACEWARS_CONFIG_DIR";

/// Settings filename within the config directory.
pub const SETTINGS_FILENAME: &str = "settings.toml";

/// Errors that can arise while loading or saving settings.
#[derive(Debug)]
pub enum SettingsError {
    /// No config directory could be resolved (no home dir, no override).
    NoConfigDir,
    /// Filesystem I/O failed.
    Io(io::Error),
    /// TOML serialization failed.
    Serialize(toml::ser::Error),
    /// TOML deserialization failed.
    Deserialize(toml::de::Error),
}

impl core::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoConfigDir => write!(f, "could not resolve a config directory"),
            Self::Io(e) => write!(f, "settings I/O error: {e}"),
            Self::Serialize(e) => write!(f, "settings serialize error: {e}"),
            Self::Deserialize(e) => write!(f, "settings deserialize error: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<io::Error> for SettingsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::ser::Error> for SettingsError {
    fn from(e: toml::ser::Error) -> Self {
        Self::Serialize(e)
    }
}

impl From<toml::de::Error> for SettingsError {
    fn from(e: toml::de::Error) -> Self {
        Self::Deserialize(e)
    }
}

/// Resolve the config directory for this install.
///
/// Priority: `$SPACEWARS_CONFIG_DIR`, then the platform default from
/// `directories::ProjectDirs` (e.g., `~/.config/spacewars/` on Linux,
/// `%APPDATA%\spacewars\` on Windows).
pub fn config_dir() -> Result<PathBuf, SettingsError> {
    if let Some(override_dir) = env::var_os(CONFIG_DIR_ENV) {
        return Ok(PathBuf::from(override_dir));
    }
    let dirs = directories::ProjectDirs::from("", "", "spacewars")
        .ok_or(SettingsError::NoConfigDir)?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Full path to the settings file.
pub fn settings_path() -> Result<PathBuf, SettingsError> {
    Ok(config_dir()?.join(SETTINGS_FILENAME))
}

impl Settings {
    /// Load from `settings_path()`; return defaults (without writing) if the
    /// file is missing. Propagates I/O and parse errors.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(&settings_path()?)
    }

    /// Load from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Save to `settings_path()`, creating parent directories as needed.
    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&settings_path()?)
    }

    /// Save to an explicit path, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        let s = Settings::load_from(&path).unwrap();
        assert_eq!(s.last_scenario, None);
        assert_eq!(s.video.width, 1280);
        assert!(!path.exists(), "load should not create the file.");
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.toml");

        let mut s = Settings::default();
        s.last_scenario = Some("spacewars".into());
        s.video.width = 1920;
        s.audio.muted = true;
        s.save_to(&path).unwrap();

        let reloaded = Settings::load_from(&path).unwrap();
        assert_eq!(reloaded.last_scenario.as_deref(), Some("spacewars"));
        assert_eq!(reloaded.video.width, 1920);
        assert!(reloaded.audio.muted);
    }

    #[test]
    fn config_dir_env_override_wins() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: tests that touch process-global env must run single-threaded;
        // we mark this test serial implicitly by keeping env mutations local
        // and asserting synchronously before returning.
        unsafe {
            env::set_var(CONFIG_DIR_ENV, dir.path());
        }
        let resolved = config_dir().unwrap();
        unsafe {
            env::remove_var(CONFIG_DIR_ENV);
        }
        assert_eq!(resolved, dir.path());
    }
}
