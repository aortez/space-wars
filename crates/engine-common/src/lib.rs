//! Shared types and traits across engine crates and scenarios.
//!
//! Stable contracts live here: the [`Scenario`] trait, input / observation
//! types, render primitives, and user [`Settings`].

use std::time::Duration;

use serde::{
    Deserialize, Serialize,
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
};

pub mod render;

pub use render::*;

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
#[serde(default)]
pub struct Settings {
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub controls: ControlBindings,
    pub launch: LaunchSettings,
    pub runtime: RuntimeSettings,
    pub last_scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
pub struct ControlBindings {
    // Keymap lands here once the input schema is defined.
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    #[serde(
        default = "default_launch_scenario",
        deserialize_with = "deserialize_launch_scenario"
    )]
    pub scenario: String,
    #[serde(
        default = "default_launch_seed",
        deserialize_with = "deserialize_launch_seed"
    )]
    pub seed: u64,
    #[serde(default = "default_launch_renderer")]
    pub renderer: RendererSetting,
    #[serde(
        default = "default_launch_raster_scale",
        deserialize_with = "deserialize_launch_raster_scale"
    )]
    pub raster_scale: f32,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            scenario: "spacewars".into(),
            seed: 0,
            renderer: RendererSetting::Vector,
            raster_scale: 1.0,
        }
    }
}

fn default_launch_scenario() -> String {
    "spacewars".into()
}

const fn default_launch_seed() -> u64 {
    0
}

const fn default_launch_renderer() -> RendererSetting {
    RendererSetting::Vector
}

const fn default_launch_raster_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererSetting {
    #[default]
    Vector,
    Raster,
}

impl<'de> Deserialize<'de> for RendererSetting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(RendererSettingVisitor)
    }
}

struct RendererSettingVisitor;

impl<'de> Visitor<'de> for RendererSettingVisitor {
    type Value = RendererSetting;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a renderer name")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(match value {
            "vector" => RendererSetting::Vector,
            "raster" => RendererSetting::Raster,
            _ => default_launch_renderer(),
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value.as_str())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_renderer())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_renderer())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_renderer())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_renderer())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_renderer())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(default_launch_renderer())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(default_launch_renderer())
    }
}

fn deserialize_launch_scenario<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(LaunchScenarioVisitor)
}

struct LaunchScenarioVisitor;

impl<'de> Visitor<'de> for LaunchScenarioVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a scenario name")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.into())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value)
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_scenario())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_scenario())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_scenario())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_scenario())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_scenario())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(default_launch_scenario())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(default_launch_scenario())
    }
}

fn deserialize_launch_seed<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(LaunchSeedVisitor)
}

struct LaunchSeedVisitor;

impl<'de> Visitor<'de> for LaunchSeedVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a non-negative integer seed")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u64::try_from(value).unwrap_or_else(|_| default_launch_seed()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64 {
            Ok(value as u64)
        } else {
            Ok(default_launch_seed())
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value
            .trim()
            .parse()
            .unwrap_or_else(|_| default_launch_seed()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value.as_str())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_seed())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_seed())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(default_launch_seed())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(default_launch_seed())
    }
}

fn deserialize_launch_raster_scale<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(LaunchRasterScaleVisitor)
}

struct LaunchRasterScaleVisitor;

impl<'de> Visitor<'de> for LaunchRasterScaleVisitor {
    type Value = f32;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a raster scale number")
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value as f32)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value as f32)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value as f32)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value
            .trim()
            .parse()
            .unwrap_or_else(|_| default_launch_raster_scale()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value.as_str())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_raster_scale())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(default_launch_raster_scale())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(default_launch_raster_scale())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(default_launch_raster_scale())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
