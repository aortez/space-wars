//! Scenario hosting loop for the Slint client.

use std::fmt;
use std::time::{Duration, Instant};

use engine_common::{Action, RenderFrame, Scenario, StepResult, TickModel};
use engine_core::SpacewarsConfig;
use scenario_null::{NullConfig, NullScenario};
use scenario_spacewars::SpacewarsScenario;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

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
        present_frame(&window, scenario.render_frame());
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
    let primitives =
        render::scene_primitives_from_frame(&frame, Viewport::from_window(window.window()));
    let scene_item_count = primitives.len();
    window.set_primitives(ModelRc::new(VecModel::from(primitives)));
    window.window().request_redraw();
    scene_item_count
}

pub(crate) enum HostedScenario {
    Null(<NullScenario as Scenario>::State),
    Spacewars(Box<<SpacewarsScenario as Scenario>::State>),
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
}
