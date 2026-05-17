//! Scenario hosting loop for the Slint client.

use std::fmt;
use std::time::{Duration, Instant};

use engine_common::{Action, RenderFrame, Scenario, StepResult, TickModel};
use engine_core::{Color as CoreColor, SpacewarsConfig};
use scenario_null::{NullConfig, NullScenario};
use scenario_spacewars::{ShipForm, SpacewarsScenario, SpacewarsState};
use slint::{
    Brush, Color as SlintColor, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel,
};

use crate::MainWindow;
use crate::input::{self, ClientInput};
use crate::render::{self, Viewport};

const TIMER_INTERVAL: Duration = Duration::from_millis(16);
const MAX_FIXED_STEPS_PER_TICK: usize = 5;

pub enum HostError {
    UnknownScenario { name: String },
}

impl fmt::Debug for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::UnknownScenario { name } => {
                write!(
                    f,
                    "unknown scenario {name:?}; available scenarios: {}",
                    scenario_names().join(", ")
                )
            }
        }
    }
}

impl std::error::Error for HostError {}

pub fn validate_scenario(name: &str) -> Result<(), HostError> {
    if is_known_scenario(name) {
        Ok(())
    } else {
        Err(HostError::UnknownScenario { name: name.into() })
    }
}

pub fn scenario_names() -> &'static [&'static str] {
    &["null", "spacewars"]
}

pub fn is_known_scenario(name: &str) -> bool {
    scenario_names().contains(&name)
}

pub fn start_debug_render_loop(window: &MainWindow, stress_triangles: usize) -> Timer {
    set_spacewars_panel(window, None);

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let start = Instant::now();
    let mut frame_count = 0_u64;

    timer.start(TimerMode::Repeated, TIMER_INTERVAL, move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        let convert_start = Instant::now();
        let frame = render::debug_frame(start.elapsed(), stress_triangles);
        let scene_item_count = present_frame(&window, frame);

        frame_count += 1;
        if frame_count % 120 == 0 {
            tracing::info!(
                stress_triangles,
                scene_item_count,
                convert_ms = convert_start.elapsed().as_secs_f64() * 1000.0,
                "debug render frame converted."
            );
        }
    });

    timer
}

pub fn start_scenario_loop(
    window: &MainWindow,
    scenario: &str,
    seed: u64,
) -> Result<Timer, HostError> {
    let mut scenario = HostedScenario::new(scenario, seed)?;
    let tick_model = scenario.tick_model();
    let fixed_dt = fixed_step_duration(tick_model);
    let input = std::rc::Rc::new(std::cell::RefCell::new(ClientInput::default()));
    input::install_window_input(window, std::rc::Rc::clone(&input));

    let timer = Timer::default();
    let weak_window = window.as_weak();
    let mut last_tick = Instant::now();
    let mut accumulator = Duration::ZERO;

    timer.start(TimerMode::Repeated, TIMER_INTERVAL, move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(last_tick);
        last_tick = now;

        let mut input = input.borrow_mut();
        step_scenario(
            &mut scenario,
            tick_model,
            fixed_dt,
            elapsed,
            &mut accumulator,
            &mut input,
        );
        set_spacewars_panel(&window, scenario.spacewars_panel_state());
        present_frames(&window, scenario.render_frames(), scenario.frame_layout());
    });

    Ok(timer)
}

fn step_scenario(
    scenario: &mut HostedScenario,
    tick_model: TickModel,
    fixed_dt: Option<Duration>,
    elapsed: Duration,
    accumulator: &mut Duration,
    input: &mut ClientInput,
) {
    match (tick_model, fixed_dt) {
        (TickModel::FixedTimestep { .. }, Some(dt)) => {
            *accumulator += elapsed;
            let mut steps = 0;
            while *accumulator >= dt && steps < MAX_FIXED_STEPS_PER_TICK {
                let actions = scenario.actions_from_input(input);
                scenario.step(&actions, dt);
                *accumulator -= dt;
                steps += 1;
            }
            if steps == MAX_FIXED_STEPS_PER_TICK {
                *accumulator = Duration::ZERO;
            }
        }
        (TickModel::Variable | TickModel::EmulatorClock, _) => {
            let actions = scenario.actions_from_input(input);
            scenario.step(&actions, elapsed);
        }
        (TickModel::FixedTimestep { .. }, None) => {}
    }
}

fn fixed_step_duration(tick_model: TickModel) -> Option<Duration> {
    match tick_model {
        TickModel::FixedTimestep { hz } => Some(Duration::from_secs_f64(1.0 / hz.max(1) as f64)),
        TickModel::Variable | TickModel::EmulatorClock => None,
    }
}

fn present_frame(window: &MainWindow, frame: RenderFrame) -> usize {
    present_frames(window, vec![frame], render::FrameLayout::EqualHorizontal)
}

