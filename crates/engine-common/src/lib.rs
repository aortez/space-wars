//! Shared types and traits across engine crates and scenarios.
//!
//! Stable contracts live here: the [`Scenario`] trait, input / observation
//! types, render primitives, and user [`Settings`].

use std::time::Duration;

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
