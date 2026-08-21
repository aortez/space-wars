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

pub const DEFAULT_CONTROL_SOCKET: &str = "/tmp/spacewars-control.sock";

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
    /// One step advances one native clock quantum (for example, one NTSC NES
    /// frame); realtime hosts pace those quanta independently of UI callbacks.
    EmulatorClock,
}

/// Result of a scenario step.
#[derive(Debug, Clone, Default)]
pub struct StepResult {
    pub terminated: bool,
}

// -- Actions & observations ---------------------------------------------------

/// A player, pointer, or agent action.
///
/// Scenario-specific actions retain the compact discriminant/payload shape.
/// Pointer input is shared so clients can unproject it once without teaching
/// simulations about window-system events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Scenario { kind: u32, payload: Vec<u8> },
    Pointer(PointerAction),
}

impl Action {
    pub fn scenario(kind: u32, payload: Vec<u8>) -> Self {
        Self::Scenario { kind, payload }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerAction {
    pub position: RenderPoint,
    pub phase: PointerPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerPhase {
    Press,
    Drag,
    Release,
    Cancel,
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
    pub nes: NesSettings,
    pub spacewars: SpacewarsSettings,
    pub pizza: PizzaSettings,
    pub runtime: RuntimeSettings,
    pub last_scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub vsync: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fullscreen: false,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NesSettings {
    /// SHA-256 identity of the last user cartridge selected in the launcher.
    pub selected_rom_id: Option<String>,
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

pub const DEFAULT_SPACEWARS_UNIVERSE_RADIUS: u32 = 1200;
pub const MIN_SPACEWARS_UNIVERSE_RADIUS: u32 = 300;
pub const MAX_SPACEWARS_UNIVERSE_RADIUS: u32 = 10_000;
pub const DEFAULT_SPACEWARS_USE_PLANETS: bool = true;
pub const DEFAULT_SPACEWARS_ASTEROIDS_ENABLED: bool = true;
pub const DEFAULT_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC: f32 = 20.0;
pub const MIN_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC: f32 = 0.0;
pub const MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC: f32 = 1000.0;
pub const DEFAULT_SPACEWARS_PLAYER_HEALTH_PERCENT: u32 = 100;
pub const MIN_SPACEWARS_PLAYER_HEALTH_PERCENT: u32 = 1;
pub const MAX_SPACEWARS_PLAYER_HEALTH_PERCENT: u32 = 500;
pub const DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT: f32 = 320.0;
pub const MIN_SPACEWARS_PLAYER_VIEW_HEIGHT: f32 = 15.0;
pub const MAX_SPACEWARS_PLAYER_VIEW_HEIGHT: f32 = 30_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacewarsSettings {
    #[serde(default = "default_spacewars_universe_radius")]
    pub universe_radius: u32,
    #[serde(default = "default_spacewars_use_planets")]
    pub use_planets: bool,
    #[serde(default = "default_spacewars_asteroids_enabled")]
    pub asteroids_enabled: bool,
    #[serde(default = "default_spacewars_asteroid_probability_per_sec")]
    pub asteroid_probability_per_sec: f32,
    #[serde(default = "default_spacewars_player_health_percent")]
    pub player_health_percent: u32,
    #[serde(default = "default_spacewars_player_view_height")]
    pub player_1_view_height: f32,
    #[serde(default = "default_spacewars_player_view_height")]
    pub player_2_view_height: f32,
}

impl Default for SpacewarsSettings {
    fn default() -> Self {
        Self {
            universe_radius: DEFAULT_SPACEWARS_UNIVERSE_RADIUS,
            use_planets: DEFAULT_SPACEWARS_USE_PLANETS,
            asteroids_enabled: DEFAULT_SPACEWARS_ASTEROIDS_ENABLED,
            asteroid_probability_per_sec: DEFAULT_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC,
            player_health_percent: DEFAULT_SPACEWARS_PLAYER_HEALTH_PERCENT,
            player_1_view_height: DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT,
            player_2_view_height: DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT,
        }
    }
}

impl SpacewarsSettings {
    pub fn normalized(&self) -> Self {
        Self {
            universe_radius: self
                .universe_radius
                .clamp(MIN_SPACEWARS_UNIVERSE_RADIUS, MAX_SPACEWARS_UNIVERSE_RADIUS),
            use_planets: self.use_planets,
            asteroids_enabled: self.asteroids_enabled,
            asteroid_probability_per_sec: normalize_spacewars_asteroid_probability(
                self.asteroid_probability_per_sec,
            ),
            player_health_percent: self.player_health_percent.clamp(
                MIN_SPACEWARS_PLAYER_HEALTH_PERCENT,
                MAX_SPACEWARS_PLAYER_HEALTH_PERCENT,
            ),
            player_1_view_height: normalize_spacewars_player_view_height(self.player_1_view_height),
            player_2_view_height: normalize_spacewars_player_view_height(self.player_2_view_height),
        }
    }
}

const fn default_spacewars_universe_radius() -> u32 {
    DEFAULT_SPACEWARS_UNIVERSE_RADIUS
}

const fn default_spacewars_use_planets() -> bool {
    DEFAULT_SPACEWARS_USE_PLANETS
}

const fn default_spacewars_asteroids_enabled() -> bool {
    DEFAULT_SPACEWARS_ASTEROIDS_ENABLED
}

const fn default_spacewars_asteroid_probability_per_sec() -> f32 {
    DEFAULT_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC
}

const fn default_spacewars_player_health_percent() -> u32 {
    DEFAULT_SPACEWARS_PLAYER_HEALTH_PERCENT
}

const fn default_spacewars_player_view_height() -> f32 {
    DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT
}

fn normalize_spacewars_asteroid_probability(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(
            MIN_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC,
            MAX_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC,
        )
    } else {
        DEFAULT_SPACEWARS_ASTEROID_PROBABILITY_PER_SEC
    }
}

fn normalize_spacewars_player_view_height(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(
            MIN_SPACEWARS_PLAYER_VIEW_HEIGHT,
            MAX_SPACEWARS_PLAYER_VIEW_HEIGHT,
        )
    } else {
        DEFAULT_SPACEWARS_PLAYER_VIEW_HEIGHT
    }
}

pub const DEFAULT_PIZZA_DESIRED_BALLS: u32 = 24;
pub const MAX_PIZZA_DESIRED_BALLS: u32 = 500;
pub const DEFAULT_PIZZA_BALL_SPAWN_RATE: f32 = 0.10;
pub const MIN_PIZZA_BALL_SPAWN_RATE: f32 = 0.01;
pub const MAX_PIZZA_BALL_SPAWN_RATE: f32 = 0.99;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PizzaSettings {
    #[serde(default = "default_pizza_desired_balls")]
    pub desired_balls: u32,
    #[serde(default = "default_pizza_ball_spawn_rate")]
    pub ball_spawn_rate: f32,
}

impl Default for PizzaSettings {
    fn default() -> Self {
        Self {
            desired_balls: DEFAULT_PIZZA_DESIRED_BALLS,
            ball_spawn_rate: DEFAULT_PIZZA_BALL_SPAWN_RATE,
        }
    }
}

impl PizzaSettings {
    pub fn normalized(&self) -> Self {
        Self {
            desired_balls: self.desired_balls.min(MAX_PIZZA_DESIRED_BALLS),
            ball_spawn_rate: normalize_pizza_ball_spawn_rate(self.ball_spawn_rate),
        }
    }
}

const fn default_pizza_desired_balls() -> u32 {
    DEFAULT_PIZZA_DESIRED_BALLS
}

const fn default_pizza_ball_spawn_rate() -> f32 {
    DEFAULT_PIZZA_BALL_SPAWN_RATE
}

fn normalize_pizza_ball_spawn_rate(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_PIZZA_BALL_SPAWN_RATE, MAX_PIZZA_BALL_SPAWN_RATE)
    } else {
        DEFAULT_PIZZA_BALL_SPAWN_RATE
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
