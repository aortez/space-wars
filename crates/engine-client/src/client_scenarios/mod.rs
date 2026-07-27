//! Object-safe scenario adapters and the client's compile-time registry.

use std::fmt;
use std::time::Duration;

use engine_common::{Action, RenderFrame, Settings, StepResult, TickModel};
use engine_core::Color;
use scenario_spacewars::SpacewarsBenchmarkCounts;

use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

mod null;
mod pizza;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioStartMode {
    Normal,
    Benchmark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioCapabilities {
    pub benchmark: bool,
    pub pointer_input: bool,
    pub player_zoom: bool,
    pub game_over: bool,
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
    create: fn(u64, &Settings, Viewport, ScenarioStartMode) -> Box<dyn ClientScenario>,
}

impl ScenarioRegistration {
    pub fn create(
        &'static self,
        seed: u64,
        settings: &Settings,
        viewport: Viewport,
        mode: ScenarioStartMode,
    ) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
        if mode == ScenarioStartMode::Benchmark && !self.capabilities.benchmark {
            return Err(ScenarioCreateError::BenchmarkUnsupported { name: self.id });
        }
        Ok((self.create)(seed, settings, viewport, mode))
    }
}

pub trait ClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration;
    fn tick_model(&self) -> TickModel;
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult;
    fn map_keyboard_input(&self, input: &mut ClientInput, benchmark_active: bool) -> Vec<Action>;
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame>;
    fn frame_layout(&self) -> FrameLayout;

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

    fn zoom_player_in(&mut self, _player: usize) {}
    fn zoom_player_out(&mut self, _player: usize) {}

    fn benchmark_counts(&self) -> Option<SpacewarsBenchmarkCounts> {
        None
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any;
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioCreateError {
    BenchmarkUnsupported { name: &'static str },
}

impl fmt::Display for ScenarioCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BenchmarkUnsupported { name } => {
                write!(f, "scenario {name:?} does not support benchmark mode")
            }
        }
    }
}

impl std::error::Error for ScenarioCreateError {}

static SCENARIOS: &[ScenarioRegistration] = &[
    null::REGISTRATION,
    pizza::REGISTRATION,
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

    #[test]
    fn registry_keeps_null_hostable_but_hidden() {
        assert!(registration("null").is_some());
        assert_eq!(
            launcher_registrations()
                .map(|registration| registration.id)
                .collect::<Vec<_>>(),
            vec!["pizza", "spacewars"]
        );
    }

    #[test]
    fn registry_rejects_unsupported_benchmarks() {
        let registration = registration("pizza").unwrap();
        let error = match registration.create(
            0,
            &Settings::default(),
            Viewport::new(1280.0, 720.0),
            ScenarioStartMode::Benchmark,
        ) {
            Ok(_) => panic!("pizza should not support benchmark mode"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            ScenarioCreateError::BenchmarkUnsupported { name: "pizza" }
        );
    }
}
