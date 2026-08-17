use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use engine_core::SpacewarsConfig;
use scenario_spacewars::{ShipForm, SpacewarsBenchmarkCounts, SpacewarsScenario, SpacewarsState};

use super::{
    BenchmarkCounts, BenchmarkStepMetrics, CenterPanelState, ClientScenario, PlayerPanelState,
    RenderBackend, ScenarioCapabilities, ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{self, FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: true,
        pointer_input: false,
        player_zoom: true,
        game_over: true,
    },
    controls_help: "Player 1: W thrust, S brake, X reverse, A/D turn, J wings, Space laser, K cannon, U/I zoom.\nPlayer 2: Numpad 8 thrust, Numpad 5 brake, Numpad 2 reverse, Numpad 4/6 turn, PageDown wings, Delete laser, End cannon, Insert/Home zoom.\nPad: left stick or d-pad left/right turns, RT or d-pad up thrusts, LT or d-pad down brakes, B reverses, RB closes wings, A fires laser, X fires cannon, Start pauses, Select shows controls. Zoom is available in the pause menu.",
    create,
};

pub(crate) struct SpacewarsClientScenario {
    pub(crate) state: Box<SpacewarsState>,
}

fn create(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    mode: ScenarioStartMode,
) -> Box<dyn ClientScenario> {
    let state = match mode {
        ScenarioStartMode::Normal => {
            SpacewarsScenario::init(spacewars_config_from_settings(settings), seed)
        }
        ScenarioStartMode::Benchmark(_) => SpacewarsScenario::init_benchmark(seed),
    };
    Box::new(SpacewarsClientScenario {
        state: Box::new(state),
    })
}

