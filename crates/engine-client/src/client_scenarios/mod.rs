//! Object-safe scenario adapters and the client's compile-time registry.

use std::fmt;
use std::time::{Duration, Instant};

use engine_common::{Action, NativeVideoFrame, RenderFrame, Settings, StepResult, TickModel};
use engine_core::Color;
use scenario_pizza::PizzaBenchmarkConfig;

use crate::input::ClientInput;
use crate::nes_realtime::{RealtimeTelemetry, RealtimeVideoConsumer};
use crate::render::{FrameLayout, Viewport};

mod clock;
mod falling;
mod nes;
mod null;
mod pizza;
mod rover_lab;
mod spacewars;

#[cfg(test)]
pub(crate) use pizza::PizzaClientScenario;
#[cfg(test)]
pub(crate) use spacewars::SpacewarsClientScenario;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RenderBackend {
    #[default]
    Vector,
    Raster,
}

impl RenderBackend {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Raster => "raster",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BenchmarkConfiguration {
    pub pizza: PizzaBenchmarkConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioStartMode {
    Normal,
    Benchmark(BenchmarkConfiguration),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ScenarioAsset {
    #[default]
    None,
    NesRom(crate::nes_roms::NesRomAsset),
}

impl ScenarioStartMode {
    pub const fn is_benchmark(self) -> bool {
        matches!(self, Self::Benchmark(_))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BenchmarkCounts {
    pub asteroids: usize,
    pub fragments: usize,
    pub shells: usize,
    pub particles: usize,
    pub balls: usize,
    pub gravity_sources: usize,
    pub gravity_targets: usize,
    pub gravity_nodes: usize,
    pub gravity_exact_interactions: u64,
    pub gravity_approximations: u64,
    pub gravity_applied_sources: u64,
    pub active_bodies: usize,
    pub sleeping_bodies: usize,
    pub candidate_pairs: usize,
    pub contact_pairs: usize,
    pub contacts: usize,
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BenchmarkStepMetrics {
    pub workload_time: Duration,
    pub lifecycle_time: Duration,
    pub gravity_time: Duration,
    pub gravity_validation_time: Duration,
    pub gravity_build_time: Duration,
    pub gravity_aggregation_time: Duration,
    pub gravity_traversal_time: Duration,
    pub collision_time: Duration,
    pub physics_time: Duration,
    pub snapshot_time: Duration,
    pub rapier_step_time: Duration,
    pub rapier_update_time: Duration,
    pub rapier_user_changes_time: Duration,
    pub rapier_kinematic_interpolation_time: Duration,
    pub rapier_collision_detection_time: Duration,
    pub rapier_broad_phase_time: Duration,
    pub rapier_final_broad_phase_time: Duration,
    pub rapier_narrow_phase_time: Duration,
    pub rapier_island_time: Duration,
    pub rapier_island_constraints_time: Duration,
    pub rapier_solver_time: Duration,
    pub rapier_ccd_time: Duration,
    pub added: usize,
    pub removed: usize,
}

impl std::ops::AddAssign for BenchmarkStepMetrics {
    fn add_assign(&mut self, rhs: Self) {
        self.workload_time += rhs.workload_time;
        self.lifecycle_time += rhs.lifecycle_time;
        self.gravity_time += rhs.gravity_time;
        self.gravity_validation_time += rhs.gravity_validation_time;
        self.gravity_build_time += rhs.gravity_build_time;
        self.gravity_aggregation_time += rhs.gravity_aggregation_time;
        self.gravity_traversal_time += rhs.gravity_traversal_time;
        self.collision_time += rhs.collision_time;
        self.physics_time += rhs.physics_time;
        self.snapshot_time += rhs.snapshot_time;
        self.rapier_step_time += rhs.rapier_step_time;
        self.rapier_update_time += rhs.rapier_update_time;
        self.rapier_user_changes_time += rhs.rapier_user_changes_time;
        self.rapier_kinematic_interpolation_time += rhs.rapier_kinematic_interpolation_time;
        self.rapier_collision_detection_time += rhs.rapier_collision_detection_time;
        self.rapier_broad_phase_time += rhs.rapier_broad_phase_time;
        self.rapier_final_broad_phase_time += rhs.rapier_final_broad_phase_time;
        self.rapier_narrow_phase_time += rhs.rapier_narrow_phase_time;
        self.rapier_island_time += rhs.rapier_island_time;
        self.rapier_island_constraints_time += rhs.rapier_island_constraints_time;
        self.rapier_solver_time += rhs.rapier_solver_time;
        self.rapier_ccd_time += rhs.rapier_ccd_time;
        self.added += rhs.added;
        self.removed += rhs.removed;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioCapabilities {
    pub benchmark: bool,
    pub pointer_input: bool,
    pub player_zoom: bool,
    pub game_over: bool,
    pub native_video: bool,
    pub captures_gamepad_start: bool,
    pub captures_gamepad_select: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CenterPanelState {
    pub player_1: PlayerPanelState,
    pub player_2: PlayerPanelState,
    pub planet_score_label: String,
    pub player_1_planet_fraction: f32,
    pub player_2_planet_fraction: f32,
    pub player_1_planet_score: String,
    pub free_planet_score: String,
    pub player_2_planet_score: String,
    pub message_text: String,
    pub performance_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPanelState {
    pub name: String,
    pub status: String,
    pub status_fraction: f32,
    pub color: Color,
}

pub struct ScenarioRegistration {
    pub id: &'static str,
    pub launcher_visible: bool,
    pub capabilities: ScenarioCapabilities,
    pub controls_help: &'static str,
    create: ScenarioFactory,
}

type ScenarioFactory = fn(
    u64,
    &Settings,
    Viewport,
    ScenarioStartMode,
    &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError>;

impl ScenarioRegistration {
    #[cfg(test)]
    pub fn create(
        &'static self,
        seed: u64,
        settings: &Settings,
        viewport: Viewport,
        mode: ScenarioStartMode,
    ) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
        self.create_with_asset(seed, settings, viewport, mode, &ScenarioAsset::None)
    }

    pub fn create_with_asset(
        &'static self,
        seed: u64,
        settings: &Settings,
        viewport: Viewport,
        mode: ScenarioStartMode,
        asset: &ScenarioAsset,
    ) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
        if mode.is_benchmark() && !self.capabilities.benchmark {
            return Err(ScenarioCreateError::BenchmarkUnsupported { name: self.id });
        }
        (self.create)(seed, settings, viewport, mode, asset)
    }
}

pub trait ClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration;
    fn tick_model(&self) -> TickModel;
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult;
    fn map_input(&self, input: &mut ClientInput, benchmark_active: bool) -> Vec<Action>;
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame>;
    fn frame_layout(&self) -> FrameLayout;

    fn native_video_frame(&self) -> Option<NativeVideoFrame<'_>> {
        None
    }

    /// Returns the bounded presentation endpoint when this client adapter is
    /// paced by a dedicated realtime worker instead of the Slint timer.
    fn realtime_video_consumer(&self) -> Option<RealtimeVideoConsumer> {
        None
    }

    /// Publishes one complete latest-state action snapshot. Realtime adapters
    /// sample it at their next authoritative frame boundary.
    fn publish_realtime_actions(&self, _actions: &[Action], _observed_at: Instant) {}

    fn set_realtime_paused(&self, _paused: bool) {}

    fn shutdown_realtime(&mut self) {}

    fn record_realtime_displayed_loop_iteration(&self) {}

    fn realtime_telemetry(&self) -> Option<RealtimeTelemetry> {
        None
    }

    fn set_viewport(&mut self, _viewport: Viewport) {}

    fn center_panel_state(
        &self,
        _paused: bool,
        _benchmark_active: bool,
        _performance_text: &str,
    ) -> Option<CenterPanelState> {
        None
    }

    fn is_game_over(&self) -> bool {
        false
    }

    fn runtime_error(&self) -> Option<String> {
        None
    }

    fn zoom_player_in(&mut self, _player: usize) {}
    fn zoom_player_out(&mut self, _player: usize) {}

    fn benchmark_counts(&self) -> Option<BenchmarkCounts> {
        None
    }

    fn benchmark_step_metrics(&self) -> Option<BenchmarkStepMetrics> {
        None
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any;
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioCreateError {
    BenchmarkUnsupported {
        name: &'static str,
    },
    // Reserved for scenarios whose assets cannot be embedded, such as a
    // future user-supplied cartridge loader.
    #[allow(dead_code)]
    MissingAsset {
        name: &'static str,
        asset: String,
    },
    InvalidAsset {
        name: &'static str,
        asset: String,
        detail: String,
    },
    UnsupportedCartridge {
        name: &'static str,
        detail: String,
    },
    RuntimeInitialization {
        name: &'static str,
        detail: String,
    },
    // Reserved for a scenario that requires audio to start. Falling degrades
    // to silent emulation when no default device is available.
    #[allow(dead_code)]
    AudioInitialization {
        name: &'static str,
        detail: String,
    },
}

impl fmt::Display for ScenarioCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BenchmarkUnsupported { name } => {
                write!(f, "scenario {name:?} does not support benchmark mode")
            }
            Self::MissingAsset { name, asset } => {
                write!(f, "scenario {name:?} is missing required asset {asset:?}")
            }
            Self::InvalidAsset {
                name,
                asset,
                detail,
            } => write!(
                f,
                "scenario {name:?} has an invalid asset {asset:?}: {detail}"
            ),
            Self::UnsupportedCartridge { name, detail } => {
                write!(f, "scenario {name:?} cannot load its cartridge: {detail}")
            }
            Self::RuntimeInitialization { name, detail } => {
                write!(f, "scenario {name:?} failed to initialize: {detail}")
            }
            Self::AudioInitialization { name, detail } => {
                write!(f, "scenario {name:?} could not initialize audio: {detail}")
            }
        }
    }
}

impl std::error::Error for ScenarioCreateError {}

static SCENARIOS: &[ScenarioRegistration] = &[
    null::REGISTRATION,
    clock::REGISTRATION,
    falling::REGISTRATION,
    nes::REGISTRATION,
    pizza::REGISTRATION,
    rover_lab::REGISTRATION,
    spacewars::REGISTRATION,
];

pub fn registrations() -> &'static [ScenarioRegistration] {
    SCENARIOS
}

pub fn registration(name: &str) -> Option<&'static ScenarioRegistration> {
    SCENARIOS
        .iter()
        .find(|registration| registration.id == name)
}

pub fn launcher_registrations() -> impl Iterator<Item = &'static ScenarioRegistration> {
    SCENARIOS
        .iter()
        .filter(|registration| registration.launcher_visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing_asset_factory(
        _seed: u64,
        _settings: &Settings,
        _viewport: Viewport,
        _mode: ScenarioStartMode,
        _asset: &ScenarioAsset,
    ) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
        Err(ScenarioCreateError::MissingAsset {
            name: "missing-test",
            asset: "test.nes".into(),
        })
    }

    static MISSING_ASSET_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
        id: "missing-test",
        launcher_visible: false,
        capabilities: ScenarioCapabilities {
            benchmark: false,
            pointer_input: false,
            player_zoom: false,
            game_over: false,
            native_video: true,
            captures_gamepad_start: true,
            captures_gamepad_select: true,
        },
        controls_help: "",
        create: missing_asset_factory,
    };

    #[test]
    fn registry_keeps_null_hostable_but_hidden() {
        assert!(registration("null").is_some());
        assert_eq!(
            launcher_registrations()
                .map(|registration| registration.id)
                .collect::<Vec<_>>(),
            vec!["clock", "falling", "nes", "pizza", "rover-lab", "spacewars"]
        );
    }

    #[test]
    fn registry_rejects_unsupported_benchmarks() {
        let registration = registration("rover-lab").unwrap();
        let error = match registration.create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Benchmark(BenchmarkConfiguration::default()),
        ) {
            Ok(_) => panic!("rover-lab should not support benchmark mode"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            ScenarioCreateError::BenchmarkUnsupported { name: "rover-lab" }
        );
    }

    #[test]
    fn scenario_factories_propagate_recoverable_asset_errors() {
        let error = match MISSING_ASSET_REGISTRATION.create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Normal,
        ) {
            Ok(_) => panic!("missing scenario asset should fail"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            ScenarioCreateError::MissingAsset {
                name: "missing-test",
                asset: "test.nes".into(),
            }
        );
        assert!(error.to_string().contains("test.nes"));
    }
}
