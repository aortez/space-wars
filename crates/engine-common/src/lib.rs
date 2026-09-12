//! Shared types and traits across engine crates and scenarios.
//!
//! Stable contracts live here: the [`Scenario`] trait, input / observation
//! types, render primitives, and user [`Settings`].

use std::time::Duration;

use serde::{
    Deserialize, Serialize,
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
};

mod activity_settings;
mod clock_message;
pub use activity_settings::{AutostartSettings, MatchSettings};
pub mod render;

pub use clock_message::{ClockMarqueeMessage, ClockMessageError, MAX_CLOCK_MESSAGE_BYTES};
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
    pub autostart: AutostartSettings,
    pub spacewars_match: MatchSettings,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub controls: ControlBindings,
    pub launch: LaunchSettings,
    pub clock: ClockSettings,
    pub nes: NesSettings,
    pub spacewars: SpacewarsSettings,
    pub surface_expedition: SurfaceExpeditionSettings,
    pub combat_breaks: CombatBreakSettings,
    pub material_combat: MaterialCombatSettings,
    pub pizza: PizzaSettings,
    pub runtime: RuntimeSettings,
    pub last_scenario: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialCombatSettings {
    pub mission: MaterialCombatMission,
    pub asteroids: MaterialAsteroidSettings,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialAsteroidSettings {
    /// Mean time between arrivals; zero disables the environmental stream.
    pub interval_seconds: u32,
    pub severity: MaterialAsteroidSeverity,
}
impl MaterialAsteroidSettings {
    pub fn normalized(self) -> Self {
        Self {
            interval_seconds: self.interval_seconds.min(60),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaterialAsteroidSeverity {
    Light,
    #[default]
    Mixed,
    Heavy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaterialCombatMission {
    #[default]
    Dogfight,
    Capture,
}

/// Optional pacing experiment for the material combat pilots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatBreakSettings {
    /// Mean engaged flight time between breaks; zero disables breaks.
    pub interval_seconds: u32,
    pub duration_seconds: u32,
}

impl Default for CombatBreakSettings {
    fn default() -> Self {
        Self {
            interval_seconds: 15,
            duration_seconds: 4,
        }
    }
}

impl CombatBreakSettings {
    pub fn normalized(self) -> Self {
        Self {
            interval_seconds: self.interval_seconds.min(120),
            duration_seconds: self.duration_seconds.clamp(1, 15),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceExpeditionSettings {
    pub players: SurfaceExpeditionPlayers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceExpeditionPlayers {
    #[default]
    #[serde(rename = "one")]
    One,
    #[serde(rename = "two")]
    Two,
}

impl SurfaceExpeditionPlayers {
    pub fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub vsync: bool,
    /// Show the host's submitted-frame and simulation-update rates in every scenario.
    pub show_fps: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fullscreen: false,
            vsync: true,
            show_fps: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.25,
            muted: false,
        }
    }
}

impl AudioSettings {
    pub fn normalized(self) -> Self {
        Self {
            master_volume: if self.master_volume.is_finite() {
                self.master_volume.clamp(0.0, 1.0)
            } else {
                Self::default().master_volume
            },
            ..self
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockTimeFormat {
    #[serde(rename = "12-hour")]
    TwelveHour,
    #[default]
    #[serde(rename = "24-hour")]
    TwentyFourHour,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockSettings {
    pub time_format: ClockTimeFormat,
    pub event_profile: ClockEventProfile,
    pub events: ClockEvents,
    pub marquee_preset: ClockMarqueePreset,
    pub marquee_message: ClockMarqueeMessage,
}

/// Bounded recipes, not separate scheduler events. The choice is captured when
/// Marquee starts; changing it does not interrupt an animation already playing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum ClockMarqueePreset {
    ClockChase = 0,
    #[default]
    ClockWave = 1,
    ClockSpin = 2,
    DigitSpin = 3,
    TextScroll = 4,
    TextRibbon = 5,
    TextSpin = 6,
}

impl ClockMarqueePreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClockChase => "clock-chase",
            Self::ClockWave => "clock-wave",
            Self::ClockSpin => "clock-spin",
            Self::DigitSpin => "digit-spin",
            Self::TextScroll => "text-scroll",
            Self::TextRibbon => "text-ribbon",
            Self::TextSpin => "text-spin",
        }
    }

    pub const ALL: [Self; 7] = [
        Self::ClockChase,
        Self::ClockWave,
        Self::ClockSpin,
        Self::DigitSpin,
        Self::TextScroll,
        Self::TextRibbon,
        Self::TextSpin,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::ClockChase => "Clock chase",
            Self::ClockWave => "Clock wave",
            Self::ClockSpin => "Clock spin",
            Self::DigitSpin => "Digit spin",
            Self::TextScroll => "Text scroll",
            Self::TextRibbon => "Text ribbon",
            Self::TextSpin => "Text spin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockMarqueeState {
    pub preset: ClockMarqueePreset,
    pub content: String,
    pub cell_count: usize,
    pub group_count: usize,
    pub progress_milli: u32,
    pub scrolling: bool,
    pub waving: bool,
    pub rotation_target: Option<String>,
    pub lighting: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockEventTrigger {
    Periodic,
    TimeChange,
}

impl ClockEventTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Periodic => "periodic",
            Self::TimeChange => "time-change",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDigitSlideState {
    pub from_digits: [Option<u8>; 4],
    pub to_digits: [Option<u8>; 4],
    pub changed_slots: [bool; 4],
    pub progress_milli: u32,
    /// Manual previews roll the current digits, without fabricating a reading.
    pub preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum ClockEventKind {
    Falling = 0,
    ColorCycle = 1,
    Meltdown = 2,
    Duck = 3,
    Marquee = 4,
    DigitSlide = 5,
}

impl ClockEventKind {
    pub const ALL: [Self; 6] = [
        Self::Falling,
        Self::ColorCycle,
        Self::Meltdown,
        Self::Duck,
        Self::Marquee,
        Self::DigitSlide,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Falling => "falling",
            Self::ColorCycle => "color-cycle",
            Self::Meltdown => "meltdown",
            Self::Duck => "duck",
            Self::Marquee => "marquee",
            Self::DigitSlide => "digit-slide",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Falling => "Falling",
            Self::ColorCycle => "Color Cycle",
            Self::Meltdown => "Meltdown",
            Self::Duck => "Duck",
            Self::Marquee => "Marquee",
            Self::DigitSlide => "Digit Slide",
        }
    }
}

/// Automatic event selection. Explicit previews also work for disabled events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockEvents {
    pub falling: bool,
    pub color_cycle: bool,
    pub meltdown: bool,
    pub duck: bool,
    pub marquee: bool,
    pub digit_slide: bool,
}

impl Default for ClockEvents {
    fn default() -> Self {
        Self {
            falling: true,
            color_cycle: true,
            meltdown: true,
            duck: true,
            marquee: true,
            digit_slide: true,
        }
    }
}

impl ClockEvents {
    pub const fn enabled(self, kind: ClockEventKind) -> bool {
        match kind {
            ClockEventKind::Falling => self.falling,
            ClockEventKind::ColorCycle => self.color_cycle,
            ClockEventKind::Meltdown => self.meltdown,
            ClockEventKind::Duck => self.duck,
            ClockEventKind::Marquee => self.marquee,
            ClockEventKind::DigitSlide => self.digit_slide,
        }
    }
}

/// Bounded Clock-local material, not Rapier bodies. One original square is
/// 1,000,000 volume units. Rounding the four water aggregates can differ by two units.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockMeltdownState {
    pub initial_cells: usize,
    pub waiting_cells: usize,
    pub airborne_cells: usize,
    pub water_columns: usize,
    pub pooled_microunits: u64,
    /// Water physically in flight, not yet deposited or drained.
    #[serde(default)]
    pub spilling_microunits: u64,
    /// Occupied body space in the displacement lab, NOT liquid volume.
    #[serde(default)]
    pub displaced_microunits: u64,
    #[serde(default)]
    pub spill_parcels: usize,
    #[serde(default)]
    pub capacity_limited_ticks: u64,
    pub drained_microunits: u64,
    /// Water/cells reclaimed during reformation, not counted as drainage.
    pub reclaimed_microunits: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockDuckOutcome {
    Exited,
    Fell,
    TimedOut,
}

/// Movement personality, independent of event scheduling and course geometry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockDuckJumpProfile {
    #[default]
    Careful,
    Flowing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockDuckCoursePattern {
    #[default]
    Platforms,
    Terraces,
    TwoJump,
    Shortcut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockDuckBehavior {
    WarmingUp,
    MeasuringRun,
    Running,
    Turning,
    Exiting,
    Approaching,
    Jumping,
    Landing,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDuckPlanState {
    pub source: usize,
    pub target: usize,
    pub takeoff_milli: [i32; 2],
    pub landing_milli: [i32; 2],
    pub flight_ticks: u32,
    pub cruise_speed_milli: u32,
    #[serde(default)]
    pub running_takeoff: bool,
    #[serde(default)]
    pub next_target: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDuckPlanningState {
    #[serde(default)]
    pub pattern: ClockDuckCoursePattern,
    pub surface_count: usize,
    pub support: Option<usize>,
    pub plan: Option<ClockDuckPlanState>,
    pub confirmed_landings: u32,
    pub undershoots: u32,
    pub overshoots: u32,
    pub wrong_surface_landings: u32,
    pub rejected_plans: u32,
    pub rejection: Option<ClockDuckRejection>,
    pub acceleration_milli: Option<u32>,
    pub generation_attempts: u32,
    pub fallback_course: bool,
    #[serde(default)]
    pub running_jumps: u32,
    #[serde(default)]
    pub flowing_fallbacks: u32,
    #[serde(default)]
    pub moving_landings: u32,
    #[serde(default)]
    pub skipped_platforms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockDuckRejection {
    TooNarrow,
    TooHigh,
    OutOfRange,
    Obstructed,
}

/// Bounded controller diagnostics; measurements come from actual body motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDuckNavigationState {
    #[serde(default)]
    pub jump_profile: ClockDuckJumpProfile,
    pub course_seed: u64,
    pub behavior: ClockDuckBehavior,
    pub facing_right: bool,
    /// Render-space left/right wall tags, independent of the entrance side.
    pub wall_tags: [u32; 2],
    pub calibrated_jumps: usize,
    pub speed_samples: usize,
    pub jump_height_milli: Option<u32>,
    pub flight_ticks: Option<u32>,
    pub run_speed_milli: Option<u32>,
    pub target_obstacle: Option<usize>,
    pub spawned_ticks: u64,
    pub exit_visible: bool,
    #[serde(default)]
    pub body_radius_milli: u32,
    #[serde(default)]
    pub planning: Option<ClockDuckPlanningState>,
}

/// Temporary course/controller telemetry. Position is in thousandths of render
/// world units; the duck and its physics are absent during opening/resetting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDuckState {
    pub left_to_right: bool,
    pub position_milli: Option<[i32; 2]>,
    pub grounded: bool,
    pub jumps: u32,
    pub cleared_obstacles: usize,
    pub obstacle_count: usize,
    pub entrance_open_milli: u32,
    pub exit_open_milli: u32,
    pub outcome: Option<ClockDuckOutcome>,
    /// Additive diagnostics: old schema-8 payloads may omit this object.
    #[serde(default)]
    pub navigation: Option<ClockDuckNavigationState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockEventProfile {
    Off,
    #[default]
    Calm,
    Demo,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpacewarsController {
    #[default]
    Human,
    RuleBot,
}

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
    #[serde(default)]
    pub player_1_controller: SpacewarsController,
    #[serde(default)]
    pub player_2_controller: SpacewarsController,
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
            player_1_controller: SpacewarsController::Human,
            player_2_controller: SpacewarsController::Human,
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
            player_1_controller: self.player_1_controller,
            player_2_controller: self.player_2_controller,
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