fn present_frames(
    window: &MainWindow,
    frames: Vec<RenderFrame>,
    layout: render::FrameLayout,
) -> usize {
    let primitives = render::scene_primitives_from_frames_with_layout(
        &frames,
        Viewport::from_window(window.window()),
        layout,
    );
    let scene_item_count = primitives.len();
    window.set_primitives(ModelRc::new(VecModel::from(primitives)));
    window.window().request_redraw();
    scene_item_count
}

pub(crate) enum HostedScenario {
    Null(<NullScenario as Scenario>::State),
    Spacewars(Box<<SpacewarsScenario as Scenario>::State>),
}

#[derive(Debug, Clone, PartialEq)]
struct SpacewarsPanelState {
    player_1: PlayerPanelState,
    player_2: PlayerPanelState,
    planet_score_label: String,
    player_1_planet_fraction: f32,
    player_2_planet_fraction: f32,
    winner_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct PlayerPanelState {
    name: String,
    status: String,
    status_fraction: f32,
    color: CoreColor,
}

impl HostedScenario {
    pub(crate) fn new(name: &str, seed: u64) -> Result<Self, HostError> {
        match name {
            "null" => Ok(Self::Null(NullScenario::init(NullConfig, seed))),
            "spacewars" => Ok(Self::Spacewars(Box::new(SpacewarsScenario::init(
                SpacewarsConfig::default(),
                seed,
            )))),
            _ => Err(HostError::UnknownScenario { name: name.into() }),
        }
    }

    pub(crate) fn tick_model(&self) -> TickModel {
        match self {
            Self::Null(_) => NullScenario::tick_model(),
            Self::Spacewars(_) => SpacewarsScenario::tick_model(),
        }
    }

    pub(crate) fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        match self {
            Self::Null(state) => NullScenario::step(state, actions, dt),
            Self::Spacewars(state) => SpacewarsScenario::step(state, actions, dt),
        }
    }

    pub(crate) fn actions_from_input(&self, input: &mut ClientInput) -> Vec<Action> {
        match self {
            Self::Null(_) => Vec::new(),
            Self::Spacewars(_) => input.actions_for_spacewars(),
        }
    }

    pub(crate) fn render_frame(&self) -> RenderFrame {
        match self {
            Self::Null(state) => NullScenario::render_frame(state),
            Self::Spacewars(state) => SpacewarsScenario::render_frame(state),
        }
    }

    pub(crate) fn render_frames(&self) -> Vec<RenderFrame> {
        match self {
            Self::Null(state) => vec![NullScenario::render_frame(state)],
            Self::Spacewars(state) => SpacewarsScenario::render_local_play_frames(state),
        }
    }

    pub(crate) fn frame_layout(&self) -> render::FrameLayout {
        match self {
            Self::Null(_) => render::FrameLayout::EqualHorizontal,
            Self::Spacewars(_) => render::FrameLayout::SpacewarsLocalPlay,
        }
    }

    fn spacewars_panel_state(&self) -> Option<SpacewarsPanelState> {
        match self {
            Self::Null(_) => None,
            Self::Spacewars(state) => Some(spacewars_panel_state(state)),
        }
    }
}