impl ClientScenario for SpacewarsClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        SpacewarsScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        SpacewarsScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, input: &mut ClientInput, benchmark_active: bool) -> Vec<Action> {
        input.actions_for_spacewars(&self.state, benchmark_active)
    }

    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let player_view_aspect_ratio =
            render::frame_viewports(viewport, 4, FrameLayout::SpacewarsLocalPlay)[0].aspect_ratio();
        if renderer == RenderBackend::Raster {
            SpacewarsScenario::render_raster_local_play_frames(
                &self.state,
                player_view_aspect_ratio,
            )
        } else {
            SpacewarsScenario::render_local_play_frames(&self.state, player_view_aspect_ratio)
        }
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::SpacewarsLocalPlay
    }

    fn center_panel_state(
        &self,
        paused: bool,
        benchmark_active: bool,
        performance_text: &str,
    ) -> Option<CenterPanelState> {
        Some(center_panel_state(
            &self.state,
            paused,
            benchmark_active,
            performance_text,
        ))
    }

    fn is_game_over(&self) -> bool {
        self.state.winner.is_some()
    }

    fn zoom_player_in(&mut self, player: usize) {
        self.state.zoom_player_in(player);
    }

    fn zoom_player_out(&mut self, player: usize) {
        self.state.zoom_player_out(player);
    }

    fn benchmark_counts(&self) -> Option<BenchmarkCounts> {
        let SpacewarsBenchmarkCounts {
            asteroids,
            fragments,
            shells,
            particles,
            active_bodies,
            sleeping_bodies,
            candidate_pairs,
            contact_pairs,
            contacts,
        } = SpacewarsScenario::benchmark_counts(&self.state);
        let gravity = self.state.last_step_metrics.gravity;
        Some(BenchmarkCounts {
            asteroids,
            fragments,
            shells,
            particles,
            gravity_sources: gravity.source_count,
            gravity_targets: gravity.target_count,
            gravity_nodes: gravity.node_count,
            gravity_exact_interactions: gravity.exact_interactions,
            gravity_approximations: gravity.approximations,
            gravity_applied_sources: gravity.applied_sources,
            active_bodies,
            sleeping_bodies,
            candidate_pairs,
            contact_pairs,
            contacts,
            added: self.state.last_step_metrics.added,
            removed: self.state.last_step_metrics.removed,
            ..BenchmarkCounts::default()
        })
    }

    fn benchmark_step_metrics(&self) -> Option<BenchmarkStepMetrics> {
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
            rapier_step_time: metrics.rapier.rapier_step_time,
            rapier_broad_phase_time: metrics.rapier.broad_phase_time,
            rapier_narrow_phase_time: metrics.rapier.narrow_phase_time,
            rapier_island_time: metrics.rapier.island_time,
            rapier_solver_time: metrics.rapier.solver_time,
            rapier_ccd_time: metrics.rapier.ccd_time,
            added: metrics.added,
            removed: metrics.removed,
            ..BenchmarkStepMetrics::default()
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

fn spacewars_config_from_settings(settings: &Settings) -> SpacewarsConfig {
    let setup = settings.spacewars.normalized();
    let mut config = SpacewarsConfig {
        universe_radius: setup.universe_radius,
        use_planets: setup.use_planets,
        asteroid_probability_per_sec: if setup.asteroids_enabled {
            setup.asteroid_probability_per_sec
        } else {
            0.0
        },
        player_view_heights: [setup.player_1_view_height, setup.player_2_view_height],
        ..SpacewarsConfig::default()
    };
    for player in &mut config.players {
        player.health_percent = setup.player_health_percent;
    }
    config
}

fn center_panel_state(
    state: &SpacewarsState,
    paused: bool,
    benchmark_active: bool,
    performance_text: &str,
) -> CenterPanelState {
    let player_1_planets = state.players[0].planet_count;
    let player_2_planets = state.players[1].planet_count;
    let free_planets = state
        .planets
        .len()
        .saturating_sub(player_1_planets + player_2_planets);
    let total_planets = state.planets.len().max(1) as f32;

    CenterPanelState {
        player_1: player_panel_state(state, 0),
        player_2: player_panel_state(state, 1),
        planet_score_label: format!(
            "Planets  P1 {player_1_planets} | Free {free_planets} | P2 {player_2_planets}"
        ),
        player_1_planet_fraction: player_1_planets as f32 / total_planets,
        player_2_planet_fraction: player_2_planets as f32 / total_planets,
        player_1_planet_score: planet_score_text(player_1_planets),
        free_planet_score: planet_score_text(free_planets),
        player_2_planet_score: planet_score_text(player_2_planets),
        message_text: panel_message(state, paused, benchmark_active),
        performance_text: performance_text.into(),
    }
}

fn planet_score_text(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        count.to_string()
    }
}

fn panel_message(state: &SpacewarsState, paused: bool, benchmark_active: bool) -> String {
    if let Some(winner) = state.winner.and_then(|winner| state.players.get(winner)) {
        return format!("P{} Wins | R Restart | B Bench | Esc Launch", winner.id + 1);
    }
    if benchmark_active && paused {
        "Bench Paused | P/Esc Resume | B Reset | Q Launch".into()
    } else if benchmark_active {
        "Bench | P/Esc Pause | B Reset | R Game".into()
    } else if paused {
        "Paused | P/Esc Resume | R Restart | B Bench | Q Launch".into()
    } else {
        "P/Esc Pause | R Restart | B Bench".into()
    }
}

fn player_panel_state(state: &SpacewarsState, player_index: usize) -> PlayerPanelState {
    let player = &state.players[player_index];
    let ship = &state.ships[player_index];
    if player.eliminated {
        return PlayerPanelState {
            name: format!("Player {}: {}", player.id + 1, player.name),
            status: "Eliminated".into(),
            status_fraction: 0.0,
            color: player.color,
        };
    }

    let status_fraction = if ship.life_max <= 0.0 {
        0.0
    } else {
        (ship.life / ship.life_max).clamp(0.0, 1.0)
    };
    let percent = (status_fraction * 100.0).round() as u32;
    let label = match ship.form {
        ShipForm::Ship => "Ship Health",
        ShipForm::EscapePod => "Pod Rebuild",
    };
    PlayerPanelState {
        name: format!("Player {}: {}", player.id + 1, player.name),
        status: format!("{label}: {percent}%"),
        status_fraction,
        color: player.color,
    }
}
