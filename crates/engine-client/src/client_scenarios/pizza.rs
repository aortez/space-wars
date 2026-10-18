use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_pizza::{PizzaBounds, PizzaConfig, PizzaScenario, PizzaState};

use super::{
    BenchmarkCounts, BenchmarkStepMetrics, ClientScenario, RenderBackend, ScenarioAsset,
    ScenarioCapabilities, ScenarioCreateError, ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "pizza",
    launcher_category: crate::client_scenarios::LauncherCategory::Main(3),
    capabilities: ScenarioCapabilities {
        benchmark: true,
        headless_benchmark: true,
        pointer_input: true,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Pizza: click empty space to create a ball, or click an existing ball to grab it. Drag for a rubbery pull and release to fling, even during gravity events. Pause and choose Run gravity & resume to start one manually. Pizza settings include automatic events and a live ball limit (including fragments). Higher limits increase CPU use; 5,000 is experimental.",
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
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let setup = settings.pizza.normalized();
    let bounds = PizzaBounds::from_aspect_ratio(viewport.aspect_ratio());
    let config = match mode {
        ScenarioStartMode::Normal => PizzaConfig {
            live_ball_limit: setup.live_ball_limit as usize,
            desired_ball_count: setup.desired_balls as usize,
            ball_spawn_rate: setup.ball_spawn_rate,
            bounds,
            gravity: Default::default(),
            automatic_events: setup.automatic_events,
            benchmark: None,
        },
        ScenarioStartMode::Benchmark(configuration) => {
            PizzaConfig::benchmark(configuration.pizza, bounds)
        }
    };
    Ok(Box::new(PizzaClientScenario {
        state: PizzaScenario::init(config, seed),
    }))
}

impl ClientScenario for PizzaClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn pizza_status(&self) -> Option<crate::host::PizzaStatus> {
        Some(crate::host::PizzaStatus {
            scenario_revision: 0,
            paused: false,
            observation: scenario_pizza::PizzaObservation {
                version: scenario_pizza::PIZZA_OBSERVATION_VERSION,
                tick: self.state.tick,
                event: self.state.event_state(),
            },
            ball_count: self.state.balls.len(),
            population: self.state.population(),
            held_ball_id: self.state.held_ball_id,
        })
    }

    fn tick_model(&self) -> TickModel {
        PizzaScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        PizzaScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
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
        let gravity = self.state.last_step_metrics.gravity;
        Some(BenchmarkCounts {
            balls: counts.balls,
            gravity_sources: gravity.source_count,
            gravity_targets: gravity.target_count,
            gravity_nodes: gravity.node_count,
            gravity_exact_interactions: gravity.exact_interactions,
            gravity_approximations: gravity.approximations,
            gravity_applied_sources: gravity.applied_sources,
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
            gravity_validation_time: metrics.gravity.validation_time,
            gravity_build_time: metrics.gravity.build_time,
            gravity_aggregation_time: metrics.gravity.aggregation_time,
            gravity_traversal_time: metrics.gravity.traversal_time,
            collision_time: metrics.collision_time,
            physics_time: metrics.physics_time,
            snapshot_time: metrics.snapshot_time,
            rapier_step_time: metrics.rapier.rapier_step_time,
            rapier_update_time: metrics.rapier.update_time,
            rapier_user_changes_time: metrics.rapier.user_changes_time,
            rapier_kinematic_interpolation_time: metrics.rapier.kinematic_interpolation_time,
            rapier_collision_detection_time: metrics.rapier.collision_detection_time,
            rapier_broad_phase_time: metrics.rapier.broad_phase_time,
            rapier_final_broad_phase_time: metrics.rapier.final_broad_phase_time,
            rapier_narrow_phase_time: metrics.rapier.narrow_phase_time,
            rapier_island_time: metrics.rapier.island_time,
            rapier_island_constraints_time: metrics.rapier.island_constraints_time,
            rapier_solver_time: metrics.rapier.solver_time,
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