fn spacewars_panel_state(state: &SpacewarsState) -> SpacewarsPanelState {
    let player_1_planets = state.players[0].planet_count;
    let player_2_planets = state.players[1].planet_count;
    let free_planets = state
        .planets
        .len()
        .saturating_sub(player_1_planets + player_2_planets);
    let total_planets = state.planets.len().max(1) as f32;

    SpacewarsPanelState {
        player_1: player_panel_state(state, 0),
        player_2: player_panel_state(state, 1),
        planet_score_label: format!(
            "Planets  P1 {player_1_planets} | Free {free_planets} | P2 {player_2_planets}"
        ),
        player_1_planet_fraction: player_1_planets as f32 / total_planets,
        player_2_planet_fraction: player_2_planets as f32 / total_planets,
        winner_text: state
            .winner
            .and_then(|winner| state.players.get(winner))
            .map(|player| format!("Game Over! {} Wins!", player.name)),
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

    let status_fraction = ship_life_fraction(ship.life, ship.life_max);
    let percent = display_percent(status_fraction);
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

fn ship_life_fraction(life: f32, life_max: f32) -> f32 {
    if life_max <= 0.0 {
        return 0.0;
    }

    (life / life_max).clamp(0.0, 1.0)
}

fn display_percent(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn set_spacewars_panel(window: &MainWindow, state: Option<SpacewarsPanelState>) {
    let Some(state) = state else {
        window.set_spacewars_ui_visible(false);
        return;
    };

    window.set_spacewars_ui_visible(true);
    window.set_p1_name(SharedString::from(state.player_1.name));
    window.set_p1_status(SharedString::from(state.player_1.status));
    window.set_p1_status_fraction(state.player_1.status_fraction);
    window.set_p1_color(brush_from_core_color(state.player_1.color));
    window.set_p2_name(SharedString::from(state.player_2.name));
    window.set_p2_status(SharedString::from(state.player_2.status));
    window.set_p2_status_fraction(state.player_2.status_fraction);
    window.set_p2_color(brush_from_core_color(state.player_2.color));
    window.set_planet_score_label(SharedString::from(state.planet_score_label));
    window.set_p1_planet_fraction(state.player_1_planet_fraction);
    window.set_p2_planet_fraction(state.player_2_planet_fraction);
    window.set_winner_text(SharedString::from(state.winner_text.unwrap_or_default()));
}

fn brush_from_core_color(color: CoreColor) -> Brush {
    Brush::SolidColor(SlintColor::from_argb_f32(
        color.a.clamp(0.0, 1.0),
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_scenario_is_rejected() {
        let err = match HostedScenario::new("bogus", 0) {
            Ok(_) => panic!("bogus scenario should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("unknown scenario"));
        assert!(err.to_string().contains("spacewars"));
    }

    #[test]
    fn null_scenario_renders_empty_frame() {
        let scenario = HostedScenario::new("null", 0).unwrap();

        assert!(scenario.render_frame().layers.is_empty());
        assert_eq!(scenario.spacewars_panel_state(), None);
    }

    #[test]
    fn spacewars_scenario_renders_initial_world() {
        let scenario = HostedScenario::new("spacewars", 0).unwrap();
        let frame = scenario.render_frame();

        match &scenario {
            HostedScenario::Spacewars(state) => {
                assert_eq!(state.config, SpacewarsConfig::default());
                assert!(state.sun.is_some());
                assert!(!state.planets.is_empty());
            }
            HostedScenario::Null(_) => panic!("spacewars scenario should not host null"),
        }
        assert!(!frame.layers.is_empty());
        assert!(matches!(
            scenario.tick_model(),
            TickModel::FixedTimestep { hz: 60 }
        ));
    }

    #[test]
    fn spacewars_scenario_renders_original_style_local_play_frames_for_client() {
        let scenario = HostedScenario::new("spacewars", 0).unwrap();
        let frames = scenario.render_frames();

        let HostedScenario::Spacewars(state) = &scenario else {
            panic!("spacewars scenario should not host null");
        };

        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].camera.center.x, state.ships[0].position.x);
        assert_eq!(frames[0].camera.center.y, state.ships[0].position.y);
        assert_eq!(frames[1].camera.center.x, state.ships[1].position.x);
        assert_eq!(frames[1].camera.center.y, state.ships[1].position.y);
        assert_eq!(frames[0].camera.height, frames[1].camera.height);
        assert_eq!(frames[2].camera.center.x, 1200.0);
        assert_eq!(frames[2].camera.center.y, 1200.0);
        assert_eq!(frames[3].camera, frames[2].camera);
        assert_eq!(
            scenario.frame_layout(),
            render::FrameLayout::SpacewarsLocalPlay
        );
    }

    #[test]
    fn spacewars_panel_state_reports_health_pod_and_planet_score() {
        let mut scenario = HostedScenario::new("spacewars", 0).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
        let total_planets = state.planets.len().max(1) as f32;
        state.ships[0].life = state.ships[0].life_max * 0.5;
        state.ships[1].form = ShipForm::EscapePod;
        state.ships[1].life = state.ships[1].life_max * 0.25;
        state.players[0].planet_count = 1;
        state.players[1].planet_count = 2;
        let free_planets = state.planets.len().saturating_sub(3);

        let panel = spacewars_panel_state(state);

        assert_eq!(panel.player_1.name, "Player 1: Player 1");
        assert_eq!(panel.player_1.status, "Ship Health: 50%");
        assert_eq!(panel.player_1.status_fraction, 0.5);
        assert_eq!(panel.player_2.name, "Player 2: Player 2");
        assert_eq!(panel.player_2.status, "Pod Rebuild: 25%");
        assert_eq!(panel.player_2.status_fraction, 0.25);
        assert_eq!(panel.winner_text, None);
        assert_eq!(
            panel.planet_score_label,
            format!("Planets  P1 1 | Free {free_planets} | P2 2")
        );
        assert_eq!(panel.player_1_planet_fraction, 1.0 / total_planets);
        assert_eq!(panel.player_2_planet_fraction, 2.0 / total_planets);
    }

    #[test]
    fn spacewars_panel_state_reports_winner_and_eliminated_player() {
        let mut scenario = HostedScenario::new("spacewars", 0).unwrap();
        let HostedScenario::Spacewars(state) = &mut scenario else {
            panic!("spacewars scenario should not host null");
        };
        state.players[0].eliminated = true;
        state.winner = Some(1);

        let panel = spacewars_panel_state(state);

        assert_eq!(panel.player_1.status, "Eliminated");
        assert_eq!(panel.player_1.status_fraction, 0.0);
        assert_eq!(panel.winner_text, Some("Game Over! Player 2 Wins!".into()));
    }
}
