use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_pizza::{PizzaBounds, PizzaConfig, PizzaScenario, PizzaState};

use super::{
    BenchmarkCounts, BenchmarkStepMetrics, ClientScenario, RenderBackend, ScenarioCapabilities,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "pizza",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: true,
        pointer_input: true,
        player_zoom: false,
        game_over: false,
    },
    controls_help: "Pizza: click empty space to create a ball, or click an existing ball to grab it. Drag for a rubbery pull and release to fling.",
    create,
};

pub(crate) struct PizzaClientScenario {
    pub(crate) state: PizzaState,
}

fn create(
    seed: u64,
    settings: &Settings,
    viewport: Viewport,
    mode: ScenarioStartMode,
) -> Box<dyn ClientScenario> {
    let setup = settings.pizza.normalized();
    let bounds = PizzaBounds::from_aspect_ratio(viewport.aspect_ratio());
    let config = match mode {
        ScenarioStartMode::Normal => PizzaConfig {
            desired_ball_count: setup.desired_balls as usize,
            ball_spawn_rate: setup.ball_spawn_rate,
            bounds,
            benchmark: None,
        },
        ScenarioStartMode::Benchmark(configuration) => {
            PizzaConfig::benchmark(configuration.pizza, bounds)
        }
    };
    Box::new(PizzaClientScenario {
        state: PizzaScenario::init(config, seed),
    })
}

impl ClientScenario for PizzaClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        PizzaScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        PizzaScenario::step(&mut self.state, actions, dt)
    }

    fn map_keyboard_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        Vec::new()
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![PizzaScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.state.set_aspect_ratio(viewport.aspect_ratio());
    }

    fn benchmark_counts(&self) -> Option<BenchmarkCounts> {
        self.state.benchmark_config()?;
        let counts = self.state.benchmark_counts();
        Some(BenchmarkCounts {
            balls: counts.balls,
            active_bodies: counts.active_bodies,
            sleeping_bodies: counts.sleeping_bodies,
            candidate_pairs: counts.candidate_pairs,
            contact_pairs: counts.contact_pairs,
            contacts: counts.contacts,
            added: counts.added,
            removed: counts.removed,
            ..BenchmarkCounts::default()
        })
    }

    fn benchmark_step_metrics(&self) -> Option<BenchmarkStepMetrics> {
        self.state.benchmark_config()?;
        let metrics = self.state.last_step_metrics;
        Some(BenchmarkStepMetrics {
            workload_time: metrics.workload_time,
            lifecycle_time: metrics.lifecycle_time,
            gravity_time: metrics.gravity_time,
            collision_time: metrics.collision_time,
            physics_time: metrics.physics_time,
            snapshot_time: metrics.snapshot_time,
            rapier_step_time: metrics.rapier.rapier_step_time,
            rapier_broad_phase_time: metrics.rapier.broad_phase_time,
            rapier_narrow_phase_time: metrics.rapier.narrow_phase_time,
            rapier_island_time: metrics.rapier.island_time,
            rapier_solver_time: metrics.rapier.solver_time,
            rapier_ccd_time: metrics.rapier.ccd_time,
            added: metrics.added,
            removed: metrics.removed,
        })
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
